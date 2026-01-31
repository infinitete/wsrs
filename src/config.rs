//! Configuration and limits for WebSocket connections.

use std::time::Duration;

/// Configuration limits for WebSocket connections.
///
/// These limits prevent resource exhaustion attacks and ensure
/// bounded memory usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Maximum size of a single frame in bytes.
    ///
    /// Default: 16 MB (16 * 1024 * 1024)
    pub max_frame_size: usize,

    /// Maximum size of a complete message in bytes.
    ///
    /// This applies to the total size after reassembling all fragments.
    ///
    /// Default: 64 MB (64 * 1024 * 1024)
    pub max_message_size: usize,

    /// Maximum number of fragments in a single message.
    ///
    /// Default: 128
    pub max_fragment_count: usize,

    /// Maximum size of handshake data in bytes.
    ///
    /// Default: 8 KB (8192)
    pub max_handshake_size: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_size: 16 * 1024 * 1024,   // 16 MB
            max_message_size: 64 * 1024 * 1024, // 64 MB
            max_fragment_count: 128,
            max_handshake_size: 8192,
        }
    }
}

impl Limits {
    /// Create new limits with custom values.
    #[must_use]
    pub const fn new(
        max_frame_size: usize,
        max_message_size: usize,
        max_fragment_count: usize,
        max_handshake_size: usize,
    ) -> Self {
        Self {
            max_frame_size,
            max_message_size,
            max_fragment_count,
            max_handshake_size,
        }
    }

    /// Create limits suitable for small embedded systems.
    ///
    /// - Max frame: 64 KB
    /// - Max message: 256 KB
    /// - Max fragments: 16
    /// - Max handshake: 4 KB
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            max_frame_size: 64 * 1024,
            max_message_size: 256 * 1024,
            max_fragment_count: 16,
            max_handshake_size: 4096,
        }
    }

    /// Create limits for unrestricted use.
    ///
    /// Warning: Use only in trusted environments.
    ///
    /// - Max frame: 1 GB (on 64-bit) or `usize::MAX` (on 32-bit)
    /// - Max message: 4 GB (on 64-bit) or `usize::MAX` (on 32-bit)
    /// - Max fragments: 1024
    /// - Max handshake: 64 KB
    ///
    /// Note: On 32-bit platforms, the limits are capped at `usize::MAX` to
    /// prevent integer overflow.
    #[cfg(target_pointer_width = "64")]
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            max_frame_size: 1024 * 1024 * 1024,       // 1 GB
            max_message_size: 4 * 1024 * 1024 * 1024, // 4 GB
            max_fragment_count: 1024,
            max_handshake_size: 64 * 1024,
        }
    }

    /// Create limits for unrestricted use (32-bit platforms).
    ///
    /// Warning: Use only in trusted environments.
    ///
    /// On 32-bit platforms, limits are set to `usize::MAX` to avoid overflow.
    #[cfg(target_pointer_width = "32")]
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            max_frame_size: usize::MAX,
            max_message_size: usize::MAX,
            max_fragment_count: 1024,
            max_handshake_size: 64 * 1024,
        }
    }

    /// Validate that message size is within limits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MessageTooLarge`](crate::Error::MessageTooLarge) if `size` exceeds the configured maximum.
    pub const fn check_message_size(&self, size: usize) -> Result<(), crate::Error> {
        if size > self.max_message_size {
            Err(crate::Error::MessageTooLarge {
                size,
                max: self.max_message_size,
            })
        } else {
            Ok(())
        }
    }

    /// Validate that frame size is within limits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FrameTooLarge`](crate::Error::FrameTooLarge) if `size` exceeds the configured maximum.
    pub const fn check_frame_size(&self, size: usize) -> Result<(), crate::Error> {
        if size > self.max_frame_size {
            Err(crate::Error::FrameTooLarge {
                size,
                max: self.max_frame_size,
            })
        } else {
            Ok(())
        }
    }

    /// Validate that fragment count is within limits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooManyFragments`](crate::Error::TooManyFragments) if `count` exceeds the configured maximum.
    pub const fn check_fragment_count(&self, count: usize) -> Result<(), crate::Error> {
        if count > self.max_fragment_count {
            Err(crate::Error::TooManyFragments {
                count,
                max: self.max_fragment_count,
            })
        } else {
            Ok(())
        }
    }

    /// Validate that handshake size is within limits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::HandshakeTooLarge`](crate::Error::HandshakeTooLarge) if `size` exceeds the configured maximum.
    pub const fn check_handshake_size(&self, size: usize) -> Result<(), crate::Error> {
        if size > self.max_handshake_size {
            Err(crate::Error::HandshakeTooLarge {
                size,
                max: self.max_handshake_size,
            })
        } else {
            Ok(())
        }
    }
}

/// Timeout configuration for WebSocket connections.
///
/// These timeouts help prevent DoS attacks and resource exhaustion.
/// Note: Enforcement is the caller's responsibility; this is configuration only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeouts {
    /// Handshake timeout.
    ///
    /// Maximum time to complete the WebSocket handshake.
    /// Default: 30 seconds
    pub handshake: Duration,

    /// Read timeout.
    ///
    /// Maximum time to wait for incoming data.
    /// Default: 60 seconds
    pub read: Duration,

    /// Write timeout.
    ///
    /// Maximum time to wait for outgoing data to be sent.
    /// Default: 60 seconds
    pub write: Duration,

    /// Idle timeout.
    ///
    /// Maximum time a connection can remain idle without activity.
    /// Default: 300 seconds (5 minutes)
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            handshake: Duration::from_secs(30),
            read: Duration::from_secs(60),
            write: Duration::from_secs(60),
            idle: Duration::from_secs(300),
        }
    }
}

impl Timeouts {
    /// Create new timeouts with custom values.
    #[must_use]
    pub const fn new(handshake: Duration, read: Duration, write: Duration, idle: Duration) -> Self {
        Self {
            handshake,
            read,
            write,
            idle,
        }
    }
}

/// WebSocket connection configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Resource limits.
    pub limits: Limits,

    /// Fragment size for outgoing messages (in bytes).
    ///
    /// Messages larger than this will be split into multiple frames.
    ///
    /// Default: 16 KB (16 * 1024)
    pub fragment_size: usize,

    /// Accept unmasked frames from clients (server only).
    ///
    /// RFC 6455 requires clients to mask all frames. Setting this to `true`
    /// violates the spec but may be useful for testing.
    ///
    /// Default: false
    pub accept_unmasked_frames: bool,

    /// Mask frames when sending (client only).
    ///
    /// RFC 6455 requires clients to mask all frames. This should always be `true`
    /// for clients and `false` for servers.
    ///
    /// Default: true
    pub mask_frames: bool,

    /// Read buffer size (in bytes).
    ///
    /// Default: 8 KB (8192)
    pub read_buffer_size: usize,

    /// Write buffer size (in bytes).
    ///
    /// Default: 8 KB (8192)
    pub write_buffer_size: usize,

    /// Timeout configuration.
    ///
    /// If `None`, no timeouts are configured (caller must implement their own).
    /// Default: None
    pub timeouts: Option<Timeouts>,

    /// Allowed origins for CSWSH protection.
    ///
    /// If `Some`, only connections from these origins are allowed.
    /// If `None`, origin validation is disabled (not recommended for production).
    /// Default: None
    pub allowed_origins: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            fragment_size: 16 * 1024,
            accept_unmasked_frames: false,
            mask_frames: true,
            read_buffer_size: 8192,
            write_buffer_size: 8192,
            timeouts: None,
            allowed_origins: None,
        }
    }
}

impl Config {
    /// Create a new configuration with default limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set custom limits.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Set fragment size for outgoing messages.
    #[must_use]
    pub const fn with_fragment_size(mut self, size: usize) -> Self {
        self.fragment_size = size;
        self
    }

    /// Set read buffer size.
    #[must_use]
    pub const fn with_read_buffer_size(mut self, size: usize) -> Self {
        self.read_buffer_size = size;
        self
    }

    /// Set write buffer size.
    #[must_use]
    pub const fn with_write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }

    /// Set timeout configuration.
    #[must_use]
    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = Some(timeouts);
        self
    }

    /// Set allowed origins for CSWSH protection.
    ///
    /// Only connections with an Origin header matching one of these values
    /// will be accepted. Pass an empty vector to require an Origin header
    /// but accept any value.
    #[must_use]
    pub fn with_allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.allowed_origins = Some(origins);
        self
    }

    /// Configure for server role (no masking, reject unmasked client frames).
    #[must_use]
    pub fn server() -> Self {
        Self {
            mask_frames: false,
            accept_unmasked_frames: false,
            ..Default::default()
        }
    }

    /// Configure for client role (mask all frames).
    #[must_use]
    pub fn client() -> Self {
        Self {
            mask_frames: true,
            accept_unmasked_frames: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_default() {
        let limits = Limits::default();
        assert_eq!(limits.max_frame_size, 16 * 1024 * 1024);
        assert_eq!(limits.max_message_size, 64 * 1024 * 1024);
        assert_eq!(limits.max_fragment_count, 128);
        assert_eq!(limits.max_handshake_size, 8192);
    }

    #[test]
    fn test_limits_embedded() {
        let limits = Limits::embedded();
        assert_eq!(limits.max_frame_size, 64 * 1024);
        assert_eq!(limits.max_message_size, 256 * 1024);
        assert_eq!(limits.max_fragment_count, 16);
        assert_eq!(limits.max_handshake_size, 4096);
    }

    #[test]
    fn test_limits_check_handshake_size() {
        let limits = Limits::default();
        assert!(limits.check_handshake_size(1024).is_ok());
        assert!(limits.check_handshake_size(10000).is_err());
    }

    #[test]
    fn test_limits_check_message_size() {
        let limits = Limits::default();
        assert!(limits.check_message_size(1024).is_ok());
        assert!(limits.check_message_size(100 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_limits_check_frame_size() {
        let limits = Limits::default();
        assert!(limits.check_frame_size(1024).is_ok());
        assert!(limits.check_frame_size(20 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_limits_check_fragment_count() {
        let limits = Limits::default();
        assert!(limits.check_fragment_count(50).is_ok());
        assert!(limits.check_fragment_count(200).is_err());
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.fragment_size, 16 * 1024);
        assert!(!config.accept_unmasked_frames);
        assert!(config.mask_frames);
    }

    #[test]
    fn test_config_server() {
        let config = Config::server();
        assert!(!config.mask_frames);
        assert!(!config.accept_unmasked_frames);
    }

    #[test]
    fn test_config_client() {
        let config = Config::client();
        assert!(config.mask_frames);
        assert!(!config.accept_unmasked_frames);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new()
            .with_limits(Limits::embedded())
            .with_fragment_size(4096);

        assert_eq!(config.fragment_size, 4096);
        assert_eq!(config.limits.max_frame_size, 64 * 1024);
    }

    #[test]
    fn test_config_buffer_size() {
        let config = Config::new()
            .with_read_buffer_size(1024)
            .with_write_buffer_size(2048);

        assert_eq!(config.read_buffer_size, 1024);
        assert_eq!(config.write_buffer_size, 2048);
    }

    #[test]
    fn test_timeouts_default() {
        let timeouts = Timeouts::default();
        assert_eq!(timeouts.handshake, Duration::from_secs(30));
        assert_eq!(timeouts.read, Duration::from_secs(60));
        assert_eq!(timeouts.write, Duration::from_secs(60));
        assert_eq!(timeouts.idle, Duration::from_secs(300));
    }

    #[test]
    fn test_config_with_timeouts() {
        let timeouts = Timeouts::default();
        let config = Config::new().with_timeouts(timeouts.clone());
        assert_eq!(config.timeouts, Some(timeouts));
    }

    #[test]
    fn test_config_with_allowed_origins() {
        let origins = vec!["https://example.com".to_string()];
        let config = Config::new().with_allowed_origins(origins.clone());
        assert_eq!(config.allowed_origins, Some(origins));
    }

    #[test]
    fn test_config_allowed_origins_none_by_default() {
        let config = Config::default();
        assert!(config.allowed_origins.is_none());
    }

    #[test]
    fn test_config_timeouts_none_by_default() {
        let config = Config::default();
        assert!(config.timeouts.is_none());
    }
}
