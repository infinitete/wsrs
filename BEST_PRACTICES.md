# Best Practices

[中文版](BEST_PRACTICES_CN.md)

This document outlines recommended patterns for using rsws effectively.

---

## Table of Contents

1. [Connection Management](#connection-management)
2. [Performance Optimization](#performance-optimization)
3. [Security](#security)
4. [Configuration](#configuration)
5. [Error Handling](#error-handling)
6. [Resource Management](#resource-management)

---

## Connection Management

### Graceful Shutdown

**DO**: Use `close()` to initiate proper RFC 6455 close handshake.

```rust
// ✅ Good: Proper close handshake
conn.close(CloseCode::Normal, "goodbye").await?;

// ❌ Bad: Dropping connection without close
drop(conn);  // Peer may report protocol error
```

### Message Loop Pattern

**DO**: Use `while let` with `is_open()` check for robust message handling.

```rust
// ✅ Recommended pattern
while conn.is_open() {
    match conn.recv().await? {
        Some(Message::Text(text)) => {
            conn.send(Message::text(process(text))).await?;
        }
        Some(Message::Binary(data)) => {
            conn.send(Message::binary(process(data))).await?;
        }
        Some(Message::Ping(_)) => {
            // Pong is sent automatically by recv()
        }
        Some(Message::Close(_)) => break,
        None => break,  // Connection closed cleanly
        _ => {}
    }
}
```

### Handshake Handling

**DO**: Complete HTTP upgrade handshake before creating `Connection`.

```rust
// Server side
let request = HandshakeRequest::parse(&request_bytes)?;
request.validate()?;
let response = HandshakeResponse::from_request(&request);
stream.write_all(&response.to_bytes()).await?;

// Now create connection
let conn = Connection::new(stream, Role::Server, Config::server());
```

---

## Performance Optimization

### Batch Sending

**DO**: Use `send_batch()` or `send_no_flush()` + `flush()` for multiple messages.

```rust
// ✅ Best: Single syscall for multiple messages
conn.send_batch([
    Message::text("one"),
    Message::text("two"),
    Message::text("three"),
]).await?;

// ✅ Alternative: Manual batching
conn.send_no_flush(Message::text("one")).await?;
conn.send_no_flush(Message::text("two")).await?;
conn.send_no_flush(Message::text("three")).await?;
conn.flush().await?;

// ❌ Inefficient: Three syscalls
conn.send(Message::text("one")).await?;
conn.send(Message::text("two")).await?;
conn.send(Message::text("three")).await?;
```

### Buffer Sizing

**DO**: Tune buffer sizes for your network characteristics.

```rust
// High-bandwidth links (10GbE+)
let config = Config::server()
    .with_read_buffer_size(64 * 1024)   // 64 KB
    .with_write_buffer_size(64 * 1024);

// Low-latency applications
let config = Config::server()
    .with_read_buffer_size(4 * 1024)    // 4 KB - faster processing
    .with_write_buffer_size(4 * 1024);

// Default: 8 KB - balanced
```

### Fragment Size

**DO**: Choose fragment size based on message patterns.

```rust
// Large file transfers
let config = Config::client()
    .with_fragment_size(64 * 1024);  // 64 KB fragments

// Small frequent messages
let config = Config::client()
    .with_fragment_size(4 * 1024);   // 4 KB fragments

// Default: 16 KB
```

### SIMD Acceleration

The library automatically uses SIMD (AVX2/SSE2/NEON) for masking operations. No configuration needed - runtime detection selects the optimal implementation.

**Throughput**: >150 GiB/s on modern CPUs

---

## Security

### Origin Validation (CSWSH Protection)

**DO**: Always configure `allowed_origins` in production.

```rust
// ✅ Production: Whitelist allowed origins
let config = Config::server()
    .with_allowed_origins(vec![
        "https://app.example.com".into(),
        "https://admin.example.com".into(),
    ]);

// ❌ Development only: Accept any origin
let config = Config::server();  // No origin check
```

### Size Limits

**DO**: Choose appropriate limits to prevent DoS attacks.

```rust
// Web application (chat, notifications)
let config = Config::server()
    .with_limits(Limits::default());  // 16 MB frame, 64 MB message

// Embedded/IoT with memory constraints
let config = Config::server()
    .with_limits(Limits::embedded());  // 64 KB frame, 256 KB message

// Internal microservices (trusted network)
let config = Config::server()
    .with_limits(Limits::unrestricted());  // 1 GB frame, 4 GB message
```

### Frame Masking

The library enforces RFC 6455 masking rules automatically:

- **Client** (`Config::client()`): All frames are masked
- **Server** (`Config::server()`): Rejects unmasked client frames

```rust
// ⚠️ Testing only: Accept unmasked frames
let config = Config {
    accept_unmasked_frames: true,  // INSECURE
    ..Config::server()
};
```

---

## Configuration

### Role-Based Presets

**DO**: Use `Config::server()` and `Config::client()` as starting points.

```rust
// Server: No masking, validates client frames
let config = Config::server();

// Client: Masks all outgoing frames
let config = Config::client();
```

### Limits Comparison

| Preset | Frame | Message | Fragments | Use Case |
|--------|-------|---------|-----------|----------|
| `default()` | 16 MB | 64 MB | 128 | General web apps |
| `embedded()` | 64 KB | 256 KB | 16 | IoT, constrained memory |
| `unrestricted()` | 1 GB | 4 GB | 1024 | Trusted internal services |

### Timeout Configuration

**DO**: Configure timeouts to handle zombie connections.

```rust
use std::time::Duration;
use rsws::config::Timeouts;

let config = Config::server()
    .with_timeouts(Timeouts {
        handshake: Duration::from_secs(10),   // Handshake timeout
        read: Duration::from_secs(30),        // Read timeout
        write: Duration::from_secs(30),       // Write timeout
        idle: Duration::from_secs(300),       // Idle timeout
    });
```

---

## Error Handling

### Error Categories

```rust
match conn.recv().await {
    Ok(Some(msg)) => { /* Process message */ }
    
    Ok(None) => {
        // Clean close - connection ended normally
    }
    
    Err(Error::ConnectionClosed(_)) => {
        // Peer disconnected (possibly unclean)
    }
    
    Err(Error::ProtocolViolation(reason)) => {
        // ⚠️ FATAL: Connection is likely desynchronized
        // Drop connection immediately
        return Err(e);
    }
    
    Err(Error::FrameTooLarge { size, max }) => {
        // Peer sent oversized frame - close with error
        conn.close(CloseCode::MessageTooBig, "frame too large").await?;
    }
    
    Err(Error::InvalidUtf8) => {
        // Text frame contains invalid UTF-8
        conn.close(CloseCode::InvalidPayload, "invalid utf-8").await?;
    }
    
    Err(e) => {
        // Other errors (I/O, etc.)
        eprintln!("Error: {}", e);
    }
}
```

### Protocol Violations

**DO**: Treat `ProtocolViolation` as fatal - drop the connection immediately.

```rust
// ❌ Don't try to recover from protocol violations
if let Err(Error::ProtocolViolation(_)) = result {
    // Connection state is undefined - close immediately
    return Err(e);
}
```

---

## Resource Management

### Connection Per Task

**DO**: Spawn one task per connection.

```rust
loop {
    let (stream, addr) = listener.accept().await?;
    
    tokio::spawn(async move {
        if let Err(e) = handle_connection(stream).await {
            eprintln!("Connection {} error: {}", addr, e);
        }
    });
}
```

### Ping/Pong Handling

Pongs are sent automatically by `recv()`. You don't need to handle them manually.

```rust
// ✅ Automatic pong - no action needed
match conn.recv().await? {
    Some(Message::Ping(data)) => {
        // Pong is already queued for next recv() call
        println!("Ping received, pong will be sent automatically");
    }
    // ...
}

// Manual ping for keepalive
conn.ping(vec![]).await?;
```

### Memory with Compression

When using `compression` feature, each connection maintains an LZ77 dictionary (~32 KB).

```rust
// High-concurrency: Consider the memory overhead
// 10,000 connections × 32 KB = 320 MB for compression state alone
```

---

## Anti-Patterns

### ❌ Blocking in Async Context

```rust
// ❌ Bad: Blocking call in async task
std::thread::sleep(Duration::from_secs(1));

// ✅ Good: Use async sleep
tokio::time::sleep(Duration::from_secs(1)).await;
```

### ❌ Ignoring Close Frames

```rust
// ❌ Bad: Ignoring close, causing peer timeout
while let Some(msg) = conn.recv().await? {
    if msg.is_text() { /* ... */ }
}

// ✅ Good: Handle close explicitly
while let Some(msg) = conn.recv().await? {
    match msg {
        Message::Close(_) => break,
        Message::Text(t) => { /* ... */ }
        _ => {}
    }
}
```

### ❌ Unbounded Message Accumulation

```rust
// ❌ Bad: Queue can grow unbounded
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

// ✅ Good: Bounded channel for backpressure
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
```

---

## Summary

| Area | Recommendation |
|------|----------------|
| Close | Always use `close()` for graceful shutdown |
| Batching | Use `send_batch()` for multiple messages |
| Security | Configure `allowed_origins` in production |
| Limits | Use `Limits::embedded()` for constrained environments |
| Errors | Treat `ProtocolViolation` as fatal |
| Pong | Handled automatically by `recv()` |
