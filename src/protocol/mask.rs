/// Scalar byte-by-byte XOR masking (original implementation).
#[inline]
pub fn apply_mask(data: &mut [u8], mask: [u8; 4]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= mask[i % 4];
    }
}

/// Scalar implementation processing 4 bytes at a time using u32 operations.
/// Used as fallback when SIMD is not available.
#[inline]
fn apply_mask_scalar(data: &mut [u8], mask: [u8; 4]) {
    let mask_u32 = u32::from_ne_bytes(mask);
    let len = data.len();
    let chunks = len / 4;
    let remainder = len % 4;

    // Process 4-byte chunks
    for i in 0..chunks {
        let offset = i * 4;
        // SAFETY: We calculated chunks = len / 4, so offset + 4 <= len
        let chunk = unsafe { data.get_unchecked_mut(offset..offset + 4) };
        let val = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let masked = (val ^ mask_u32).to_ne_bytes();
        chunk[0] = masked[0];
        chunk[1] = masked[1];
        chunk[2] = masked[2];
        chunk[3] = masked[3];
    }

    // Process remaining bytes
    let tail_start = chunks * 4;
    for i in 0..remainder {
        data[tail_start + i] ^= mask[i];
    }
}

// ============================================================================
// x86/x86_64 SIMD implementations (SSE2 and AVX2)
// ============================================================================

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86_simd {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    /// SSE2 implementation: processes 16 bytes per iteration.
    ///
    /// # Safety
    /// Caller must ensure that SSE2 is available on the current CPU.
    #[target_feature(enable = "sse2")]
    pub unsafe fn apply_mask_sse2(data: &mut [u8], mask: [u8; 4]) {
        let len = data.len();
        if len == 0 {
            return;
        }

        let mask_bytes: [u8; 16] = [
            mask[0], mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0],
            mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3],
        ];
        // SAFETY: mask_bytes is a valid 16-byte array, _mm_loadu_si128 handles unaligned loads
        let mask_vec = unsafe { _mm_loadu_si128(mask_bytes.as_ptr() as *const __m128i) };

        let chunks = len / 16;
        let ptr = data.as_mut_ptr();

        for i in 0..chunks {
            let offset = i * 16;
            // SAFETY: chunks = len / 16, so offset + 16 <= len. ptr.add(offset) is valid for 16 bytes.
            unsafe {
                let data_ptr = ptr.add(offset) as *mut __m128i;
                let data_vec = _mm_loadu_si128(data_ptr);
                let result = _mm_xor_si128(data_vec, mask_vec);
                _mm_storeu_si128(data_ptr, result);
            }
        }

        let tail_start = chunks * 16;
        for i in tail_start..len {
            // SAFETY: i < len, so ptr.add(i) is valid
            unsafe { *ptr.add(i) ^= mask[i % 4] };
        }
    }

    /// AVX2 implementation: processes 32 bytes per iteration.
    ///
    /// # Safety
    /// Caller must ensure that AVX2 is available on the current CPU.
    #[target_feature(enable = "avx2")]
    pub unsafe fn apply_mask_avx2(data: &mut [u8], mask: [u8; 4]) {
        let len = data.len();
        if len == 0 {
            return;
        }

        let mask_bytes: [u8; 32] = [
            mask[0], mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0],
            mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0], mask[1],
            mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0], mask[1], mask[2],
            mask[3], mask[0], mask[1], mask[2], mask[3],
        ];
        // SAFETY: mask_bytes is a valid 32-byte array, _mm256_loadu_si256 handles unaligned loads
        let mask_vec = unsafe { _mm256_loadu_si256(mask_bytes.as_ptr() as *const __m256i) };

        let chunks = len / 32;
        let ptr = data.as_mut_ptr();

        for i in 0..chunks {
            let offset = i * 32;
            // SAFETY: chunks = len / 32, so offset + 32 <= len. ptr.add(offset) is valid for 32 bytes.
            unsafe {
                let data_ptr = ptr.add(offset) as *mut __m256i;
                let data_vec = _mm256_loadu_si256(data_ptr);
                let result = _mm256_xor_si256(data_vec, mask_vec);
                _mm256_storeu_si256(data_ptr, result);
            }
        }

        let tail_start = chunks * 32;
        let remaining = len - tail_start;

        if remaining >= 16 {
            let mask_bytes_16: [u8; 16] = [
                mask[0], mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0],
                mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3],
            ];
            // SAFETY: remaining >= 16, so ptr.add(tail_start) is valid for 16 bytes
            unsafe {
                let mask_vec_16 = _mm_loadu_si128(mask_bytes_16.as_ptr() as *const __m128i);
                let data_ptr = ptr.add(tail_start) as *mut __m128i;
                let data_vec = _mm_loadu_si128(data_ptr);
                let result = _mm_xor_si128(data_vec, mask_vec_16);
                _mm_storeu_si128(data_ptr, result);
            }

            let scalar_start = tail_start + 16;
            for i in scalar_start..len {
                // SAFETY: i < len, so ptr.add(i) is valid
                unsafe { *ptr.add(i) ^= mask[i % 4] };
            }
        } else {
            for i in tail_start..len {
                // SAFETY: i < len, so ptr.add(i) is valid
                unsafe { *ptr.add(i) ^= mask[i % 4] };
            }
        }
    }
}

// ============================================================================
// ARM64 NEON SIMD implementation
// ============================================================================

#[cfg(target_arch = "aarch64")]
mod aarch64_simd {
    use std::arch::aarch64::*;

    /// NEON implementation: processes 16 bytes per iteration.
    ///
    /// # Safety
    /// Caller must ensure that NEON is available on the current CPU.
    #[target_feature(enable = "neon")]
    pub unsafe fn apply_mask_neon(data: &mut [u8], mask: [u8; 4]) {
        let len = data.len();
        if len == 0 {
            return;
        }

        let mask_bytes: [u8; 16] = [
            mask[0], mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3], mask[0],
            mask[1], mask[2], mask[3], mask[0], mask[1], mask[2], mask[3],
        ];
        // SAFETY: mask_bytes is a valid 16-byte array
        let mask_vec = unsafe { vld1q_u8(mask_bytes.as_ptr()) };

        let chunks = len / 16;
        let ptr = data.as_mut_ptr();

        for i in 0..chunks {
            let offset = i * 16;
            // SAFETY: chunks = len / 16, so offset + 16 <= len. ptr.add(offset) is valid for 16 bytes.
            unsafe {
                let data_ptr = ptr.add(offset);
                let data_vec = vld1q_u8(data_ptr);
                let result = veorq_u8(data_vec, mask_vec);
                vst1q_u8(data_ptr, result);
            }
        }

        let tail_start = chunks * 16;
        for i in tail_start..len {
            // SAFETY: i < len, so ptr.add(i) is valid
            unsafe { *ptr.add(i) ^= mask[i % 4] };
        }
    }
}

// ============================================================================
// Public SIMD-accelerated API with runtime CPU feature detection
// ============================================================================

/// SIMD-accelerated XOR masking with runtime CPU feature detection.
///
/// This function automatically selects the best available implementation:
/// - AVX2 (256-bit, 32 bytes/iteration) on modern x86_64
/// - SSE2 (128-bit, 16 bytes/iteration) on x86/x86_64
/// - NEON (128-bit, 16 bytes/iteration) on ARM64
/// - Scalar fallback on unsupported platforms
///
/// # Example
///
/// ```
/// use rsws::protocol::mask::apply_mask_simd;
///
/// let mask = [0x37, 0xfa, 0x21, 0x3d];
/// let mut data = b"Hello".to_vec();
/// apply_mask_simd(&mut data, mask);
/// ```
#[inline]
pub fn apply_mask_simd(data: &mut [u8], mask: [u8; 4]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // SAFETY: is_x86_feature_detected! is a safe macro that checks CPU features at runtime.
        // We only call the unsafe SIMD function if the corresponding feature is detected.
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 feature is confirmed available by the runtime check above.
            // apply_mask_avx2 requires AVX2, which we just verified is present.
            return unsafe { x86_simd::apply_mask_avx2(data, mask) };
        }
        if is_x86_feature_detected!("sse2") {
            // SAFETY: SSE2 feature is confirmed available by the runtime check above.
            // apply_mask_sse2 requires SSE2, which we just verified is present.
            return unsafe { x86_simd::apply_mask_sse2(data, mask) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: is_aarch64_feature_detected! is a safe macro that checks CPU features at runtime.
        if std::arch::is_aarch64_feature_detected!("neon") {
            // SAFETY: NEON feature is confirmed available by the runtime check above.
            // apply_mask_neon requires NEON, which we just verified is present.
            return unsafe { aarch64_simd::apply_mask_neon(data, mask) };
        }
    }

    // Fallback to scalar implementation
    apply_mask_scalar(data, mask);
}

/// Fast XOR masking using SIMD when available.
///
/// This is an alias for `apply_mask_simd` for backward compatibility.
/// Unlike the previous nightly-only implementation, this works on stable Rust.
#[inline]
pub fn apply_mask_fast(data: &mut [u8], mask: [u8; 4]) {
    apply_mask_simd(data, mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_masking_reversible() {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let original = b"Hello, WebSocket!".to_vec();
        let mut data = original.clone();

        apply_mask(&mut data, mask);
        assert_ne!(data, original);

        apply_mask(&mut data, mask);
        assert_eq!(data, original);
    }

    #[test]
    fn test_masking_example_from_rfc() {
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let mut data = b"Hello".to_vec();

        apply_mask(&mut data, mask);
        assert_eq!(data, vec![0x7f, 0x9f, 0x4d, 0x51, 0x58]);
    }

    #[test]
    fn test_masking_empty() {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut data: Vec<u8> = vec![];
        apply_mask(&mut data, mask);
        assert_eq!(data, Vec::<u8>::new());
    }

    #[test]
    fn test_masking_single_byte() {
        let mask = [0xff, 0x00, 0x00, 0x00];
        let mut data = vec![0xaa];
        apply_mask(&mut data, mask);
        assert_eq!(data, vec![0x55]);
    }

    #[test]
    fn test_masking_aligned() {
        let mask = [0x11, 0x22, 0x33, 0x44];
        let mut data = vec![0x00; 8];
        apply_mask(&mut data, mask);
        assert_eq!(data, vec![0x11, 0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn test_masking_fast_equivalent() {
        let mask = [0xab, 0xcd, 0xef, 0x12];
        let original = b"The quick brown fox jumps over the lazy dog".to_vec();

        let mut data1 = original.clone();
        let mut data2 = original.clone();

        apply_mask(&mut data1, mask);
        apply_mask_fast(&mut data2, mask);

        assert_eq!(data1, data2);
    }

    #[test]
    fn test_masking_simd_equivalent() {
        let mask = [0xab, 0xcd, 0xef, 0x12];

        // Test various sizes to cover SIMD boundaries
        let test_sizes = [
            0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 127, 128, 129, 255,
            256, 257, 511, 512, 513, 1000, 1024, 4096,
        ];

        for size in test_sizes {
            let original: Vec<u8> = (0..size).map(|i| (i & 0xff) as u8).collect();

            let mut data_scalar = original.clone();
            let mut data_simd = original.clone();

            apply_mask(&mut data_scalar, mask);
            apply_mask_simd(&mut data_simd, mask);

            assert_eq!(data_scalar, data_simd, "SIMD mismatch at size {}", size);
        }
    }

    #[test]
    fn test_masking_simd_reversible() {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let original = b"Hello, WebSocket! This is a longer message for SIMD testing.".to_vec();
        let mut data = original.clone();

        apply_mask_simd(&mut data, mask);
        assert_ne!(data, original);

        apply_mask_simd(&mut data, mask);
        assert_eq!(data, original);
    }

    #[test]
    fn test_masking_simd_empty() {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut data: Vec<u8> = vec![];
        apply_mask_simd(&mut data, mask);
        assert_eq!(data, Vec::<u8>::new());
    }

    #[test]
    fn test_masking_simd_single_byte() {
        let mask = [0xff, 0x00, 0x00, 0x00];
        let mut data = vec![0xaa];
        apply_mask_simd(&mut data, mask);
        assert_eq!(data, vec![0x55]);
    }

    #[test]
    fn test_masking_simd_aligned_16() {
        let mask = [0x11, 0x22, 0x33, 0x44];
        let mut data = vec![0x00; 16];
        apply_mask_simd(&mut data, mask);
        let expected = vec![
            0x11, 0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44, 0x11, 0x22, 0x33, 0x44, 0x11, 0x22,
            0x33, 0x44,
        ];
        assert_eq!(data, expected);
    }

    #[test]
    fn test_masking_simd_aligned_32() {
        let mask = [0x11, 0x22, 0x33, 0x44];
        let mut data = vec![0x00; 32];
        apply_mask_simd(&mut data, mask);
        let expected: Vec<u8> = (0..32).map(|i| mask[i % 4]).collect();
        assert_eq!(data, expected);
    }

    #[test]
    fn test_masking_scalar_function() {
        let mask = [0xab, 0xcd, 0xef, 0x12];
        let original = b"Testing scalar implementation directly".to_vec();

        let mut data1 = original.clone();
        let mut data2 = original.clone();

        apply_mask(&mut data1, mask);
        apply_mask_scalar(&mut data2, mask);

        assert_eq!(data1, data2);
    }
}
