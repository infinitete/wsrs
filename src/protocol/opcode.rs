//! WebSocket frame opcodes as defined in RFC 6455.

use crate::error::{Error, Result};

/// WebSocket frame opcode.
///
/// Defines the interpretation of the payload data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum OpCode {
    /// Continuation frame (0x0).
    ///
    /// Used for fragmented messages after the initial frame.
    Continuation = 0x0,

    /// Text frame (0x1).
    ///
    /// Payload must be valid UTF-8.
    Text = 0x1,

    /// Binary frame (0x2).
    ///
    /// Payload is arbitrary binary data.
    Binary = 0x2,

    /// Close frame (0x8).
    ///
    /// Initiates connection close. May contain status code and reason.
    Close = 0x8,

    /// Ping frame (0x9).
    ///
    /// Used for keepalive. Receiver must respond with Pong.
    Ping = 0x9,

    /// Pong frame (0xA).
    ///
    /// Response to Ping. May be sent unsolicited as unidirectional heartbeat.
    Pong = 0xA,
}

impl OpCode {
    /// Create OpCode from raw byte value.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidOpcode` if the value is not a valid opcode.
    /// Returns `Error::ReservedOpcode` if the value is reserved for future use.
    pub fn from_u8(byte: u8) -> Result<Self> {
        match byte {
            0x0 => Ok(OpCode::Continuation),
            0x1 => Ok(OpCode::Text),
            0x2 => Ok(OpCode::Binary),
            0x3..=0x7 => Err(Error::ReservedOpcode(byte)),
            0x8 => Ok(OpCode::Close),
            0x9 => Ok(OpCode::Ping),
            0xA => Ok(OpCode::Pong),
            0xB..=0xF => Err(Error::ReservedOpcode(byte)),
            _ => Err(Error::InvalidOpcode(byte)),
        }
    }

    /// Convert OpCode to raw byte value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Check if this is a control frame opcode.
    ///
    /// Control frames: Close (0x8), Ping (0x9), Pong (0xA).
    #[inline]
    #[must_use]
    pub const fn is_control(self) -> bool {
        matches!(self, OpCode::Close | OpCode::Ping | OpCode::Pong)
    }

    /// Check if this is a data frame opcode.
    ///
    /// Data frames: Continuation (0x0), Text (0x1), Binary (0x2).
    #[inline]
    #[must_use]
    pub const fn is_data(self) -> bool {
        matches!(self, OpCode::Continuation | OpCode::Text | OpCode::Binary)
    }

    /// Get human-readable name for this opcode.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            OpCode::Continuation => "Continuation",
            OpCode::Text => "Text",
            OpCode::Binary => "Binary",
            OpCode::Close => "Close",
            OpCode::Ping => "Ping",
            OpCode::Pong => "Pong",
        }
    }
}

impl std::fmt::Display for OpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_from_u8_valid() {
        assert_eq!(OpCode::from_u8(0x0).unwrap(), OpCode::Continuation);
        assert_eq!(OpCode::from_u8(0x1).unwrap(), OpCode::Text);
        assert_eq!(OpCode::from_u8(0x2).unwrap(), OpCode::Binary);
        assert_eq!(OpCode::from_u8(0x8).unwrap(), OpCode::Close);
        assert_eq!(OpCode::from_u8(0x9).unwrap(), OpCode::Ping);
        assert_eq!(OpCode::from_u8(0xA).unwrap(), OpCode::Pong);
    }

    #[test]
    fn test_opcode_from_u8_reserved() {
        for reserved in [0x3, 0x4, 0x5, 0x6, 0x7, 0xB, 0xC, 0xD, 0xE, 0xF] {
            assert!(matches!(
                OpCode::from_u8(reserved),
                Err(Error::ReservedOpcode(_))
            ));
        }
    }

    #[test]
    fn test_opcode_as_u8() {
        assert_eq!(OpCode::Text.as_u8(), 0x1);
        assert_eq!(OpCode::Binary.as_u8(), 0x2);
        assert_eq!(OpCode::Close.as_u8(), 0x8);
    }

    #[test]
    fn test_opcode_is_control() {
        assert!(!OpCode::Continuation.is_control());
        assert!(!OpCode::Text.is_control());
        assert!(!OpCode::Binary.is_control());
        assert!(OpCode::Close.is_control());
        assert!(OpCode::Ping.is_control());
        assert!(OpCode::Pong.is_control());
    }

    #[test]
    fn test_opcode_is_data() {
        assert!(OpCode::Continuation.is_data());
        assert!(OpCode::Text.is_data());
        assert!(OpCode::Binary.is_data());
        assert!(!OpCode::Close.is_data());
        assert!(!OpCode::Ping.is_data());
        assert!(!OpCode::Pong.is_data());
    }

    #[test]
    fn test_opcode_display() {
        assert_eq!(OpCode::Text.to_string(), "Text");
        assert_eq!(OpCode::Close.to_string(), "Close");
    }

    #[test]
    fn test_opcode_debug() {
        assert_eq!(format!("{:?}", OpCode::Ping), "Ping");
    }
}
