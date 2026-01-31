//! Message fragmentation and reassembly for WebSocket (RFC 6455).

use bytes::BytesMut;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::protocol::utf8::Utf8Validator;
use crate::protocol::{Frame, OpCode};

/// Reassembles fragmented WebSocket messages.
pub struct MessageAssembler {
    buffer: BytesMut,
    fragment_count: usize,
    opcode: Option<OpCode>,
    total_size: usize,
    utf8_validator: Option<Utf8Validator>,
    config: Config,
}

impl MessageAssembler {
    pub fn new(config: Config) -> Self {
        Self {
            buffer: BytesMut::new(),
            fragment_count: 0,
            opcode: None,
            total_size: 0,
            utf8_validator: None,
            config,
        }
    }

    /// Add a frame to the message being assembled.
    /// Returns Some(complete_message) when FIN=1, None otherwise.
    pub fn push(&mut self, frame: Frame) -> Result<Option<AssembledMessage>> {
        if frame.opcode.is_control() {
            return Ok(None);
        }

        if frame.opcode == OpCode::Continuation {
            if self.opcode.is_none() {
                return Err(Error::ProtocolViolation(
                    "Unexpected continuation frame".into(),
                ));
            }
        } else {
            if self.opcode.is_some() {
                return Err(Error::ProtocolViolation(
                    "Expected continuation frame".into(),
                ));
            }
            self.opcode = Some(frame.opcode);

            if frame.opcode == OpCode::Text {
                self.utf8_validator = Some(Utf8Validator::new());
            }
        }

        self.config
            .limits
            .check_fragment_count(self.fragment_count + 1)?;

        let new_size = self.total_size + frame.payload().len();
        self.config.limits.check_message_size(new_size)?;

        if let Some(ref mut validator) = self.utf8_validator {
            validator.validate(frame.payload(), frame.fin)?;
        }

        self.total_size = new_size;
        self.buffer.extend_from_slice(frame.payload());
        self.fragment_count += 1;

        if frame.fin {
            let payload = self.buffer.split().freeze().to_vec();
            let opcode = self.opcode.take().unwrap();
            self.total_size = 0;
            self.fragment_count = 0;
            self.utf8_validator = None;
            Ok(Some(AssembledMessage { opcode, payload }))
        } else {
            Ok(None)
        }
    }

    pub fn is_assembling(&self) -> bool {
        self.opcode.is_some()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.fragment_count = 0;
        self.opcode = None;
        self.total_size = 0;
        self.utf8_validator = None;
    }
}

/// A fully assembled WebSocket message.
pub struct AssembledMessage {
    pub opcode: OpCode,
    pub payload: Vec<u8>,
}

impl AssembledMessage {
    pub fn into_text(self) -> Result<String> {
        String::from_utf8(self.payload).map_err(|_| Error::InvalidUtf8)
    }

    pub fn into_binary(self) -> Vec<u8> {
        self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Limits;

    fn test_config() -> Config {
        Config::new()
    }

    fn small_limits_config() -> Config {
        Config::new().with_limits(Limits::new(1024, 100, 3, 4096))
    }

    #[test]
    fn test_single_frame_message() {
        let mut assembler = MessageAssembler::new(test_config());
        let frame = Frame::text(b"Hello".to_vec());

        let result = assembler.push(frame).unwrap();
        assert!(result.is_some());

        let msg = result.unwrap();
        assert_eq!(msg.opcode, OpCode::Text);
        assert_eq!(msg.payload, b"Hello");
        assert!(!assembler.is_assembling());
    }

    #[test]
    fn test_two_fragment_message() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Text, b"Hel".to_vec());
        assert!(assembler.push(frame1).unwrap().is_none());
        assert!(assembler.is_assembling());

        let frame2 = Frame::new(true, OpCode::Continuation, b"lo".to_vec());
        let result = assembler.push(frame2).unwrap();
        assert!(result.is_some());

        let msg = result.unwrap();
        assert_eq!(msg.opcode, OpCode::Text);
        assert_eq!(msg.payload, b"Hello");
    }

    #[test]
    fn test_many_fragments() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Binary, vec![1, 2]);
        assert!(assembler.push(frame1).unwrap().is_none());

        let frame2 = Frame::new(false, OpCode::Continuation, vec![3, 4]);
        assert!(assembler.push(frame2).unwrap().is_none());

        let frame3 = Frame::new(false, OpCode::Continuation, vec![5, 6]);
        assert!(assembler.push(frame3).unwrap().is_none());

        let frame4 = Frame::new(true, OpCode::Continuation, vec![7, 8]);
        let result = assembler.push(frame4).unwrap();

        let msg = result.unwrap();
        assert_eq!(msg.opcode, OpCode::Binary);
        assert_eq!(msg.payload, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn test_interleaved_control_frame() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Text, b"Hel".to_vec());
        assert!(assembler.push(frame1).unwrap().is_none());

        let ping = Frame::ping(b"ping".to_vec());
        assert!(assembler.push(ping).unwrap().is_none());
        assert!(assembler.is_assembling());

        let frame2 = Frame::new(true, OpCode::Continuation, b"lo".to_vec());
        let result = assembler.push(frame2).unwrap();

        let msg = result.unwrap();
        assert_eq!(msg.payload, b"Hello");
    }

    #[test]
    fn test_max_message_size_exceeded() {
        let mut assembler = MessageAssembler::new(small_limits_config());

        let frame = Frame::text(vec![0u8; 150]);
        let result = assembler.push(frame);

        assert!(matches!(result, Err(Error::MessageTooLarge { .. })));
    }

    #[test]
    fn test_max_fragment_count_exceeded() {
        let mut assembler = MessageAssembler::new(small_limits_config());

        let f1 = Frame::new(false, OpCode::Binary, vec![1]);
        let f2 = Frame::new(false, OpCode::Continuation, vec![2]);
        let f3 = Frame::new(false, OpCode::Continuation, vec![3]);
        let f4 = Frame::new(true, OpCode::Continuation, vec![4]);

        assert!(assembler.push(f1).is_ok());
        assert!(assembler.push(f2).is_ok());
        assert!(assembler.push(f3).is_ok());

        let result = assembler.push(f4);
        assert!(matches!(result, Err(Error::TooManyFragments { .. })));
    }

    #[test]
    fn test_continuation_without_start_fails() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame = Frame::new(true, OpCode::Continuation, b"data".to_vec());
        let result = assembler.push(frame);

        assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    }

    #[test]
    fn test_text_message_utf8_validation() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Text, vec![0xf0, 0x9f]);
        assert!(assembler.push(frame1).is_ok());

        let frame2 = Frame::new(true, OpCode::Continuation, vec![0x8e, 0x89]);
        let result = assembler.push(frame2).unwrap();

        let msg = result.unwrap();
        assert_eq!(msg.into_text().unwrap(), "🎉");
    }

    #[test]
    fn test_text_message_invalid_utf8_fails() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame = Frame::new(true, OpCode::Text, vec![0x80, 0x81]);
        let result = assembler.push(frame);

        assert!(matches!(result, Err(Error::InvalidUtf8)));
    }

    #[test]
    fn test_binary_message_no_utf8_validation() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame = Frame::new(true, OpCode::Binary, vec![0x80, 0x81, 0xff]);
        let result = assembler.push(frame).unwrap();

        let msg = result.unwrap();
        assert_eq!(msg.opcode, OpCode::Binary);
        assert_eq!(msg.into_binary(), vec![0x80, 0x81, 0xff]);
    }

    #[test]
    fn test_assembled_message_into_text() {
        let msg = AssembledMessage {
            opcode: OpCode::Text,
            payload: b"Hello".to_vec(),
        };
        assert_eq!(msg.into_text().unwrap(), "Hello");
    }

    #[test]
    fn test_reset() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Text, b"partial".to_vec());
        assembler.push(frame1).unwrap();
        assert!(assembler.is_assembling());

        assembler.reset();
        assert!(!assembler.is_assembling());

        let frame2 = Frame::text(b"fresh".to_vec());
        let result = assembler.push(frame2).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_new_message_without_continuation_fails() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Text, b"first".to_vec());
        assembler.push(frame1).unwrap();

        let frame2 = Frame::new(true, OpCode::Text, b"second".to_vec());
        let result = assembler.push(frame2);

        assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    }

    #[test]
    fn test_reassembly_single_allocation() {
        let mut assembler = MessageAssembler::new(test_config());

        let frame1 = Frame::new(false, OpCode::Binary, vec![1, 2, 3, 4]);
        assert!(assembler.push(frame1).unwrap().is_none());

        let frame2 = Frame::new(false, OpCode::Continuation, vec![5, 6, 7, 8]);
        assert!(assembler.push(frame2).unwrap().is_none());

        let frame3 = Frame::new(true, OpCode::Continuation, vec![9, 10, 11, 12]);
        let result = assembler.push(frame3).unwrap();

        let msg = result.unwrap();
        assert_eq!(msg.opcode, OpCode::Binary);
        assert_eq!(msg.payload, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);

        assert!(!assembler.is_assembling());
    }
}
