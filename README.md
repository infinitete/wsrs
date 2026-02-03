# rsws

[![CI](https://github.com/infinitete/rust-ws/actions/workflows/ci.yml/badge.svg)](https://github.com/infinitete/rust-ws/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A production-grade, RFC 6455 compliant WebSocket library for Rust.

## Features

- **RFC 6455 Compliant** — Full protocol implementation with strict validation
- **Async/Await** — Built on Tokio for high-performance async I/O
- **Zero-Copy Parsing** — Minimal allocations in hot paths
- **SIMD Acceleration** — Runtime-detected AVX2/SSE2/NEON for >150 GiB/s masking throughput
- **TLS Support** — Secure WebSocket (wss://) via rustls or native-tls
- **Compression** — Per-message deflate (RFC 7692)
- **Configurable Limits** — Protection against resource exhaustion attacks

## Installation

```toml
[dependencies]
rsws = "0.1"
```

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `async-tokio` | Async I/O with Tokio runtime | Yes |
| `tls-rustls` | TLS via rustls (pure Rust) | No |
| `tls-native` | TLS via native-tls (platform) | No |
| `compression` | Per-message deflate (RFC 7692) | No |

```toml
# With TLS
rsws = { version = "0.1", features = ["tls-rustls"] }

# With compression
rsws = { version = "0.1", features = ["compression"] }

# Full featured
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```

## Quick Start

### Echo Server

```rust
use rsws::{Connection, Config, Role, Message};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    
    loop {
        let (stream, _) = listener.accept().await?;
        
        tokio::spawn(async move {
            // Note: Handshake must be performed before wrapping
            let mut conn = Connection::new(stream, Role::Server, Config::server());
            
            while let Ok(Some(msg)) = conn.recv().await {
                match msg {
                    Message::Text(text) => {
                        conn.send(Message::text(text)).await.ok();
                    }
                    Message::Binary(data) => {
                        conn.send(Message::binary(data)).await.ok();
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });
    }
}
```

### Client

```rust
use rsws::{Connection, Config, Role, Message, CloseCode};
use rsws::protocol::handshake::{HandshakeRequest, HandshakeResponse, compute_accept_key};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    
    // Perform HTTP upgrade handshake
    let key = rsws::protocol::handshake::generate_key();
    let request = HandshakeRequest::new("127.0.0.1:8080", "/", &key);
    stream.write_all(request.to_string().as_bytes()).await?;
    
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = HandshakeResponse::parse(&buf[..n])?;
    assert_eq!(response.accept, compute_accept_key(&key));
    
    // Create WebSocket connection
    let mut conn = Connection::new(stream, Role::Client, Config::client());
    
    conn.send(Message::text("Hello, WebSocket!")).await?;
    
    if let Ok(Some(msg)) = conn.recv().await {
        println!("Received: {:?}", msg);
    }
    
    conn.close(CloseCode::Normal, "done").await?;
    Ok(())
}
```

## API Reference

### Core Types

| Type | Description |
|------|-------------|
| `Connection<T>` | WebSocket connection over async stream `T` |
| `Config` | Connection configuration (limits, buffering, masking) |
| `Limits` | Resource limits (frame size, message size, fragments) |
| `Message` | WebSocket message (Text, Binary, Ping, Pong, Close) |
| `CloseCode` | RFC 6455 close status codes |
| `CloseFrame` | Close frame with code and reason |
| `Role` | Connection role (Client or Server) |
| `ConnectionState` | State machine (Open, Closing, Closed) |

### Connection Methods

```rust
// Send a message (auto-flushes)
conn.send(Message::text("hello")).await?;

// Send without flushing (for batching)
conn.send_no_flush(Message::text("one")).await?;
conn.send_no_flush(Message::text("two")).await?;
conn.flush().await?;

// Batch send (single flush at end)
conn.send_batch([Message::text("a"), Message::text("b")]).await?;

// Receive next message
while let Some(msg) = conn.recv().await? {
    // Handle message
}

// Initiate close handshake
conn.close(CloseCode::Normal, "goodbye").await?;

// Check connection state
if conn.is_open() { /* ... */ }
```

### Message Builders

```rust
let text = Message::text("Hello");
let binary = Message::binary(vec![0x01, 0x02, 0x03]);
let ping = Message::ping(vec![]);
let pong = Message::pong(data);
let close = Message::close(CloseCode::Normal, "goodbye");

// Type checks
msg.is_text();
msg.is_binary();
msg.is_data();      // text or binary
msg.is_control();   // ping, pong, or close

// Extract data
msg.as_text();      // Option<&str>
msg.as_binary();    // Option<&[u8]>
msg.into_text();    // Option<String>
msg.into_binary();  // Option<Vec<u8>>
```

### Configuration

```rust
// Role-based presets
let server_config = Config::server();  // No masking, validates client frames
let client_config = Config::client();  // Masks all outgoing frames

// Custom configuration
let config = Config::new()
    .with_limits(Limits::default())
    .with_fragment_size(16 * 1024)
    .with_read_buffer_size(8192)
    .with_write_buffer_size(8192)
    .with_timeouts(Timeouts::default())
    .with_allowed_origins(vec!["https://example.com".into()]);
```

### Limits Presets

| Preset | Frame | Message | Fragments | Use Case |
|--------|-------|---------|-----------|----------|
| `Limits::default()` | 16 MB | 64 MB | 128 | General purpose |
| `Limits::embedded()` | 64 KB | 256 KB | 16 | Resource-constrained |
| `Limits::unrestricted()` | 1 GB | 4 GB | 1024 | Trusted environments |

### Error Handling

```rust
use rsws::{Error, Result};

match conn.recv().await {
    Ok(Some(msg)) => { /* handle message */ }
    Ok(None) => { /* connection closed */ }
    Err(Error::ConnectionClosed(_)) => { /* peer closed */ }
    Err(Error::FrameTooLarge { size, max }) => {
        eprintln!("Frame {} exceeds limit {}", size, max);
    }
    Err(Error::InvalidUtf8) => { /* invalid text frame */ }
    Err(Error::ProtocolViolation(reason)) => { /* RFC violation */ }
    Err(e) => { /* other error */ }
}
```

## TLS Support

### Server with rustls

```rust
use rsws::{Connection, Config, Role};
use rsws::tls::{TlsAcceptor, load_certs_from_file, load_private_key_from_file, server_config};
use std::sync::Arc;

let certs = load_certs_from_file("cert.pem")?;
let key = load_private_key_from_file("key.pem")?;
let tls_config = server_config(certs, key)?;
let acceptor = TlsAcceptor::new(Arc::new(tls_config));

let (tcp_stream, _) = listener.accept().await?;
let tls_stream = acceptor.accept(tcp_stream).await?;
let conn = Connection::new(tls_stream, Role::Server, Config::server());
```

### Client with rustls

```rust
use rsws::tls::{TlsConnector, client_config_with_native_roots};

let tls_config = client_config_with_native_roots()?;
let connector = TlsConnector::new(tls_config);
let tls_stream = connector.connect("example.com", tcp_stream).await?;
let conn = Connection::new(tls_stream, Role::Client, Config::client());
```

## Performance

rsws achieves **>150 GiB/s** masking throughput via SIMD acceleration:

| Payload | Scalar | SIMD (AVX2/NEON) | Speedup |
|---------|--------|------------------|---------|
| 64 KB | ~10 GiB/s | 154.9 GiB/s | ~15x |
| 1 MB | 7.07 GiB/s | 101.2 GiB/s | ~14x |

**Optimizations:**
- Runtime CPU feature detection (AVX2/SSE2/NEON)
- Zero-copy `Bytes`-based parsing for unmasked frames
- Single-buffer message reassembly
- Batch sending with `send_batch()` to reduce syscalls
- Configurable read/write buffer sizes

Run benchmarks:
```bash
cargo bench --bench benchmarks
```

## RFC 6455 Compliance

| Section | Feature | Status |
|---------|---------|--------|
| §4 | Opening Handshake | ✅ |
| §5.2 | Frame Format | ✅ |
| §5.3 | Client-to-Server Masking | ✅ |
| §5.4 | Fragmentation | ✅ |
| §5.5 | Control Frames | ✅ |
| §6 | UTF-8 Validation | ✅ |
| §7 | Closing Handshake | ✅ |
| §7.4 | Status Codes | ✅ |
| §9 | Extensions | ✅ |
| §10 | Security | ✅ |

### Extensions

| Extension | RFC | Status |
|-----------|-----|--------|
| permessage-deflate | RFC 7692 | ✅ (feature-gated) |

### Security Features

- CSWSH protection via origin validation
- CRLF injection prevention in headers
- Configurable size limits (DoS protection)
- Proper masking enforcement per role

### Autobahn Test Suite

See [autobahn/README.md](autobahn/README.md) for compliance verification.

## Examples

```bash
# Echo server
cargo run --example echo_server

# Client
cargo run --example client

# WSS client (TLS)
cargo run --example wss_client --features tls-rustls

# Stress testing
cargo run --example stress_server
cargo run --example stress_client

# Autobahn compliance
cargo run --example autobahn_server
```

## License

MIT
