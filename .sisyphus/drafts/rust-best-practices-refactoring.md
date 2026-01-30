# Draft: Rust Best Practices Refactoring

## Requirements (confirmed)
- Apply Clippy pedantic/nursery lint fixes
- Conform to official Rust API Guidelines
- Maintain all 202 passing tests
- No breaking changes to public API

## Technical Decisions

### 1. `#[must_use]` Attributes
- Add to all constructors returning `Self`
- Add to all builder methods returning `Self`
- Add to pure functions with no side effects
- Message format: `#[must_use = "this returns the result of the operation, without modifying the original"]`

### 2. `const fn` Candidates
- `Limits::check_*` methods CANNOT be const (they return Result with Error enum)
- `Config::with_*` methods CANNOT be const (they use `mut self`)
- `CloseCode::from_u16`, `as_u16`, `is_valid` CAN be const
- `OpCode` methods already have some const fn

### 3. `#[non_exhaustive]` Candidates
- `Error` enum - yes (may add new error types)
- `CloseCode` enum - yes (has `Other(u16)` variant, but still good practice)
- `Message` enum - yes (may add new message types)
- `OpCode` enum - maybe (RFC defines all opcodes, but extensions possible)
- `ConnectionState` enum - yes (may add intermediate states)
- `Role` enum - no (only Client/Server, unlikely to change)

### 4. `# Errors` Documentation
Files with public Result-returning functions:
- `config.rs`: `check_message_size`, `check_frame_size`, `check_fragment_count`
- `protocol/frame.rs`: `parse`, `validate`, `write` (already has some docs)
- `protocol/handshake.rs`: `parse`, `validate`, `from_request`
- `extensions/mod.rs`: Various extension methods

### 5. Digit Separators
- `codec/framed.rs`: `0x9E3779B9` → `0x9E37_79B9`
- `codec/framed.rs`: `0x85EBCA6B` → `0x85EB_CA6B`
- `codec/framed.rs`: `0xC2B2AE35` → `0xC2B2_AE35`

### 6. Send/Sync Compile-time Tests
Add to `src/lib.rs`:
```rust
#[cfg(test)]
mod send_sync_tests {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    
    #[test]
    fn test_types_are_send_sync() {
        assert_send::<crate::Config>();
        assert_sync::<crate::Config>();
        // ... etc
    }
}
```

## Research Findings
- Codebase has NO existing `#[must_use]` attributes
- Codebase has NO existing `#[non_exhaustive]` attributes
- Some `const fn` already exist in `config.rs`, `opcode.rs`
- Documentation style uses `///` with `# Arguments`, `# Example` but missing `# Errors`
- Good test coverage with 202 tests

## Scope Boundaries
- INCLUDE: All 11 files listed in requirements
- INCLUDE: All public API items
- EXCLUDE: Private/internal items
- EXCLUDE: Test code (except adding Send/Sync tests)
- EXCLUDE: Adding new functionality

## Open Questions
- None - requirements are clear

## File-by-File Analysis

### 1. src/config.rs (285 lines)
**`#[must_use]` needed:**
- `Limits::new()` (line 39)
- `Limits::embedded()` (line 56)
- `Limits::unrestricted()` (lines 75, 89)
- `Config::new()` (line 177)
- `Config::with_limits()` (line 182)
- `Config::with_fragment_size()` (line 188)
- `Config::server()` (line 194)
- `Config::client()` (line 203)

**`# Errors` docs needed:**
- `check_message_size()` (line 98)
- `check_frame_size()` (line 110)
- `check_fragment_count()` (line 122)

### 2. src/error.rs (155 lines)
**`#[non_exhaustive]` needed:**
- `Error` enum (line 12)

### 3. src/message.rs (323 lines)
**`#[must_use]` needed:**
- `CloseCode::from_u16()` (line 19)
- `CloseCode::as_u16()` (line 34)
- `CloseCode::is_valid()` (line 49)
- `CloseFrame::new()` (line 68)
- `Message::text()` (line 91)
- `Message::binary()` (line 95)
- `Message::ping()` (line 99)
- `Message::pong()` (line 103)
- `Message::close()` (line 107)
- `Message::is_text()` (line 111)
- `Message::is_binary()` (line 115)
- `Message::is_data()` (line 119)
- `Message::is_control()` (line 123)
- `Message::into_text()` (line 130)
- `Message::into_binary()` (line 137)
- `Message::as_text()` (line 144)
- `Message::as_binary()` (line 151)

**`#[non_exhaustive]` needed:**
- `CloseCode` enum (line 4)
- `Message` enum (line 76)

**`const fn` candidates:**
- `CloseCode::from_u16()` - YES
- `CloseCode::as_u16()` - YES
- `CloseCode::is_valid()` - YES (calls as_u16 which would need to be const first)

### 4. src/protocol/frame.rs (869 lines)
**`#[must_use]` needed:**
- `Frame::new()` (line 64)
- `Frame::text()` (line 76)
- `Frame::binary()` (line 81)
- `Frame::close()` (line 86)
- `Frame::ping()` (line 98)
- `Frame::pong()` (line 103)
- `Frame::payload()` (line 109)
- `Frame::into_payload()` (line 116)
- `Frame::wire_size()` (line 363)

**`# Errors` docs already exist but need review:**
- `parse()` (line 132) - has docs
- `validate()` (line 245) - has docs
- `write()` (line 279) - has docs

### 5. src/codec/framed.rs (329 lines)
**Digit separators needed:**
- Line 46: `0x9E3779B9` → `0x9E37_79B9`
- Line 48: `0x85EBCA6B` → `0x85EB_CA6B`
- Line 50: `0xC2B2AE35` → `0xC2B2_AE35`

**`#[must_use]` needed:**
- `WebSocketCodec::new()` (line 23)
- `WebSocketCodec::role()` (line 37)
- `WebSocketCodec::config()` (line 41)
- `WebSocketCodec::into_inner()` (line 128)

### 6. src/lib.rs (47 lines)
**Send/Sync tests needed:**
- Add compile-time assertions for all public types
