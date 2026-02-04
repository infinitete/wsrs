//! SIMD-accelerated UTF-8 validation for WebSocket text frames.
//!
//! This module provides high-performance UTF-8 validation using NEON SIMD
//! instructions on aarch64, with a scalar fallback for other platforms.
//!
//! The implementation uses the "range-check" algorithm which validates UTF-8
//! by checking that each byte falls within valid ranges for its position in
//! a multi-byte sequence.

use crate::error::{Error, Result};

// ============================================================================
// ARM64 NEON SIMD implementation
// ============================================================================

#[cfg(target_arch = "aarch64")]
mod aarch64_simd {
    use std::arch::aarch64::*;

    /// Check if all bytes in a 16-byte vector are ASCII (< 0x80).
    ///
    /// # Safety
    /// Requires NEON support on the target platform.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn is_ascii_chunk(chunk: uint8x16_t) -> bool {
        // Shift right by 7 - if any byte >= 0x80, this will produce non-zero
        // SAFETY: NEON intrinsic, valid for uint8x16_t input
        let high_bits = vshrq_n_u8::<7>(chunk);
        // Get maximum value across all lanes
        // SAFETY: NEON intrinsic, valid for uint8x16_t input
        vmaxvq_u8(high_bits) == 0
    }

    /// NEON-accelerated UTF-8 validation.
    ///
    /// Uses a simple, correct approach:
    /// 1. Fast path: SIMD check if entire buffer is ASCII
    /// 2. Slow path: Fall back to std::str::from_utf8 for non-ASCII
    ///
    /// This approach is:
    /// - Correct: Uses battle-tested std::str::from_utf8 for non-ASCII
    /// - Fast for ASCII: SIMD check is very fast, and most WebSocket traffic is ASCII
    /// - Simple: No complex state machine to get wrong
    ///
    /// # Safety
    /// Caller must ensure NEON is available on the current CPU.
    #[target_feature(enable = "neon")]
    pub unsafe fn validate_utf8_neon(data: &[u8]) -> bool {
        let len = data.len();
        if len == 0 {
            return true;
        }

        let ptr = data.as_ptr();
        let chunks = len / 16;
        let mut all_ascii = true;

        for i in 0..chunks {
            // SAFETY: chunks = len / 16, so i * 16 + 16 <= len
            // ptr.add(i * 16) points to valid memory for 16 bytes
            let chunk = unsafe { vld1q_u8(ptr.add(i * 16)) };
            if unsafe { !is_ascii_chunk(chunk) } {
                all_ascii = false;
                break;
            }
        }

        if all_ascii {
            let tail_start = chunks * 16;
            for i in tail_start..len {
                // SAFETY: i < len, so data.get_unchecked(i) is valid
                if unsafe { *data.get_unchecked(i) } >= 0x80 {
                    all_ascii = false;
                    break;
                }
            }
        }

        if all_ascii {
            return true;
        }

        std::str::from_utf8(data).is_ok()
    }
}

// ============================================================================
// Scalar fallback implementation
// ============================================================================

/// Scalar UTF-8 validation using standard library.
#[inline]
fn validate_utf8_scalar(data: &[u8]) -> bool {
    std::str::from_utf8(data).is_ok()
}

// ============================================================================
// Public API with runtime CPU feature detection
// ============================================================================

/// SIMD-accelerated UTF-8 validation with runtime CPU feature detection.
///
/// This function automatically selects the best available implementation:
/// - NEON (128-bit, 16 bytes/iteration) on ARM64
/// - Scalar fallback on unsupported platforms
///
/// # Errors
///
/// Returns `Error::InvalidUtf8` if the data is not valid UTF-8.
///
/// # Example
///
/// ```
/// use rsws::protocol::utf8_simd::validate_utf8_simd;
///
/// assert!(validate_utf8_simd(b"Hello, World!").is_ok());
/// assert!(validate_utf8_simd("こんにちは".as_bytes()).is_ok());
/// assert!(validate_utf8_simd(&[0x80, 0x81]).is_err());
/// ```
#[inline]
pub fn validate_utf8_simd(data: &[u8]) -> Result<()> {
    let is_valid = {
        #[cfg(target_arch = "aarch64")]
        {
            // SAFETY: is_aarch64_feature_detected! is a safe macro that checks
            // CPU features at runtime. We only call the unsafe SIMD function
            // if the corresponding feature is detected.
            if std::arch::is_aarch64_feature_detected!("neon") {
                // SAFETY: NEON feature is confirmed available by the runtime
                // check above. validate_utf8_neon requires NEON, which we just
                // verified is present.
                unsafe { aarch64_simd::validate_utf8_neon(data) }
            } else {
                validate_utf8_scalar(data)
            }
        }

        #[cfg(not(target_arch = "aarch64"))]
        {
            validate_utf8_scalar(data)
        }
    };

    if is_valid {
        Ok(())
    } else {
        Err(Error::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Valid ASCII Tests
    // ========================================================================

    #[test]
    fn test_valid_ascii_empty() {
        assert!(validate_utf8_simd(b"").is_ok());
    }

    #[test]
    fn test_valid_ascii_single_byte() {
        assert!(validate_utf8_simd(b"a").is_ok());
        assert!(validate_utf8_simd(b"Z").is_ok());
        assert!(validate_utf8_simd(b"0").is_ok());
        assert!(validate_utf8_simd(b" ").is_ok());
    }

    #[test]
    fn test_valid_ascii_short() {
        assert!(validate_utf8_simd(b"Hello").is_ok());
        assert!(validate_utf8_simd(b"Hello, World!").is_ok());
    }

    #[test]
    fn test_valid_ascii_exactly_16_bytes() {
        assert!(validate_utf8_simd(b"0123456789ABCDEF").is_ok());
    }

    #[test]
    fn test_valid_ascii_exactly_32_bytes() {
        assert!(validate_utf8_simd(b"0123456789ABCDEF0123456789ABCDEF").is_ok());
    }

    #[test]
    fn test_valid_ascii_long() {
        let long_ascii = "The quick brown fox jumps over the lazy dog. ".repeat(10);
        assert!(validate_utf8_simd(long_ascii.as_bytes()).is_ok());
    }

    #[test]
    fn test_valid_ascii_all_printable() {
        // All printable ASCII characters
        let printable: Vec<u8> = (0x20..=0x7E).collect();
        assert!(validate_utf8_simd(&printable).is_ok());
    }

    #[test]
    fn test_valid_ascii_control_chars() {
        // Control characters (valid UTF-8)
        let control: Vec<u8> = (0x00..=0x1F).collect();
        assert!(validate_utf8_simd(&control).is_ok());
    }

    // ========================================================================
    // Valid Multi-byte UTF-8 Tests
    // ========================================================================

    #[test]
    fn test_valid_2byte_sequences() {
        // 2-byte sequences: U+0080 to U+07FF
        // Latin Extended: é (U+00E9) = C3 A9
        assert!(validate_utf8_simd("é".as_bytes()).is_ok());
        // Greek: Ω (U+03A9) = CE A9
        assert!(validate_utf8_simd("Ω".as_bytes()).is_ok());
        // Cyrillic: Я (U+042F) = D0 AF
        assert!(validate_utf8_simd("Я".as_bytes()).is_ok());
    }

    #[test]
    fn test_valid_3byte_sequences() {
        // 3-byte sequences: U+0800 to U+FFFF
        // Euro sign: € (U+20AC) = E2 82 AC
        assert!(validate_utf8_simd("€".as_bytes()).is_ok());
        // Japanese hiragana: あ (U+3042) = E3 81 82
        assert!(validate_utf8_simd("あ".as_bytes()).is_ok());
        // Chinese: 中 (U+4E2D) = E4 B8 AD
        assert!(validate_utf8_simd("中".as_bytes()).is_ok());
    }

    #[test]
    fn test_valid_4byte_sequences() {
        // 4-byte sequences: U+10000 to U+10FFFF
        // Emoji: 🎉 (U+1F389) = F0 9F 8E 89
        assert!(validate_utf8_simd("🎉".as_bytes()).is_ok());
        // Emoji: 😀 (U+1F600) = F0 9F 98 80
        assert!(validate_utf8_simd("😀".as_bytes()).is_ok());
        // Musical symbol: 𝄞 (U+1D11E) = F0 9D 84 9E
        assert!(validate_utf8_simd("𝄞".as_bytes()).is_ok());
    }

    #[test]
    fn test_valid_mixed_utf8() {
        assert!(validate_utf8_simd("Hello 世界".as_bytes()).is_ok());
        assert!(validate_utf8_simd("こんにちは World".as_bytes()).is_ok());
        assert!(validate_utf8_simd("Emoji: 🎉🌍🚀".as_bytes()).is_ok());
        assert!(validate_utf8_simd("Mixed: aéあ🎉".as_bytes()).is_ok());
    }

    #[test]
    fn test_valid_utf8_boundary_codepoints() {
        // Test boundary codepoints for each byte length
        // U+007F (max 1-byte) = 7F
        assert!(validate_utf8_simd(&[0x7F]).is_ok());
        // U+0080 (min 2-byte) = C2 80
        assert!(validate_utf8_simd(&[0xC2, 0x80]).is_ok());
        // U+07FF (max 2-byte) = DF BF
        assert!(validate_utf8_simd(&[0xDF, 0xBF]).is_ok());
        // U+0800 (min 3-byte) = E0 A0 80
        assert!(validate_utf8_simd(&[0xE0, 0xA0, 0x80]).is_ok());
        // U+FFFF (max 3-byte) = EF BF BF
        assert!(validate_utf8_simd(&[0xEF, 0xBF, 0xBF]).is_ok());
        // U+10000 (min 4-byte) = F0 90 80 80
        assert!(validate_utf8_simd(&[0xF0, 0x90, 0x80, 0x80]).is_ok());
        // U+10FFFF (max 4-byte, max Unicode) = F4 8F BF BF
        assert!(validate_utf8_simd(&[0xF4, 0x8F, 0xBF, 0xBF]).is_ok());
    }

    // ========================================================================
    // Invalid UTF-8 Tests
    // ========================================================================

    #[test]
    fn test_invalid_continuation_byte_alone() {
        // Continuation bytes (0x80-0xBF) without lead byte
        assert!(validate_utf8_simd(&[0x80]).is_err());
        assert!(validate_utf8_simd(&[0x8F]).is_err());
        assert!(validate_utf8_simd(&[0xBF]).is_err());
    }

    #[test]
    fn test_invalid_lead_without_continuation() {
        // 2-byte lead without continuation
        assert!(validate_utf8_simd(&[0xC2]).is_err());
        assert!(validate_utf8_simd(&[0xDF]).is_err());
        // 3-byte lead without enough continuations
        assert!(validate_utf8_simd(&[0xE0]).is_err());
        assert!(validate_utf8_simd(&[0xE0, 0xA0]).is_err());
        // 4-byte lead without enough continuations
        assert!(validate_utf8_simd(&[0xF0]).is_err());
        assert!(validate_utf8_simd(&[0xF0, 0x90]).is_err());
        assert!(validate_utf8_simd(&[0xF0, 0x90, 0x80]).is_err());
    }

    #[test]
    fn test_invalid_overlong_2byte() {
        // Overlong encodings: using more bytes than necessary
        // U+0000 encoded as 2 bytes: C0 80 (should be 00)
        assert!(validate_utf8_simd(&[0xC0, 0x80]).is_err());
        // U+007F encoded as 2 bytes: C1 BF (should be 7F)
        assert!(validate_utf8_simd(&[0xC1, 0xBF]).is_err());
    }

    #[test]
    fn test_invalid_overlong_3byte() {
        // U+0000 encoded as 3 bytes: E0 80 80
        assert!(validate_utf8_simd(&[0xE0, 0x80, 0x80]).is_err());
        // U+007F encoded as 3 bytes: E0 81 BF
        assert!(validate_utf8_simd(&[0xE0, 0x81, 0xBF]).is_err());
        // U+07FF encoded as 3 bytes: E0 9F BF (should be DF BF)
        assert!(validate_utf8_simd(&[0xE0, 0x9F, 0xBF]).is_err());
    }

    #[test]
    fn test_invalid_overlong_4byte() {
        // U+0000 encoded as 4 bytes: F0 80 80 80
        assert!(validate_utf8_simd(&[0xF0, 0x80, 0x80, 0x80]).is_err());
        // U+FFFF encoded as 4 bytes: F0 8F BF BF (should be EF BF BF)
        assert!(validate_utf8_simd(&[0xF0, 0x8F, 0xBF, 0xBF]).is_err());
    }

    #[test]
    fn test_invalid_surrogates() {
        // UTF-16 surrogates are not valid UTF-8
        // U+D800 (high surrogate): ED A0 80
        assert!(validate_utf8_simd(&[0xED, 0xA0, 0x80]).is_err());
        // U+DFFF (low surrogate): ED BF BF
        assert!(validate_utf8_simd(&[0xED, 0xBF, 0xBF]).is_err());
        // U+D834 (high surrogate): ED A0 B4
        assert!(validate_utf8_simd(&[0xED, 0xA0, 0xB4]).is_err());
    }

    #[test]
    fn test_invalid_beyond_unicode() {
        // Codepoints beyond U+10FFFF
        // U+110000: F4 90 80 80
        assert!(validate_utf8_simd(&[0xF4, 0x90, 0x80, 0x80]).is_err());
        // F5-F7 would encode codepoints > U+10FFFF
        assert!(validate_utf8_simd(&[0xF5, 0x80, 0x80, 0x80]).is_err());
        // F8-FB (5-byte, obsolete)
        assert!(validate_utf8_simd(&[0xF8]).is_err());
        // FC-FD (6-byte, obsolete)
        assert!(validate_utf8_simd(&[0xFC]).is_err());
        // FE-FF (never valid in UTF-8)
        assert!(validate_utf8_simd(&[0xFE]).is_err());
        assert!(validate_utf8_simd(&[0xFF]).is_err());
    }

    #[test]
    fn test_invalid_wrong_continuation() {
        // Lead byte followed by non-continuation
        assert!(validate_utf8_simd(&[0xC2, 0x00]).is_err());
        assert!(validate_utf8_simd(&[0xC2, 0x7F]).is_err());
        assert!(validate_utf8_simd(&[0xC2, 0xC0]).is_err());
        assert!(validate_utf8_simd(&[0xE0, 0xA0, 0x00]).is_err());
        assert!(validate_utf8_simd(&[0xF0, 0x90, 0x80, 0x7F]).is_err());
    }

    #[test]
    fn test_invalid_in_middle() {
        // Valid start, invalid middle, valid end
        let mut data = b"Hello".to_vec();
        data.push(0x80); // Invalid continuation byte
        data.extend_from_slice(b"World");
        assert!(validate_utf8_simd(&data).is_err());
    }

    // ========================================================================
    // Edge Cases and Boundary Tests
    // ========================================================================

    #[test]
    fn test_boundary_15_bytes() {
        // Just under 16-byte SIMD boundary
        assert!(validate_utf8_simd(b"123456789012345").is_ok());
    }

    #[test]
    fn test_boundary_17_bytes() {
        // Just over 16-byte SIMD boundary
        assert!(validate_utf8_simd(b"12345678901234567").is_ok());
    }

    #[test]
    fn test_boundary_31_bytes() {
        assert!(validate_utf8_simd(b"1234567890123456789012345678901").is_ok());
    }

    #[test]
    fn test_boundary_33_bytes() {
        assert!(validate_utf8_simd(b"123456789012345678901234567890123").is_ok());
    }

    #[test]
    fn test_multibyte_at_chunk_boundary() {
        // Multi-byte sequence split across 16-byte chunk boundary
        let mut data = vec![b'A'; 15];
        data.extend_from_slice("é".as_bytes()); // 2-byte sequence at position 15-16
        assert!(validate_utf8_simd(&data).is_ok());

        let mut data = vec![b'A'; 14];
        data.extend_from_slice("€".as_bytes()); // 3-byte sequence at position 14-16
        assert!(validate_utf8_simd(&data).is_ok());

        let mut data = vec![b'A'; 13];
        data.extend_from_slice("🎉".as_bytes()); // 4-byte sequence at position 13-16
        assert!(validate_utf8_simd(&data).is_ok());
    }

    #[test]
    fn test_tail_bytes_validation() {
        // Test that tail bytes (after full chunks) are properly validated
        let mut data = vec![b'A'; 16]; // One full chunk
        data.extend_from_slice("Valid tail".as_bytes());
        assert!(validate_utf8_simd(&data).is_ok());

        let mut data = vec![b'A'; 16];
        data.push(0x80); // Invalid continuation in tail
        assert!(validate_utf8_simd(&data).is_err());
    }

    // ========================================================================
    // Consistency with Standard Library
    // ========================================================================

    #[test]
    fn test_matches_stdlib() {
        let test_cases: Vec<&[u8]> = vec![
            b"",
            b"Hello",
            b"Hello, World!",
            "こんにちは".as_bytes(),
            "🎉🌍🚀".as_bytes(),
            "Mixed: aéあ🎉".as_bytes(),
            &[0x80],
            &[0xC0, 0x80],
            &[0xFF],
            &[0xED, 0xA0, 0x80],       // Surrogate
            &[0xF4, 0x90, 0x80, 0x80], // Beyond Unicode
        ];

        for data in test_cases {
            let stdlib_result = std::str::from_utf8(data).is_ok();
            let simd_result = validate_utf8_simd(data).is_ok();
            assert_eq!(
                stdlib_result, simd_result,
                "Mismatch for {:?}: stdlib={}, simd={}",
                data, stdlib_result, simd_result
            );
        }
    }

    #[test]
    fn test_matches_stdlib_random_lengths() {
        // Test various sizes to cover SIMD boundaries
        for size in [
            0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 255, 256,
        ] {
            let data: Vec<u8> = (0..size).map(|i| (i % 128) as u8).collect();
            let stdlib_result = std::str::from_utf8(&data).is_ok();
            let simd_result = validate_utf8_simd(&data).is_ok();
            assert_eq!(
                stdlib_result, simd_result,
                "Mismatch at size {}: stdlib={}, simd={}",
                size, stdlib_result, simd_result
            );
        }
    }

    // ========================================================================
    // ARM64 SIMD Path Verification
    // ========================================================================

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn test_neon_path_is_used_on_arm64() {
        // This test verifies that NEON is detected and available on ARM64.
        // If this test runs on ARM64 without NEON, it will panic.
        assert!(
            std::arch::is_aarch64_feature_detected!("neon"),
            "Running on ARM64 but NEON not detected - SIMD code path not verified! \
             All previous tests may have used the scalar fallback."
        );

        // Additionally verify the SIMD path produces correct results
        // for a variety of inputs when we KNOW we're on ARM64 with NEON
        let test_cases: &[(&[u8], bool)] = &[
            (b"Hello, World!", true),
            ("こんにちは".as_bytes(), true),
            ("🎉🌍🚀".as_bytes(), true),
            (&[0x80], false),
            (&[0xC0, 0x80], false),
            (&[0xED, 0xA0, 0x80], false), // Surrogate
        ];

        for (data, expected_valid) in test_cases {
            let result = validate_utf8_simd(data).is_ok();
            assert_eq!(
                result, *expected_valid,
                "NEON path mismatch for {:?}: expected {}, got {}",
                data, expected_valid, result
            );
        }
    }

    #[test]
    #[cfg(not(target_arch = "aarch64"))]
    fn test_scalar_fallback_on_non_arm64() {
        // On non-ARM64 platforms, verify the scalar fallback works correctly
        let test_cases: &[(&[u8], bool)] = &[
            (b"Hello, World!", true),
            ("こんにちは".as_bytes(), true),
            (&[0x80], false),
        ];

        for (data, expected_valid) in test_cases {
            let result = validate_utf8_simd(data).is_ok();
            assert_eq!(
                result, *expected_valid,
                "Scalar fallback mismatch for {:?}: expected {}, got {}",
                data, expected_valid, result
            );
        }
    }
}
