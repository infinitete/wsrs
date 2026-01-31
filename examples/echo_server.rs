//! Simple WebSocket echo server example.
//!
//! Run with: cargo run --example echo_server
//! Then connect with: cargo run --example client

use rsws::{Config, Connection, HandshakeRequest, HandshakeResponse, Message, Role};
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const ADDR: &str = "127.0.0.1:9001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("WebSocket Echo Server listening on {}", ADDR);

    let listener = TcpListener::bind(ADDR).await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    println!("  Handshake request for path: {}", request.path);

    // Step 3: Generate and send handshake response
    let response = HandshakeResponse::from_request(&request);
    let mut response_bytes = Vec::new();
    response.write(&mut response_bytes);
    stream.write_all(&response_bytes).await?;
    println!("  Handshake complete");

    // Step 4: Create WebSocket connection
    let config = Config::server();
    let mut conn = Connection::new(stream, Role::Server, config);

    // Step 5: Echo loop - handle messages
    while conn.is_open() {
        match conn.recv().await? {
            Some(Message::Text(text)) => {
                println!("  Received text: {}", text);
                conn.send(Message::text(text)).await?;
            }
            Some(Message::Binary(data)) => {
                println!("  Received binary: {} bytes", data.len());
                conn.send(Message::binary(data)).await?;
            }
            Some(Message::Ping(data)) => {
                println!(
                    "  Received ping ({} bytes) - pong sent automatically",
                    data.len()
                );
                // Pong is sent automatically by Connection::recv()
            }
            Some(Message::Pong(data)) => {
                println!("  Received pong: {} bytes", data.len());
            }
            Some(Message::Close(frame)) => {
                if let Some(cf) = frame {
                    println!("  Received close: {} - {}", cf.code.as_u16(), cf.reason);
                } else {
                    println!("  Received close (no code)");
                }
                break;
            }
            None => {
                println!("  Connection closed");
                break;
            }
            _ => {
                // Handle future message types gracefully
            }
        }
    }

    println!("  Session ended");
    Ok(())
}
