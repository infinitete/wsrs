# AGENTS.md - rsws WebSocket Library

Production-grade, RFC 6455 compliant WebSocket protocol library for Rust.

## Quick Reference

```bash
# Build
cargo build                          # Build with default features (async-tokio)
cargo build --all-features           # Build with all features
cargo build --no-default-features    # Build core only (no async)

# Test
cargo test                           # Run all tests
cargo test --lib                     # Library tests only
cargo test frame                     # Run tests matching "frame"
cargo test test_parse_masked         # Run single test by name
cargo test -p rsws -- --nocapture    # Show println output

# Lint & Format
cargo fmt                            # Format code
cargo fmt -- --check                 # Check formatting (CI)
cargo clippy --all-features -- -D warnings  # Lint (strict)

# Benchmarks
cargo bench                          # Run all benchmarks
cargo bench --bench benchmarks       # Run specific benchmark suite

# Examples
cargo run --example echo_server --features tls-rustls
cargo run --example client
```

## Project Structure

```
rsws/
├── src/
│   ├── lib.rs              # Public API exports
│   ├── error.rs            # Error types (thiserror-based)
│   ├── config.rs           # Config, Limits structs
│   ├── message.rs          # Message, CloseCode, CloseFrame
│   ├── codec/              # Async frame encoding/decoding
│   │   ├── mod.rs
│   │   └── framed.rs       # WebSocketCodec implementation
│   ├── connection/         # High-level Connection API
│   │   ├── mod.rs
│   │   ├── connection.rs   # Connection<T> implementation
│   │   ├── state.rs        # ConnectionState enum
│   │   └── role.rs         # Role (Client/Server)
│   ├── protocol/           # Low-level WebSocket protocol
│   │   ├── mod.rs
│   │   ├── frame.rs        # Frame parsing/serialization
│   │   ├── opcode.rs       # OpCode enum
│   │   ├── mask.rs         # XOR masking operations
│   │   ├── handshake.rs    # HTTP upgrade handshake
│   │   ├── assembler.rs    # Message fragment assembly
│   │   ├── validation.rs   # Protocol validation
│   │   └── utf8.rs         # UTF-8 validation
│   ├── extensions/         # WebSocket extensions
│   │   ├── mod.rs
│   │   └── deflate.rs      # permessage-deflate (feature-gated)
│   └── tls/                # TLS support (feature-gated)
│       ├── mod.rs
│       ├── rustls_impl.rs  # rustls backend
│       └── native.rs       # native-tls backend
├── tests/                  # Integration tests
├── benches/                # Criterion benchmarks
├── examples/               # Usage examples
└── autobahn/               # Autobahn test suite config
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `async-tokio` | Async I/O with Tokio runtime | Yes |
| `tls-rustls` | TLS via rustls (pure Rust) | No |
| `tls-native` | TLS via native-tls (platform) | No |
| `compression` | permessage-deflate extension | No |

## Code Style Guidelines

### Imports

```rust
// Order: std -> external crates -> crate-local
use std::io::Result;

use tokio::io::{AsyncRead, AsyncWrite};
use thiserror::Error;

use crate::error::{Error, Result};
use crate::protocol::Frame;
```

### Error Handling

- **Library code**: Use `thiserror` with structured error enums
- **Error variants**: Include context fields (size, max, etc.)
- **Result alias**: Define `pub type Result<T> = std::result::Result<T, Error>;`

```rust
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    #[error("Frame too large: {size} bytes (max: {max})")]
    FrameTooLarge { size: usize, max: usize },
}
```

### Type Annotations

- Use `#[must_use]` on constructors and methods returning new values
- Use `#[inline]` on small, hot-path methods
- Derive `Debug, Clone, PartialEq, Eq` where applicable
- Use `#[non_exhaustive]` on public enums

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Types | PascalCase | `CloseFrame`, `OpCode` |
| Functions | snake_case | `compute_accept_key` |
| Constants | SCREAMING_SNAKE | `MAX_CONTROL_FRAME_PAYLOAD` |
| Feature flags | kebab-case | `tls-rustls`, `async-tokio` |

### Documentation

- All public items must have doc comments
- Use `# Errors` section for fallible functions
- Use `# Panics` section if function can panic
- Include `# Example` with `rust,ignore` for async code

```rust
/// Parse a frame from a buffer.
///
/// # Errors
///
/// - `Error::IncompleteFrame` if not enough data
/// - `Error::InvalidOpcode` if opcode is invalid
pub fn parse(buf: &[u8]) -> Result<(Self, usize)>
```

### Testing

- Unit tests go in `#[cfg(test)] mod tests` within each file
- Use descriptive test names: `test_parse_masked_text_frame`
- Property tests use `proptest` crate (in `tests/property.rs`)
- Integration tests in `tests/` directory

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unmasked_text_frame() {
        let data = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(frame.payload(), b"Hello");
    }

    #[tokio::test]
    async fn test_send_message() {
        // Async tests require tokio::test attribute
    }
}
```

## Architecture Patterns

### Zero-Copy Design

- Use `&[u8]` slices for parsing when possible
- Avoid intermediate allocations in hot paths
- `Frame::parse()` returns borrowed data where feasible

### Async-First

- Core types are `Send + Sync`
- Connection requires `AsyncRead + AsyncWrite + Unpin`
- Feature-gate async code with `#[cfg(feature = "async-tokio")]`

### Builder Pattern

```rust
let config = Config::new()
    .with_limits(Limits::embedded())
    .with_fragment_size(4096);
```

## Common Patterns in This Codebase

### Frame Validation (Two-Phase)

```rust
let (frame, consumed) = Frame::parse(buf)?;  // Parse structure
frame.validate()?;                            // Validate RFC compliance
```

### Connection State Machine

States: `Open` -> `Closing` -> `Closed`
- Check `state.can_send()` before sending
- Check `state.can_receive()` before receiving

### Config Presets

```rust
Config::server()  // No masking, reject unmasked frames
Config::client()  // Mask all outgoing frames
```

## Anti-Patterns to Avoid

- **No `as any` / type suppression**: Fix the actual type issue
- **No `unwrap()` in library code**: Use `?` or proper error handling
- **No blocking in async**: Use `tokio::task::spawn_blocking` if needed
- **No `unsafe`**: Unless absolutely necessary with justification

## Dependencies

Core: `thiserror`, `sha1`, `base64`
Async: `tokio`, `bytes`, `futures-core`
TLS: `tokio-rustls` / `native-tls`
Dev: `proptest`, `criterion`, `rcgen`

## Performance Notes

- Frame parsing: <50ns per frame (minimal allocations)
- Masking: >2 GB/s throughput (XOR operations)
- Use `Limits::embedded()` for constrained environments
- Use `Limits::unrestricted()` only in trusted environments
