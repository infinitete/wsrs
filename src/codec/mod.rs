//! WebSocket codec for async I/O.
//!
//! This module provides frame-level encoding/decoding over async streams.

#[cfg(feature = "async-tokio")]
mod framed;

#[cfg(feature = "async-tokio")]
pub use framed::WebSocketCodec;
