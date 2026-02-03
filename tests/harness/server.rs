//! WebSocket test server for concurrency testing.
//!
//! Provides a TestServer implementation that can spawn echo servers on random ports.

use rsws::{Config, Connection, HandshakeRequest, HandshakeResponse, Message, Role};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

/// A test server that echoes WebSocket messages back to clients.
///
/// The server runs in a spawned task and can be gracefully shut down.
pub struct TestServer {
    shutdown_tx: oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// Spawn a new test server on an OS-assigned port.
    ///
    /// Returns the server handle and the address it's listening on.
    pub async fn spawn() -> (Self, SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            Self::run_server(listener, shutdown_rx).await;
        });

        (
            TestServer {
                shutdown_tx,
                handle,
            },
            addr,
        )
    }

    /// Gracefully shut down the server.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.handle.await;
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
                                let _ = Self::handle_connection(stream).await;
                            });
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Step 1: Read HTTP upgrade request
        let mut reader = BufReader::new(&mut stream);
        let mut request_bytes = Vec::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            request_bytes.extend_from_slice(line.as_bytes());
            if line == "\r\n" {
                break;
            }
        }

        // Step 2: Parse and validate handshake request
        let request = HandshakeRequest::parse(&request_bytes)?;
        request.validate()?;

        // Step 3: Generate and send handshake response
        let response = HandshakeResponse::from_request(&request);
        let mut response_bytes = Vec::new();
        response.write(&mut response_bytes);
        stream.write_all(&response_bytes).await?;

        // Step 4: Create WebSocket connection
        let config = Config::server();
        let mut conn = Connection::new(stream, Role::Server, config);

        // Step 5: Echo loop - handle messages
        while conn.is_open() {
            match conn.recv().await? {
                Some(Message::Text(text)) => {
                    conn.send(Message::text(text)).await?;
                }
                Some(Message::Binary(data)) => {
                    conn.send(Message::binary(data)).await?;
                }
                Some(Message::Ping(_) | Message::Pong(_)) => {}
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
