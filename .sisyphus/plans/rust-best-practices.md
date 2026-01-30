# Rust Best Practices Refactoring

## TL;DR

> **Quick Summary**: Apply Rust API Guidelines compliance fixes across 11 source files, adding `#[must_use]`, `#[non_exhaustive]`, `const fn`, `# Errors` docs, and digit separators to satisfy Clippy pedantic/nursery lints.
> 
> **Deliverables**:
> - All Clippy pedantic/nursery lints resolved
> - Full Rust API Guidelines compliance
> - Send/Sync compile-time guarantees documented
> - All 202 tests passing
> 
> **Estimated Effort**: Medium (2-3 hours)
> **Parallel Execution**: YES - 3 waves
> **Critical Path**: Wave 1 (foundations) → Wave 2 (bulk changes) → Wave 3 (integration tests)

---

## Context

### Original Request
Refactor rsws WebSocket library to conform to Rust best practices. Clippy with pedantic/nursery lints identified:
1. Missing `#[must_use]` attributes on constructors/builders
2. Missing `const fn` opportunities
3. Missing `# Errors` documentation sections
4. Unreadable hex literals without digit separators
5. Missing `#[non_exhaustive]` on public enums

### Interview Summary
**Key Discussions**:
- Mechanical refactoring only - no behavioral changes
- All 202 existing tests must continue passing
- Changes are additive (attributes, docs) not breaking API

**Research Findings**:
- No existing `#[must_use]` or `#[non_exhaustive]` in codebase
- Some `const fn` already exists in `config.rs`, `opcode.rs`
- Documentation uses `# Arguments`, `# Example` but missing `# Errors`
- Codebase uses `thiserror` for error types

### Self-Review Gap Analysis
**Guardrails Applied**:
- Do NOT change any function signatures
- Do NOT modify test assertions
- Do NOT add new dependencies
- Do NOT change error enum variant names (would break `#[non_exhaustive]`)
- Verify `cargo test` passes after each wave

---

## Work Objectives

### Core Objective
Make rsws fully compliant with Rust API Guidelines and resolve all Clippy pedantic/nursery lints without breaking existing functionality.

### Concrete Deliverables
- 11 modified source files with best practices applied
- Zero Clippy warnings with `#![warn(clippy::pedantic, clippy::nursery)]`
- Send/Sync compile-time test suite in `src/lib.rs`

### Definition of Done
- [x] `cargo clippy --all-features -- -W clippy::pedantic -W clippy::nursery` shows no warnings for targeted lints
- [x] `cargo test --all-features` passes (202 tests)
- [x] `cargo doc --all-features` builds without warnings

### Must Have
- `#[must_use]` on all constructors returning `Self`
- `#[must_use]` on all builder methods returning `Self`
- `#[must_use]` on pure functions (getters, converters)
- `#[non_exhaustive]` on `Error`, `CloseCode`, `Message`, `ConnectionState` enums
- `# Errors` documentation on all public `Result`-returning functions
- Digit separators on hex literals (4-digit grouping)
- Send/Sync compile-time assertions

### Must NOT Have (Guardrails)
- NO changes to function signatures or return types
- NO changes to error variant names or fields
- NO changes to test assertions or test logic
- NO new dependencies added
- NO removal of existing functionality
- NO changes to public API behavior

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (`cargo test`)
- **User wants tests**: YES (existing tests + new Send/Sync tests)
- **Framework**: Rust built-in test framework

### Verification Commands
```bash
# After each wave:
cargo test --all-features                    # All 202+ tests pass
cargo clippy --all-features -- -W clippy::pedantic -W clippy::nursery 2>&1 | grep -E "(must_use|non_exhaustive|missing_errors_doc|unreadable_literal)"  # No targeted warnings
cargo doc --all-features --no-deps           # Docs build clean
```

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation - Start Immediately):
├── Task 1: src/error.rs - Add #[non_exhaustive] to Error enum
├── Task 2: src/message.rs - Add #[non_exhaustive], #[must_use], const fn
└── Task 3: src/codec/framed.rs - Fix digit separators

Wave 2 (Bulk Changes - After Wave 1):
├── Task 4: src/config.rs - Add #[must_use], # Errors docs
├── Task 5: src/protocol/frame.rs - Add #[must_use], verify # Errors docs
├── Task 6: src/protocol/opcode.rs - Add #[must_use]
├── Task 7: src/protocol/handshake.rs - Add # Errors docs
├── Task 8: src/connection/role.rs - Add #[must_use]
├── Task 9: src/connection/state.rs - Add #[must_use], #[non_exhaustive]
└── Task 10: src/extensions/mod.rs - Add # Errors docs

Wave 3 (Integration - After Wave 2):
└── Task 11: src/lib.rs - Add Send/Sync compile-time tests

Critical Path: Wave 1 → Wave 2 → Wave 3
Parallel Speedup: ~50% faster than sequential
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|---------------------|
| 1 | None | 11 | 2, 3 |
| 2 | None | 11 | 1, 3 |
| 3 | None | 11 | 1, 2 |
| 4 | 1, 2, 3 | 11 | 5, 6, 7, 8, 9, 10 |
| 5 | 1, 2, 3 | 11 | 4, 6, 7, 8, 9, 10 |
| 6 | 1, 2, 3 | 11 | 4, 5, 7, 8, 9, 10 |
| 7 | 1, 2, 3 | 11 | 4, 5, 6, 8, 9, 10 |
| 8 | 1, 2, 3 | 11 | 4, 5, 6, 7, 9, 10 |
| 9 | 1, 2, 3 | 11 | 4, 5, 6, 7, 8, 10 |
| 10 | 1, 2, 3 | 11 | 4, 5, 6, 7, 8, 9 |
| 11 | 4-10 | None | None (final) |

---

## TODOs

- [x] 1. Add `#[non_exhaustive]` to Error enum

  **What to do**:
  - Add `#[non_exhaustive]` attribute above `pub enum Error` definition
  - This allows adding new error variants in future without breaking downstream code

  **Must NOT do**:
  - Do NOT rename any error variants
  - Do NOT change error messages
  - Do NOT modify `From` implementations

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single-file, single attribute addition - minimal change
  - **Skills**: None required
    - Simple attribute addition, no specialized knowledge needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3)
  - **Blocks**: Task 11
  - **Blocked By**: None

  **References**:
  - `src/error.rs:12` - Error enum definition location
  - Rust Reference: `#[non_exhaustive]` allows adding variants without semver break

  **Acceptance Criteria**:
  - [ ] `src/error.rs` contains `#[non_exhaustive]` above `pub enum Error`
  - [ ] `cargo test --all-features` passes
  - [ ] `cargo clippy` shows no new warnings

  **Commit**: YES
  - Message: `refactor(error): add #[non_exhaustive] to Error enum for future compatibility`
  - Files: `src/error.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 2. Add `#[non_exhaustive]`, `#[must_use]`, and `const fn` to message.rs

  **What to do**:
  - Add `#[non_exhaustive]` to `CloseCode` enum (line 4)
  - Add `#[non_exhaustive]` to `Message` enum (line 76)
  - Add `#[must_use]` to these methods:
    - `CloseCode::from_u16()` - returns new value
    - `CloseCode::as_u16()` - pure function
    - `CloseCode::is_valid()` - pure function
    - `CloseFrame::new()` - constructor
    - `Message::text()`, `binary()`, `ping()`, `pong()`, `close()` - constructors
    - `Message::is_text()`, `is_binary()`, `is_data()`, `is_control()` - pure predicates
    - `Message::into_text()`, `into_binary()` - consuming converters
    - `Message::as_text()`, `as_binary()` - borrowing converters
  - Convert to `const fn`:
    - `CloseCode::from_u16()` - only uses match, can be const
    - `CloseCode::as_u16()` - only uses match, can be const
    - `CloseCode::is_valid()` - only uses as_u16() and matches!, can be const

  **Must NOT do**:
  - Do NOT change function signatures
  - Do NOT modify enum variants
  - Do NOT change test assertions

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single file with multiple mechanical attribute additions
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 3)
  - **Blocks**: Task 11
  - **Blocked By**: None

  **References**:
  - `src/message.rs:4-16` - CloseCode enum definition
  - `src/message.rs:18-53` - CloseCode methods
  - `src/message.rs:67-74` - CloseFrame::new
  - `src/message.rs:76-88` - Message enum definition
  - `src/message.rs:90-156` - Message methods

  **Acceptance Criteria**:
  - [ ] `CloseCode` and `Message` enums have `#[non_exhaustive]`
  - [ ] All listed methods have `#[must_use]`
  - [ ] `CloseCode::from_u16`, `as_u16`, `is_valid` are `const fn`
  - [ ] `cargo test --all-features` passes (all 20 message tests)
  - [ ] `cargo clippy` shows no `must_use` warnings for message.rs

  **Commit**: YES
  - Message: `refactor(message): add #[must_use], #[non_exhaustive], const fn for API compliance`
  - Files: `src/message.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 3. Fix unreadable hex literals in codec/framed.rs

  **What to do**:
  - Line 46: Change `0x9E3779B9` to `0x9E37_79B9`
  - Line 48: Change `0x85EBCA6B` to `0x85EB_CA6B`
  - Line 50: Change `0xC2B2AE35` to `0xC2B2_AE35`
  - Add `#[must_use]` to:
    - `WebSocketCodec::new()` (line 23)
    - `WebSocketCodec::role()` (line 37)
    - `WebSocketCodec::config()` (line 41)
    - `WebSocketCodec::into_inner()` (line 128)

  **Must NOT do**:
  - Do NOT change the numeric values (only formatting)
  - Do NOT modify PRNG algorithm logic

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Simple find-replace for literals + attribute additions
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2)
  - **Blocks**: Task 11
  - **Blocked By**: None

  **References**:
  - `src/codec/framed.rs:45-52` - generate_mask() function with hex literals
  - `src/codec/framed.rs:22-35` - WebSocketCodec::new()
  - `src/codec/framed.rs:37-43` - role() and config() methods
  - `src/codec/framed.rs:128-130` - into_inner() method

  **Acceptance Criteria**:
  - [ ] All hex literals use 4-digit grouping with underscores
  - [ ] All listed methods have `#[must_use]`
  - [ ] `cargo test --all-features` passes (codec tests)
  - [ ] `cargo clippy` shows no `unreadable_literal` warnings

  **Commit**: YES
  - Message: `style(codec): add digit separators to hex literals, add #[must_use]`
  - Files: `src/codec/framed.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 4. Add `#[must_use]` and `# Errors` docs to config.rs

  **What to do**:
  - Add `#[must_use]` to:
    - `Limits::new()` (line 39)
    - `Limits::embedded()` (line 56)
    - `Limits::unrestricted()` (lines 75, 89 - both cfg variants)
    - `Config::new()` (line 177)
    - `Config::with_limits()` (line 182)
    - `Config::with_fragment_size()` (line 188)
    - `Config::server()` (line 194)
    - `Config::client()` (line 203)
  - Add `# Errors` documentation to:
    - `check_message_size()` - document `Error::MessageTooLarge`
    - `check_frame_size()` - document `Error::FrameTooLarge`
    - `check_fragment_count()` - document `Error::TooManyFragments`

  **Must NOT do**:
  - Do NOT change function implementations
  - Do NOT modify Limits/Config struct fields

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Attribute and documentation additions only
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 5-10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/config.rs:37-49` - Limits::new()
  - `src/config.rs:56-62` - Limits::embedded()
  - `src/config.rs:74-95` - Limits::unrestricted() (both cfg variants)
  - `src/config.rs:97-131` - check_* methods
  - `src/config.rs:175-209` - Config methods
  - `src/error.rs:27-51` - Error variants for documentation

  **Acceptance Criteria**:
  - [ ] All listed constructors/builders have `#[must_use]`
  - [ ] All check_* methods have `# Errors` doc section listing specific error variant
  - [ ] `cargo test --all-features` passes (config tests)
  - [ ] `cargo doc` builds without warnings

  **Commit**: YES
  - Message: `docs(config): add #[must_use] and # Errors documentation`
  - Files: `src/config.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 5. Add `#[must_use]` to protocol/frame.rs and verify `# Errors` docs

  **What to do**:
  - Add `#[must_use]` to:
    - `Frame::new()` (line 64)
    - `Frame::text()` (line 76)
    - `Frame::binary()` (line 81)
    - `Frame::close()` (line 86)
    - `Frame::ping()` (line 98)
    - `Frame::pong()` (line 103)
    - `Frame::payload()` (line 109)
    - `Frame::into_payload()` (line 116)
    - `Frame::wire_size()` (line 363)
  - Verify `# Errors` docs exist and are complete for:
    - `Frame::parse()` - already has docs at lines 126-131, verify completeness
    - `Frame::validate()` - already has docs at lines 240-244, verify completeness
    - `Frame::write()` - already has docs at lines 276-278, verify completeness

  **Must NOT do**:
  - Do NOT change frame parsing logic
  - Do NOT modify payload handling

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Attribute additions and doc verification
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4, 6-10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/protocol/frame.rs:56-73` - Frame::new()
  - `src/protocol/frame.rs:75-105` - Frame constructors (text, binary, close, ping, pong)
  - `src/protocol/frame.rs:107-120` - payload() and into_payload()
  - `src/protocol/frame.rs:122-236` - parse() with existing # Errors docs
  - `src/protocol/frame.rs:238-265` - validate() with existing # Errors docs
  - `src/protocol/frame.rs:267-360` - write() with existing # Errors docs
  - `src/protocol/frame.rs:362-374` - wire_size()

  **Acceptance Criteria**:
  - [ ] All Frame constructors and getters have `#[must_use]`
  - [ ] `# Errors` sections are complete (list all possible error variants)
  - [ ] `cargo test --all-features` passes (34 frame tests)
  - [ ] `cargo doc` builds without warnings

  **Commit**: YES
  - Message: `docs(frame): add #[must_use] and complete # Errors documentation`
  - Files: `src/protocol/frame.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 6. Add `#[must_use]` to protocol/opcode.rs

  **What to do**:
  - Review OpCode enum and methods
  - Add `#[must_use]` to:
    - `OpCode::from_u8()` if not already const/must_use
    - `OpCode::as_u8()` - pure function
    - `OpCode::is_control()` - pure predicate
    - `OpCode::is_data()` - pure predicate (if exists)
    - Any other pure getters/converters

  **Must NOT do**:
  - Do NOT change opcode values
  - Do NOT modify from_u8 logic

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Simple attribute additions
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4, 5, 7-10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/protocol/opcode.rs` - Full file (need to read for method locations)
  - `src/protocol/mod.rs` - Re-exports

  **Acceptance Criteria**:
  - [ ] All OpCode pure functions have `#[must_use]`
  - [ ] `cargo test --all-features` passes
  - [ ] `cargo clippy` shows no `must_use` warnings for opcode.rs

  **Commit**: YES
  - Message: `refactor(opcode): add #[must_use] to pure functions`
  - Files: `src/protocol/opcode.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 7. Add `# Errors` documentation to protocol/handshake.rs

  **What to do**:
  - Find all public functions returning `Result`
  - Add `# Errors` documentation section to each, listing:
    - Which error variants can be returned
    - Under what conditions each error occurs

  **Must NOT do**:
  - Do NOT change handshake logic
  - Do NOT modify HTTP parsing

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Documentation additions only
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4-6, 8-10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/protocol/handshake.rs` - Full file (need to read for Result-returning functions)
  - `src/error.rs:57-59` - `Error::InvalidHandshake` variant

  **Acceptance Criteria**:
  - [ ] All public Result-returning functions have `# Errors` section
  - [ ] Each error variant is documented with conditions
  - [ ] `cargo doc` builds without warnings
  - [ ] `cargo test --all-features` passes

  **Commit**: YES
  - Message: `docs(handshake): add # Errors documentation sections`
  - Files: `src/protocol/handshake.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 8. Add `#[must_use]` to connection/role.rs

  **What to do**:
  - Review Role enum and methods
  - Add `#[must_use]` to:
    - Any constructors or factory methods
    - `must_mask()` or similar predicates
    - Any pure getters

  **Must NOT do**:
  - Do NOT change Role variants (Client, Server)
  - Do NOT modify masking logic

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Simple attribute additions
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4-7, 9-10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/connection/role.rs` - Full file
  - `src/connection/mod.rs` - Re-exports

  **Acceptance Criteria**:
  - [ ] All Role pure functions have `#[must_use]`
  - [ ] `cargo test --all-features` passes
  - [ ] `cargo clippy` shows no `must_use` warnings for role.rs

  **Commit**: YES
  - Message: `refactor(role): add #[must_use] to pure functions`
  - Files: `src/connection/role.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 9. Add `#[must_use]` and `#[non_exhaustive]` to connection/state.rs

  **What to do**:
  - Add `#[non_exhaustive]` to `ConnectionState` enum
  - Add `#[must_use]` to:
    - Any state transition methods
    - State query predicates (is_open, is_closed, etc.)
    - Any pure getters

  **Must NOT do**:
  - Do NOT change state transition logic
  - Do NOT modify ConnectionState variants

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Attribute additions only
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4-8, 10)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/connection/state.rs` - Full file
  - `src/connection/mod.rs` - Re-exports

  **Acceptance Criteria**:
  - [ ] `ConnectionState` enum has `#[non_exhaustive]`
  - [ ] All state predicates have `#[must_use]`
  - [ ] `cargo test --all-features` passes
  - [ ] `cargo clippy` shows no warnings for state.rs

  **Commit**: YES
  - Message: `refactor(state): add #[must_use], #[non_exhaustive] for API compliance`
  - Files: `src/connection/state.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 10. Add `# Errors` documentation to extensions/mod.rs

  **What to do**:
  - Find all public functions returning `Result`
  - Add `# Errors` documentation section to each
  - Document `Error::Extension` and `Error::InvalidExtension` variants

  **Must NOT do**:
  - Do NOT change extension negotiation logic
  - Do NOT modify RSV bit handling

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Documentation additions only
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4-9)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 1, 2, 3 (Wave 1)

  **References**:
  - `src/extensions/mod.rs` - Full file
  - `src/error.rs:66-67` - `Error::Extension` variant
  - `src/error.rs:109-110` - `Error::InvalidExtension` variant

  **Acceptance Criteria**:
  - [ ] All public Result-returning functions have `# Errors` section
  - [ ] `cargo doc` builds without warnings
  - [ ] `cargo test --all-features` passes

  **Commit**: YES
  - Message: `docs(extensions): add # Errors documentation sections`
  - Files: `src/extensions/mod.rs`
  - Pre-commit: `cargo test --all-features`

---

- [x] 11. Add Send/Sync compile-time tests to lib.rs

  **What to do**:
  - Add a new test module in `src/lib.rs`:
    ```rust
    #[cfg(test)]
    mod send_sync_tests {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        #[test]
        fn public_types_are_send() {
            assert_send::<crate::Config>();
            assert_send::<crate::Limits>();
            assert_send::<crate::Error>();
            assert_send::<crate::Message>();
            assert_send::<crate::CloseCode>();
            assert_send::<crate::CloseFrame>();
            assert_send::<crate::OpCode>();
            assert_send::<crate::Role>();
            assert_send::<crate::ConnectionState>();
        }

        #[test]
        fn public_types_are_sync() {
            assert_sync::<crate::Config>();
            assert_sync::<crate::Limits>();
            assert_sync::<crate::Error>();
            assert_sync::<crate::Message>();
            assert_sync::<crate::CloseCode>();
            assert_sync::<crate::CloseFrame>();
            assert_sync::<crate::OpCode>();
            assert_sync::<crate::Role>();
            assert_sync::<crate::ConnectionState>();
        }
    }
    ```

  **Must NOT do**:
  - Do NOT modify existing re-exports
  - Do NOT change module structure

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Adding test code only
  - **Skills**: None required

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 3 (sequential, after all others)
  - **Blocks**: None (final task)
  - **Blocked By**: Tasks 1-10

  **References**:
  - `src/lib.rs:34-43` - Public re-exports (types to test)
  - Rust std library pattern for Send/Sync assertions

  **Acceptance Criteria**:
  - [ ] `src/lib.rs` contains send_sync_tests module
  - [ ] `cargo test send_sync` passes (2 new tests)
  - [ ] `cargo test --all-features` passes (204 total tests)

  **Commit**: YES
  - Message: `test(lib): add compile-time Send/Sync assertions for public types`
  - Files: `src/lib.rs`
  - Pre-commit: `cargo test --all-features`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `refactor(error): add #[non_exhaustive]` | error.rs | `cargo test` |
| 2 | `refactor(message): add #[must_use], #[non_exhaustive], const fn` | message.rs | `cargo test` |
| 3 | `style(codec): add digit separators, #[must_use]` | framed.rs | `cargo test` |
| Wave 1 Complete | Verify: `cargo test && cargo clippy` | - | All passing |
| 4 | `docs(config): add #[must_use] and # Errors` | config.rs | `cargo test` |
| 5 | `docs(frame): add #[must_use] and # Errors` | frame.rs | `cargo test` |
| 6 | `refactor(opcode): add #[must_use]` | opcode.rs | `cargo test` |
| 7 | `docs(handshake): add # Errors` | handshake.rs | `cargo test` |
| 8 | `refactor(role): add #[must_use]` | role.rs | `cargo test` |
| 9 | `refactor(state): add #[must_use], #[non_exhaustive]` | state.rs | `cargo test` |
| 10 | `docs(extensions): add # Errors` | mod.rs | `cargo test` |
| Wave 2 Complete | Verify: `cargo test && cargo clippy && cargo doc` | - | All passing |
| 11 | `test(lib): add Send/Sync assertions` | lib.rs | `cargo test` |
| Final | Verify: `cargo clippy --all-features -- -W clippy::pedantic` | - | No targeted warnings |

---

## Success Criteria

### Verification Commands
```bash
# Full test suite (expect 204 tests after adding Send/Sync tests)
cargo test --all-features

# Clippy with pedantic lints (should show no warnings for targeted lints)
cargo clippy --all-features -- -W clippy::pedantic -W clippy::nursery

# Documentation builds clean
cargo doc --all-features --no-deps

# Specific lint checks
cargo clippy --all-features 2>&1 | grep -c "must_use"  # Should be 0
cargo clippy --all-features 2>&1 | grep -c "missing_errors_doc"  # Should be 0
cargo clippy --all-features 2>&1 | grep -c "unreadable_literal"  # Should be 0
```

### Final Checklist
- [x] All "Must Have" items present (verified by cargo clippy)
- [x] All "Must NOT Have" items absent (no breaking changes)
- [x] All 202+ tests pass
- [x] cargo doc builds without warnings
- [x] No Clippy warnings for targeted lints
