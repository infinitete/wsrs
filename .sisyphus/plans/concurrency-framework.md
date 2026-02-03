# High-Concurrency Verification Framework for rsws

## TL;DR

> **Quick Summary**: Implement a comprehensive testing framework for stress-testing the rsws WebSocket library with 1000-10000 concurrent clients, validating RFC 6455 compliance under high load, and measuring throughput/latency metrics.
> 
> **Deliverables**:
> - `tests/harness/` - Reusable test server, client factory, metrics utilities
> - `tests/concurrency.rs` - Barrier-synchronized multi-client protocol tests
> - `tests/stress.rs` - High-load throughput/latency benchmarks
> 
> **Estimated Effort**: Large (10 tasks, ~8-12 hours)
> **Parallel Execution**: YES - 4 waves
> **Critical Path**: Task 1 → Task 2/3 → Task 5/6 → Task 9

---

## Context

### Original Request
Implement a high-concurrency verification framework for the rsws WebSocket library with:
1. Load Testing - 1000-10000 concurrent WebSocket client connections
2. Protocol Correctness Verification - RFC 6455 compliance under high concurrency
3. Race Condition Detection - Data race and deadlock detection

### Interview Summary
**Key Discussions**:
- User provided comprehensive requirements with specific deliverables
- Existing test infrastructure: proptest, tokio::test, criterion benchmarks
- MockStream pattern already exists in `connection.rs` and `framed.rs`
- Connection<T> uses `&mut self` (single-owner, no internal Mutex/channels)

**Research Findings**:
- JoinSet + Semaphore pattern for bounded concurrent task spawning
- tokio::sync::Barrier for synchronized test starts
- AtomicUsize for lock-free metrics collection
- loom crate available but marked optional by user
- Existing TLS integration tests show spawn server + client connect pattern

### Gap Analysis
**Identified Gaps** (addressed):
- Port allocation: Use OS-assigned (port 0) per `tls_integration.rs` pattern
- Handshake in test clients: Extract from `examples/client.rs` pattern
- Client count configurability: Env var `RSWS_STRESS_CLIENTS` with default 1000
- Loom integration: Deferred to future work (user marked optional)

---

## Work Objectives

### Core Objective
Create a reusable, pure-Rust testing framework that validates rsws protocol correctness and performance under high-concurrency conditions (1000-10000 simultaneous WebSocket connections).

### Concrete Deliverables
- `tests/harness/mod.rs` - Module root with re-exports
- `tests/harness/server.rs` - TestServer utility (spawn, shutdown, port allocation)
- `tests/harness/client.rs` - TestClient factory (connect, handshake, send/recv)
- `tests/harness/metrics.rs` - Metrics collector (AtomicUsize counters, latency tracking)
- `tests/concurrency.rs` - Multi-client protocol correctness tests
- `tests/stress.rs` - High-load throughput/latency benchmarks

### Definition of Done
- [x] `cargo test` passes with no failures
- [x] `cargo test -- --ignored` runs stress tests successfully
- [x] 1000 concurrent clients can connect, exchange messages, and disconnect
- [x] Throughput and latency metrics (p50/p95/p99) are captured and reported
- [x] No data races detected under high concurrency

### Must Have
- Barrier-synchronized simultaneous connection tests
- Message ordering validation with sequence numbers
- Configurable client count via `RSWS_STRESS_CLIENTS` env var
- All heavy tests marked `#[ignore]`
- Multi-threaded tokio runtime for concurrency tests

### Must NOT Have (Guardrails)
- **NO modifications to `src/`** - Only add test files
- **NO external dependencies** - Use only existing dev-deps (tokio, proptest) + std
- **NO blocking operations in async** - All I/O via tokio async
- **NO hardcoded ports** - Always use port 0 for OS allocation
- **NO loom integration** - Deferred to future work (optional per user)
- **NO TLS stress testing** - Use existing `tls_integration.rs` patterns if needed

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (tokio test-util, proptest in Cargo.toml)
- **User wants tests**: Tests-after (harness utilities verified by stress tests using them)
- **Framework**: tokio::test with multi_thread flavor

### Automated Verification (NO User Intervention)

Each TODO includes EXECUTABLE verification procedures:

| Type | Verification Tool | Automated Procedure |
|------|------------------|---------------------|
| **Test harness** | `cargo test` | Run specific test, assert exit code 0 |
| **Concurrency tests** | `cargo test concurrency` | Run test suite, verify pass count |
| **Stress tests** | `cargo test --ignored` | Run ignored tests, capture metrics output |

**Evidence Requirements (Agent-Executable):**
- Command output captured and compared against expected patterns
- Test pass/fail counts verified
- Metrics output validated for expected fields

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately - Foundation):
├── Task 1: Create tests/harness/ module structure
├── Task 2: Implement TestServer utility
└── Task 3: Implement TestClient factory

Wave 2 (After Wave 1 - Core Utilities):
├── Task 4: Implement Metrics collector
├── Task 5: Create tests/concurrency.rs skeleton
└── Task 6: Create tests/stress.rs skeleton

Wave 3 (After Wave 2 - Full Tests):
├── Task 7: Barrier-synchronized connection tests
├── Task 8: Message ordering validation tests
└── Task 9: Throughput/latency stress tests

Wave 4 (Final - Polish):
└── Task 10: Protocol correctness under load + close handshake tests

Critical Path: Task 1 → Task 2 → Task 5 → Task 9
Parallel Speedup: ~40% faster than sequential
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|---------------------|
| 1 | None | 2, 3, 4 | None |
| 2 | 1 | 5, 6, 7, 8, 9, 10 | 3, 4 |
| 3 | 1 | 5, 6, 7, 8, 9, 10 | 2, 4 |
| 4 | 1 | 9 | 2, 3 |
| 5 | 2, 3 | 7, 8 | 6 |
| 6 | 2, 3, 4 | 9 | 5 |
| 7 | 5 | 10 | 8 |
| 8 | 5 | 10 | 7 |
| 9 | 6 | 10 | 7, 8 |
| 10 | 7, 8, 9 | None | None (final) |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Dispatch |
|------|-------|---------------------|
| 1 | 1, 2, 3 | Task 1 first (creates dirs), then 2+3 parallel |
| 2 | 4, 5, 6 | All parallel after Wave 1 |
| 3 | 7, 8, 9 | All parallel after Wave 2 |
| 4 | 10 | Sequential (final integration) |

---

## TODOs

- [x] 1. Create tests/harness/ module structure

  **What to do**:
  - Create `tests/harness/mod.rs` with submodule declarations
  - Create empty `tests/harness/server.rs`, `client.rs`, `metrics.rs` files
  - Add common imports and re-exports in `mod.rs`

  **Must NOT do**:
  - Do not implement any logic yet - just structure
  - Do not modify any existing files

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Simple file creation, no complex logic
  - **Skills**: [`git-master`]
    - `git-master`: For atomic commit of new test structure

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 1 (first)
  - **Blocks**: Tasks 2, 3, 4
  - **Blocked By**: None (can start immediately)

  **References**:
  - `tests/tls_integration.rs:1-10` - Import patterns for integration tests
  - `tests/property.rs:1-6` - Module-level documentation style

  **Acceptance Criteria**:
  ```bash
  # Agent runs:
  test -f tests/harness/mod.rs && test -f tests/harness/server.rs && \
  test -f tests/harness/client.rs && test -f tests/harness/metrics.rs
  # Assert: Exit code 0 (all files exist)
  
  cargo check --tests 2>&1 | grep -v "warning"
  # Assert: No errors (warnings OK for empty modules)
  ```

  **Commit**: YES
  - Message: `test: add harness module structure for concurrency tests`
  - Files: `tests/harness/mod.rs`, `tests/harness/server.rs`, `tests/harness/client.rs`, `tests/harness/metrics.rs`

---

- [x] 2. Implement TestServer utility

  **What to do**:
  - Create `TestServer` struct with `spawn() -> (TestServer, SocketAddr)` method
  - Use `TcpListener::bind("127.0.0.1:0")` for OS-assigned port
  - Implement echo server loop based on `examples/echo_server.rs`
  - Add `shutdown()` method using `tokio::sync::oneshot` channel
  - Server runs in spawned task, handles multiple connections

  **Must NOT do**:
  - Do not hardcode ports
  - Do not block on accept (use async)
  - Do not panic on connection errors (log and continue)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Async server implementation requires careful design
  - **Skills**: None needed
    - No browser/git operations

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 3, 4)
  - **Blocks**: Tasks 5, 6, 7, 8, 9, 10
  - **Blocked By**: Task 1

  **References**:
  - `examples/echo_server.rs:13-29` - Server loop pattern with TcpListener
  - `examples/echo_server.rs:31-99` - Connection handling with handshake
  - `tests/tls_integration.rs:43-62` - spawn pattern with TcpListener::bind("127.0.0.1:0")
  - `src/connection/connection.rs:56-64` - Connection::new() API

  **Acceptance Criteria**:
  ```bash
  # Agent creates a minimal test in tests/harness/server.rs:
  # #[tokio::test]
  # async fn test_server_spawn_and_shutdown() {
  #     let (server, addr) = TestServer::spawn().await;
  #     assert!(addr.port() > 0);
  #     server.shutdown().await;
  # }
  
  cargo test --test harness server::test_server_spawn -- --nocapture
  # Assert: Test passes, output shows port binding
  ```

  **Commit**: YES
  - Message: `test(harness): implement TestServer with spawn/shutdown`
  - Files: `tests/harness/server.rs`, `tests/harness/mod.rs`
  - Pre-commit: `cargo test --test harness server`

---

- [x] 3. Implement TestClient factory

  **What to do**:
  - Create `TestClient` struct wrapping `Connection<TcpStream>`
  - Implement `connect(addr: SocketAddr) -> Result<TestClient>` factory
  - Handle WebSocket handshake (HTTP upgrade) based on `examples/client.rs`
  - Add `send_text()`, `recv_text()`, `close()` convenience methods
  - Add `with_id(id: usize)` for client identification in tests

  **Must NOT do**:
  - Do not implement retry logic yet (simple connect)
  - Do not add TLS support (plain TCP only)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Async client with handshake requires protocol knowledge
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 4)
  - **Blocks**: Tasks 5, 6, 7, 8, 9, 10
  - **Blocked By**: Task 1

  **References**:
  - `examples/client.rs:17-53` - TCP connect and HTTP upgrade handshake
  - `examples/client.rs:55-86` - Connection usage pattern (send/recv/close)
  - `examples/client.rs:89-104` - Base64 key generation for handshake
  - `src/connection/connection.rs:79-111` - send() API signature

  **Acceptance Criteria**:
  ```bash
  # Integration test with TestServer (after Task 2):
  # #[tokio::test]
  # async fn test_client_connect_and_echo() {
  #     let (server, addr) = TestServer::spawn().await;
  #     let mut client = TestClient::connect(addr).await.unwrap();
  #     client.send_text("hello").await.unwrap();
  #     let msg = client.recv_text().await.unwrap();
  #     assert_eq!(msg, "hello");
  #     client.close().await.unwrap();
  #     server.shutdown().await;
  # }
  
  cargo test --test harness client::test_client_connect -- --nocapture
  # Assert: Test passes, shows send/recv cycle
  ```

  **Commit**: YES
  - Message: `test(harness): implement TestClient with connect/send/recv`
  - Files: `tests/harness/client.rs`, `tests/harness/mod.rs`
  - Pre-commit: `cargo test --test harness client`

---

- [x] 4. Implement Metrics collector

  **What to do**:
  - Create `Metrics` struct with `AtomicUsize` counters:
    - `connections_total`, `connections_active`, `messages_sent`, `messages_received`, `errors`
  - Add `Latencies` struct with thread-safe latency collection (parking_lot Mutex or channel)
  - Implement `record_latency(Duration)`, `percentile(f64) -> Duration` methods
  - Add `report()` method that prints formatted metrics summary
  - Make `Metrics` cloneable via `Arc` wrapper

  **Must NOT do**:
  - Do not add external dependencies (use std atomics)
  - Do not use heavy locks on hot paths (AtomicUsize for counters)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Lock-free concurrent data structures
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3)
  - **Blocks**: Task 9
  - **Blocked By**: Task 1

  **References**:
  - Research finding: AtomicUsize pattern from Solana `streamer/src/quic.rs`
  - Research finding: Latency percentile calculation (sort + index)
  - `std::sync::atomic::{AtomicUsize, Ordering}` - Ordering::Relaxed for counters

  **Acceptance Criteria**:
  ```bash
  # Unit test in tests/harness/metrics.rs:
  # #[test]
  # fn test_metrics_concurrent_increment() {
  #     let metrics = Metrics::new();
  #     std::thread::scope(|s| {
  #         for _ in 0..100 {
  #             s.spawn(|| metrics.record_message_sent());
  #         }
  #     });
  #     assert_eq!(metrics.messages_sent(), 100);
  # }
  
  cargo test --test harness metrics::test_metrics -- --nocapture
  # Assert: Test passes, counter equals expected value
  ```

  **Commit**: YES
  - Message: `test(harness): implement Metrics with AtomicUsize counters`
  - Files: `tests/harness/metrics.rs`, `tests/harness/mod.rs`
  - Pre-commit: `cargo test --test harness metrics`

---

- [x] 5. Create tests/concurrency.rs skeleton

  **What to do**:
  - Create `tests/concurrency.rs` with module imports
  - Add `mod harness;` to import test utilities
  - Create placeholder tests with `#[tokio::test(flavor = "multi_thread")]`:
    - `test_multiple_clients_sequential` - 10 clients, one at a time
    - `test_multiple_clients_parallel` - 10 clients, concurrent
    - `test_barrier_synchronized_connect` - placeholder for Task 7
    - `test_message_ordering` - placeholder for Task 8
  - Implement the first two basic tests

  **Must NOT do**:
  - Do not implement barrier/ordering tests yet (Tasks 7, 8)
  - Do not use more than 10 clients in non-ignored tests

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Test implementation with async patterns
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 6)
  - **Blocks**: Tasks 7, 8
  - **Blocked By**: Tasks 2, 3

  **References**:
  - `tests/tls_integration.rs:43-79` - Async test structure with spawn/join
  - `tests/property.rs:34-58` - Proptest macro usage (for future property tests)
  - Research finding: `#[tokio::test(flavor = "multi_thread")]` for concurrency

  **Acceptance Criteria**:
  ```bash
  cargo test concurrency::test_multiple_clients_sequential -- --nocapture
  # Assert: Test passes, output shows 10 clients connected

  cargo test concurrency::test_multiple_clients_parallel -- --nocapture
  # Assert: Test passes, 10 clients complete successfully
  ```

  **Commit**: YES
  - Message: `test: add concurrency.rs with basic multi-client tests`
  - Files: `tests/concurrency.rs`
  - Pre-commit: `cargo test concurrency`

---

- [x] 6. Create tests/stress.rs skeleton

  **What to do**:
  - Create `tests/stress.rs` with module imports
  - Add `mod harness;` to import test utilities
  - Read `RSWS_STRESS_CLIENTS` env var with default 1000
  - Create placeholder tests with `#[ignore]` attribute:
    - `test_stress_connections` - many clients connect/disconnect
    - `test_stress_throughput` - placeholder for Task 9
    - `test_stress_latency` - placeholder for Task 9
  - Implement `test_stress_connections` with JoinSet + Semaphore pattern

  **Must NOT do**:
  - Do not remove `#[ignore]` from heavy tests
  - Do not implement throughput/latency collection yet (Task 9)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Complex async patterns with JoinSet/Semaphore
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 5)
  - **Blocks**: Task 9
  - **Blocked By**: Tasks 2, 3, 4

  **References**:
  - Research finding: JoinSet + Semaphore pattern for bounded concurrency
  - Research finding: `std::env::var("RSWS_STRESS_CLIENTS")` for configurability
  - `tokio::task::JoinSet` - Task collection with `spawn()` and `join_next()`
  - `tokio::sync::Semaphore` - Bounded concurrency control

  **Acceptance Criteria**:
  ```bash
  # Default (skipped):
  cargo test stress::test_stress_connections
  # Assert: Test is ignored (not run)

  # With --ignored flag:
  RSWS_STRESS_CLIENTS=100 cargo test stress::test_stress_connections -- --ignored --nocapture
  # Assert: Test runs, output shows "100 clients connected successfully"
  ```

  **Commit**: YES
  - Message: `test: add stress.rs with configurable load testing`
  - Files: `tests/stress.rs`
  - Pre-commit: `cargo test stress`

---

- [x] 7. Barrier-synchronized connection tests

  **What to do**:
  - Implement `test_barrier_synchronized_connect` in `tests/concurrency.rs`
  - Use `tokio::sync::Barrier` to synchronize all clients before connecting
  - Verify all clients connect within a tight time window
  - Test with 50 clients (non-ignored) and 500 clients (ignored)
  - Add `test_barrier_synchronized_send` - all clients send simultaneously

  **Must NOT do**:
  - Do not use more than 50 clients in non-ignored tests
  - Do not hardcode barrier count (derive from client count)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Barrier synchronization patterns
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 8, 9)
  - **Blocks**: Task 10
  - **Blocked By**: Task 5

  **References**:
  - Research finding: Barrier usage from Cloudflare Pingora benchmarks
  - Research finding: `Barrier::new(num_clients)` + `barrier.wait().await`
  - `tokio::sync::Barrier` - Synchronization primitive

  **Acceptance Criteria**:
  ```bash
  cargo test concurrency::test_barrier_synchronized_connect -- --nocapture
  # Assert: Test passes, output shows "50 clients synchronized"

  cargo test concurrency::test_barrier_synchronized_send -- --nocapture
  # Assert: Test passes, all messages received
  ```

  **Commit**: YES
  - Message: `test(concurrency): add barrier-synchronized connection tests`
  - Files: `tests/concurrency.rs`
  - Pre-commit: `cargo test concurrency::test_barrier`

---

- [x] 8. Message ordering validation tests

  **What to do**:
  - Implement `test_message_ordering` in `tests/concurrency.rs`
  - Each client sends messages with embedded sequence numbers: `"client:{id}:msg:{seq}"`
  - Verify received messages maintain order per client
  - Test with 20 clients, 50 messages each (non-ignored)
  - Add `test_message_ordering_under_load` with 100 clients, 100 messages (ignored)

  **Must NOT do**:
  - Do not expect global ordering across clients (only per-client)
  - Do not use binary messages (text with parseable format)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Protocol validation logic
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 7, 9)
  - **Blocks**: Task 10
  - **Blocked By**: Task 5

  **References**:
  - `examples/echo_server.rs:64-70` - Echo server returns same message
  - `src/connection/connection.rs:164-219` - recv() returns messages in order
  - RFC 6455 Section 5.4 - Fragmentation preserves message order

  **Acceptance Criteria**:
  ```bash
  cargo test concurrency::test_message_ordering -- --nocapture
  # Assert: Test passes, output shows "All 1000 messages in order"

  cargo test concurrency::test_message_ordering_under_load -- --ignored --nocapture
  # Assert: Test passes, "10000 messages validated"
  ```

  **Commit**: YES
  - Message: `test(concurrency): add message ordering validation`
  - Files: `tests/concurrency.rs`
  - Pre-commit: `cargo test concurrency::test_message_ordering`

---

- [x] 9. Throughput/latency stress tests

  **What to do**:
  - Implement `test_stress_throughput` in `tests/stress.rs`
  - Measure messages/second across all clients
  - Implement `test_stress_latency` with percentile calculations (p50, p95, p99)
  - Use Metrics struct from harness for data collection
  - Print formatted report at end of test
  - All tests marked `#[ignore]`

  **Must NOT do**:
  - Do not fail on specific throughput targets (just measure)
  - Do not block main test thread for metrics collection

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Performance measurement with concurrent collection
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 7, 8)
  - **Blocks**: Task 10
  - **Blocked By**: Task 6

  **References**:
  - Research finding: Latency percentile calculation (sort + index)
  - Research finding: Criterion async benchmark patterns
  - `tests/harness/metrics.rs` - Metrics struct from Task 4
  - `std::time::Instant` - High-resolution timing

  **Acceptance Criteria**:
  ```bash
  RSWS_STRESS_CLIENTS=500 cargo test stress::test_stress_throughput -- --ignored --nocapture
  # Assert: Output includes "Throughput: X.XX msg/sec"

  RSWS_STRESS_CLIENTS=500 cargo test stress::test_stress_latency -- --ignored --nocapture
  # Assert: Output includes "P50:", "P95:", "P99:" latency values
  ```

  **Commit**: YES
  - Message: `test(stress): implement throughput and latency measurements`
  - Files: `tests/stress.rs`
  - Pre-commit: `cargo test stress -- --ignored`

---

- [x] 10. Protocol correctness under load + close handshake tests

  **What to do**:
  - Add `test_close_handshake_under_load` in `tests/concurrency.rs`
  - Verify proper close handshake (code 1000) with 50 concurrent clients
  - Add `test_ping_pong_under_load` - verify ping/pong responses
  - Add `test_fragmented_messages_concurrent` - large messages with fragmentation
  - Final integration test combining all patterns

  **Must NOT do**:
  - Do not test invalid protocol scenarios (that's for property tests)
  - Do not modify echo server behavior

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: Protocol compliance validation
  - **Skills**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 4 (final)
  - **Blocks**: None (final task)
  - **Blocked By**: Tasks 7, 8, 9

  **References**:
  - `src/connection/connection.rs:247-257` - close() implementation
  - `src/connection/state.rs:1-47` - ConnectionState machine
  - `examples/echo_server.rs:72-85` - Close frame handling
  - RFC 6455 Section 5.5.1 - Close frame requirements

  **Acceptance Criteria**:
  ```bash
  cargo test concurrency::test_close_handshake_under_load -- --nocapture
  # Assert: Test passes, "50 clients closed gracefully"

  cargo test concurrency::test_ping_pong_under_load -- --nocapture
  # Assert: Test passes, all pings received pongs

  # Full test suite:
  cargo test concurrency && cargo test stress
  # Assert: All non-ignored tests pass
  ```

  **Commit**: YES
  - Message: `test(concurrency): add protocol correctness tests under load`
  - Files: `tests/concurrency.rs`
  - Pre-commit: `cargo test concurrency`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `test: add harness module structure` | tests/harness/*.rs | cargo check |
| 2 | `test(harness): implement TestServer` | tests/harness/server.rs | cargo test harness::server |
| 3 | `test(harness): implement TestClient` | tests/harness/client.rs | cargo test harness::client |
| 4 | `test(harness): implement Metrics` | tests/harness/metrics.rs | cargo test harness::metrics |
| 5 | `test: add concurrency.rs skeleton` | tests/concurrency.rs | cargo test concurrency |
| 6 | `test: add stress.rs skeleton` | tests/stress.rs | cargo test stress |
| 7 | `test(concurrency): barrier-sync tests` | tests/concurrency.rs | cargo test barrier |
| 8 | `test(concurrency): message ordering` | tests/concurrency.rs | cargo test ordering |
| 9 | `test(stress): throughput/latency` | tests/stress.rs | cargo test --ignored |
| 10 | `test(concurrency): protocol correctness` | tests/concurrency.rs | cargo test concurrency |

---

## Success Criteria

### Verification Commands
```bash
# All unit tests pass
cargo test
# Expected: All tests pass (including new harness, concurrency tests)

# Stress tests run successfully
RSWS_STRESS_CLIENTS=1000 cargo test -- --ignored
# Expected: All ignored tests pass, metrics output shown

# No compiler warnings in new code
cargo clippy --tests -- -D warnings
# Expected: No warnings

# Code formatted correctly
cargo fmt -- --check
# Expected: No formatting issues
```

### Final Checklist
- [x] All "Must Have" features implemented
- [x] All "Must NOT Have" guardrails respected
- [x] 1000+ concurrent clients tested successfully
- [x] Throughput and latency metrics captured
- [x] All tests pass with `cargo test`
- [x] Heavy tests properly marked `#[ignore]`
