//! WebSocket test client for concurrency testing.
//!
//! Provides a TestClient implementation for connecting and handshaking.

use rsws::{CloseCode, Config, Connection, HandshakeResponse, Message, Role, compute_accept_key};
use std::error::Error;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Test client wrapping a WebSocket connection.
pub struct TestClient {
    conn: Connection<TcpStream>,
    /// Optional client identifier for debugging/logging.
    #[allow(dead_code)]
    id: Option<usize>,
}

impl TestClient {
    /// Connect to a WebSocket server at the given address.
    pub async fn connect(addr: SocketAddr) -> Result<TestClient, Box<dyn Error + Send + Sync>> {
        Self::connect_internal(addr, None).await
    }

    /// Connect to a WebSocket server with a client identifier.
    pub async fn connect_with_id(
        addr: SocketAddr,
        id: usize,
    ) -> Result<TestClient, Box<dyn Error + Send + Sync>> {
        Self::connect_internal(addr, Some(id)).await
    }

    async fn connect_internal(
        addr: SocketAddr,
        id: Option<usize>,
    ) -> Result<TestClient, Box<dyn Error + Send + Sync>> {
        let mut stream = TcpStream::connect(addr).await?;
        let key = base64_encode_random_key();

        let request = format!(
            "GET / HTTP/1.1\r\n\
             Host: {}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n",
            addr, key
        );
        stream.write_all(request.as_bytes()).await?;

        let mut reader = BufReader::new(&mut stream);
        let mut response_bytes = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            response_bytes.extend_from_slice(line.as_bytes());
            if line == "\r\n" {
                break;
            }
        }

        let response = HandshakeResponse::parse(&response_bytes)?;
        let expected_accept = compute_accept_key(&key);
        if response.accept != expected_accept {
            return Err("Invalid Sec-WebSocket-Accept".into());
        }

        let config = Config::client();
        let conn = Connection::new(stream, Role::Client, config);

        Ok(TestClient { conn, id })
    }

    /// Send a text message.
    pub async fn send_text(&mut self, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.conn.send(Message::text(text)).await?;
        Ok(())
    }

    /// Receive a text message. Returns None if connection closed.
    pub async fn recv_text(&mut self) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        match self.conn.recv().await? {
            Some(Message::Text(text)) => Ok(Some(text)),
            Some(Message::Close(_)) | Some(_) | None => Ok(None),
        }
    }

    /// Close the connection gracefully.
    pub async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.conn.close(CloseCode::Normal, "").await?;
        while let Some(msg) = self.conn.recv().await? {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
        Ok(())
    }
}

fn base64_encode_random_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = ((seed >> (i * 4)) & 0xFF) as u8;
    }

    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}
