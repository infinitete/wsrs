//! WebSocket protocol core implementation (RFC 6455).

pub mod assembler;
pub mod frame;
pub mod handshake;
pub mod mask;
pub mod opcode;
pub mod utf8;
pub mod validation;

pub use assembler::{AssembledMessage, MessageAssembler};
pub use frame::Frame;
pub use handshake::{compute_accept_key, HandshakeRequest, HandshakeResponse, WS_GUID};
pub use mask::{apply_mask, apply_mask_fast};
pub use opcode::OpCode;
pub use utf8::{validate_utf8, Utf8Validator};
pub use validation::FrameValidator;
