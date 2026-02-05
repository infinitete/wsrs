use tokio::io::{AsyncRead, AsyncWrite};

use crate::codec::WebSocketCodec;
use crate::config::Config;
use crate::connection::fragmenter::MessageFragmenter;
use crate::connection::{ConnectionState, Role};
use crate::error::{Error, Result};
use crate::extensions::ExtensionRegistry;
use crate::message::{CloseCode, CloseFrame, Message};
use crate::protocol::assembler::{AssembledMessage, MessageAssembler};
use crate::protocol::{Frame, OpCode};

/// A WebSocket connection wrapping an async I/O stream.
///
/// `Connection` provides high-level message-based communication over a WebSocket
/// connection. It handles frame parsing/serialization, message fragmentation,
/// and the WebSocket state machine.
///
/// ## Type Parameters
///
/// - `T`: The underlying async I/O stream (e.g., `TcpStream`, `TlsStream`)
///
/// ## Example
///
/// ```rust,ignore
/// use rsws::{Connection, Config, Role, Message};
///
/// let stream = tokio::net::TcpSocket::new_v4()?.connect("localhost:8080").await?;
/// let config = Config::client();
/// let mut conn = Connection::new(stream, Role::Client, config);
///
/// // Send a message
/// conn.send(Message::text("Hello")).await?;
///
/// // Receive a message
/// while let Some(msg) = conn.recv().await? {
///     println!("Received: {:?}", msg);
/// }
/// ```
pub struct Connection<T> {
    codec: WebSocketCodec<T>,
    state: ConnectionState,
    assembler: MessageAssembler,
    pending_pong: Option<Vec<u8>>,
    extensions: ExtensionRegistry,
}

impl<T> Connection<T> {
    /// Create a new WebSocket connection.
    ///
    /// This does not perform the HTTP upgrade handshake. Use this with a raw
    /// stream after completing the WebSocket handshake separately.
    ///
    /// ## Arguments
    ///
    /// - `io`: The underlying async I/O stream
    /// - `role`: The connection role (Client or Server)
    /// - `config`: Connection configuration
    pub fn new(io: T, role: Role, config: Config) -> Self {
        Self::with_extensions(io, role, config, ExtensionRegistry::new())
    }

    /// Create a new WebSocket connection with pre-configured extensions.
    ///
    /// Use this when you have already negotiated extensions during the handshake
    /// and want to apply them to the connection.
    ///
    /// ## Arguments
    ///
    /// - `io`: The underlying async I/O stream
    /// - `role`: The connection role (Client or Server)
    /// - `config`: Connection configuration
    /// - `extensions`: Pre-configured extension registry
    pub fn with_extensions(
        io: T,
        role: Role,
        config: Config,
        extensions: ExtensionRegistry,
    ) -> Self {
        let assembler = MessageAssembler::new(config.clone());
        Self {
            codec: WebSocketCodec::new(io, role, config),
            state: ConnectionState::Open,
            assembler,
            pending_pong: None,
            extensions,
        }
    }

    /// Get the current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Check if the connection is in an open state.
    ///
    /// Returns `true` if messages can be sent and received.
    pub fn is_open(&self) -> bool {
        self.state == ConnectionState::Open
    }

    /// Get mutable access to the extension registry.
    pub fn extensions_mut(&mut self) -> &mut ExtensionRegistry {
        &mut self.extensions
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> Connection<T> {
    /// Send a message over the WebSocket connection.
    ///
    /// Data messages (Text/Binary) are automatically fragmented if they exceed
    /// the configured `fragment_size` (default: 16 KB). Control frames (Ping,
    /// Pong, Close) are never fragmented per RFC 6455.
    ///
    /// ## Errors
    ///
    /// - `Error::ConnectionClosed` if the connection is not in a state that allows sending
    /// - `Error::MessageTooLarge` if the message exceeds `limits.max_message_size`
    /// - `Error::FrameTooLarge` if a fragment exceeds `limits.max_frame_size`
    /// - I/O errors from the underlying stream
    pub async fn send(&mut self, message: Message) -> Result<()> {
        if !self.state.can_send() {
            return Err(Error::ConnectionClosed(None));
        }

        // Control frames are never fragmented
        if message.is_control() {
            let frame = Frame::from(message);
            self.codec.write_frame(&frame).await?;
            self.codec.flush().await?;
            return Ok(());
        }

        // Validate message size before processing
        let payload = message.payload();
        self.codec
            .config()
            .limits
            .check_message_size(payload.len())?;

        let opcode = if message.is_text() {
            OpCode::Text
        } else {
            OpCode::Binary
        };

        let fragment_size = self.codec.config().fragment_size;

        if payload.len() <= fragment_size {
            // Small message: single frame with extension encoding
            let mut frame = Frame::from(message);
            self.extensions.encode(&mut frame)?;
            self.codec.write_frame(&frame).await?;
        } else {
            // Large message: fragment into multiple frames
            let fragmenter = MessageFragmenter::new(payload, opcode, fragment_size);
            let mut is_first = true;

            for mut frame in fragmenter {
                // RFC 7692: Extension encoding only on first frame
                if is_first && frame.opcode.is_data() {
                    self.extensions.encode(&mut frame)?;
                    is_first = false;
                }
                self.codec.write_frame(&frame).await?;
            }
        }

        self.codec.flush().await?;
        Ok(())
    }

    /// Send message without flushing. Call flush() when ready.
    pub async fn send_no_flush(&mut self, message: Message) -> Result<()> {
        if !self.state.can_send() {
            return Err(Error::ConnectionClosed(None));
        }

        // Control frames are never fragmented
        if message.is_control() {
            let frame = Frame::from(message);
            self.codec.write_frame(&frame).await?;
            return Ok(());
        }

        // Validate message size before processing
        let payload = message.payload();
        self.codec
            .config()
            .limits
            .check_message_size(payload.len())?;

        let opcode = if message.is_text() {
            OpCode::Text
        } else {
            OpCode::Binary
        };

        let fragment_size = self.codec.config().fragment_size;

        if payload.len() <= fragment_size {
            let mut frame = Frame::from(message);
            self.extensions.encode(&mut frame)?;
            self.codec.write_frame(&frame).await?;
        } else {
            let fragmenter = MessageFragmenter::new(payload, opcode, fragment_size);
            let mut is_first = true;

            for mut frame in fragmenter {
                if is_first && frame.opcode.is_data() {
                    self.extensions.encode(&mut frame)?;
                    is_first = false;
                }
                self.codec.write_frame(&frame).await?;
            }
        }

        Ok(())
    }

    /// Send multiple messages with single flush at end.
    pub async fn send_batch(&mut self, messages: impl IntoIterator<Item = Message>) -> Result<()> {
        for message in messages {
            self.send_no_flush(message).await?;
        }
        self.flush().await
    }

    /// Flush pending writes to the underlying stream.
    pub async fn flush(&mut self) -> Result<()> {
        self.codec.flush().await
    }

    /// Receive the next message from the WebSocket connection.
    ///
    /// This method handles:
    /// - Automatic pong response to ping frames
    /// - Message reassembly from fragments
    /// - Close frame handling and response
    ///
    /// Returns `Ok(Some(Message))` for normal messages, `Ok(None)` when the
    /// connection has been closed, or an error.
    ///
    /// ## Errors
    ///
    /// - Protocol errors (invalid frame, UTF-8 violation, etc.)
    /// - I/O errors from the underlying stream
    pub async fn recv(&mut self) -> Result<Option<Message>> {
        if !self.state.can_receive() {
            return Ok(None);
        }

        loop {
            if let Some(pong_data) = self.pending_pong.take() {
                let pong_frame = Frame::pong(pong_data);
                self.codec.write_frame(&pong_frame).await?;
                self.codec.flush().await?;
            }

            let frame = match self.codec.read_frame().await {
                Ok(f) => f,
                Err(Error::ConnectionClosed(_)) => {
                    self.state = ConnectionState::Closed;
                    return Ok(None);
                }
                Err(e) => return Err(e),
            };

            match frame.opcode {
                OpCode::Ping => {
                    frame.validate()?;
                    self.pending_pong = Some(frame.payload().to_vec());
                    return Ok(Some(Message::Ping(frame.into_payload())));
                }
                OpCode::Pong => {
                    frame.validate()?;
                    return Ok(Some(Message::Pong(frame.into_payload())));
                }
                OpCode::Close => {
                    frame.validate()?;
                    let close_frame = self.parse_close_frame(&frame);

                    if self.state == ConnectionState::Open {
                        self.state = ConnectionState::Closing;
                        let response = if let Some(ref cf) = close_frame {
                            Frame::close(Some(cf.code.as_u16()), &cf.reason)
                        } else {
                            Frame::close(None, "")
                        };
                        let _ = self.codec.write_frame(&response).await;
                        let _ = self.codec.flush().await;
                    }

                    self.state = ConnectionState::Closed;
                    return Ok(Some(Message::Close(close_frame)));
                }
                OpCode::Text | OpCode::Binary | OpCode::Continuation => {
                    frame.validate()?;
                    if let Some(assembled) = self.assembler.push(frame)? {
                        return Ok(Some(self.assembled_to_message(assembled)?));
                    }
                }
            }
        }
    }

    /// Send a ping frame.
    ///
    /// This is a convenience method that wraps `send(Message::Ping(...))`.
    pub async fn ping(&mut self, data: Vec<u8>) -> Result<()> {
        self.send(Message::Ping(data)).await
    }

    /// Send a pong frame.
    ///
    /// This is a convenience method that wraps `send(Message::Pong(...))`.
    pub async fn pong(&mut self, data: Vec<u8>) -> Result<()> {
        self.send(Message::Pong(data)).await
    }

    /// Initiate a close handshake.
    ///
    /// Sends a close frame with the given status code and reason, then waits
    /// for the peer's close response.
    ///
    /// ## Arguments
    ///
    /// - `code`: The close status code
    /// - `reason`: Human-readable reason for closing
    ///
    /// This does not close the underlying stream; you should drop the
    /// `Connection` after calling this.
    pub async fn close(&mut self, code: CloseCode, reason: &str) -> Result<()> {
        if self.state != ConnectionState::Open {
            return Ok(());
        }

        if code.is_reserved() {
            return Err(Error::InvalidCloseCode(code.as_u16()));
        }

        self.state = ConnectionState::Closing;
        let frame = Frame::close(Some(code.as_u16()), reason);
        self.codec.write_frame(&frame).await?;
        self.codec.flush().await?;
        Ok(())
    }

    fn parse_close_frame(&self, frame: &Frame) -> Option<CloseFrame> {
        let payload = frame.payload();
        if payload.len() >= 2 {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            match std::str::from_utf8(&payload[2..]) {
                Ok(reason) => Some(CloseFrame::new(
                    CloseCode::from_u16(code),
                    reason.to_owned(),
                )),
                Err(_) => Some(CloseFrame::new(CloseCode::InvalidPayload, "")),
            }
        } else if payload.is_empty() {
            None
        } else {
            Some(CloseFrame::new(
                CloseCode::ProtocolError,
                "Invalid close frame",
            ))
        }
    }

    fn assembled_to_message(&mut self, assembled: AssembledMessage) -> Result<Message> {
        let payload = if assembled.rsv1 && self.extensions.negotiated_count() > 0 {
            let mut frame = Frame::new(true, assembled.opcode, assembled.payload);
            frame.rsv1 = true;
            self.extensions.decode(&mut frame)?;
            frame.into_payload()
        } else {
            assembled.payload
        };

        match assembled.opcode {
            OpCode::Text => {
                let text = String::from_utf8(payload).map_err(|_| Error::InvalidUtf8)?;
                Ok(Message::Text(text))
            }
            OpCode::Binary => Ok(Message::Binary(payload)),
            _ => Err(Error::ProtocolViolation("Unexpected opcode".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    struct MockStream {
        read_data: Cursor<Vec<u8>>,
        write_data: Vec<u8>,
    }

    impl MockStream {
        fn new(data: Vec<u8>) -> Self {
            Self {
                read_data: Cursor::new(data),
                write_data: Vec::new(),
            }
        }

        fn written(&self) -> &[u8] {
            &self.write_data
        }
    }

    impl AsyncRead for MockStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let pos = self.read_data.position() as usize;
            let data = self.read_data.get_ref();
            if pos >= data.len() {
                return Poll::Ready(Ok(()));
            }
            let remaining = &data[pos..];
            let to_copy = std::cmp::min(remaining.len(), buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.read_data.set_position((pos + to_copy) as u64);
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for MockStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.write_data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn test_connection_new() {
        let stream = MockStream::new(vec![]);
        let conn = Connection::new(stream, Role::Client, Config::client());
        assert_eq!(conn.state(), ConnectionState::Open);
        assert!(conn.is_open());
    }

    #[tokio::test]
    async fn test_send_text_message() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.send(Message::text("Hello")).await.unwrap();

        let written = conn.codec.into_inner().written().to_vec();
        assert_eq!(written[0], 0x81);
        assert_eq!(written[1], 0x05);
        assert_eq!(&written[2..7], b"Hello");
    }

    #[tokio::test]
    async fn test_send_binary_message() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.send(Message::binary(vec![1, 2, 3])).await.unwrap();

        let written = conn.codec.into_inner().written().to_vec();
        assert_eq!(written[0], 0x82);
        assert_eq!(written[1], 0x03);
        assert_eq!(&written[2..5], &[1, 2, 3]);
    }

    #[tokio::test]
    async fn test_recv_message() {
        // Masked "Hello": mask [0x37, 0xfa, 0x21, 0x3d], payload [0x7f, 0x9f, 0x4d, 0x51, 0x58]
        let data = vec![
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let stream = MockStream::new(data);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let msg = conn.recv().await.unwrap().unwrap();
        assert!(matches!(msg, Message::Text(s) if s == "Hello"));
    }

    #[tokio::test]
    async fn test_ping_pong() {
        // Masked ping "ping": mask [0x00, 0x00, 0x00, 0x00] (identity)
        let ping_frame = vec![0x89, 0x84, 0x00, 0x00, 0x00, 0x00, 0x70, 0x69, 0x6e, 0x67];
        let stream = MockStream::new(ping_frame);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let msg = conn.recv().await.unwrap().unwrap();
        assert!(matches!(msg, Message::Ping(ref d) if d == b"ping"));

        assert!(conn.pending_pong.is_some());
    }

    #[tokio::test]
    async fn test_close_handshake() {
        // Masked close with code 1000: mask [0x00, 0x00, 0x00, 0x00], payload [0x03, 0xe8]
        let close_frame = vec![0x88, 0x82, 0x00, 0x00, 0x00, 0x00, 0x03, 0xe8];
        let stream = MockStream::new(close_frame);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let msg = conn.recv().await.unwrap().unwrap();

        match msg {
            Message::Close(Some(cf)) => {
                assert_eq!(cf.code, CloseCode::Normal);
            }
            _ => panic!("Expected close message"),
        }

        assert_eq!(conn.state(), ConnectionState::Closed);
    }

    #[tokio::test]
    async fn test_state_transitions() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        assert_eq!(conn.state(), ConnectionState::Open);
        assert!(conn.is_open());

        conn.close(CloseCode::Normal, "goodbye").await.unwrap();
        assert_eq!(conn.state(), ConnectionState::Closing);
        assert!(!conn.is_open());
    }

    #[tokio::test]
    async fn test_recv_binary_message() {
        // Masked [0x01, 0x02, 0x03]: mask [0x00, 0x00, 0x00, 0x00]
        let data = vec![0x82, 0x83, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03];
        let stream = MockStream::new(data);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let msg = conn.recv().await.unwrap().unwrap();
        assert!(matches!(msg, Message::Binary(ref d) if d == &[1, 2, 3]));
    }

    #[tokio::test]
    async fn test_recv_pong() {
        // Masked pong "pong": mask [0x00, 0x00, 0x00, 0x00]
        let pong_frame = vec![0x8a, 0x84, 0x00, 0x00, 0x00, 0x00, 0x70, 0x6f, 0x6e, 0x67];
        let stream = MockStream::new(pong_frame);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let msg = conn.recv().await.unwrap().unwrap();
        assert!(matches!(msg, Message::Pong(ref d) if d == b"pong"));
    }

    #[tokio::test]
    async fn test_send_close() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.close(CloseCode::Normal, "bye").await.unwrap();

        let written = conn.codec.into_inner().written().to_vec();
        assert_eq!(written[0], 0x88);
    }

    #[tokio::test]
    async fn test_send_after_close_fails() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.close(CloseCode::Normal, "bye").await.unwrap();

        let result = conn.send(Message::text("test")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_recv_after_close_returns_none() {
        // Masked empty close: mask [0x00, 0x00, 0x00, 0x00]
        let close_frame = vec![0x88, 0x80, 0x00, 0x00, 0x00, 0x00];
        let stream = MockStream::new(close_frame);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let _ = conn.recv().await;

        let msg = conn.recv().await.unwrap();
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_send_no_flush() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.send_no_flush(Message::text("Hello")).await.unwrap();

        // Even though we haven't flushed, MockStream's poll_write is immediate in this mock.
        // In a real AsyncWrite with buffering, it wouldn't reach the OS until flush.
        let written = conn.codec.into_inner().written().to_vec();
        assert_eq!(written[0], 0x81);
        assert_eq!(written[1], 0x05);
        assert_eq!(&written[2..7], b"Hello");
    }

    #[tokio::test]
    async fn test_send_batch() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        let messages = vec![Message::text("One"), Message::text("Two")];

        conn.send_batch(messages).await.unwrap();

        let written = conn.codec.into_inner().written().to_vec();
        // First frame
        assert_eq!(written[0], 0x81);
        assert_eq!(written[1], 0x03);
        assert_eq!(&written[2..5], b"One");
        // Second frame
        assert_eq!(written[5], 0x81);
        assert_eq!(written[6], 0x03);
        assert_eq!(&written[7..10], b"Two");
    }

    #[tokio::test]
    async fn test_flush() {
        let stream = MockStream::new(vec![]);
        let mut conn = Connection::new(stream, Role::Server, Config::server());

        conn.send_no_flush(Message::text("test")).await.unwrap();
        conn.flush().await.unwrap();

        let written = conn.codec.into_inner().written().to_vec();
        assert_eq!(written[0], 0x81);
    }
}
