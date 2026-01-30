//! Error types for the WebSocket protocol implementation.
//!
//! This module defines all error conditions that can occur during WebSocket
//! operations, following RFC 6455 requirements.

use thiserror::Error;

/// Result type alias for WebSocket operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during WebSocket operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Invalid frame structure or header.
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    /// Protocol violation detected.
    #[error("Protocol violation: {0}")]
    ProtocolViolation(String),

    /// Invalid UTF-8 in text frame.
    #[error("Invalid UTF-8 in text frame")]
    InvalidUtf8,

    /// Frame size exceeds configured maximum.
    #[error("Frame too large: {size} bytes (max: {max})")]
    FrameTooLarge {
        /// Actual frame size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Message size exceeds configured maximum.
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge {
        /// Actual message size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Too many fragments in a single message.
    #[error("Too many fragments: {count} (max: {max})")]
    TooManyFragments {
        /// Actual fragment count.
        count: usize,
        /// Maximum allowed fragments.
        max: usize,
    },

    /// Connection has been closed.
    #[error("Connection closed: {0:?}")]
    ConnectionClosed(Option<u16>),

    /// Invalid WebSocket handshake.
    #[error("Invalid handshake: {0}")]
    InvalidHandshake(String),

    /// I/O error occurred.
    #[error("I/O error: {0}")]
    Io(String),

    /// Extension-related error.
    #[error("Extension error: {0}")]
    Extension(String),

    /// Invalid close code.
    #[error("Invalid close code: {0}")]
    InvalidCloseCode(u16),

    /// Reserved opcode used.
    #[error("Reserved opcode: {0:#x}")]
    ReservedOpcode(u8),

    /// Control frame fragmented (RFC violation).
    #[error("Control frames cannot be fragmented")]
    FragmentedControlFrame,

    /// Control frame payload too large (>125 bytes).
    #[error("Control frame payload too large: {0} bytes (max: 125)")]
    ControlFrameTooLarge(usize),

    /// Unmasked client frame (security violation).
    #[error("Client frame must be masked")]
    UnmaskedClientFrame,

    /// Masked server frame (security violation).
    #[error("Server frame must not be masked")]
    MaskedServerFrame,

    /// Reserved bits set without extension.
    #[error("Reserved bits set without negotiated extension")]
    ReservedBitsSet,

    /// Incomplete frame data.
    #[error("Incomplete frame: need {needed} more bytes")]
    IncompleteFrame {
        /// Number of additional bytes needed.
        needed: usize,
    },

    /// Invalid opcode value.
    #[error("Invalid opcode: {0:#x}")]
    InvalidOpcode(u8),

    /// Invalid extension configuration or negotiation.
    #[error("Invalid extension: {0}")]
    InvalidExtension(String),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err.to_string())
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_: std::str::Utf8Error) -> Self {
        Error::InvalidUtf8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::FrameTooLarge {
            size: 20_000_000,
            max: 16_000_000,
        };
        assert_eq!(
            err.to_string(),
            "Frame too large: 20000000 bytes (max: 16000000)"
        );
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let ws_err: Error = io_err.into();
        assert!(matches!(ws_err, Error::Io(_)));
    }

    #[test]
    fn test_error_clone() {
        let err = Error::InvalidUtf8;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }
}
