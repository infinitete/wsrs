//! Test harness utilities for high-concurrency WebSocket testing.
//!
//! This module provides reusable components for stress testing and
//! concurrency validation of the rsws WebSocket library.

mod client;
mod metrics;
mod server;

pub use client::TestClient;
pub use metrics::{Latencies, Metrics};
pub use server::TestServer;
