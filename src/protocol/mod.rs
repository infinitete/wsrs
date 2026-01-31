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
pub use handshake::{HandshakeRequest, HandshakeResponse, WS_GUID, compute_accept_key};
pub use mask::{apply_mask, apply_mask_fast};
pub use opcode::OpCode;
pub use utf8::{Utf8Validator, validate_utf8};
pub use validation::FrameValidator;
