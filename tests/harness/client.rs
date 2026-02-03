//! WebSocket test client for concurrency testing.
//!
//! Provides a TestClient implementation for connecting and handshaking.

use std::sync::Arc;
use tokio::net::TcpStream;
