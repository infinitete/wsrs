//! WebSocket connection state machine as defined in RFC 6455.

/// WebSocket connection state.
///
/// Represents the lifecycle states of a WebSocket connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ConnectionState {
    /// Connection is being established (handshake in progress).
    #[default]
    Connecting,
    /// Connection is open and ready for data transfer.
    Open,
    /// Close handshake initiated, waiting for peer's close frame.
    Closing,
    /// Connection is fully closed.
    Closed,
}

impl ConnectionState {
    /// Check if the connection is in an active state.
    ///
    /// Returns `true` for `Connecting`, `Open`, or `Closing` states.
    #[must_use]
    #[inline]
    pub const fn is_active(&self) -> bool {
        !matches!(self, ConnectionState::Closed)
    }

    /// Check if sending data is allowed in this state.
    ///
    /// Returns `true` only for `Open` state.
    #[must_use]
    #[inline]
    pub const fn can_send(&self) -> bool {
        matches!(self, ConnectionState::Open)
    }

    /// Check if receiving data is allowed in this state.
    ///
    /// Returns `true` for `Open` or `Closing` states.
    #[must_use]
    #[inline]
    pub const fn can_receive(&self) -> bool {
        matches!(self, ConnectionState::Open | ConnectionState::Closing)
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Connecting => write!(f, "Connecting"),
            ConnectionState::Open => write!(f, "Open"),
            ConnectionState::Closing => write!(f, "Closing"),
            ConnectionState::Closed => write!(f, "Closed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = ConnectionState::default();
        assert_eq!(state, ConnectionState::Connecting);
    }

    #[test]
    fn test_state_transitions() {
        assert_eq!(ConnectionState::Connecting, ConnectionState::Connecting);
        assert_eq!(ConnectionState::Open, ConnectionState::Open);
        assert_eq!(ConnectionState::Closing, ConnectionState::Closing);
        assert_eq!(ConnectionState::Closed, ConnectionState::Closed);
    }

    #[test]
    fn test_can_send_in_each_state() {
        assert!(!ConnectionState::Connecting.can_send());
        assert!(ConnectionState::Open.can_send());
        assert!(!ConnectionState::Closing.can_send());
        assert!(!ConnectionState::Closed.can_send());
    }

    #[test]
    fn test_can_receive_in_each_state() {
        assert!(!ConnectionState::Connecting.can_receive());
        assert!(ConnectionState::Open.can_receive());
        assert!(ConnectionState::Closing.can_receive());
        assert!(!ConnectionState::Closed.can_receive());
    }

    #[test]
    fn test_is_active() {
        assert!(ConnectionState::Connecting.is_active());
        assert!(ConnectionState::Open.is_active());
        assert!(ConnectionState::Closing.is_active());
        assert!(!ConnectionState::Closed.is_active());
    }

    #[test]
    fn test_state_display() {
        assert_eq!(ConnectionState::Connecting.to_string(), "Connecting");
        assert_eq!(ConnectionState::Open.to_string(), "Open");
        assert_eq!(ConnectionState::Closing.to_string(), "Closing");
        assert_eq!(ConnectionState::Closed.to_string(), "Closed");
    }

    #[test]
    fn test_state_clone_and_copy() {
        let state = ConnectionState::Open;
        let cloned = state.clone();
        let copied = state;
        assert_eq!(state, cloned);
        assert_eq!(state, copied);
    }
}
