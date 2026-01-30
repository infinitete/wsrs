//! # rsws - Production-grade WebSocket Protocol Implementation
//!
//! `rsws` is a high-performance, RFC 6455 compliant WebSocket protocol library for Rust.
//!
//! ## Features
//!
//! - **Zero-copy frame parsing** for optimal performance
//! - **Async-first design** with runtime-agnostic core
//! - **Full RFC 6455 compliance** with strict validation
//! - **TLS/HTTP integration** for wss:// support
//! - **Extension framework** with permessage-deflate compression
//! - **Production-ready** security and resource limits
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use rsws::{Connection, Config, Role};
//!
//! // Client connection
//! let config = Config::client();
//! let conn = Connection::new(stream, Role::Client, config).await?;
//! ```

pub mod config;
pub mod connection;
pub mod error;
pub mod extensions;
pub mod message;
pub mod protocol;

#[cfg(feature = "async-tokio")]
pub mod codec;

pub use config::{Config, Limits};
#[cfg(feature = "async-tokio")]
pub use connection::Connection;
pub use connection::{ConnectionState, Role};
pub use error::{Error, Result};
pub use message::{CloseCode, CloseFrame, Message};
pub use protocol::{compute_accept_key, HandshakeRequest, HandshakeResponse, OpCode, WS_GUID};

#[cfg(feature = "async-tokio")]
pub use codec::WebSocketCodec;

#[cfg(feature = "tls-rustls")]
pub mod tls;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn test_public_types_are_send() {
        assert_send::<Error>();
        assert_send::<Config>();
        assert_send::<Limits>();
        assert_send::<Message>();
        assert_send::<CloseCode>();
        assert_send::<CloseFrame>();
        assert_send::<ConnectionState>();
        assert_send::<Role>();
    }

    #[test]
    fn test_public_types_are_sync() {
        assert_sync::<Error>();
        assert_sync::<Config>();
        assert_sync::<Limits>();
        assert_sync::<Message>();
        assert_sync::<CloseCode>();
        assert_sync::<CloseFrame>();
        assert_sync::<ConnectionState>();
        assert_sync::<Role>();
    }
}
