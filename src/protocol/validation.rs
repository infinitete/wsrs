//! Frame validation for security hardening (RFC 6455).
//!
//! This module provides validation logic to enforce WebSocket security requirements:
//! - Masking rules per RFC 6455 Section 5.1
//! - RSV bits validation
//! - Frame size limits

use crate::config::Limits;
use crate::connection::Role;
use crate::error::{Error, Result};

/// Frame validator for incoming WebSocket frames.
///
/// Enforces RFC 6455 security requirements based on connection role.
#[derive(Debug, Clone)]
pub struct FrameValidator {
    /// Connection role (Client or Server).
    role: Role,
    /// Size limits for frames.
    limits: Limits,
    /// Whether to accept unmasked frames (server-side, non-compliant).
    accept_unmasked_frames: bool,
}

impl FrameValidator {
    /// Create a new frame validator.
    ///
    /// # Arguments
    ///
    /// * `role` - The connection role (Client or Server)
    /// * `limits` - Frame size limits
    pub fn new(role: Role, limits: Limits) -> Self {
        Self {
            role,
            limits,
            accept_unmasked_frames: false,
        }
    }

    /// Create a validator that accepts unmasked frames (non-RFC compliant).
    ///
    /// This is useful for testing but should not be used in production.
    pub fn with_accept_unmasked(mut self, accept: bool) -> Self {
        self.accept_unmasked_frames = accept;
        self
    }

    /// Validate an incoming frame.
    ///
    /// # Arguments
    ///
    /// * `masked` - Whether the frame was masked
    /// * `rsv1` - Reserved bit 1
    /// * `rsv2` - Reserved bit 2
    /// * `rsv3` - Reserved bit 3
    /// * `payload_len` - Length of the payload in bytes
    ///
    /// # Errors
    ///
    /// - `Error::UnmaskedClientFrame` - Server received unmasked frame from client
    /// - `Error::MaskedServerFrame` - Client received masked frame from server
    /// - `Error::ReservedBitsSet` - RSV bits set without negotiated extension
    /// - `Error::FrameTooLarge` - Frame exceeds size limit
    pub fn validate_incoming(
        &self,
        masked: bool,
        rsv1: bool,
        rsv2: bool,
        rsv3: bool,
        payload_len: usize,
    ) -> Result<()> {
        // Validate masking based on role (RFC 6455 Section 5.1)
        self.validate_masking(masked)?;

        // Validate RSV bits (RFC 6455 Section 5.2)
        self.validate_rsv_bits(rsv1, rsv2, rsv3)?;

        // Validate frame size
        self.validate_frame_size(payload_len)?;

        Ok(())
    }

    /// Validate masking rules per RFC 6455 Section 5.1.
    ///
    /// - Server MUST reject unmasked client frames
    /// - Client MUST reject masked server frames
    fn validate_masking(&self, masked: bool) -> Result<()> {
        match self.role {
            Role::Server => {
                // Server expects masked frames from clients
                if !masked && !self.accept_unmasked_frames {
                    return Err(Error::UnmaskedClientFrame);
                }
            }
            Role::Client => {
                // Client expects unmasked frames from servers
                if masked {
                    return Err(Error::MaskedServerFrame);
                }
            }
        }
        Ok(())
    }

    /// Validate RSV bits per RFC 6455 Section 5.2.
    ///
    /// RSV bits MUST be 0 unless an extension is negotiated that defines
    /// meanings for non-zero values.
    fn validate_rsv_bits(&self, rsv1: bool, rsv2: bool, rsv3: bool) -> Result<()> {
        if rsv1 || rsv2 || rsv3 {
            return Err(Error::ReservedBitsSet);
        }
        Ok(())
    }

    /// Validate frame size against configured limits.
    fn validate_frame_size(&self, payload_len: usize) -> Result<()> {
        self.limits.check_frame_size(payload_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // TDD: Tests written first for security validation
    // ==========================================================================

    // --------------------------------------------------------------------------
    // Masking validation tests (RFC 6455 Section 5.1)
    // --------------------------------------------------------------------------

    #[test]
    fn test_server_rejects_unmasked_client_frame() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            false, // unmasked
            false, false, false, 10,
        );

        assert!(matches!(result, Err(Error::UnmaskedClientFrame)));
    }

    #[test]
    fn test_server_accepts_masked_client_frame() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, // masked
            false, false, false, 10,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_client_rejects_masked_server_frame() {
        let validator = FrameValidator::new(Role::Client, Limits::default());

        let result = validator.validate_incoming(
            true, // masked (invalid for server->client)
            false, false, false, 10,
        );

        assert!(matches!(result, Err(Error::MaskedServerFrame)));
    }

    #[test]
    fn test_client_accepts_unmasked_server_frame() {
        let validator = FrameValidator::new(Role::Client, Limits::default());

        let result = validator.validate_incoming(
            false, // unmasked
            false, false, false, 10,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_server_accepts_unmasked_when_configured() {
        let validator =
            FrameValidator::new(Role::Server, Limits::default()).with_accept_unmasked(true);

        let result = validator.validate_incoming(
            false, // unmasked
            false, false, false, 10,
        );

        assert!(result.is_ok());
    }

    // --------------------------------------------------------------------------
    // RSV bits validation tests (RFC 6455 Section 5.2)
    // --------------------------------------------------------------------------

    #[test]
    fn test_rejects_rsv1_set() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, true, // RSV1 set
            false, false, 10,
        );

        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    #[test]
    fn test_rejects_rsv2_set() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, false, true, // RSV2 set
            false, 10,
        );

        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    #[test]
    fn test_rejects_rsv3_set() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, false, false, true, // RSV3 set
            10,
        );

        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    #[test]
    fn test_rejects_all_rsv_bits_set() {
        let validator = FrameValidator::new(Role::Client, Limits::default());

        let result = validator.validate_incoming(
            false, true, true, true, // All RSV bits set
            10,
        );

        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    #[test]
    fn test_accepts_no_rsv_bits_set() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, false, false, false, // No RSV bits set
            10,
        );

        assert!(result.is_ok());
    }

    // --------------------------------------------------------------------------
    // Frame size validation tests
    // --------------------------------------------------------------------------

    #[test]
    fn test_rejects_frame_exceeding_limit() {
        let limits = Limits::new(1024, 4096, 10); // 1KB max frame
        let validator = FrameValidator::new(Role::Server, limits);

        let result = validator.validate_incoming(
            true, false, false, false, 2048, // 2KB payload
        );

        assert!(matches!(
            result,
            Err(Error::FrameTooLarge {
                size: 2048,
                max: 1024
            })
        ));
    }

    #[test]
    fn test_accepts_frame_within_limit() {
        let limits = Limits::new(1024, 4096, 10);
        let validator = FrameValidator::new(Role::Server, limits);

        let result = validator.validate_incoming(
            true, false, false, false, 512, // 512 bytes (within 1KB limit)
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_accepts_frame_at_exact_limit() {
        let limits = Limits::new(1024, 4096, 10);
        let validator = FrameValidator::new(Role::Server, limits);

        let result = validator.validate_incoming(
            true, false, false, false, 1024, // Exactly at limit
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_embedded_limits_reject_large_frames() {
        let validator = FrameValidator::new(Role::Server, Limits::embedded());

        // Embedded limits: 64KB max frame
        let result = validator.validate_incoming(
            true,
            false,
            false,
            false,
            100 * 1024, // 100KB
        );

        assert!(matches!(result, Err(Error::FrameTooLarge { .. })));
    }

    // --------------------------------------------------------------------------
    // Combined validation tests
    // --------------------------------------------------------------------------

    #[test]
    fn test_masking_checked_before_rsv() {
        // Ensure masking is checked first (fail-fast on security violation)
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            false, // unmasked (should fail)
            true,  // RSV1 set (would also fail)
            false, false, 10,
        );

        // Should fail on masking, not RSV
        assert!(matches!(result, Err(Error::UnmaskedClientFrame)));
    }

    #[test]
    fn test_rsv_checked_before_size() {
        let limits = Limits::new(100, 1000, 10);
        let validator = FrameValidator::new(Role::Server, limits);

        let result = validator.validate_incoming(
            true, true, // RSV1 set (should fail)
            false, false, 200, // Over limit (would also fail)
        );

        // Should fail on RSV, not size
        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    #[test]
    fn test_valid_server_frame_passes_all_checks() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, // masked
            false, false, false, // no RSV bits
            1000,  // reasonable size
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_client_frame_passes_all_checks() {
        let validator = FrameValidator::new(Role::Client, Limits::default());

        let result = validator.validate_incoming(
            false, // unmasked
            false, false, false, // no RSV bits
            1000,  // reasonable size
        );

        assert!(result.is_ok());
    }

    // --------------------------------------------------------------------------
    // Edge cases
    // --------------------------------------------------------------------------

    #[test]
    fn test_zero_payload_size() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        let result = validator.validate_incoming(
            true, false, false, false, 0, // Empty payload
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_large_payload_with_default_limits() {
        let validator = FrameValidator::new(Role::Server, Limits::default());

        // Default limit is 16MB
        let result = validator.validate_incoming(
            true,
            false,
            false,
            false,
            10 * 1024 * 1024, // 10MB (within limit)
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_clone() {
        let validator = FrameValidator::new(Role::Server, Limits::default());
        let cloned = validator.clone();

        // Both should behave the same
        let result1 = validator.validate_incoming(true, false, false, false, 10);
        let result2 = cloned.validate_incoming(true, false, false, false, 10);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}
