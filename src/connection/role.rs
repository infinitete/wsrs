//! WebSocket connection role (client or server).

/// WebSocket connection role.
///
/// Determines masking behavior per RFC 6455.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Client role - must mask outgoing frames.
    Client,
    /// Server role - must not mask outgoing frames.
    Server,
}

impl Role {
    /// Check if this role must mask outgoing frames.
    ///
    /// Clients must mask all frames sent to servers.
    #[inline]
    #[must_use]
    pub const fn must_mask(&self) -> bool {
        matches!(self, Role::Client)
    }

    /// Check if this role expects incoming frames to be masked.
    ///
    /// Servers expect masked frames from clients.
    #[inline]
    #[must_use]
    pub const fn expects_masked(&self) -> bool {
        matches!(self, Role::Server)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Client => write!(f, "Client"),
            Role::Server => write!(f, "Server"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_must_mask() {
        assert!(Role::Client.must_mask());
    }

    #[test]
    fn test_server_must_not_mask() {
        assert!(!Role::Server.must_mask());
    }

    #[test]
    fn test_server_expects_masked() {
        assert!(Role::Server.expects_masked());
    }

    #[test]
    fn test_client_expects_unmasked() {
        assert!(!Role::Client.expects_masked());
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Client.to_string(), "Client");
        assert_eq!(Role::Server.to_string(), "Server");
    }

    #[test]
    fn test_role_clone_and_copy() {
        let role = Role::Client;
        let cloned = role.clone();
        let copied = role;
        assert_eq!(role, cloned);
        assert_eq!(role, copied);
    }
}
