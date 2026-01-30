//! Test server utility for spawning echo WebSocket servers.

use rsws::{Config, Connection, HandshakeRequest, HandshakeResponse, Message, Role};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;

const HANDSHAKE_TIMEOUT_SECS: u64 = 10;
const MAX_HEADER_SIZE: usize = 8192;

/// A test WebSocket echo server that can be spawned and shut down.
pub struct TestServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    addr: SocketAddr,
}

impl TestServer {
    /// Spawn a new echo server on an OS-assigned port.
    ///
    /// Returns the server handle and the address it's listening on.
    pub async fn spawn() -> (Self, SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(Self::run_server(listener, shutdown_rx));

        let server = TestServer {
            shutdown_tx: Some(shutdown_tx),
            addr,
        };

        (server, addr)
    }

    /// Get the address the server is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Shut down the server gracefully.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Give tasks a moment to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    async fn run_server(listener: TcpListener, mut shutdown_rx: oneshot::Receiver<()>) {
        loop {
            tokio::select! {
                biased;

                _ = &mut shutdown_rx => {
                    break;
                }

                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            tokio::spawn(async move {
                                if let Err(_e) = Self::handle_connection(stream).await {
                                    // Connection errors are expected during stress tests
                                }
                            });
                        }
                        Err(_) => {
                            // Accept errors during shutdown are expected
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut reader = BufReader::new(&mut stream);
        let mut request_bytes = Vec::new();

        let handshake_result = timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS), async {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                request_bytes.extend_from_slice(line.as_bytes());
                if request_bytes.len() > MAX_HEADER_SIZE {
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

        let request = HandshakeRequest::parse(&request_bytes)?;
        request.validate()?;

        let response = HandshakeResponse::from_request(&request);
        let mut response_bytes = Vec::new();
        response.write(&mut response_bytes);
        stream.write_all(&response_bytes).await?;

        let config = Config::server();
        let mut conn = Connection::new(stream, Role::Server, config);

        while conn.is_open() {
            match conn.recv().await? {
                Some(Message::Text(text)) => {
                    conn.send(Message::text(text)).await?;
                }
                Some(Message::Binary(data)) => {
                    conn.send(Message::binary(data)).await?;
                }
                Some(Message::Ping(_)) => {}
                Some(Message::Pong(_)) => {}
                Some(Message::Close(_)) => {
                    break;
                }
                None => {
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_spawn_and_shutdown() {
        let (server, addr) = TestServer::spawn().await;
        assert!(addr.port() > 0);
        server.shutdown().await;
    }
}
