//! Simple WebSocket client example.
//!
//! Run the echo server first: cargo run --example echo_server
//! Then run: cargo run --example client

use rsws::{CloseCode, Config, Connection, HandshakeResponse, Message, Role, compute_accept_key};
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const SERVER_ADDR: &str = "127.0.0.1:9001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Connecting to ws://{}", SERVER_ADDR);

    let mut stream = TcpStream::connect(SERVER_ADDR).await?;

    // Generate a random 16-byte key (base64 encoded)
    let key = base64_encode_random_key();

    // Send HTTP upgrade request
    let request = format!(
        "GET / HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        SERVER_ADDR, key
    );
    stream.write_all(request.as_bytes()).await?;

    // Read HTTP response
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

    // Parse and validate handshake response
    let response = HandshakeResponse::parse(&response_bytes)?;
    let expected_accept = compute_accept_key(&key);
    if response.accept != expected_accept {
        return Err("Invalid Sec-WebSocket-Accept".into());
    }
    println!("Handshake complete");

    // Create WebSocket connection
    let config = Config::client();
    let mut conn = Connection::new(stream, Role::Client, config);

    // Send a text message
    let message = "Hello, WebSocket!";
    println!("Sending: {}", message);
    conn.send(Message::text(message)).await?;

    // Receive the echo response
    if let Some(msg) = conn.recv().await? {
        match msg {
            Message::Text(text) => println!("Received: {}", text),
            Message::Binary(data) => println!("Received binary: {} bytes", data.len()),
            _ => println!("Received: {:?}", msg),
        }
    }

    // Close gracefully
    println!("Closing connection...");
    conn.close(CloseCode::Normal, "goodbye").await?;

    // Wait for server's close response
    while let Some(msg) = conn.recv().await? {
        if matches!(msg, Message::Close(_)) {
            println!("Received close confirmation");
            break;
        }
    }

    println!("Done");
    Ok(())
}

fn base64_encode_random_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Simple pseudo-random key generation for example purposes
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
