# Draft: rsws Performance Optimization

## Requirements (confirmed)
- Optimize WebSocket library for throughput
- Target: 2-4x improvement for large messages
- Maintain backward API compatibility where possible
- All changes must pass existing tests
- Add benchmarks for new optimizations

## Codebase Analysis (VERIFIED)

### 1. MASKING - src/protocol/mask.rs (88 lines)
**Current State:**
- `apply_mask()` (line 2-6): Byte-by-byte XOR with `i % 4` modulo - SLOW
- `apply_mask_fast()` (line 9-22): 4-byte chunks with `as_chunks_mut::<4>()` - requires NIGHTLY
- Uses `u32::from_ne_bytes()` for 4-byte XOR - scalar, no SIMD

**Optimization Opportunity:** Add AVX2/SSE2/NEON paths for 16-32 bytes/iteration
**Expected Gain:** 4-8x for large payloads

### 2. FRAME PARSING - src/protocol/frame.rs (878 lines)
**Current State:**
- Line 226: `.to_vec()` allocation for EVERY masked frame
- Line 231: `.to_vec()` allocation for EVERY unmasked frame  
- `Payload` enum (line 14-17) only has `Owned(Vec<u8>)` variant
- README claims "zero-copy" but Frame::parse ALWAYS copies

**Optimization Opportunity:** Add `Payload::Borrowed(&'a [u8])` or use `Bytes` crate
**Expected Gain:** Eliminate allocation per frame for unmasked frames

### 3. MESSAGE REASSEMBLY - src/protocol/assembler.rs (301 lines)
**Current State:**
- Line 10: `fragments: Vec<Vec<u8>>` - double indirection
- Line 66: `frame.payload().to_vec()` - copies every fragment
- Line 69: `.drain(..).flatten().collect()` - creates ANOTHER Vec

**Optimization Opportunity:** Single contiguous `BytesMut` buffer, extend directly
**Expected Gain:** Eliminate N+1 allocations for N-fragment messages

### 4. FLUSH PATTERNS - src/connection/connection.rs (463 lines)
**Current State:**
- Lines 108-109: `write_frame()` + `flush()` for EVERY send
- Lines 135-136: `write_frame()` + `flush()` for EVERY pong
- Lines 168-169: Same pattern for close response
- NO batch send API

**Optimization Opportunity:** Add `send_batch()`, `send_no_flush()`, optional flush param
**Expected Gain:** Reduce syscalls by 50%+ for bulk sends

### 5. CODEC BUFFER MANAGEMENT - src/codec/framed.rs (329 lines)
**Current State:**
- Line 10: 8KB initial buffer (DEFAULT_BUFFER_SIZE)
- Lines 98-103: 4KB stack temp buffer, then copy to BytesMut
- Lines 115-116: `clear()` + `resize()` on every write

**Optimization Opportunity:** Read directly into `BytesMut::chunk_mut()`
**Expected Gain:** Eliminate one copy per read

### 6. EXISTING BENCHMARKS - benches/benchmarks.rs (245 lines)
- Frame parsing benchmarks (10B, 1KB, 64KB)
- Masking benchmarks (`apply_mask` and `apply_mask_fast` at 64B, 1KB, 64KB, 1MB)
- Handshake benchmarks
- Uses Criterion with Throughput metrics

## Technical Decisions
- [ ] SIMD approach: std::arch intrinsics vs portable-simd vs `wide` crate?
- [ ] Zero-copy approach: `bytes` crate already in deps (optional), extend usage?
- [ ] API compatibility: Breaking changes allowed for perf gains?
- [ ] Feature flags: Optional SIMD behind feature flag?

## Open Questions
1. What's the primary use case - many small messages or fewer large messages?
2. `bytes` crate is already optional dep - make it required for async-tokio?
3. Should SIMD be optional (feature flag) or always-on with runtime detection?
4. Are breaking API changes acceptable for Connection::send() signature?
5. What's the minimum supported Rust version (MSRV)? (currently edition 2024)
6. Target platforms - x86_64 only or also ARM/WASM?

## Scope Boundaries
- INCLUDE: Masking, frame parsing, reassembly, flush patterns, buffer management
- EXCLUDE: Handshake optimization, TLS layer, compression (permessage-deflate)

## Dependencies Analysis
**Current (Cargo.toml):**
- `bytes = "1.5"` - ALREADY optional with async-tokio feature
- `tokio = "1.36"` - optional
- Edition 2024 (requires Rust 1.93+)

**Potential additions:**
- None needed for SIMD (use std::arch)
- `bytes` already available
