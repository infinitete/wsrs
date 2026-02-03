//! WebSocket test server for concurrency testing.
//!
//! Provides a TestServer implementation that can spawn echo servers on random ports.

use std::sync::Arc;
use tokio::net::TcpListener;
