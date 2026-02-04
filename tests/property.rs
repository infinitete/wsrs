//! Property-based tests for WebSocket frame parsing.
//!
//! These tests use proptest to fuzz the frame parsing logic and find edge cases.

use proptest::prelude::*;
use rsws::protocol::{Frame, HandshakeRequest, OpCode, apply_mask};

/// Strategy for generating valid data frame opcodes.
fn data_opcode_strategy() -> impl Strategy<Value = OpCode> {
    prop_oneof![
        Just(OpCode::Text),
        Just(OpCode::Binary),
        Just(OpCode::Continuation),
    ]
}

fn control_opcode_strategy() -> impl Strategy<Value = OpCode> {
    prop_oneof![Just(OpCode::Close), Just(OpCode::Ping), Just(OpCode::Pong),]
}

fn any_opcode_strategy() -> impl Strategy<Value = OpCode> {
    prop_oneof![
        Just(OpCode::Continuation),
        Just(OpCode::Text),
        Just(OpCode::Binary),
        Just(OpCode::Close),
        Just(OpCode::Ping),
        Just(OpCode::Pong),
    ]
}

proptest! {
    // =========================================================================
    // Property 1: Roundtrip - parse(write(frame)) == frame (unmasked)
    // =========================================================================
    #[test]
    fn test_roundtrip_unmasked(
        fin in any::<bool>(),
        opcode in data_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 0..1000)
    ) {
        let frame = Frame::new(fin, opcode, payload.clone());
        let mut buf = vec![0u8; frame.wire_size(false)];
        let written = frame.write(&mut buf, None);
        prop_assert!(written.is_ok(), "write failed: {:?}", written);
        let written = written.unwrap();

        let parsed = Frame::parse(&buf[..written]);
        prop_assert!(parsed.is_ok(), "parse failed: {:?}", parsed);
        let (parsed, consumed) = parsed.unwrap();

        prop_assert_eq!(consumed, written);
        prop_assert_eq!(frame.fin, parsed.fin);
        prop_assert_eq!(frame.opcode, parsed.opcode);
        prop_assert_eq!(frame.payload(), parsed.payload());
    }

    // =========================================================================
    // Property 2: Roundtrip with masking
    // =========================================================================
    #[test]
    fn test_roundtrip_masked(
        fin in any::<bool>(),
        opcode in data_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 0..500),
        mask in any::<[u8; 4]>()
    ) {
        let frame = Frame::new(fin, opcode, payload.clone());
        let mut buf = vec![0u8; frame.wire_size(true)];
        let written = frame.write(&mut buf, Some(mask));
        prop_assert!(written.is_ok(), "write failed: {:?}", written);
        let written = written.unwrap();

        let parsed = Frame::parse(&buf[..written]);
        prop_assert!(parsed.is_ok(), "parse failed: {:?}", parsed);
        let (parsed, _) = parsed.unwrap();

        // After parsing, payload should be unmasked and match original
        prop_assert_eq!(frame.payload(), parsed.payload());
        prop_assert_eq!(frame.fin, parsed.fin);
        prop_assert_eq!(frame.opcode, parsed.opcode);
    }

    // =========================================================================
    // Property 3: Masking is reversible (XOR is self-inverse)
    // =========================================================================
    #[test]
    fn test_mask_reversible(
        data in prop::collection::vec(any::<u8>(), 0..2000),
        mask in any::<[u8; 4]>()
    ) {
        let mut masked = data.clone();
        apply_mask(&mut masked, mask);
        apply_mask(&mut masked, mask);
        prop_assert_eq!(data, masked);
    }

    // =========================================================================
    // Property 4: Payload length encoding is correct for all sizes
    // =========================================================================
    #[test]
    fn test_payload_length_encoding(
        payload in prop::collection::vec(any::<u8>(), 0..70000)
    ) {
        let frame = Frame::new(true, OpCode::Binary, payload.clone());
        let mut buf = vec![0u8; frame.wire_size(false)];
        let written = frame.write(&mut buf, None);
        prop_assert!(written.is_ok(), "write failed: {:?}", written);
        let written = written.unwrap();

        let parsed = Frame::parse(&buf[..written]);
        prop_assert!(parsed.is_ok(), "parse failed: {:?}", parsed);
        let (parsed, consumed) = parsed.unwrap();

        prop_assert_eq!(consumed, written);
        prop_assert_eq!(parsed.payload().len(), payload.len());
    }

    // =========================================================================
    // Property 5: Control frames with valid payload size pass validation
    // =========================================================================
    #[test]
    fn test_control_frame_size_limit(
        opcode in control_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 0..=125)
    ) {
        let frame = Frame::new(true, opcode, payload);
        let result = frame.validate();
        prop_assert!(result.is_ok(), "validation failed for valid control frame: {:?}", result);
    }

    // =========================================================================
    // Property 6: Control frames exceeding 125 bytes fail validation
    // =========================================================================
    #[test]
    fn test_control_frame_exceeds_limit(
        opcode in control_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 126..256)
    ) {
        let frame = Frame::new(true, opcode, payload);
        let result = frame.validate();
        prop_assert!(result.is_err(), "validation should fail for oversized control frame");
    }

    // =========================================================================
    // Property 7: Wire size calculation matches actual written bytes
    // =========================================================================
    #[test]
    fn test_wire_size_accuracy(
        fin in any::<bool>(),
        opcode in any_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 0..10000),
        masked in any::<bool>()
    ) {
        let frame = Frame::new(fin, opcode, payload);
        let expected_size = frame.wire_size(masked);

        let mask = if masked { Some([0x12, 0x34, 0x56, 0x78]) } else { None };
        let mut buf = vec![0u8; expected_size + 100]; // Extra space
        let written = frame.write(&mut buf, mask);
        prop_assert!(written.is_ok(), "write failed: {:?}", written);
        let written = written.unwrap();

        prop_assert_eq!(expected_size, written, "wire_size() mismatch with actual written bytes");
    }

    // =========================================================================
    // Property 8: Parsing incomplete data returns IncompleteFrame error
    // =========================================================================
    #[test]
    fn test_incomplete_frame_detection(
        fin in any::<bool>(),
        opcode in data_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 1..500),
        truncate_by in 1..50usize
    ) {
        let frame = Frame::new(fin, opcode, payload);
        let mut buf = vec![0u8; frame.wire_size(false)];
        let written = frame.write(&mut buf, None);
        prop_assert!(written.is_ok());
        let written = written.unwrap();

        // Truncate the buffer
        let truncated_len = written.saturating_sub(truncate_by).max(1);
        if truncated_len < written {
            let result = Frame::parse(&buf[..truncated_len]);
            prop_assert!(result.is_err(), "should fail parsing truncated frame");
        }
    }

    // =========================================================================
    // Property 9: FIN bit is preserved through roundtrip
    // =========================================================================
    #[test]
    fn test_fin_bit_preserved(
        fin in any::<bool>(),
        opcode in data_opcode_strategy(),
        payload in prop::collection::vec(any::<u8>(), 0..100)
    ) {
        let frame = Frame::new(fin, opcode, payload);
        let mut buf = vec![0u8; frame.wire_size(false)];
        let _ = frame.write(&mut buf, None);

        let (parsed, _) = Frame::parse(&buf).unwrap();
        prop_assert_eq!(fin, parsed.fin, "FIN bit not preserved");
    }

    // =========================================================================
    // Property 10: Multiple frames can be parsed sequentially
    // =========================================================================
    #[test]
    fn test_sequential_frame_parsing(
        payloads in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..100), 1..5)
    ) {
        // Build multiple frames into a single buffer
        let frames: Vec<_> = payloads.iter()
            .map(|p| Frame::new(true, OpCode::Binary, p.clone()))
            .collect();

        let total_size: usize = frames.iter().map(|f| f.wire_size(false)).sum();
        let mut buf = vec![0u8; total_size];

        let mut offset = 0;
        for frame in &frames {
            let written = frame.write(&mut buf[offset..], None);
            prop_assert!(written.is_ok());
            offset += written.unwrap();
        }

        // Parse them back sequentially
        let mut parse_offset = 0;
        for (i, original) in frames.iter().enumerate() {
            let result = Frame::parse(&buf[parse_offset..]);
            prop_assert!(result.is_ok(), "failed to parse frame {}: {:?}", i, result);
            let (parsed, consumed) = result.unwrap();
            prop_assert_eq!(original.payload(), parsed.payload(), "frame {} payload mismatch", i);
            parse_offset += consumed;
        }

        prop_assert_eq!(parse_offset, total_size, "not all bytes consumed");
    }
}

#[cfg(test)]
mod targeted_tests {
    use super::*;

    /// Test 7-bit length encoding boundary (0-125 bytes)
    #[test]
    fn test_7bit_length_boundary() {
        for len in [0, 1, 124, 125] {
            let payload = vec![0xAB; len];
            let frame = Frame::new(true, OpCode::Binary, payload.clone());
            let mut buf = vec![0u8; frame.wire_size(false)];
            frame.write(&mut buf, None).unwrap();

            let (parsed, _) = Frame::parse(&buf).unwrap();
            assert_eq!(parsed.payload().len(), len);
        }
    }

    /// Test 16-bit length encoding boundary (126-65535 bytes)
    #[test]
    fn test_16bit_length_boundary() {
        for len in [126, 127, 255, 256, 65534, 65535] {
            let payload = vec![0xCD; len];
            let frame = Frame::new(true, OpCode::Binary, payload.clone());
            let mut buf = vec![0u8; frame.wire_size(false)];
            frame.write(&mut buf, None).unwrap();

            let (parsed, _) = Frame::parse(&buf).unwrap();
            assert_eq!(parsed.payload().len(), len);
        }
    }

    /// Test 64-bit length encoding (>65535 bytes)
    #[test]
    fn test_64bit_length_boundary() {
        let len = 65536;
        let payload = vec![0xEF; len];
        let frame = Frame::new(true, OpCode::Binary, payload.clone());
        let mut buf = vec![0u8; frame.wire_size(false)];
        frame.write(&mut buf, None).unwrap();

        let (parsed, _) = Frame::parse(&buf).unwrap();
        assert_eq!(parsed.payload().len(), len);
    }

    /// Test all zero mask (edge case)
    #[test]
    fn test_zero_mask() {
        let payload = b"test payload".to_vec();
        let frame = Frame::new(true, OpCode::Text, payload.clone());
        let mask = [0, 0, 0, 0];
        let mut buf = vec![0u8; frame.wire_size(true)];
        frame.write(&mut buf, Some(mask)).unwrap();

        let (parsed, _) = Frame::parse(&buf).unwrap();
        assert_eq!(parsed.payload(), payload.as_slice());
    }

    /// Test all 0xFF mask (edge case)
    #[test]
    fn test_ff_mask() {
        let payload = b"test payload".to_vec();
        let frame = Frame::new(true, OpCode::Text, payload.clone());
        let mask = [0xFF, 0xFF, 0xFF, 0xFF];
        let mut buf = vec![0u8; frame.wire_size(true)];
        frame.write(&mut buf, Some(mask)).unwrap();

        let (parsed, _) = Frame::parse(&buf).unwrap();
        assert_eq!(parsed.payload(), payload.as_slice());
    }
}

proptest! {
    #[test]
    fn test_handshake_parse_no_panic(data in prop::collection::vec(any::<u8>(), 0..2000)) {
        let _ = HandshakeRequest::parse(&data);
    }

    #[test]
    fn test_handshake_truncated(truncate_at in 1usize..200) {
        let valid_request = b"GET /chat HTTP/1.1\r\n\
            Host: example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\r\n";

        let truncated_len = truncate_at.min(valid_request.len());
        let truncated = &valid_request[..truncated_len];

        if truncated_len < valid_request.len() {
            let _ = HandshakeRequest::parse(truncated);
        }
    }

    #[test]
    fn test_handshake_valid_variations(
        path in "/[a-z]{1,20}",
        host in "[a-z]{3,10}\\.[a-z]{2,4}"
    ) {
        let request = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n",
            path, host
        );

        let result = HandshakeRequest::parse(request.as_bytes());
        prop_assert!(result.is_ok(), "Valid request should parse: {:?}", result);
    }
}
