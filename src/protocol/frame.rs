//! WebSocket frame parsing and serialization (RFC 6455).
//!
//! This module provides zero-copy frame parsing with full RFC 6455 compliance.

use bytes::Bytes;

use crate::error::{Error, Result};
use crate::protocol::OpCode;
use crate::protocol::mask::{apply_mask, apply_mask_simd};

/// Maximum payload size for control frames (RFC 6455).
pub const MAX_CONTROL_FRAME_PAYLOAD: usize = 125;

#[derive(Debug, Clone)]
struct FrameHeader {
    fin: bool,
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    opcode: OpCode,
    mask: Option<[u8; 4]>,
    payload_len: usize,
    header_len: usize,
}

/// Parse frame header from buffer.
///
/// This is the common header parsing logic shared between `Frame::parse()`
/// and `Frame::parse_zero_copy()`.
///
/// # Errors
///
/// - `Error::IncompleteFrame` if not enough data is available
/// - `Error::InvalidOpcode` if the opcode is invalid
/// - `Error::ReservedOpcode` if a reserved opcode is used
/// - `Error::PayloadTooLargeForPlatform` if payload length exceeds platform limits
#[inline]
fn parse_header(buf: &[u8]) -> Result<FrameHeader> {
    // Need at least 2 bytes for the header
    if buf.len() < 2 {
        return Err(Error::IncompleteFrame {
            needed: 2 - buf.len(),
        });
    }

    let byte0 = buf[0];
    let byte1 = buf[1];

    // Parse first byte
    let fin = (byte0 & 0x80) != 0;
    let rsv1 = (byte0 & 0x40) != 0;
    let rsv2 = (byte0 & 0x20) != 0;
    let rsv3 = (byte0 & 0x10) != 0;
    let opcode = OpCode::from_u8(byte0 & 0x0F)?;

    // Parse second byte
    let masked = (byte1 & 0x80) != 0;
    let payload_len_initial = byte1 & 0x7F;

    // Calculate header size and payload length
    let (payload_len, header_size) = match payload_len_initial {
        0..=125 => (payload_len_initial as usize, 2),
        126 => {
            if buf.len() < 4 {
                return Err(Error::IncompleteFrame {
                    needed: 4 - buf.len(),
                });
            }
            let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
            (len, 4)
        }
        127 => {
            if buf.len() < 10 {
                return Err(Error::IncompleteFrame {
                    needed: 10 - buf.len(),
                });
            }
            let len_u64 = u64::from_be_bytes([
                buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
            ]);
            let len = usize::try_from(len_u64).map_err(|_| Error::PayloadTooLargeForPlatform {
                size: len_u64,
                max: usize::MAX as u64,
            })?;
            (len, 10)
        }
        _ => unreachable!(),
    };

    // Calculate mask key offset and total header size
    let mask_offset = header_size;
    let total_header_size = if masked { header_size + 4 } else { header_size };

    // Check if we have enough data for mask key
    if masked && buf.len() < total_header_size {
        return Err(Error::IncompleteFrame {
            needed: total_header_size - buf.len(),
        });
    }

    // Extract mask key if present
    let mask = if masked {
        Some([
            buf[mask_offset],
            buf[mask_offset + 1],
            buf[mask_offset + 2],
            buf[mask_offset + 3],
        ])
    } else {
        None
    };

    Ok(FrameHeader {
        fin,
        rsv1,
        rsv2,
        rsv3,
        opcode,
        mask,
        payload_len,
        header_len: total_header_size,
    })
}

/// Internal payload representation for zero-copy optimization.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Payload {
    /// Owned data (used after unmasking or when creating new frames).
    Owned(Vec<u8>),
    /// Shared data for zero-copy parsing of unmasked frames.
    Shared(Bytes),
}

/// A WebSocket frame as defined in RFC 6455.
///
/// Frames are the basic unit of communication in the WebSocket protocol.
/// This struct supports both parsing incoming frames and creating outgoing frames.
///
/// ## Frame Structure
///
/// ```text
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-------+-+-------------+-------------------------------+
/// |F|R|R|R| opcode |M| Payload len |    Extended payload length    |
/// |I|S|S|S|  (4)   |A|     (7)     |             (16/64)           |
/// |N|V|V|V|       |S|             |   (if payload len==126/127)   |
/// | |1|2|3|       |K|             |                               |
/// +-+-+-+-+-------+-+-------------+-------------------------------+
/// |                         Masking key (if present)              |
/// +---------------------------------------------------------------+
/// |                     Payload data                              |
/// +---------------------------------------------------------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Final fragment flag. True if this is the last fragment of a message.
    pub fin: bool,
    /// Reserved bit 1. Must be 0 unless extension is negotiated.
    pub rsv1: bool,
    /// Reserved bit 2. Must be 0 unless extension is negotiated.
    pub rsv2: bool,
    /// Reserved bit 3. Must be 0 unless extension is negotiated.
    pub rsv3: bool,
    /// Frame opcode defining the interpretation of payload data.
    pub opcode: OpCode,
    /// Frame payload data.
    payload: Payload,
}

impl Frame {
    /// Create a new frame with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `fin` - Whether this is the final fragment
    /// * `opcode` - The frame opcode
    /// * `payload` - The payload data
    #[must_use]
    pub fn new(fin: bool, opcode: OpCode, payload: Vec<u8>) -> Self {
        Self {
            fin,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode,
            payload: Payload::Owned(payload),
        }
    }

    /// Create a text frame.
    #[must_use]
    pub fn text(data: impl Into<Vec<u8>>) -> Self {
        Self::new(true, OpCode::Text, data.into())
    }

    /// Create a binary frame.
    #[must_use]
    pub fn binary(data: impl Into<Vec<u8>>) -> Self {
        Self::new(true, OpCode::Binary, data.into())
    }

    /// Create a close frame with optional status code and reason.
    #[must_use]
    pub fn close(code: Option<u16>, reason: &str) -> Self {
        let payload = if let Some(code) = code {
            let mut data = code.to_be_bytes().to_vec();
            data.extend_from_slice(reason.as_bytes());
            data
        } else {
            Vec::new()
        };
        Self::new(true, OpCode::Close, payload)
    }

    /// Create a ping frame.
    #[must_use]
    pub fn ping(data: impl Into<Vec<u8>>) -> Self {
        Self::new(true, OpCode::Ping, data.into())
    }

    /// Create a pong frame.
    #[must_use]
    pub fn pong(data: impl Into<Vec<u8>>) -> Self {
        Self::new(true, OpCode::Pong, data.into())
    }

    /// Get the payload bytes.
    #[inline]
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        match &self.payload {
            Payload::Owned(data) => data,
            Payload::Shared(data) => data,
        }
    }

    /// Take ownership of the payload.
    #[must_use]
    pub fn into_payload(self) -> Vec<u8> {
        match self.payload {
            Payload::Owned(data) => data,
            Payload::Shared(data) => data.to_vec(),
        }
    }

    /// Parse a frame from a buffer.
    ///
    /// Returns the parsed frame and the number of bytes consumed.
    ///
    /// ## Errors
    ///
    /// - `Error::IncompleteFrame` if not enough data is available
    /// - `Error::InvalidOpcode` if the opcode is invalid
    /// - `Error::ReservedOpcode` if a reserved opcode is used
    #[inline]
    pub fn parse(buf: &[u8]) -> Result<(Self, usize)> {
        let header = parse_header(buf)?;

        let total_size = header.header_len.checked_add(header.payload_len).ok_or(
            Error::PayloadTooLargeForPlatform {
                size: header.payload_len as u64,
                max: usize::MAX as u64,
            },
        )?;

        if buf.len() < total_size {
            return Err(Error::IncompleteFrame {
                needed: total_size - buf.len(),
            });
        }

        let payload_start = header.header_len;
        let payload_end = payload_start + header.payload_len;
        let payload = if let Some(mask) = header.mask {
            let mut data = buf[payload_start..payload_end].to_vec();
            apply_mask_simd(&mut data, mask);
            Payload::Owned(data)
        } else {
            Payload::Owned(buf[payload_start..payload_end].to_vec())
        };

        let frame = Frame {
            fin: header.fin,
            rsv1: header.rsv1,
            rsv2: header.rsv2,
            rsv3: header.rsv3,
            opcode: header.opcode,
            payload,
        };

        Ok((frame, total_size))
    }

    /// Parse a frame from a `Bytes` buffer with zero-copy for unmasked frames.
    ///
    /// For unmasked frames, the payload uses `Bytes::slice()` for zero-copy sharing.
    /// For masked frames, the payload is copied and unmasked (required for in-place XOR).
    ///
    /// ## Errors
    ///
    /// - `Error::IncompleteFrame` if not enough data is available
    /// - `Error::InvalidOpcode` if the opcode is invalid
    /// - `Error::ReservedOpcode` if a reserved opcode is used
    #[inline]
    pub fn parse_zero_copy(buf: &Bytes) -> Result<(Self, usize)> {
        let header = parse_header(buf)?;

        let total_size =
            header
                .header_len
                .checked_add(header.payload_len)
                .ok_or(Error::FrameTooLarge {
                    size: header.payload_len,
                    max: usize::MAX - header.header_len,
                })?;

        if buf.len() < total_size {
            return Err(Error::IncompleteFrame {
                needed: total_size - buf.len(),
            });
        }

        let payload_start = header.header_len;
        let payload_end = payload_start + header.payload_len;
        let payload = if let Some(mask) = header.mask {
            let mut data = buf[payload_start..payload_end].to_vec();
            apply_mask_simd(&mut data, mask);
            Payload::Owned(data)
        } else {
            Payload::Shared(buf.slice(payload_start..payload_end))
        };

        let frame = Frame {
            fin: header.fin,
            rsv1: header.rsv1,
            rsv2: header.rsv2,
            rsv3: header.rsv3,
            opcode: header.opcode,
            payload,
        };

        Ok((frame, total_size))
    }

    /// Validate the frame according to RFC 6455.
    ///
    /// # Errors
    ///
    /// - `Error::ReservedBitsSet` if RSV bits are set without extension
    /// - `Error::FragmentedControlFrame` if control frame has FIN=0
    /// - `Error::ControlFrameTooLarge` if control frame payload > 125 bytes
    pub fn validate(&self) -> Result<()> {
        // Check reserved bits (must be 0 without extensions)
        if self.rsv1 || self.rsv2 || self.rsv3 {
            return Err(Error::ReservedBitsSet);
        }

        // Control frame validations
        if self.opcode.is_control() {
            // Control frames must not be fragmented
            if !self.fin {
                return Err(Error::FragmentedControlFrame);
            }

            // Control frame payload must be <= 125 bytes
            if self.payload().len() > MAX_CONTROL_FRAME_PAYLOAD {
                return Err(Error::ControlFrameTooLarge(self.payload().len()));
            }
        }

        Ok(())
    }

    /// Write the frame to a buffer.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Arguments
    ///
    /// * `buf` - The buffer to write to
    /// * `mask` - Optional masking key (required for client frames)
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer is too small.
    pub fn write(&self, buf: &mut [u8], mask: Option<[u8; 4]>) -> Result<usize> {
        let payload = self.payload();
        let payload_len = payload.len();

        // Calculate header size
        let (len_bytes, extended_len_size) = if payload_len <= 125 {
            (payload_len as u8, 0)
        } else if payload_len <= 65535 {
            (126, 2)
        } else {
            (127, 8)
        };

        let mask_size = if mask.is_some() { 4 } else { 0 };
        let header_size = 2 + extended_len_size + mask_size;
        let total_size = header_size + payload_len;

        // Check buffer size
        if buf.len() < total_size {
            return Err(Error::InvalidFrame(format!(
                "Buffer too small: need {} bytes, have {}",
                total_size,
                buf.len()
            )));
        }

        // Build first byte
        let mut byte0 = self.opcode.as_u8();
        if self.fin {
            byte0 |= 0x80;
        }
        if self.rsv1 {
            byte0 |= 0x40;
        }
        if self.rsv2 {
            byte0 |= 0x20;
        }
        if self.rsv3 {
            byte0 |= 0x10;
        }
        buf[0] = byte0;

        // Build second byte
        let mut byte1 = len_bytes;
        if mask.is_some() {
            byte1 |= 0x80;
        }
        buf[1] = byte1;

        // Write extended payload length
        let mut offset = 2;
        match extended_len_size {
            2 => {
                let len_bytes = (payload_len as u16).to_be_bytes();
                buf[offset] = len_bytes[0];
                buf[offset + 1] = len_bytes[1];
                offset += 2;
            }
            8 => {
                let len_bytes = (payload_len as u64).to_be_bytes();
                buf[offset..offset + 8].copy_from_slice(&len_bytes);
                offset += 8;
            }
            _ => {}
        }

        // Write masking key
        if let Some(mask_key) = mask {
            buf[offset..offset + 4].copy_from_slice(&mask_key);
            offset += 4;
        }

        // Write payload
        buf[offset..offset + payload_len].copy_from_slice(payload);

        // Apply mask if needed
        if let Some(mask_key) = mask {
            apply_mask(&mut buf[offset..offset + payload_len], mask_key);
        }

        Ok(total_size)
    }

    /// Calculate the size needed to write this frame.
    #[must_use]
    pub fn wire_size(&self, masked: bool) -> usize {
        let payload_len = self.payload().len();
        let extended_len_size = if payload_len <= 125 {
            0
        } else if payload_len <= 65535 {
            2
        } else {
            8
        };
        let mask_size = if masked { 4 } else { 0 };
        2 + extended_len_size + mask_size + payload_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // RED PHASE: Tests written first, implementation follows
    // ==========================================================================

    // --------------------------------------------------------------------------
    // Test 1: Unmasked text frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_unmasked_text_frame() {
        // FIN=1, opcode=1 (text), unmasked, payload="Hello"
        let data = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 7);
        assert!(frame.fin);
        assert!(!frame.rsv1);
        assert!(!frame.rsv2);
        assert!(!frame.rsv3);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload(), b"Hello");
    }

    // --------------------------------------------------------------------------
    // Test 2: Masked text frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_masked_text_frame() {
        // FIN=1, opcode=1 (text), masked, payload="Hello"
        // Mask key: 0x37, 0xfa, 0x21, 0x3d
        // Masked payload: 0x7f, 0x9f, 0x4d, 0x51, 0x58
        let data = &[
            0x81, 0x85, // FIN + Text, MASK + len=5
            0x37, 0xfa, 0x21, 0x3d, // Mask key
            0x7f, 0x9f, 0x4d, 0x51, 0x58, // Masked "Hello"
        ];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 11);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload(), b"Hello");
    }

    // --------------------------------------------------------------------------
    // Test 3: Binary frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_binary_frame() {
        // FIN=1, opcode=2 (binary), unmasked, payload=[0x01, 0x02, 0x03]
        let data = &[0x82, 0x03, 0x01, 0x02, 0x03];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 5);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Binary);
        assert_eq!(frame.payload(), &[0x01, 0x02, 0x03]);
    }

    // --------------------------------------------------------------------------
    // Test 4: Close frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_close_frame() {
        // FIN=1, opcode=8 (close), unmasked, payload=[0x03, 0xe8] (1000 = normal close)
        let data = &[0x88, 0x02, 0x03, 0xe8];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 4);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Close);
        assert_eq!(frame.payload(), &[0x03, 0xe8]);
    }

    // --------------------------------------------------------------------------
    // Test 5: Ping frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_ping_frame() {
        // FIN=1, opcode=9 (ping), unmasked, payload="ping"
        let data = &[0x89, 0x04, 0x70, 0x69, 0x6e, 0x67];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 6);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Ping);
        assert_eq!(frame.payload(), b"ping");
    }

    // --------------------------------------------------------------------------
    // Test 6: Pong frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_pong_frame() {
        // FIN=1, opcode=10 (pong), unmasked, payload="pong"
        let data = &[0x8a, 0x04, 0x70, 0x6f, 0x6e, 0x67];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 6);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Pong);
        assert_eq!(frame.payload(), b"pong");
    }

    // --------------------------------------------------------------------------
    // Test 7: Fragmented message (FIN=0)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_fragmented_frame() {
        // FIN=0, opcode=1 (text), unmasked, payload="Hel"
        let data = &[0x01, 0x03, 0x48, 0x65, 0x6c];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 5);
        assert!(!frame.fin);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload(), b"Hel");
    }

    // --------------------------------------------------------------------------
    // Test 8: Continuation frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_continuation_frame() {
        // FIN=1, opcode=0 (continuation), unmasked, payload="lo"
        let data = &[0x80, 0x02, 0x6c, 0x6f];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 4);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Continuation);
        assert_eq!(frame.payload(), b"lo");
    }

    // --------------------------------------------------------------------------
    // Test 9: Extended payload length (126)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_extended_length_126() {
        // FIN=1, opcode=2 (binary), unmasked, len=126 (16-bit extended)
        // Payload: 256 bytes of 0xAB
        let mut data = vec![0x82, 0x7e, 0x01, 0x00]; // len=256
        data.extend(vec![0xab; 256]);

        let (frame, len) = Frame::parse(&data).unwrap();
        assert_eq!(len, 4 + 256);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Binary);
        assert_eq!(frame.payload().len(), 256);
        assert!(frame.payload().iter().all(|&b| b == 0xab));
    }

    // --------------------------------------------------------------------------
    // Test 10: Extended payload length (127)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_extended_length_127() {
        // FIN=1, opcode=2 (binary), unmasked, len=127 (64-bit extended)
        // Payload: 65536 bytes of 0xCD
        let mut data = vec![0x82, 0x7f];
        data.extend(65536u64.to_be_bytes()); // 8 bytes for length
        data.extend(vec![0xcd; 65536]);

        let (frame, len) = Frame::parse(&data).unwrap();
        assert_eq!(len, 10 + 65536);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Binary);
        assert_eq!(frame.payload().len(), 65536);
        assert!(frame.payload().iter().all(|&b| b == 0xcd));
    }

    // --------------------------------------------------------------------------
    // Test 11: Empty payload
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_empty_payload() {
        // FIN=1, opcode=1 (text), unmasked, len=0
        let data = &[0x81, 0x00];
        let (frame, len) = Frame::parse(data).unwrap();
        assert_eq!(len, 2);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload(), b"");
    }

    // --------------------------------------------------------------------------
    // Test 12: Fragmented control frame (should fail validation)
    // --------------------------------------------------------------------------
    #[test]
    fn test_validate_fragmented_control_frame() {
        // Create ping with FIN=0 (invalid)
        let mut frame = Frame::ping(b"test".to_vec());
        frame.fin = false;

        let result = frame.validate();
        assert!(matches!(result, Err(Error::FragmentedControlFrame)));
    }

    // --------------------------------------------------------------------------
    // Test 13: Control frame too large (should fail validation)
    // --------------------------------------------------------------------------
    #[test]
    fn test_validate_control_frame_too_large() {
        // Create ping with 126 bytes (> 125 limit)
        let frame = Frame::ping(vec![0u8; 126]);

        let result = frame.validate();
        assert!(matches!(result, Err(Error::ControlFrameTooLarge(126))));
    }

    // --------------------------------------------------------------------------
    // Test 14: Reserved bits set (should fail validation)
    // --------------------------------------------------------------------------
    #[test]
    fn test_validate_reserved_bits_set() {
        let mut frame = Frame::text(b"test".to_vec());
        frame.rsv1 = true;

        let result = frame.validate();
        assert!(matches!(result, Err(Error::ReservedBitsSet)));
    }

    // --------------------------------------------------------------------------
    // Test 15: Invalid opcode
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_invalid_opcode() {
        // FIN=1, opcode=3 (reserved)
        let data = &[0x83, 0x00];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::ReservedOpcode(0x03))));
    }

    // --------------------------------------------------------------------------
    // Test 16: Reserved opcode
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_reserved_opcode() {
        // FIN=1, opcode=0x0B (reserved control)
        let data = &[0x8b, 0x00];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::ReservedOpcode(0x0B))));
    }

    // --------------------------------------------------------------------------
    // Test 17: Incomplete frame (not enough header bytes)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_incomplete_header() {
        let data = &[0x81]; // Only 1 byte, need 2
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::IncompleteFrame { needed: 1 })));
    }

    // --------------------------------------------------------------------------
    // Test 18: Incomplete frame (not enough payload bytes)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_incomplete_payload() {
        // FIN=1, opcode=1, len=5 but only 3 bytes of payload
        let data = &[0x81, 0x05, 0x48, 0x65, 0x6c];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::IncompleteFrame { needed: 2 })));
    }

    // --------------------------------------------------------------------------
    // Test 19: Incomplete extended length (126)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_incomplete_extended_length_126() {
        // Need 4 bytes for header, only have 3
        let data = &[0x82, 0x7e, 0x01];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::IncompleteFrame { needed: 1 })));
    }

    // --------------------------------------------------------------------------
    // Test 20: Incomplete extended length (127)
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_incomplete_extended_length_127() {
        // Need 10 bytes for header, only have 5
        let data = &[0x82, 0x7f, 0x00, 0x00, 0x00];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::IncompleteFrame { needed: 5 })));
    }

    // --------------------------------------------------------------------------
    // Test 21: Frame serialization (write unmasked)
    // --------------------------------------------------------------------------
    #[test]
    fn test_write_unmasked_text_frame() {
        let frame = Frame::text(b"Hello".to_vec());
        let mut buf = vec![0u8; 32];

        let len = frame.write(&mut buf, None).unwrap();

        assert_eq!(len, 7);
        assert_eq!(&buf[..7], &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    }

    // --------------------------------------------------------------------------
    // Test 22: Frame serialization (write masked)
    // --------------------------------------------------------------------------
    #[test]
    fn test_write_masked_text_frame() {
        let frame = Frame::text(b"Hello".to_vec());
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let mut buf = vec![0u8; 32];

        let len = frame.write(&mut buf, Some(mask)).unwrap();

        assert_eq!(len, 11);
        assert_eq!(buf[0], 0x81); // FIN + Text
        assert_eq!(buf[1], 0x85); // MASK + len=5
        assert_eq!(&buf[2..6], &mask); // Mask key
        assert_eq!(&buf[6..11], &[0x7f, 0x9f, 0x4d, 0x51, 0x58]); // Masked "Hello"
    }

    // --------------------------------------------------------------------------
    // Test 23: Frame serialization with extended length (126)
    // --------------------------------------------------------------------------
    #[test]
    fn test_write_extended_length_126() {
        let payload = vec![0xab; 256];
        let frame = Frame::binary(payload);
        let mut buf = vec![0u8; 512];

        let len = frame.write(&mut buf, None).unwrap();

        assert_eq!(len, 4 + 256);
        assert_eq!(buf[0], 0x82); // FIN + Binary
        assert_eq!(buf[1], 0x7e); // Extended length indicator
        assert_eq!(&buf[2..4], &[0x01, 0x00]); // Length = 256
        assert!(buf[4..4 + 256].iter().all(|&b| b == 0xab));
    }

    // --------------------------------------------------------------------------
    // Test 24: Frame serialization with extended length (127)
    // --------------------------------------------------------------------------
    #[test]
    fn test_write_extended_length_127() {
        let payload = vec![0xcd; 65536];
        let frame = Frame::binary(payload);
        let mut buf = vec![0u8; 70000];

        let len = frame.write(&mut buf, None).unwrap();

        assert_eq!(len, 10 + 65536);
        assert_eq!(buf[0], 0x82); // FIN + Binary
        assert_eq!(buf[1], 0x7f); // Extended length indicator (64-bit)
        assert_eq!(&buf[2..10], &65536u64.to_be_bytes()); // Length
        assert!(buf[10..10 + 65536].iter().all(|&b| b == 0xcd));
    }

    // --------------------------------------------------------------------------
    // Test 25: Round-trip parse/write
    // --------------------------------------------------------------------------
    #[test]
    fn test_roundtrip_unmasked() {
        let original = Frame::text(b"WebSocket roundtrip test!".to_vec());
        let mut buf = vec![0u8; 64];

        let written = original.write(&mut buf, None).unwrap();
        let (parsed, consumed) = Frame::parse(&buf[..written]).unwrap();

        assert_eq!(consumed, written);
        assert_eq!(parsed.fin, original.fin);
        assert_eq!(parsed.opcode, original.opcode);
        assert_eq!(parsed.payload(), original.payload());
    }

    // --------------------------------------------------------------------------
    // Test 26: Round-trip with masking
    // --------------------------------------------------------------------------
    #[test]
    fn test_roundtrip_masked() {
        let original = Frame::text(b"Masked roundtrip test!".to_vec());
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut buf = vec![0u8; 64];

        let written = original.write(&mut buf, Some(mask)).unwrap();
        let (parsed, consumed) = Frame::parse(&buf[..written]).unwrap();

        assert_eq!(consumed, written);
        assert_eq!(parsed.fin, original.fin);
        assert_eq!(parsed.opcode, original.opcode);
        assert_eq!(parsed.payload(), original.payload());
    }

    // --------------------------------------------------------------------------
    // Test 27: Write buffer too small
    // --------------------------------------------------------------------------
    #[test]
    fn test_write_buffer_too_small() {
        let frame = Frame::text(b"Hello".to_vec());
        let mut buf = vec![0u8; 4]; // Need 7 bytes

        let result = frame.write(&mut buf, None);
        assert!(matches!(result, Err(Error::InvalidFrame(_))));
    }

    // --------------------------------------------------------------------------
    // Test 28: Wire size calculation
    // --------------------------------------------------------------------------
    #[test]
    fn test_wire_size() {
        // Small payload, unmasked: 2 header + 5 payload
        let frame = Frame::text(b"Hello".to_vec());
        assert_eq!(frame.wire_size(false), 7);
        assert_eq!(frame.wire_size(true), 11); // +4 for mask

        // Medium payload (256 bytes): 4 header + 256 payload
        let frame = Frame::binary(vec![0u8; 256]);
        assert_eq!(frame.wire_size(false), 260);
        assert_eq!(frame.wire_size(true), 264);

        // Large payload (65536 bytes): 10 header + 65536 payload
        let frame = Frame::binary(vec![0u8; 65536]);
        assert_eq!(frame.wire_size(false), 65546);
        assert_eq!(frame.wire_size(true), 65550);
    }

    // --------------------------------------------------------------------------
    // Test 29: Close frame with code and reason
    // --------------------------------------------------------------------------
    #[test]
    fn test_close_frame_with_reason() {
        let frame = Frame::close(Some(1000), "Normal closure");
        assert_eq!(frame.opcode, OpCode::Close);
        assert!(frame.fin);

        let payload = frame.payload();
        assert_eq!(u16::from_be_bytes([payload[0], payload[1]]), 1000);
        assert_eq!(&payload[2..], b"Normal closure");
    }

    // --------------------------------------------------------------------------
    // Test 30: Validate valid frame
    // --------------------------------------------------------------------------
    #[test]
    fn test_validate_valid_frame() {
        let frame = Frame::text(b"Valid frame".to_vec());
        assert!(frame.validate().is_ok());

        let ping = Frame::ping(b"ping".to_vec());
        assert!(ping.validate().is_ok());

        let close = Frame::close(Some(1000), "bye");
        assert!(close.validate().is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 31: Parse with RSV bits set
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_rsv_bits_set() {
        // FIN=1, RSV1=1, opcode=1 (text), unmasked, len=0
        let data = &[0xc1, 0x00]; // 0xc1 = 1100 0001 (FIN + RSV1 + Text)
        let (frame, _) = Frame::parse(data).unwrap();
        assert!(frame.rsv1);
        assert!(!frame.rsv2);
        assert!(!frame.rsv3);
        // Validation should fail
        assert!(matches!(frame.validate(), Err(Error::ReservedBitsSet)));
    }

    // --------------------------------------------------------------------------
    // Test 32: Maximum control frame payload (125 bytes)
    // --------------------------------------------------------------------------
    #[test]
    fn test_max_control_frame_payload() {
        let frame = Frame::ping(vec![0u8; 125]);
        assert!(frame.validate().is_ok());
    }

    // --------------------------------------------------------------------------
    // Test 33: Into payload
    // --------------------------------------------------------------------------
    #[test]
    fn test_into_payload() {
        let frame = Frame::text(b"Owned data".to_vec());
        let payload = frame.into_payload();
        assert_eq!(payload, b"Owned data");
    }

    // --------------------------------------------------------------------------
    // Test 34: Incomplete mask key
    // --------------------------------------------------------------------------
    #[test]
    fn test_parse_incomplete_mask_key() {
        // FIN=1, opcode=1, MASK=1, len=5, but only 2 bytes of mask key
        let data = &[0x81, 0x85, 0x37, 0xfa];
        let result = Frame::parse(data);
        assert!(matches!(result, Err(Error::IncompleteFrame { .. })));
    }

    #[test]
    fn test_parse_unmasked_zero_copy() {
        let data = Bytes::from_static(&[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
        let (frame, len) = Frame::parse_zero_copy(&data).unwrap();
        assert_eq!(len, 7);
        assert!(frame.fin);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload(), b"Hello");

        match &frame.payload {
            Payload::Shared(bytes) => {
                assert_eq!(bytes.as_ref(), b"Hello");
            }
            Payload::Owned(_) => panic!("Expected Payload::Shared for unmasked zero-copy parse"),
        }
    }

    // --------------------------------------------------------------------------
    // Test 35: Payload exceeds platform max (32-bit overflow protection)
    // --------------------------------------------------------------------------
    #[test]
    fn test_payload_exceeds_platform_max() {
        // Construct a frame header claiming u64::MAX length
        // 0x82 = binary final, 0xFF = masked + 127 (64-bit length follows)
        let mut data = vec![0x82, 0xFF];
        // Add 8 bytes of u64::MAX
        data.extend_from_slice(&u64::MAX.to_be_bytes());
        // Add 4 bytes mask key
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        let result = Frame::parse(&data);

        // On 64-bit platforms: IncompleteFrame (usize::MAX is huge)
        // On 32-bit platforms: PayloadTooLargeForPlatform
        // Either way, it must be an error and not panic
        assert!(result.is_err());
    }

    // --------------------------------------------------------------------------
    // Test 36: Large valid payload header still parses correctly
    // --------------------------------------------------------------------------
    #[test]
    fn test_large_valid_payload_header() {
        // Test normal large payload still works
        // 0x82 = binary final, 0x7E = 16-bit length (300)
        let mut data = vec![0x82, 0x7E, 0x01, 0x2C]; // 300 bytes
        data.extend_from_slice(&vec![0xAB; 300]);

        let result = Frame::parse(&data);
        assert!(result.is_ok());
        let (frame, _) = result.unwrap();
        assert_eq!(frame.payload().len(), 300);
    }
}
