//! Autobahn-compatible WebSocket echo server for compliance testing.
//!
//! This server is designed to pass the Autobahn WebSocket test suite.
//! It handles all message types and edge cases required for RFC 6455 compliance.
//!
//! Run with: cargo run --example autobahn_server
//!
//! Then run Autobahn tests:
//! ```bash
//! docker run -it --rm \
//!   -v "${PWD}/autobahn:/config" \
//!   -v "${PWD}/autobahn/reports:/reports" \
//!   --network host \
//!   crossbario/autobahn-testsuite \
//!   wstest -m fuzzingclient -s /config/fuzzingclient.json
//! ```

use rsws::{Config, Connection, HandshakeRequest, HandshakeResponse, Message, Role};
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const ADDR: &str = "127.0.0.1:9001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Autobahn WebSocket Test Server listening on {}", ADDR);
    println!("Run Autobahn test suite with:");
    println!("  docker run -it --rm \\");
    println!("    -v \"${{PWD}}/autobahn:/config\" \\");
    println!("    -v \"${{PWD}}/autobahn/reports:/reports\" \\");
    println!("    --network host \\");
    println!("    crossbario/autobahn-testsuite \\");
    println!("    wstest -m fuzzingclient -s /config/fuzzingclient.json");
    println!();

    let listener = TcpListener::bind(ADDR).await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("Connection from: {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Connection ended: {} - {}", addr, e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut reader = BufReader::new(&mut stream);
    let mut request_bytes = Vec::new();

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Ok(());
        }
        request_bytes.extend_from_slice(line.as_bytes());
        if line == "\r\n" {
            break;
        }
    }

    let request = HandshakeRequest::parse(&request_bytes)?;
    request.validate()?;

    let response = HandshakeResponse::from_request(&request);
    let mut response_bytes = Vec::new();
    let _ = response.write(&mut response_bytes);
    stream.write_all(&response_bytes).await?;

    let mut config = Config::server();
    config.limits.max_message_size = 64 * 1024 * 1024;
    config.limits.max_frame_size = 64 * 1024 * 1024;

    let mut conn = Connection::new(stream, Role::Server, config);

    while conn.is_open() {
        match conn.recv().await? {
            Some(Message::Text(text)) => {
                conn.send(Message::text(text)).await?;
            }
            Some(Message::Binary(data)) => {
                conn.send(Message::binary(data)).await?;
            }
            Some(Message::Ping(_)) | Some(Message::Pong(_)) => {}
            Some(Message::Close(_)) | None => break,
            _ => {
                // Handle future message types gracefully
            }
        }
    }

    Ok(())
}
