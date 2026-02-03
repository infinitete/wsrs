# rsws - Production-Grade WebSocket Library

English | [中文](README_CN.md)

`rsws` is a high-performance, RFC 6455 compliant WebSocket protocol library for Rust. Designed for production use with zero-copy parsing, async-first architecture, and comprehensive security features.

## Features

- **Zero-copy frame parsing** - Minimizes memory allocations for optimal throughput
- **Async-first design** - Runtime-agnostic core with Tokio support
- **Full RFC 6455 compliance** - Strict validation and protocol correctness
- **TLS/HTTPS support** - Secure WebSocket (wss://) via rustls or native-tls
- **Per-message deflate compression** - Reduce bandwidth usage with negotiated extensions
- **Production-ready limits** - Configurable frame/message size limits prevent resource exhaustion
- **Comprehensive error handling** - Detailed error types for debugging

## Installation

Add `rsws` to your `Cargo.toml`:

```toml
[dependencies]
rsws = "0.1"
```

## Quick Start

### Client Example

```rust
use rsws::{Connection, Config, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stream = tokio::net::TcpStream::connect("echo.websocket.org:80").await?;
    let config = Config::client();
    let mut conn = Connection::new(stream, Role::Client, config);

    conn.send(rsws::Message::text("Hello, WebSocket!")).await?;
    
    if let Some(msg) = conn.recv().await? {
        println!("Received: {:?}", msg);
    }
    
    conn.close(rsws::CloseCode::Normal, "done").await?;
    Ok(())
}
```

### Server Example

```rust
use rsws::{Connection, Config, Role, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    
    loop {
        let (stream, _) = listener.accept().await?;
        let config = Config::server();
        
        tokio::spawn(async move {
            let mut conn = Connection::new(stream, Role::Server, config);
            
            while let Some(msg) = conn.recv().await.unwrap() {
                // Echo back the message
                match msg {
                    Message::Text(text) => {
                        conn.send(Message::text(text)).await.unwrap();
                    }
                    Message::Binary(data) => {
                        conn.send(Message::binary(data)).await.unwrap();
                    }
                    _ => { /* handle control frames */ }
                }
            }
        });
    }
}
```

### TLS Server Example

```rust
use rsws::{tls::TlsAcceptor, Connection, Config, Role};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let certs = rsws::tls::load_certs_from_file("cert.pem")?;
    let key = rsws::tls::load_private_key_from_file("key.pem")?;
    let tls_config = rsws::tls::server_config(certs, key)?;
    let tls_acceptor = TlsAcceptor::new(Arc::new(tls_config));
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8443").await?;
    
    loop {
        let stream = listener.accept().await?;
        let tls_stream = tls_acceptor.accept(stream).await?;
        let config = Config::server();
        let mut conn = Connection::new(tls_stream, Role::Server, config);
        
        // Handle connection...
    }
}
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `async-tokio` | Enable async I/O with Tokio | Yes |
| `tls-rustls` | Enable TLS via rustls (pure Rust) | No |
| `tls-native` | Enable TLS via native-tls (platform-native) | No |
| `compression` | Enable per-message deflate compression | No |

### Recommended Configurations

```toml
# Minimal (no TLS, no compression)
[dependencies]
rsws = "0.1"

# With TLS via rustls
[dependencies]
rsws = { version = "0.1", features = ["tls-rustls"] }

# With compression
[dependencies]
rsws = { version = "0.1", features = ["compression"] }

# Full featured
[dependencies]
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```

## Performance 🚀

`rsws` has been re-engineered for extreme performance, achieving over **150 GiB/s** throughput using SIMD acceleration.

### Benchmark Results

| Payload Size | Scalar (Baseline) | SIMD (AVX2/NEON) | Improvement |
|--------------|-------------------|------------------|-------------|
| **64 KB**    | ~10.0 GiB/s       | **154.9 GiB/s**  | **~15x** 🚀 |
| **1 MB**     | 7.07 GiB/s        | **101.2 GiB/s**  | **~14x** 🚀 |

### Key Optimizations

- **SIMD Acceleration**: Runtime-detected AVX2/SSE2/NEON implementation for massive parallel processing of masking operations.
- **Zero-Copy Architecture**: 
  - `Bytes`-based parsing for unmasked frames (0 allocations).
  - Single-buffer message reassembly (eliminating N+1 allocations).
- **Efficient I/O**: `send_batch()` reduces syscall overhead, and direct buffer I/O eliminates intermediate copies.
- **Configurable Buffers**: Tune `read_buffer_size` and `write_buffer_size` for your workload.

To reproduce these results on your hardware:

```bash
cargo bench --bench benchmarks
```

## API Overview

### Core Types

- **`Connection<T>`** - Main WebSocket connection type, wrapping an async I/O stream
- **`Config`** - Connection configuration including limits and fragment sizes
- **`Limits`** - Resource limits for frame size, message size, and fragment count
- **`Message`** - Enum representing WebSocket messages (Text, Binary, Ping, Pong, Close)
- **`Frame`** - Low-level frame type for direct protocol manipulation

### Handshake Functions

- **`compute_accept_key`** - Compute the Sec-WebSocket-Accept header value
- **`HandshakeRequest`** / **`HandshakeResponse`** - Types for HTTP upgrade handshake

### Message Builders

```rust
// Create messages
let text = Message::text("Hello");
let binary = Message::binary(vec![0x01, 0x02, 0x03]);
let ping = Message::ping(vec![]);
let pong = Message::pong(data);
let close = Message::close(CloseCode::Normal, "goodbye");

// Check message type
if msg.is_text() { /* ... */ }
if msg.is_binary() { /* ... */ }
if msg.is_data() { /* ... */ }
if msg.is_control() { /* ... */ }

// Extract data
if let Some(text) = msg.into_text() { /* ... */ }
if let Some(data) = msg.into_binary() { /* ... */ }
```

### Configuration

```rust
// Default configuration
let config = Config::default();

// Server role (no masking, validate client frames)
let server_config = Config::server();

// Client role (mask all frames)
let client_config = Config::client();

// Custom limits
let config = Config::new()
    .with_limits(Limits::embedded())  // For resource-constrained environments
    .with_limits(Limits::unrestricted())  // For trusted environments
    .with_fragment_size(4096);  // Fragment large messages
```

## Error Handling

```rust
use rsws::{Error, Result};

match connection.send(Message::text("hello")).await {
    Ok(()) => println!("Sent successfully"),
    Err(Error::ConnectionClosed(None)) => println!("Connection already closed"),
    Err(Error::FrameTooLarge { size, max }) => {
        println!("Frame too large: {} > {}", size, max)
    }
    Err(e) => println!("Error: {:?}", e),
}
```

## RFC 6455 Compliance

`rsws` provides **full compliance** with [RFC 6455 - The WebSocket Protocol](https://tools.ietf.org/html/rfc6455).

### Compliance Summary

| Section | Feature | Status |
|---------|---------|--------|
| §4 | Opening Handshake | ✅ Full |
| §5.2 | Frame Format (FIN, RSV, Opcode, Mask, Payload) | ✅ Full |
| §5.3 | Client-to-Server Masking | ✅ Full |
| §5.4 | Fragmentation | ✅ Full |
| §5.5 | Control Frames (Close, Ping, Pong) | ✅ Full |
| §6 | UTF-8 Validation | ✅ Full |
| §7 | Closing Handshake | ✅ Full |
| §7.4 | Status Codes (1000-4999) | ✅ Full |
| §9 | Extensions | ✅ Full |
| §10 | Security (Origin, CSWSH) | ✅ Full |

### Implemented Features

- **Frame Parsing & Serialization**: All opcodes (0x0-0xA), 7/16/64-bit payload lengths
- **Masking**: SIMD-accelerated XOR (AVX2/SSE2/NEON), >150 GiB/s throughput
- **Fragmentation**: Message assembly with configurable limits
- **Control Frames**: Payload ≤125 bytes, no fragmentation enforced
- **Close Codes**: All standard codes (1000-1015) plus application codes (3000-4999)
- **UTF-8 Validation**: Incremental validation across fragments
- **Security Hardening**:
  - Origin validation (CSWSH protection)
  - CRLF injection prevention
  - Configurable size limits (DoS protection)
  - Duplicate header rejection

### Extensions

| Extension | RFC | Status |
|-----------|-----|--------|
| permessage-deflate | [RFC 7692](https://tools.ietf.org/html/rfc7692) | ✅ Implemented (feature-gated) |

### Autobahn Test Suite

This library includes [Autobahn WebSocket Testsuite](https://github.com/crossbario/autobahn-testsuite) integration for compliance verification. See [autobahn/README.md](autobahn/README.md) for details.

## License

[MIT License](LICENSE)
