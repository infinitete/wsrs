# rsws API Reference

Production-grade, RFC 6455 compliant WebSocket protocol library for Rust.

## Table of Contents

- [Core Types](#core-types)
- [Connection](#connection)
- [Messages](#messages)
- [Protocol](#protocol)
- [Configuration](#configuration)
- [Extensions](#extensions)
- [TLS Support](#tls-support)
- [Error Handling](#error-handling)

---

## Core Types

### Re-exports from `rsws`

```rust
pub use config::{Config, Limits};
pub use connection::{Connection, ConnectionState, Role};
pub use error::{Error, Result};
pub use message::{CloseCode, CloseFrame, Message};
pub use protocol::{HandshakeRequest, HandshakeResponse, OpCode, WS_GUID, compute_accept_key};
pub use codec::WebSocketCodec;  // feature = "async-tokio"
pub mod tls;                     // feature = "tls-rustls"
```

---

## Connection

### `Connection<T>`

High-level WebSocket connection wrapping an async I/O stream.

```rust
use rsws::{Connection, Config, Role, Message};

// Create a client connection
let config = Config::client();
let mut conn = Connection::new(stream, Role::Client, config);

// Send and receive messages
conn.send(Message::text("Hello")).await?;
if let Some(msg) = conn.recv().await? {
    println!("Received: {:?}", msg);
}

// Close gracefully
conn.close(CloseCode::Normal, "goodbye").await?;
```

#### Methods

| Method | Description |
|--------|-------------|
| `new(stream, role, config)` | Create a new connection |
| `send(message)` | Send a message (auto-flushes) |
| `send_no_flush(message)` | Send without flushing |
| `send_batch(messages)` | Send multiple messages with single flush |
| `recv()` | Receive next message (handles control frames) |
| `close(code, reason)` | Initiate close handshake |
| `flush()` | Flush write buffer |
| `state()` | Get current connection state |

### `ConnectionState`

```rust
pub enum ConnectionState {
    Open,     // Connection is active
    Closing,  // Close frame sent, awaiting response
    Closed,   // Connection fully closed
}
```

### `Role`

```rust
pub enum Role {
    Client,  // Must mask outgoing frames
    Server,  // Must NOT mask outgoing frames
}
```

---

## Messages

### `Message`

```rust
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}
```

#### Constructors

```rust
Message::text("Hello")           // Text message
Message::binary(vec![1, 2, 3])   // Binary message
Message::ping(vec![])            // Ping (keepalive)
Message::pong(data)              // Pong (response to ping)
Message::close(CloseCode::Normal, "bye")  // Close frame
```

#### Inspection Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `is_text()` | `bool` | True if Text variant |
| `is_binary()` | `bool` | True if Binary variant |
| `is_data()` | `bool` | True if Text or Binary |
| `is_control()` | `bool` | True if Ping, Pong, or Close |
| `as_text()` | `Option<&str>` | Borrow text content |
| `as_binary()` | `Option<&[u8]>` | Borrow binary content |
| `into_text()` | `Option<String>` | Consume and extract text |
| `into_binary()` | `Option<Vec<u8>>` | Consume and extract binary |

### `CloseCode`

RFC 6455 close status codes.

```rust
pub enum CloseCode {
    Normal,            // 1000 - Normal closure
    GoingAway,         // 1001 - Endpoint going away
    ProtocolError,     // 1002 - Protocol error
    UnsupportedData,   // 1003 - Unsupported data type
    InvalidPayload,    // 1007 - Invalid UTF-8
    PolicyViolation,   // 1008 - Policy violation
    MessageTooBig,     // 1009 - Message too large
    MandatoryExtension,// 1010 - Missing required extension
    InternalError,     // 1011 - Internal server error
    Other(u16),        // Custom code (3000-4999)
}
```

#### Methods

| Method | Description |
|--------|-------------|
| `from_u16(code)` | Create from numeric code |
| `as_u16()` | Get numeric value |
| `is_valid()` | Check if valid for sending (RFC 6455 §7.4.1) |
| `is_reserved()` | Check if reserved (1004-1006, 1015) |

### `CloseFrame`

```rust
pub struct CloseFrame {
    pub code: CloseCode,
    pub reason: String,
}
```

---

## Protocol

### `OpCode`

WebSocket frame opcodes.

```rust
pub enum OpCode {
    Continuation,  // 0x0 - Fragment continuation
    Text,          // 0x1 - Text frame
    Binary,        // 0x2 - Binary frame
    Close,         // 0x8 - Close control frame
    Ping,          // 0x9 - Ping control frame
    Pong,          // 0xA - Pong control frame
}
```

#### Methods

| Method | Description |
|--------|-------------|
| `from_u8(byte)` | Parse from byte (returns `Result`) |
| `as_u8()` | Convert to byte |
| `is_control()` | True for Close, Ping, Pong |
| `is_data()` | True for Text, Binary, Continuation |

### `Frame`

Low-level WebSocket frame.

```rust
// Create frames
let text = Frame::text(b"Hello".to_vec());
let binary = Frame::binary(data);
let ping = Frame::ping(vec![]);
let close = Frame::close(Some(1000), "bye");

// Parse from buffer
let (frame, consumed) = Frame::parse(&buffer)?;
frame.validate()?;

// Serialize to buffer
let size = frame.wire_size(masked);
frame.write(&mut buffer, mask_key)?;
```

### Handshake

```rust
use rsws::{HandshakeRequest, HandshakeResponse, compute_accept_key};

// Parse client request
let request = HandshakeRequest::parse(&buffer)?;
request.validate()?;

// Create server response
let response = HandshakeResponse::from_request(&request);
response.write(&mut buffer)?;

// Compute Sec-WebSocket-Accept
let accept = compute_accept_key(client_key);
```

### Masking

```rust
use rsws::protocol::{apply_mask, apply_mask_simd};

// Standard masking (byte-by-byte)
apply_mask(&mut data, mask_key);

// SIMD-accelerated masking (auto-detects AVX2/SSE2/NEON)
apply_mask_simd(&mut data, mask_key);
```

---

## Configuration

### `Config`

```rust
// Presets
let client = Config::client();  // Masks frames
let server = Config::server();  // Validates client masking

// Builder pattern
let config = Config::new()
    .with_limits(Limits::embedded())
    .with_fragment_size(4096)
    .with_read_buffer_size(8192)
    .with_write_buffer_size(8192);
```

### `Limits`

Resource limits for DoS protection.

```rust
// Presets
Limits::default()       // 16MB frame, 64MB message
Limits::embedded()      // 64KB frame, 256KB message
Limits::unrestricted()  // No limits (trusted environments only)

// Validation
limits.check_frame_size(size)?;
limits.check_message_size(size)?;
limits.check_fragment_count(count)?;
```

| Field | Default | Description |
|-------|---------|-------------|
| `max_frame_size` | 16 MB | Maximum single frame size |
| `max_message_size` | 64 MB | Maximum reassembled message size |
| `max_fragment_count` | 1024 | Maximum fragments per message |
| `max_handshake_size` | 8 KB | Maximum HTTP upgrade request size |

---

## Extensions

### Extension Framework

```rust
use rsws::extensions::{Extension, ExtensionRegistry, ExtensionOffer};

let mut registry = ExtensionRegistry::new();
registry.add(Box::new(my_extension))?;

// Client: generate offer header
let offer = registry.offer_header();

// Server: negotiate
let accepted = registry.negotiate(&client_offers);

// Apply to frames
registry.encode(&mut frame)?;
registry.decode(&mut frame)?;
```

### `DeflateExtension` (feature = "compression")

Per-message deflate compression (RFC 7692).

```rust
use rsws::extensions::deflate::{DeflateConfig, DeflateExtension};

let config = DeflateConfig::new()
    .server_no_context_takeover(true)
    .compression_level(6);

let extension = DeflateExtension::server(config);
```

---

## TLS Support

### rustls (feature = "tls-rustls")

```rust
use rsws::tls::{TlsConnector, TlsAcceptor, client_config_with_native_roots};

// Client
let config = client_config_with_native_roots()?;
let connector = TlsConnector::new(config);
let tls_stream = connector.connect("example.com", tcp_stream).await?;

// Server
let certs = load_certs_from_file("cert.pem")?;
let key = load_private_key_from_file("key.pem")?;
let config = server_config(certs, key)?;
let acceptor = TlsAcceptor::new(config);
let tls_stream = acceptor.accept(tcp_stream).await?;
```

---

## Error Handling

### `Error`

```rust
pub enum Error {
    InvalidFrame(String),
    ProtocolViolation(String),
    InvalidUtf8,
    FrameTooLarge { size: usize, max: usize },
    MessageTooLarge { size: usize, max: usize },
    TooManyFragments { count: usize, max: usize },
    ConnectionClosed(Option<u16>),
    InvalidHandshake(String),
    Io(String),
    Extension(String),
    InvalidCloseCode(u16),
    ReservedOpcode(u8),
    FragmentedControlFrame,
    ControlFrameTooLarge(usize),
    UnmaskedClientFrame,
    MaskedServerFrame,
    ReservedBitsSet,
    IncompleteFrame { needed: usize },
    InvalidOpcode(u8),
    // ... more variants
}
```

### `Result<T>`

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

---

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `async-tokio` | Async I/O with Tokio runtime | Yes |
| `tls-rustls` | TLS via rustls (pure Rust) | No |
| `tls-native` | TLS via native-tls (platform) | No |
| `compression` | permessage-deflate extension | No |

```toml
[dependencies]
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```
