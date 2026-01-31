use bytes::{Buf, BufMut, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::config::Config;
use crate::connection::Role;
use crate::error::{Error, Result};
use crate::protocol::Frame;
use crate::protocol::validation::FrameValidator;

/// Generate a random seed for mask generation.
/// Falls back to system time if getrandom fails.
fn random_mask_seed() -> u32 {
    let mut buf = [0u8; 4];
    if getrandom::getrandom(&mut buf).is_ok() {
        u32::from_le_bytes(buf)
    } else {
        // Fallback to system time
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u32)
            .unwrap_or(0x12345678)
    }
}

pub struct WebSocketCodec<T> {
    io: T,
    read_buf: BytesMut,
    write_buf: BytesMut,
    role: Role,
    config: Config,
    mask_counter: u32,
    validator: FrameValidator,
}

impl<T> WebSocketCodec<T> {
    #[must_use]
    pub fn new(io: T, role: Role, config: Config) -> Self {
        let validator = FrameValidator::new(role, config.limits.clone())
            .with_accept_unmasked(config.accept_unmasked_frames);
        Self {
            io,
            read_buf: BytesMut::with_capacity(config.read_buffer_size),
            write_buf: BytesMut::with_capacity(config.write_buffer_size),
            role,
            config,
            mask_counter: random_mask_seed(),
            validator,
        }
    }

    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    fn generate_mask(&mut self) -> [u8; 4] {
        self.mask_counter = self.mask_counter.wrapping_add(0x9E37_79B9);
        let a = self.mask_counter;
        let b = a.wrapping_mul(0x85EB_CA6B);
        let c = b ^ (b >> 13);
        let d = c.wrapping_mul(0xC2B2_AE35);
        d.to_le_bytes()
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> WebSocketCodec<T> {
    pub async fn read_frame(&mut self) -> Result<Frame> {
        loop {
            if self.read_buf.len() >= 2 {
                // Validate frame before parsing (extract metadata from raw buffer)
                let byte0 = self.read_buf[0];
                let byte1 = self.read_buf[1];
                let rsv1 = (byte0 & 0x40) != 0;
                let rsv2 = (byte0 & 0x20) != 0;
                let rsv3 = (byte0 & 0x10) != 0;
                let masked = (byte1 & 0x80) != 0;
                let payload_len_initial = byte1 & 0x7F;

                // Calculate payload length for validation
                let payload_len = match payload_len_initial {
                    0..=125 => Some(payload_len_initial as usize),
                    126 if self.read_buf.len() >= 4 => {
                        Some(u16::from_be_bytes([self.read_buf[2], self.read_buf[3]]) as usize)
                    }
                    127 if self.read_buf.len() >= 10 => Some(u64::from_be_bytes([
                        self.read_buf[2],
                        self.read_buf[3],
                        self.read_buf[4],
                        self.read_buf[5],
                        self.read_buf[6],
                        self.read_buf[7],
                        self.read_buf[8],
                        self.read_buf[9],
                    ]) as usize),
                    _ => None,
                };

                // Validate if we have enough bytes to determine payload length
                if let Some(len) = payload_len {
                    self.validator
                        .validate_incoming(masked, rsv1, rsv2, rsv3, len)?;
                }

                match Frame::parse(&self.read_buf) {
                    Ok((frame, consumed)) => {
                        self.read_buf.advance(consumed);
                        return Ok(frame);
                    }
                    Err(Error::IncompleteFrame { .. }) => {}
                    Err(e) => return Err(e),
                }
            }

            self.read_buf.reserve(4096);

            // SAFETY: `chunk_mut()` returns uninitialized memory as `UninitSlice`.
            // We create a raw slice to pass to `read()`, which only writes to it.
            // We only advance by the exact number of bytes `read()` reports writing.
            let buf = self.read_buf.chunk_mut();
            let buf_slice =
                unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr(), buf.len().min(4096)) };

            let n = self.io.read(buf_slice).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed(None));
            }

            // SAFETY: `read()` guarantees it initialized exactly `n` bytes.
            // We advance by `n` to mark those bytes as part of the buffer.
            unsafe { self.read_buf.advance_mut(n) };
        }
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
        let mask = if self.role.must_mask() {
            Some(self.generate_mask())
        } else {
            None
        };

        let wire_size = frame.wire_size(mask.is_some());
        self.write_buf.clear();
        self.write_buf.resize(wire_size, 0);

        let written = frame.write(&mut self.write_buf, mask)?;
        self.io.write_all(&self.write_buf[..written]).await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.io.flush().await?;
        Ok(())
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.io
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
    fn test_codec_new() {
        let stream = MockStream::new(vec![]);
        let codec = WebSocketCodec::new(stream, Role::Client, Config::client());
        assert_eq!(codec.role(), Role::Client);
    }

    #[tokio::test]
    async fn test_write_frame_masked() {
        let stream = MockStream::new(vec![]);
        let mut codec = WebSocketCodec::new(stream, Role::Client, Config::client());

        let frame = Frame::text(b"Hi".to_vec());
        codec.write_frame(&frame).await.unwrap();

        let written = codec.io.written();
        assert_eq!(written[0], 0x81);
        assert_eq!(written[1], 0x82);
        assert_eq!(written.len(), 8);
    }

    #[tokio::test]
    async fn test_write_frame_unmasked() {
        let stream = MockStream::new(vec![]);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let frame = Frame::text(b"Hi".to_vec());
        codec.write_frame(&frame).await.unwrap();

        let written = codec.io.written();
        assert_eq!(written[0], 0x81);
        assert_eq!(written[1], 0x02);
        assert_eq!(&written[2..4], b"Hi");
        assert_eq!(written.len(), 4);
    }

    #[tokio::test]
    async fn test_read_frame() {
        // Server receives masked frame from client: "Hello"
        // Mask: [0x37, 0xfa, 0x21, 0x3d], Masked payload: [0x7f, 0x9f, 0x4d, 0x51, 0x58]
        let data = vec![
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let stream = MockStream::new(data);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let frame = codec.read_frame().await.unwrap();
        assert!(frame.fin);
        assert_eq!(frame.payload(), b"Hello");
    }

    #[tokio::test]
    async fn test_read_incomplete_frame() {
        // Two masked frames from client:
        // Frame 1: Text "Hello" - mask [0x37, 0xfa, 0x21, 0x3d]
        // Frame 2: Binary [0x01, 0x02, 0x03] - mask [0x11, 0x22, 0x33, 0x44]
        let data = vec![
            // Frame 1: Text "Hello"
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
            // Frame 2: Binary [0x01, 0x02, 0x03] masked with [0x11, 0x22, 0x33, 0x44]
            // Masked: [0x01^0x11, 0x02^0x22, 0x03^0x33] = [0x10, 0x20, 0x30]
            0x82, 0x83, 0x11, 0x22, 0x33, 0x44, 0x10, 0x20, 0x30,
        ];
        let stream = MockStream::new(data);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let frame1 = codec.read_frame().await.unwrap();
        assert_eq!(frame1.payload(), b"Hello");

        let frame2 = codec.read_frame().await.unwrap();
        assert_eq!(frame2.payload(), &[0x01, 0x02, 0x03]);
    }

    #[tokio::test]
    async fn test_read_multiple_frames() {
        // Two masked frames from client:
        // Frame 1: Text "Hi" - mask [0x12, 0x34, 0x56, 0x78]
        // "Hi" = [0x48, 0x69] masked = [0x48^0x12, 0x69^0x34] = [0x5a, 0x5d]
        // Frame 2: Binary [0x01, 0x02] - mask [0xaa, 0xbb, 0xcc, 0xdd]
        // Masked: [0x01^0xaa, 0x02^0xbb] = [0xab, 0xb9]
        let data = vec![
            // Frame 1: Text "Hi"
            0x81, 0x82, 0x12, 0x34, 0x56, 0x78, 0x5a, 0x5d,
            // Frame 2: Binary [0x01, 0x02]
            0x82, 0x82, 0xaa, 0xbb, 0xcc, 0xdd, 0xab, 0xb9,
        ];
        let stream = MockStream::new(data);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let frame1 = codec.read_frame().await.unwrap();
        assert_eq!(frame1.payload(), b"Hi");

        let frame2 = codec.read_frame().await.unwrap();
        assert_eq!(frame2.payload(), &[0x01, 0x02]);
    }

    #[tokio::test]
    async fn test_flush() {
        let stream = MockStream::new(vec![]);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());
        assert!(codec.flush().await.is_ok());
    }

    #[tokio::test]
    async fn test_codec_with_large_payload() {
        // Large masked frame: 300 bytes of 0xAB
        // Mask: [0x00, 0x00, 0x00, 0x00] - identity mask for simplicity
        let payload = vec![0xAB; 300];
        // 0x82 = binary final, 0xFE = masked + 16-bit length follows
        // 0x01, 0x2C = 300 in big-endian
        // Then 4-byte mask [0x00, 0x00, 0x00, 0x00]
        let mut data = vec![0x82, 0xFE, 0x01, 0x2C, 0x00, 0x00, 0x00, 0x00];
        data.extend_from_slice(&payload); // With zero mask, masked == unmasked

        let stream = MockStream::new(data);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let frame = codec.read_frame().await.unwrap();
        assert_eq!(frame.payload().len(), 300);
        assert!(frame.payload().iter().all(|&b| b == 0xAB));
    }

    #[tokio::test]
    async fn test_read_connection_closed() {
        let stream = MockStream::new(vec![]);
        let mut codec = WebSocketCodec::new(stream, Role::Server, Config::server());

        let result = codec.read_frame().await;
        assert!(matches!(result, Err(Error::ConnectionClosed(None))));
    }

    #[tokio::test]
    async fn test_mask_not_zero_initially() {
        // 创建多个 codec，验证掩码不全为零
        // 注意：理论上可能随机到 0，但概率极低
        let mut found_nonzero = false;
        for _ in 0..10 {
            let stream = MockStream::new(vec![]);
            let mut codec = WebSocketCodec::new(stream, Role::Client, Config::client());

            // 通过 write_frame 触发掩码生成
            let frame = Frame::text(b"test".to_vec());
            let _ = codec.write_frame(&frame).await;

            let written = codec.io.written();
            if written.len() >= 6 {
                let mask = &written[2..6];
                if mask != [0, 0, 0, 0] {
                    found_nonzero = true;
                    break;
                }
            }
        }
        // 10次尝试中至少应该有一次非零掩码
        assert!(found_nonzero, "Mask should not always be zero");
    }

    #[tokio::test]
    async fn test_masks_differ_between_codecs() {
        // 创建两个 codec，它们的初始掩码应该不同
        use std::collections::HashSet;

        let mut masks = HashSet::new();
        for _ in 0..5 {
            let stream = MockStream::new(vec![]);
            let mut codec = WebSocketCodec::new(stream, Role::Client, Config::client());

            let frame = Frame::text(b"x".to_vec());
            let _ = codec.write_frame(&frame).await;

            let written = codec.io.written();
            if written.len() >= 6 {
                let mask: [u8; 4] = [written[2], written[3], written[4], written[5]];
                masks.insert(mask);
            }
        }
        // 5 个不同的 codec 应该产生至少 2 个不同的掩码
        assert!(
            masks.len() >= 2,
            "Different codecs should produce different masks"
        );
    }
}
