//! WebSocket connection management and state machine.
//!
//! This module provides the core `Connection` type for managing WebSocket
//! connections, including message sending/receiving, state management, and
//! the protocol state machine.
//!
//! ## Connection Lifecycle
//!
//! 1. **Open** - Initial state after successful handshake
//! 2. **Closing** - Close frame sent, waiting for peer close
//! 3. **Closed** - Connection fully closed
//!
//! ## Example
//!
//! ```rust,ignore
//! use rsws::{Connection, Config, Role};
//!
//! let stream = tokio::net::TcpStream::connect("example.com:80").await?;
//! let config = Config::client();
//! let mut conn = Connection::new(stream, Role::Client, config);
//!
//! conn.send(Message::text("Hello")).await?;
//! if let Some(msg) = conn.recv().await? {
//!     println!("Received: {:?}", msg);
//! }
//! conn.close(CloseCode::Normal, "done").await?;
//! ```

mod role;
mod state;

pub use role::Role;
pub use state::ConnectionState;

#[cfg(feature = "async-tokio")]
#[allow(clippy::module_inception)]
mod connection;

#[cfg(feature = "async-tokio")]
pub use connection::Connection;
