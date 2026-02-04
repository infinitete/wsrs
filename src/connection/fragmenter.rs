//! Message fragmentation for outgoing WebSocket messages (RFC 6455).

use crate::protocol::{Frame, OpCode};

/// Iterator that produces frames from a message payload.
///
/// Splits large payloads into multiple frames according to the configured
/// fragment size. First frame uses the original opcode, continuation frames
/// use `OpCode::Continuation`.
pub struct MessageFragmenter<'a> {
    payload: &'a [u8],
    opcode: OpCode,
    fragment_size: usize,
    offset: usize,
    is_first: bool,
}

impl<'a> MessageFragmenter<'a> {
    /// Create a new fragmenter for the given payload.
    #[inline]
    #[must_use]
    pub fn new(payload: &'a [u8], opcode: OpCode, fragment_size: usize) -> Self {
        Self {
            payload,
            opcode,
            fragment_size: fragment_size.max(1), // Ensure at least 1 byte per fragment
            offset: 0,
            is_first: true,
        }
    }

    /// Check if fragmentation is needed (payload exceeds fragment_size).
    #[inline]
    #[must_use]
    pub fn needs_fragmentation(&self) -> bool {
        self.payload.len() > self.fragment_size
    }

    /// Get remaining bytes to send.
    #[inline]
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.payload.len().saturating_sub(self.offset)
    }
}

impl<'a> Iterator for MessageFragmenter<'a> {
    type Item = Frame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.payload.len() {
            // Handle empty payload case
            if self.is_first && self.payload.is_empty() {
                self.is_first = false;
                return Some(Frame::new(true, self.opcode, Vec::new()));
            }
            return None;
        }

        let remaining = self.payload.len() - self.offset;
        let chunk_size = remaining.min(self.fragment_size);
        let is_final = self.offset + chunk_size >= self.payload.len();

        let chunk = self.payload[self.offset..self.offset + chunk_size].to_vec();
        self.offset += chunk_size;

        let opcode = if self.is_first {
            self.is_first = false;
            self.opcode
        } else {
            OpCode::Continuation
        };

        Some(Frame::new(is_final, opcode, chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_fragmentation_needed() {
        let payload = b"Hello";
        let frag = MessageFragmenter::new(payload, OpCode::Text, 1024);

        assert!(!frag.needs_fragmentation());

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(frames[0].payload(), b"Hello");
    }

    #[test]
    fn test_exact_fragmentation() {
        let payload = vec![0xAB; 30];
        let frag = MessageFragmenter::new(&payload, OpCode::Binary, 10);

        assert!(frag.needs_fragmentation());

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 3);

        // First frame
        assert!(!frames[0].fin);
        assert_eq!(frames[0].opcode, OpCode::Binary);
        assert_eq!(frames[0].payload().len(), 10);

        // Middle frame
        assert!(!frames[1].fin);
        assert_eq!(frames[1].opcode, OpCode::Continuation);
        assert_eq!(frames[1].payload().len(), 10);

        // Last frame
        assert!(frames[2].fin);
        assert_eq!(frames[2].opcode, OpCode::Continuation);
        assert_eq!(frames[2].payload().len(), 10);
    }

    #[test]
    fn test_uneven_fragmentation() {
        let payload = vec![0xCD; 25];
        let frag = MessageFragmenter::new(&payload, OpCode::Binary, 10);

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 3);

        assert_eq!(frames[0].payload().len(), 10);
        assert_eq!(frames[1].payload().len(), 10);
        assert_eq!(frames[2].payload().len(), 5);
        assert!(frames[2].fin);
    }

    #[test]
    fn test_empty_payload() {
        let payload = b"";
        let frag = MessageFragmenter::new(payload, OpCode::Text, 1024);

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].payload().len(), 0);
    }

    #[test]
    fn test_payload_equals_fragment_size() {
        let payload = vec![0xEF; 100];
        let frag = MessageFragmenter::new(&payload, OpCode::Binary, 100);

        assert!(!frag.needs_fragmentation());

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].fin);
        assert_eq!(frames[0].payload().len(), 100);
    }

    #[test]
    fn test_text_fragmentation() {
        let text = "A".repeat(25);
        let frag = MessageFragmenter::new(text.as_bytes(), OpCode::Text, 10);

        let frames: Vec<_> = frag.collect();
        assert_eq!(frames.len(), 3);

        assert_eq!(frames[0].opcode, OpCode::Text);
        assert_eq!(frames[1].opcode, OpCode::Continuation);
        assert_eq!(frames[2].opcode, OpCode::Continuation);
    }

    #[test]
    fn test_remaining_bytes() {
        let payload = vec![0xAB; 30];
        let mut frag = MessageFragmenter::new(&payload, OpCode::Binary, 10);

        assert_eq!(frag.remaining(), 30);
        frag.next();
        assert_eq!(frag.remaining(), 20);
        frag.next();
        assert_eq!(frag.remaining(), 10);
        frag.next();
        assert_eq!(frag.remaining(), 0);
    }
}
