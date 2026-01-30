//! UTF-8 validation for WebSocket text frames (RFC 6455).
//!
//! This module provides incremental UTF-8 validation for fragmented messages,
//! handling partial multi-byte sequences across fragment boundaries.

use crate::error::{Error, Result};

/// Incremental UTF-8 validator for fragmented WebSocket messages.
///
/// Handles validation across fragment boundaries, saving incomplete
/// multi-byte sequences for continuation in the next fragment.
#[derive(Debug, Clone)]
pub struct Utf8Validator {
    /// Buffer for incomplete multi-byte sequences.
    incomplete: [u8; 4],
    /// Number of bytes in the incomplete buffer.
    incomplete_len: usize,
}

impl Default for Utf8Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Utf8Validator {
    /// Create a new UTF-8 validator.
    pub fn new() -> Self {
        Self {
            incomplete: [0; 4],
            incomplete_len: 0,
        }
    }

    /// Validate a fragment of UTF-8 data.
    ///
    /// For non-final fragments (`is_final = false`), incomplete multi-byte
    /// sequences at the end are saved for the next fragment.
    ///
    /// For final fragments (`is_final = true`), all bytes must form complete
    /// valid UTF-8 sequences.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidUtf8` if the data contains invalid UTF-8 sequences.
    pub fn validate(&mut self, data: &[u8], is_final: bool) -> Result<()> {
        // Prepend any incomplete bytes from previous fragment
        let check_data = if self.incomplete_len > 0 {
            let mut combined = Vec::with_capacity(self.incomplete_len + data.len());
            combined.extend_from_slice(&self.incomplete[..self.incomplete_len]);
            combined.extend_from_slice(data);
            combined
        } else {
            data.to_vec()
        };

        // Reset incomplete buffer
        self.incomplete_len = 0;

        if check_data.is_empty() {
            return Ok(());
        }

        match std::str::from_utf8(&check_data) {
            Ok(_) => Ok(()),
            Err(e) => {
                let valid_up_to = e.valid_up_to();

                // Check if this might be an incomplete sequence at the end
                if !is_final {
                    // error_len() returns None for incomplete sequences
                    if e.error_len().is_none() {
                        // This is an incomplete sequence at the end
                        let remaining = &check_data[valid_up_to..];
                        if remaining.len() <= 4 {
                            self.incomplete[..remaining.len()].copy_from_slice(remaining);
                            self.incomplete_len = remaining.len();
                            return Ok(());
                        }
                    }
                }

                // Invalid UTF-8 sequence
                Err(Error::InvalidUtf8)
            }
        }
    }

    /// Reset the validator state, discarding any incomplete sequences.
    pub fn reset(&mut self) {
        self.incomplete_len = 0;
    }

    /// Check if there are pending incomplete bytes.
    pub fn has_incomplete(&self) -> bool {
        self.incomplete_len > 0
    }
}

/// Validate that a byte slice is valid UTF-8.
///
/// This is a convenience function for validating complete (non-fragmented) data.
///
/// # Errors
///
/// Returns `Error::InvalidUtf8` if the data is not valid UTF-8.
pub fn validate_utf8(data: &[u8]) -> Result<()> {
    std::str::from_utf8(data)
        .map(|_| ())
        .map_err(|_| Error::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // UTF-8 Validation Tests (TDD - Tests written first)
    // ==========================================================================

    // --------------------------------------------------------------------------
    // Test 1: Valid UTF-8 string
    // --------------------------------------------------------------------------
    #[test]
    fn test_valid_utf8() {
        let mut validator = Utf8Validator::new();

        // ASCII
        assert!(validator.validate(b"Hello, World!", true).is_ok());

        // Multi-byte characters
        validator.reset();
        assert!(validator.validate("こんにちは".as_bytes(), true).is_ok());

        // Mixed
        validator.reset();
        assert!(validator.validate("Hello 世界 🌍".as_bytes(), true).is_ok());

        // Convenience function
        assert!(validate_utf8(b"Valid UTF-8").is_ok());
        assert!(validate_utf8("émoji 🎉".as_bytes()).is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 2: Invalid UTF-8 string
    // --------------------------------------------------------------------------
    #[test]
    fn test_invalid_utf8() {
        let mut validator = Utf8Validator::new();

        // Invalid continuation byte
        assert!(validator.validate(&[0x80], true).is_err());

        // Overlong encoding
        validator.reset();
        assert!(validator.validate(&[0xc0, 0x80], true).is_err());

        // Invalid start byte
        validator.reset();
        assert!(validator.validate(&[0xff], true).is_err());

        // Truncated sequence in the middle
        validator.reset();
        assert!(validator.validate(&[0xe0, 0x80], true).is_err());

        // Convenience function
        assert!(validate_utf8(&[0x80, 0x81]).is_err());
    }

    // --------------------------------------------------------------------------
    // Test 3: Incomplete sequence at end of non-final fragment (should succeed)
    // --------------------------------------------------------------------------
    #[test]
    fn test_incomplete_sequence_non_final() {
        let mut validator = Utf8Validator::new();

        // First byte of a 3-byte sequence (e.g., Euro sign €)
        // € = E2 82 AC
        let incomplete = &[0xe2];
        assert!(validator.validate(incomplete, false).is_ok());
        assert!(validator.has_incomplete());

        // Complete the sequence in next fragment
        assert!(validator.validate(&[0x82, 0xac], true).is_ok());
        assert!(!validator.has_incomplete());
    }

    // --------------------------------------------------------------------------
    // Test 4: Incomplete sequence at end of final fragment (should fail)
    // --------------------------------------------------------------------------
    #[test]
    fn test_incomplete_sequence_final_fails() {
        let mut validator = Utf8Validator::new();

        // First byte of a 3-byte sequence, marked as final
        let incomplete = &[0xe2];
        assert!(validator.validate(incomplete, true).is_err());
    }

    // --------------------------------------------------------------------------
    // Test 5: Multi-byte character split across fragments
    // --------------------------------------------------------------------------
    #[test]
    fn test_multibyte_split_across_fragments() {
        let mut validator = Utf8Validator::new();

        // 4-byte character: 🎉 = F0 9F 8E 89
        // Split: F0 9F | 8E 89

        // First fragment with partial character
        assert!(validator.validate(&[0xf0, 0x9f], false).is_ok());
        assert!(validator.has_incomplete());

        // Complete in second fragment
        assert!(validator.validate(&[0x8e, 0x89], true).is_ok());
        assert!(!validator.has_incomplete());

        // Test another split pattern: F0 | 9F 8E 89
        validator.reset();
        assert!(validator.validate(&[0xf0], false).is_ok());
        assert!(validator.validate(&[0x9f, 0x8e, 0x89], true).is_ok());

        // Test 3-way split: F0 | 9F | 8E 89
        validator.reset();
        assert!(validator.validate(&[0xf0], false).is_ok());
        assert!(validator.validate(&[0x9f], false).is_ok());
        assert!(validator.validate(&[0x8e, 0x89], true).is_ok());

        // Test with valid ASCII before split
        validator.reset();
        let mut data = b"Hello ".to_vec();
        data.push(0xf0); // First byte of emoji
        assert!(validator.validate(&data, false).is_ok());
        assert!(validator.validate(&[0x9f, 0x8e, 0x89], true).is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 6: Empty fragment
    // --------------------------------------------------------------------------
    #[test]
    fn test_empty_fragment() {
        let mut validator = Utf8Validator::new();

        // Empty non-final
        assert!(validator.validate(&[], false).is_ok());
        assert!(!validator.has_incomplete());

        // Empty final
        assert!(validator.validate(&[], true).is_ok());

        // Empty after incomplete
        validator.reset();
        assert!(validator.validate(&[0xe2], false).is_ok());
        assert!(validator.validate(&[], false).is_ok()); // Should preserve incomplete
        assert!(validator.has_incomplete());
        assert!(validator.validate(&[0x82, 0xac], true).is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 7: Validator reset
    // --------------------------------------------------------------------------
    #[test]
    fn test_validator_reset() {
        let mut validator = Utf8Validator::new();

        // Start with incomplete sequence
        assert!(validator.validate(&[0xe2], false).is_ok());
        assert!(validator.has_incomplete());

        // Reset discards incomplete bytes
        validator.reset();
        assert!(!validator.has_incomplete());

        // Should work with fresh data now
        assert!(validator.validate(b"Fresh start", true).is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 8: Default implementation
    // --------------------------------------------------------------------------
    #[test]
    fn test_default() {
        let validator = Utf8Validator::default();
        assert!(!validator.has_incomplete());
    }

    // --------------------------------------------------------------------------
    // Test 9: Complex multi-fragment scenario
    // --------------------------------------------------------------------------
    #[test]
    fn test_complex_multi_fragment() {
        let mut validator = Utf8Validator::new();

        // "Hello 世界" split awkwardly
        // 世 = E4 B8 96
        // 界 = E7 95 8C

        // "Hello " + first byte of 世
        let mut frag1 = b"Hello ".to_vec();
        frag1.push(0xe4);
        assert!(validator.validate(&frag1, false).is_ok());

        // Rest of 世 + part of 界
        assert!(validator.validate(&[0xb8, 0x96, 0xe7, 0x95], false).is_ok());

        // Complete 界
        assert!(validator.validate(&[0x8c], true).is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 10: Invalid sequence in middle of fragment
    // --------------------------------------------------------------------------
    #[test]
    fn test_invalid_in_middle() {
        let mut validator = Utf8Validator::new();

        // Valid start, invalid middle, valid end
        let data = &[0x48, 0x65, 0x80, 0x6c, 0x6f]; // "He" + invalid + "lo"
        assert!(validator.validate(data, false).is_err());
    }
}
