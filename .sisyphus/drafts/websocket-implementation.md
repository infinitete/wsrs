# Draft: WebSocket Implementation Plan

## Requirements (confirmed)

### User Request
- Implement complete WebSocket protocol (RFC 6455)
- Production-grade quality
- Follow best practices from BEST_PRACTICE.md
- Zero-copy performance targets

### Oracle Architecture (provided)
**Modules:**
- protocol/ (frame.rs, opcode.rs, mask.rs, handshake.rs)
- connection/ (state.rs, role.rs)
- message.rs (high-level API)
- error.rs (thiserror-based)
- config.rs (limits)
- codec/ (async integration)
- extensions/ (extension framework)

**Performance Targets:**
- <50ns frame parsing
- >2GB/s masking throughput

**Timeline:**
- 4 phases, 8 weeks total

### Protocol Requirements (RFC 6455)
- Client MUST mask frames
- Server MUST NOT mask frames
- Control frames ≤125 bytes, no fragmentation
- Text frames MUST be valid UTF-8
- RSV bits = 0 (unless extension negotiated)

### Recommended Limits
- MAX_FRAME_SIZE: 16MB
- MAX_MESSAGE_SIZE: 64MB
- MAX_FRAGMENTS: 128

---

## Technical Decisions

**RESOLVED - Using Industry Best Practices:**

### Test Strategy ✅
- **ADOPTED**: TDD with Autobahn compliance testing
- **Rationale**: All production WebSocket libraries (tungstenite, ws-rs, tokio-tungstenite) use Autobahn. Non-negotiable for RFC 6455 compliance.
- **Implementation**: Phase 1 includes test infrastructure setup, all tasks follow RED-GREEN-REFACTOR

### Phase Execution ✅
- **ADOPTED**: Maximum parallelization (Wave-based execution)
- **Rationale**: Performance-critical library benefits from concurrent development of independent modules
- **Implementation**: Identify parallel waves, optimize critical path

### Dependencies ✅
- **ADOPTED**: Best-practice dependencies with feature gating
- **Core**: `thiserror` (errors), `bytes` (zero-copy buffers)
- **Optional**: Runtime integration via features (`tokio`, `async-std`)
- **Rationale**: `bytes` is industry standard for zero-copy (used by Tokio, Hyper, etc.)

### Scope Boundaries ✅
- **Phase 4 Scope**: COMPLETE extension system
  - Full Extension trait system
  - permessage-deflate (RFC 7692) complete implementation
  - Custom extension support + examples
  - Extension negotiation mechanism
  - Extension composition and chaining
  - Zero-copy extension processing
- **TLS/WSS**: INCLUDED - Full integration
  - rustls + tokio-native-tls support (feature-gated)
  - wss:// client support
  - TLS server integration
  - Certificate validation and configuration
- **HTTP Integration**: INCLUDED - Complete HTTP/1.1 handshake
  - Full HTTP/1.1 upgrade parsing and validation
  - Request/response header handling
  - HTTPS server integration examples
- **WASM**: Deferred to future versions
- **Rationale**: Production-ready library with batteries included

---

## Research Findings

### From Librarian (RFC 6455)
- Handshake: Sec-WebSocket-Key + magic GUID → SHA-1 → base64
- Frame structure: 2-14 byte header + payload
- Security: Masking prevents cache poisoning attacks
- UTF-8 validation required for text frames (can defer to end of message)

### From Explore (Codebase)
- Current state: Minimal lib.rs (15 lines)
- Edition 2024 (requires Rust 1.93.0+)
- No dependencies
- Embedded unit tests pattern

### From Oracle (Architecture)
- State machine for connection lifecycle
- Trait-based codec abstraction (runtime-agnostic async)
- Buffer pooling for zero-allocation operation
- SIMD masking for performance

---

## Open Questions

1. Test infrastructure requirements?
2. Phase execution strategy?
3. Dependency philosophy?
4. Extension scope for v1.0?
5. TLS/HTTP integration boundaries?

---

## Scope Boundaries

**INCLUDE (tentative):**
- Full RFC 6455 core protocol
- Client + Server roles
- Async codec traits (runtime-agnostic)
- Production limits and validation
- Performance optimization (SIMD masking)

**EXCLUDE (tentative):**
- TLS termination (user responsibility)
- HTTP server implementation (user brings transport)
- WASM compilation (future consideration)
- Auto-reconnect logic (application concern)
