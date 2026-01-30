//! Test client utility for connecting to WebSocket servers.

use rsws::{compute_accept_key, CloseCode, Config, Connection, HandshakeResponse, Message, Role};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

const HANDSHAKE_TIMEOUT_SECS: u64 = 10;
const MAX_HEADER_SIZE: usize = 8192;

/// A test WebSocket client with convenience methods.
pub struct TestClient {
    conn: Connection<TcpStream>,
    id: usize,
}

impl TestClient {
    /// Connect to a WebSocket server at the given address.
    pub async fn connect(addr: SocketAddr) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    {
        Self::connect_with_id(addr, 0).await
    }

    /// Connect to a WebSocket server with a specific client ID.
    pub async fn connect_with_id(
        addr: SocketAddr,
        id: usize,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = TcpStream::connect(addr).await?;

        let key = generate_websocket_key(id);

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

        let handshake_result = timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS), async {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                response_bytes.extend_from_slice(line.as_bytes());
                if response_bytes.len() > MAX_HEADER_SIZE {
                    return Err("Header too large".into());
                }
                if line == "\r\n" {
                    break;
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await;

        match handshake_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("Handshake timeout".into()),
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

    /// Get the client's ID.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Send a text message.
    pub async fn send_text(
        &mut self,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.conn.send(Message::text(text)).await?;
        Ok(())
    }

    /// Send a binary message.
    pub async fn send_binary(
        &mut self,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.conn.send(Message::binary(data)).await?;
        Ok(())
    }

    /// Send a ping.
    pub async fn send_ping(
        &mut self,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.conn.send(Message::ping(data)).await?;
        Ok(())
    }

    /// Receive a message, expecting text.
    pub async fn recv_text(
        &mut self,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        match self.conn.recv().await? {
            Some(Message::Text(text)) => Ok(Some(text)),
            Some(Message::Close(_)) => Ok(None),
            Some(other) => Err(format!("Expected text, got {:?}", other).into()),
            None => Ok(None),
        }
    }

    /// Receive any message.
    pub async fn recv(&mut self) -> Result<Option<Message>, Box<dyn std::error::Error + Send + Sync>>
    {
        Ok(self.conn.recv().await?)
    }

    /// Close the connection gracefully.
    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.conn.close(CloseCode::Normal, "test complete").await?;

        // Wait for close confirmation
        while let Some(msg) = self.conn.recv().await? {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }

        Ok(())
    }

    /// Check if the connection is still open.
    pub fn is_open(&self) -> bool {
        self.conn.is_open()
    }
}

/// Generate a WebSocket key for handshake.
fn generate_websocket_key(seed: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let combined_seed = time_seed.wrapping_add(seed as u128);

    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = ((combined_seed >> (i * 4)) & 0xFF) as u8;
    }

    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestServer;

    #[tokio::test]
    async fn test_client_connect_and_echo() {
        let (server, addr) = TestServer::spawn().await;

        let mut client = TestClient::connect(addr).await.unwrap();
        client.send_text("hello").await.unwrap();
        let msg = client.recv_text().await.unwrap();
        assert_eq!(msg, Some("hello".to_string()));

        client.close().await.unwrap();
        server.shutdown().await;
    }

    #[tokio::test]
    async fn test_client_with_id() {
        let (server, addr) = TestServer::spawn().await;

        let client = TestClient::connect_with_id(addr, 42).await.unwrap();
        assert_eq!(client.id(), 42);

        server.shutdown().await;
    }
}
