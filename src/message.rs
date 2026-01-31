//! WebSocket message types and close codes as defined in RFC 6455.

/// WebSocket close status code per RFC 6455 Section 7.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum CloseCode {
    /// Normal closure (1000). The connection successfully completed.
    #[default]
    Normal,
    /// Going away (1001). Endpoint is going away (e.g., server shutdown, browser navigating away).
    GoingAway,
    /// Protocol error (1002). Endpoint received a malformed frame or protocol violation.
    ProtocolError,
    /// Unsupported data (1003). Endpoint received data type it cannot handle.
    UnsupportedData,
    /// Invalid payload (1007). Endpoint received a message with invalid data (e.g., non-UTF-8 in text).
    InvalidPayload,
    /// Policy violation (1008). Endpoint received a message that violates its policy.
    PolicyViolation,
    /// Message too big (1009). Endpoint received a message too large to process.
    MessageTooBig,
    /// Mandatory extension (1010). Client expected server to negotiate an extension.
    MandatoryExtension,
    /// Internal error (1011). Server encountered an unexpected condition.
    InternalError,
    /// Custom close code (3000-4999 for applications, 1012-1014 for registered codes).
    Other(u16),
}

impl CloseCode {
    /// Create a `CloseCode` from its numeric value.
    #[must_use]
    pub const fn from_u16(code: u16) -> Self {
        match code {
            1000 => CloseCode::Normal,
            1001 => CloseCode::GoingAway,
            1002 => CloseCode::ProtocolError,
            1003 => CloseCode::UnsupportedData,
            1007 => CloseCode::InvalidPayload,
            1008 => CloseCode::PolicyViolation,
            1009 => CloseCode::MessageTooBig,
            1010 => CloseCode::MandatoryExtension,
            1011 => CloseCode::InternalError,
            other => CloseCode::Other(other),
        }
    }

    /// Get the numeric value of this close code.
    #[must_use]
    pub const fn as_u16(&self) -> u16 {
        match self {
            CloseCode::Normal => 1000,
            CloseCode::GoingAway => 1001,
            CloseCode::ProtocolError => 1002,
            CloseCode::UnsupportedData => 1003,
            CloseCode::InvalidPayload => 1007,
            CloseCode::PolicyViolation => 1008,
            CloseCode::MessageTooBig => 1009,
            CloseCode::MandatoryExtension => 1010,
            CloseCode::InternalError => 1011,
            CloseCode::Other(code) => *code,
        }
    }

    /// Check if this close code is valid for sending per RFC 6455 Section 7.4.1.
    ///
    /// Valid codes:
    /// - 1000-1003: Normal, GoingAway, ProtocolError, UnsupportedData
    /// - 1007-1011: InvalidPayload, PolicyViolation, MessageTooBig, MandatoryExtension, InternalError
    /// - 1012-1014: ServiceRestart, TryAgainLater, BadGateway (RFC 6455 registered)
    /// - 3000-4999: Reserved for libraries/frameworks and applications
    ///
    /// Invalid/Reserved codes (MUST NOT be sent):
    /// - 1004-1006, 1015: Reserved, cannot be set in Close frame
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        let code = self.as_u16();
        matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999)
    }

    /// Check if this close code is reserved and MUST NOT be sent in a Close frame.
    ///
    /// Reserved codes per RFC 6455 Section 7.4.1:
    /// - 1004: Reserved
    /// - 1005: No Status Received (MUST NOT be set by endpoint)
    /// - 1006: Abnormal Closure (MUST NOT be set by endpoint)
    /// - 1015: TLS Handshake (MUST NOT be set by endpoint)
    #[must_use]
    pub const fn is_reserved(&self) -> bool {
        let code = self.as_u16();
        matches!(code, 1004..=1006 | 1015)
    }
}

/// Close frame containing status code and optional reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    /// The close status code.
    pub code: CloseCode,
    /// Human-readable reason for closing (UTF-8, max 123 bytes).
    pub reason: String,
}

impl CloseFrame {
    /// Create a new close frame with the given code and reason.
    #[must_use]
    pub fn new(code: CloseCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}

/// WebSocket message types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message {
    /// A text message (UTF-8 encoded).
    Text(String),
    /// A binary message (arbitrary bytes).
    Binary(Vec<u8>),
    /// A ping frame (control frame, payload <= 125 bytes).
    Ping(Vec<u8>),
    /// A pong frame (control frame, payload <= 125 bytes).
    Pong(Vec<u8>),
    /// A close frame (control frame, may include status code and reason).
    Close(Option<CloseFrame>),
}

impl Message {
    /// Create a text message.
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Message::Text(s.into())
    }

    /// Create a binary message.
    #[must_use]
    pub fn binary(data: impl Into<Vec<u8>>) -> Self {
        Message::Binary(data.into())
    }

    /// Create a ping message.
    #[must_use]
    pub fn ping(data: impl Into<Vec<u8>>) -> Self {
        Message::Ping(data.into())
    }

    /// Create a pong message.
    #[must_use]
    pub fn pong(data: impl Into<Vec<u8>>) -> Self {
        Message::Pong(data.into())
    }

    /// Create a close message with status code and reason.
    #[must_use]
    pub fn close(code: CloseCode, reason: impl Into<String>) -> Self {
        Message::Close(Some(CloseFrame::new(code, reason)))
    }

    /// Returns `true` if this is a text message.
    #[must_use]
    pub const fn is_text(&self) -> bool {
        matches!(self, Message::Text(_))
    }

    /// Returns `true` if this is a binary message.
    #[must_use]
    pub const fn is_binary(&self) -> bool {
        matches!(self, Message::Binary(_))
    }

    /// Returns `true` if this is a data message (text or binary).
    #[must_use]
    pub const fn is_data(&self) -> bool {
        matches!(self, Message::Text(_) | Message::Binary(_))
    }

    /// Returns `true` if this is a control message (ping, pong, or close).
    #[must_use]
    pub const fn is_control(&self) -> bool {
        matches!(
            self,
            Message::Ping(_) | Message::Pong(_) | Message::Close(_)
        )
    }

    /// Consume and return the text content, if this is a text message.
    #[must_use]
    pub fn into_text(self) -> Option<String> {
        match self {
            Message::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Consume and return the binary content, if this is a binary message.
    #[must_use]
    pub fn into_binary(self) -> Option<Vec<u8>> {
        match self {
            Message::Binary(data) => Some(data),
            _ => None,
        }
    }

    /// Borrow the text content, if this is a text message.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Message::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Borrow the binary content, if this is a binary message.
    #[must_use]
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            Message::Binary(data) => Some(data),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_text_creation() {
        let msg = Message::text("hello");
        assert!(matches!(msg, Message::Text(s) if s == "hello"));

        let msg = Message::text(String::from("world"));
        assert!(matches!(msg, Message::Text(s) if s == "world"));
    }

    #[test]
    fn test_message_binary_creation() {
        let msg = Message::binary(vec![1, 2, 3]);
        assert!(matches!(msg, Message::Binary(ref d) if d == &[1, 2, 3]));

        let msg = Message::binary([4, 5, 6]);
        assert!(matches!(msg, Message::Binary(ref d) if d == &[4, 5, 6]));
    }

    #[test]
    fn test_message_ping_pong() {
        let ping = Message::ping(vec![1, 2, 3]);
        assert!(matches!(ping, Message::Ping(ref d) if d == &[1, 2, 3]));

        let pong = Message::pong(vec![1, 2, 3]);
        assert!(matches!(pong, Message::Pong(ref d) if d == &[1, 2, 3]));
    }

    #[test]
    fn test_message_close_with_code() {
        let msg = Message::close(CloseCode::Normal, "goodbye");
        match msg {
            Message::Close(Some(frame)) => {
                assert_eq!(frame.code, CloseCode::Normal);
                assert_eq!(frame.reason, "goodbye");
            }
            _ => panic!("Expected Close message with frame"),
        }
    }

    #[test]
    fn test_message_close_without_code() {
        let msg = Message::Close(None);
        assert!(matches!(msg, Message::Close(None)));
    }

    #[test]
    fn test_message_is_data() {
        assert!(Message::text("hello").is_data());
        assert!(Message::binary(vec![1]).is_data());
        assert!(!Message::ping(vec![]).is_data());
        assert!(!Message::pong(vec![]).is_data());
        assert!(!Message::Close(None).is_data());
    }

    #[test]
    fn test_message_is_control() {
        assert!(!Message::text("hello").is_control());
        assert!(!Message::binary(vec![1]).is_control());
        assert!(Message::ping(vec![]).is_control());
        assert!(Message::pong(vec![]).is_control());
        assert!(Message::Close(None).is_control());
    }

    #[test]
    fn test_message_into_text() {
        let msg = Message::text("hello");
        assert_eq!(msg.into_text(), Some(String::from("hello")));

        let msg = Message::binary(vec![1]);
        assert_eq!(msg.into_text(), None);
    }

    #[test]
    fn test_message_into_binary() {
        let msg = Message::binary(vec![1, 2, 3]);
        assert_eq!(msg.into_binary(), Some(vec![1, 2, 3]));

        let msg = Message::text("hello");
        assert_eq!(msg.into_binary(), None);
    }

    #[test]
    fn test_message_as_text() {
        let msg = Message::text("hello");
        assert_eq!(msg.as_text(), Some("hello"));

        let msg = Message::binary(vec![1]);
        assert_eq!(msg.as_text(), None);
    }

    #[test]
    fn test_message_as_binary() {
        let msg = Message::binary(vec![1, 2, 3]);
        assert_eq!(msg.as_binary(), Some([1, 2, 3].as_slice()));

        let msg = Message::text("hello");
        assert_eq!(msg.as_binary(), None);
    }

    #[test]
    fn test_close_code_from_u16() {
        assert_eq!(CloseCode::from_u16(1000), CloseCode::Normal);
        assert_eq!(CloseCode::from_u16(1001), CloseCode::GoingAway);
        assert_eq!(CloseCode::from_u16(1002), CloseCode::ProtocolError);
        assert_eq!(CloseCode::from_u16(1003), CloseCode::UnsupportedData);
        assert_eq!(CloseCode::from_u16(1007), CloseCode::InvalidPayload);
        assert_eq!(CloseCode::from_u16(1008), CloseCode::PolicyViolation);
        assert_eq!(CloseCode::from_u16(1009), CloseCode::MessageTooBig);
        assert_eq!(CloseCode::from_u16(1010), CloseCode::MandatoryExtension);
        assert_eq!(CloseCode::from_u16(1011), CloseCode::InternalError);
        assert_eq!(CloseCode::from_u16(3000), CloseCode::Other(3000));
        assert_eq!(CloseCode::from_u16(4999), CloseCode::Other(4999));
    }

    #[test]
    fn test_close_code_as_u16() {
        assert_eq!(CloseCode::Normal.as_u16(), 1000);
        assert_eq!(CloseCode::GoingAway.as_u16(), 1001);
        assert_eq!(CloseCode::ProtocolError.as_u16(), 1002);
        assert_eq!(CloseCode::Other(3500).as_u16(), 3500);
    }

    #[test]
    fn test_close_code_validity() {
        assert!(CloseCode::Normal.is_valid());
        assert!(CloseCode::GoingAway.is_valid());
        assert!(CloseCode::ProtocolError.is_valid());
        assert!(CloseCode::UnsupportedData.is_valid());
        assert!(CloseCode::InvalidPayload.is_valid());
        assert!(CloseCode::PolicyViolation.is_valid());
        assert!(CloseCode::MessageTooBig.is_valid());
        assert!(CloseCode::MandatoryExtension.is_valid());
        assert!(CloseCode::InternalError.is_valid());

        // RFC 6455 registered codes 1012-1014
        assert!(CloseCode::Other(1012).is_valid()); // Service Restart
        assert!(CloseCode::Other(1013).is_valid()); // Try Again Later
        assert!(CloseCode::Other(1014).is_valid()); // Bad Gateway

        assert!(CloseCode::Other(3000).is_valid());
        assert!(CloseCode::Other(4999).is_valid());

        assert!(!CloseCode::Other(0).is_valid());
        assert!(!CloseCode::Other(999).is_valid());
        assert!(!CloseCode::Other(1004).is_valid());
        assert!(!CloseCode::Other(1005).is_valid());
        assert!(!CloseCode::Other(1006).is_valid());
        assert!(!CloseCode::Other(1015).is_valid()); // TLS Handshake - reserved
        assert!(!CloseCode::Other(2999).is_valid());
        assert!(!CloseCode::Other(5000).is_valid());
    }

    #[test]
    fn test_close_code_reserved() {
        assert!(CloseCode::Other(1004).is_reserved());
        assert!(CloseCode::Other(1005).is_reserved());
        assert!(CloseCode::Other(1006).is_reserved());
        assert!(CloseCode::Other(1015).is_reserved());

        assert!(!CloseCode::Normal.is_reserved());
        assert!(!CloseCode::Other(1012).is_reserved());
        assert!(!CloseCode::Other(3000).is_reserved());
    }

    #[test]
    fn test_message_is_text() {
        assert!(Message::text("hello").is_text());
        assert!(!Message::binary(vec![1]).is_text());
        assert!(!Message::ping(vec![]).is_text());
    }

    #[test]
    fn test_message_is_binary() {
        assert!(Message::binary(vec![1]).is_binary());
        assert!(!Message::text("hello").is_binary());
        assert!(!Message::pong(vec![]).is_binary());
    }
}
