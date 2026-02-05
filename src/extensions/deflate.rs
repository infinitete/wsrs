//! Permessage-deflate WebSocket compression extension (RFC 7692).

use crate::error::{Error, Result};
use crate::extensions::{Extension, ExtensionParam, RsvBits};
use crate::protocol::Frame;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};

const MIN_WINDOW_BITS: u8 = 8;
const MAX_WINDOW_BITS: u8 = 15;
const DEFAULT_WINDOW_BITS: u8 = 15;
const DEFLATE_TRAILER: [u8; 4] = [0x00, 0x00, 0xff, 0xff];
const MAX_COMPRESSION_ITERATIONS: usize = 100_000;
const MAX_DECOMPRESSION_RATIO: usize = 100;
const DEFAULT_MAX_DECOMPRESSED_SIZE: usize = 64 * 1024 * 1024;

/// Configuration for the permessage-deflate extension.
///
/// Controls compression parameters like window bits and context takeover.
#[derive(Debug, Clone)]
pub struct DeflateConfig {
    /// If true, server discards compression context after each message.
    pub server_no_context_takeover: bool,
    /// If true, client discards compression context after each message.
    pub client_no_context_takeover: bool,
    /// Server's LZ77 sliding window size (8-15, default 15).
    pub server_max_window_bits: u8,
    /// Client's LZ77 sliding window size (8-15, default 15).
    pub client_max_window_bits: u8,
    /// Compression level (0-9, default 6). Higher = better compression, slower.
    pub compression_level: u32,
    /// Maximum decompressed message size in bytes (default 64MB).
    /// Prevents decompression bomb attacks.
    pub max_decompressed_size: usize,
}

impl Default for DeflateConfig {
    fn default() -> Self {
        Self {
            server_no_context_takeover: false,
            client_no_context_takeover: false,
            server_max_window_bits: DEFAULT_WINDOW_BITS,
            client_max_window_bits: DEFAULT_WINDOW_BITS,
            compression_level: 6,
            max_decompressed_size: DEFAULT_MAX_DECOMPRESSED_SIZE,
        }
    }
}

impl DeflateConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set server_no_context_takeover (builder pattern).
    #[must_use]
    pub fn server_no_context_takeover(mut self, value: bool) -> Self {
        self.server_no_context_takeover = value;
        self
    }

    /// Set client_no_context_takeover (builder pattern).
    #[must_use]
    pub fn client_no_context_takeover(mut self, value: bool) -> Self {
        self.client_no_context_takeover = value;
        self
    }

    /// Set server_max_window_bits (8-15).
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidExtension` if bits is not in range 8-15.
    pub fn server_max_window_bits(mut self, bits: u8) -> Result<Self> {
        if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&bits) {
            return Err(Error::InvalidExtension(format!(
                "server_max_window_bits must be {}-{}, got {}",
                MIN_WINDOW_BITS, MAX_WINDOW_BITS, bits
            )));
        }
        self.server_max_window_bits = bits;
        Ok(self)
    }

    /// Set client_max_window_bits (8-15).
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidExtension` if bits is not in range 8-15.
    pub fn client_max_window_bits(mut self, bits: u8) -> Result<Self> {
        if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&bits) {
            return Err(Error::InvalidExtension(format!(
                "client_max_window_bits must be {}-{}, got {}",
                MIN_WINDOW_BITS, MAX_WINDOW_BITS, bits
            )));
        }
        self.client_max_window_bits = bits;
        Ok(self)
    }

    /// Set compression level (0-9).
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidExtension` if level is greater than 9.
    pub fn compression_level(mut self, level: u32) -> Result<Self> {
        if level > 9 {
            return Err(Error::InvalidExtension(format!(
                "compression_level must be 0-9, got {}",
                level
            )));
        }
        self.compression_level = level;
        Ok(self)
    }
}

/// Permessage-deflate WebSocket extension (RFC 7692).
///
/// Compresses data frames to reduce bandwidth usage.
///
/// This struct maintains persistent encoder/decoder state for LZ77 context takeover,
/// allowing the compression dictionary to be reused across messages for better
/// compression ratios.
pub struct DeflateExtension {
    config: DeflateConfig,
    negotiated: bool,
    /// Whether this extension is used on the server side.
    is_server: bool,
    /// Persistent compression state for context takeover.
    encoder: Option<Compress>,
    /// Persistent decompression state for context takeover.
    decoder: Option<Decompress>,
}

impl DeflateExtension {
    /// Create a new extension with the given configuration.
    pub fn new(config: DeflateConfig, is_server: bool) -> Self {
        Self {
            config,
            negotiated: false,
            is_server,
            encoder: None,
            decoder: None,
        }
    }

    /// Create a client-side extension.
    pub fn client(config: DeflateConfig) -> Self {
        Self::new(config, false)
    }

    /// Create a server-side extension.
    pub fn server(config: DeflateConfig) -> Self {
        Self::new(config, true)
    }

    pub(crate) fn ensure_encoder(&mut self) -> Result<&mut Compress> {
        if self.encoder.is_none() {
            let window_bits = if self.is_server {
                self.config.server_max_window_bits
            } else {
                self.config.client_max_window_bits
            };
            let compression = Compression::new(self.config.compression_level);
            self.encoder = Some(Compress::new_with_window_bits(
                compression,
                false, // raw deflate, no zlib header
                window_bits,
            ));
        }
        self.encoder
            .as_mut()
            .ok_or_else(|| Error::Extension("Failed to initialize encoder".into()))
    }

    pub(crate) fn ensure_decoder(&mut self) -> Result<&mut Decompress> {
        if self.decoder.is_none() {
            let window_bits = if self.is_server {
                self.config.client_max_window_bits
            } else {
                self.config.server_max_window_bits
            };
            self.decoder = Some(Decompress::new_with_window_bits(
                false, // raw deflate, no zlib header
                window_bits,
            ));
        }
        self.decoder
            .as_mut()
            .ok_or_else(|| Error::Extension("Failed to initialize decoder".into()))
    }

    fn compress(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let encoder = self.ensure_encoder()?;
        let mut compressed = Vec::with_capacity(data.len());
        let mut input_pos = 0;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_COMPRESSION_ITERATIONS {
                return Err(Error::Extension(
                    "Compression exceeded max iterations".into(),
                ));
            }
            let remaining = &data[input_pos..];
            if remaining.is_empty() {
                break;
            }

            let old_len = compressed.len();
            compressed.resize(old_len + 4096, 0);

            let before_in = encoder.total_in();
            let before_out = encoder.total_out();

            let flush = if input_pos + remaining.len() >= data.len() {
                FlushCompress::Sync
            } else {
                FlushCompress::None
            };

            encoder
                .compress(remaining, &mut compressed[old_len..], flush)
                .map_err(|e| Error::Extension(format!("Compression failed: {}", e)))?;

            let consumed = (encoder.total_in() - before_in) as usize;
            let produced = (encoder.total_out() - before_out) as usize;

            compressed.truncate(old_len + produced);
            input_pos += consumed;

            if consumed == 0 && produced == 0 {
                break;
            }
        }

        if compressed.len() >= DEFLATE_TRAILER.len()
            && compressed[compressed.len() - 4..] == DEFLATE_TRAILER
        {
            compressed.truncate(compressed.len() - 4);
        }

        if (self.is_server && self.config.server_no_context_takeover)
            || (!self.is_server && self.config.client_no_context_takeover)
        {
            self.encoder = None;
        }

        Ok(compressed)
    }

    fn decompress(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let mut input = data.to_vec();
        input.extend_from_slice(&DEFLATE_TRAILER);

        let max_size = self.config.max_decompressed_size;
        let max_ratio_size = data.len().saturating_mul(MAX_DECOMPRESSION_RATIO);

        let decoder = self.ensure_decoder()?;
        let mut decompressed = Vec::with_capacity(data.len().min(4096));
        let mut input_pos = 0;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_COMPRESSION_ITERATIONS {
                return Err(Error::Extension(
                    "Decompression exceeded max iterations".into(),
                ));
            }
            let remaining_input = &input[input_pos..];
            if remaining_input.is_empty() {
                break;
            }

            let old_len = decompressed.len();
            decompressed.resize(old_len + 4096, 0);

            let before_in = decoder.total_in();
            let before_out = decoder.total_out();

            let status = decoder
                .decompress(
                    remaining_input,
                    &mut decompressed[old_len..],
                    FlushDecompress::Sync,
                )
                .map_err(|e| Error::Extension(format!("Decompression failed: {}", e)))?;

            let consumed = (decoder.total_in() - before_in) as usize;
            let produced = (decoder.total_out() - before_out) as usize;

            decompressed.truncate(old_len + produced);
            input_pos += consumed;

            if decompressed.len() > max_size {
                return Err(Error::Extension(format!(
                    "Decompressed size {} exceeds limit {}",
                    decompressed.len(),
                    max_size
                )));
            }

            if decompressed.len() > max_ratio_size {
                return Err(Error::Extension(format!(
                    "Decompression ratio exceeded: {}x (max {}x)",
                    decompressed.len() / data.len().max(1),
                    MAX_DECOMPRESSION_RATIO
                )));
            }

            if status == flate2::Status::StreamEnd || produced == 0 {
                break;
            }
        }

        if (self.is_server && self.config.client_no_context_takeover)
            || (!self.is_server && self.config.server_no_context_takeover)
        {
            self.decoder = None;
        }

        Ok(decompressed)
    }

    fn parse_window_bits(value: Option<&str>) -> Result<u8> {
        match value {
            Some(s) => {
                let bits: u8 = s.parse().map_err(|_| {
                    Error::InvalidExtension(format!("Invalid window bits value: {}", s))
                })?;
                if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&bits) {
                    return Err(Error::InvalidExtension(format!(
                        "Window bits must be {}-{}, got {}",
                        MIN_WINDOW_BITS, MAX_WINDOW_BITS, bits
                    )));
                }
                Ok(bits)
            }
            None => Ok(DEFAULT_WINDOW_BITS),
        }
    }

    fn should_compress_frame(&self, frame: &Frame) -> bool {
        !frame.opcode.is_control() && frame.fin && !frame.payload().is_empty()
    }
}

// SAFETY: `flate2::Compress` and `flate2::Decompress` are Send + Sync when using
// the default miniz_oxide backend (pure Rust). The zlib feature also uses thread-safe
// implementations. We verify this at compile time below.
unsafe impl Send for DeflateExtension {}
unsafe impl Sync for DeflateExtension {}

// Compile-time verification that flate2 types are Send + Sync
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}
    // These will fail to compile if Compress/Decompress are not Send+Sync
    assert_send::<flate2::Compress>();
    assert_sync::<flate2::Compress>();
    assert_send::<flate2::Decompress>();
    assert_sync::<flate2::Decompress>();
};

impl Extension for DeflateExtension {
    fn name(&self) -> &str {
        "permessage-deflate"
    }

    fn rsv_bits(&self) -> RsvBits {
        RsvBits::RSV1
    }

    fn negotiate(&mut self, params: &[ExtensionParam]) -> Result<Vec<ExtensionParam>> {
        let mut response = Vec::new();

        for param in params {
            match param.name.as_str() {
                "server_no_context_takeover" => {
                    self.config.server_no_context_takeover = true;
                    response.push(ExtensionParam::flag("server_no_context_takeover"));
                }
                "client_no_context_takeover" => {
                    self.config.client_no_context_takeover = true;
                    response.push(ExtensionParam::flag("client_no_context_takeover"));
                }
                "server_max_window_bits" => {
                    let bits = Self::parse_window_bits(param.value.as_deref())?;
                    self.config.server_max_window_bits = bits;
                    response.push(ExtensionParam::new(
                        "server_max_window_bits",
                        bits.to_string(),
                    ));
                }
                "client_max_window_bits" => {
                    let bits = if param.value.is_some() {
                        Self::parse_window_bits(param.value.as_deref())?
                    } else {
                        self.config.client_max_window_bits
                    };
                    self.config.client_max_window_bits = bits;
                    response.push(ExtensionParam::new(
                        "client_max_window_bits",
                        bits.to_string(),
                    ));
                }
                _ => {
                    return Err(Error::InvalidExtension(format!(
                        "Unknown parameter: {}",
                        param.name
                    )));
                }
            }
        }

        self.negotiated = true;
        Ok(response)
    }

    fn configure(&mut self, params: &[ExtensionParam]) -> Result<()> {
        for param in params {
            match param.name.as_str() {
                "server_no_context_takeover" => {
                    self.config.server_no_context_takeover = true;
                }
                "client_no_context_takeover" => {
                    self.config.client_no_context_takeover = true;
                }
                "server_max_window_bits" => {
                    let bits = Self::parse_window_bits(param.value.as_deref())?;
                    self.config.server_max_window_bits = bits;
                }
                "client_max_window_bits" => {
                    let bits = Self::parse_window_bits(param.value.as_deref())?;
                    self.config.client_max_window_bits = bits;
                }
                _ => {
                    return Err(Error::InvalidExtension(format!(
                        "Unknown parameter: {}",
                        param.name
                    )));
                }
            }
        }
        self.negotiated = true;
        Ok(())
    }

    fn encode(&mut self, frame: &mut Frame) -> Result<()> {
        if !self.should_compress_frame(frame) {
            return Ok(());
        }

        let compressed = self.compress(frame.payload())?;
        *frame = Frame::new(frame.fin, frame.opcode, compressed);
        frame.rsv1 = true;

        Ok(())
    }

    fn decode(&mut self, frame: &mut Frame) -> Result<()> {
        if !frame.rsv1 {
            return Ok(());
        }

        if frame.opcode.is_control() {
            return Err(Error::Extension("RSV1 set on control frame".to_string()));
        }

        let decompressed = self.decompress(frame.payload())?;
        *frame = Frame::new(frame.fin, frame.opcode, decompressed);
        frame.rsv1 = false;

        Ok(())
    }

    fn offer_params(&self) -> Vec<ExtensionParam> {
        let mut params = Vec::new();

        if self.config.server_no_context_takeover {
            params.push(ExtensionParam::flag("server_no_context_takeover"));
        }
        if self.config.client_no_context_takeover {
            params.push(ExtensionParam::flag("client_no_context_takeover"));
        }
        if self.config.server_max_window_bits != DEFAULT_WINDOW_BITS {
            params.push(ExtensionParam::new(
                "server_max_window_bits",
                self.config.server_max_window_bits.to_string(),
            ));
        }
        if self.config.client_max_window_bits != DEFAULT_WINDOW_BITS {
            params.push(ExtensionParam::new(
                "client_max_window_bits",
                self.config.client_max_window_bits.to_string(),
            ));
        }

        params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OpCode;

    #[test]
    fn test_compression_roundtrip() {
        let mut client_ext = DeflateExtension::client(DeflateConfig::default());
        let mut server_ext = DeflateExtension::server(DeflateConfig::default());
        client_ext.negotiated = true;
        server_ext.negotiated = true;

        let original_data = b"Hello, WebSocket compression! This is a test message.".to_vec();
        let mut frame = Frame::text(original_data.clone());

        client_ext.encode(&mut frame).unwrap();
        assert!(frame.rsv1);
        assert_ne!(frame.payload(), &original_data[..]);

        server_ext.decode(&mut frame).unwrap();
        assert!(!frame.rsv1);
        assert_eq!(frame.payload(), &original_data[..]);
    }

    #[test]
    fn test_parameter_negotiation() {
        let mut ext = DeflateExtension::new(DeflateConfig::default(), true);

        let params = vec![
            ExtensionParam::flag("server_no_context_takeover"),
            ExtensionParam::new("client_max_window_bits", "12"),
        ];

        let response = ext.negotiate(&params).unwrap();

        assert!(ext.config.server_no_context_takeover);
        assert_eq!(ext.config.client_max_window_bits, 12);
        assert!(
            response
                .iter()
                .any(|p| p.name == "server_no_context_takeover")
        );
        assert!(response.iter().any(|p| p.name == "client_max_window_bits"));
    }

    #[test]
    fn test_control_frame_bypass() {
        let mut ext = DeflateExtension::new(DeflateConfig::default(), false);
        ext.negotiated = true;

        let ping_data = b"ping".to_vec();
        let mut ping_frame = Frame::ping(ping_data.clone());

        ext.encode(&mut ping_frame).unwrap();
        assert!(!ping_frame.rsv1);
        assert_eq!(ping_frame.payload(), &ping_data[..]);

        let pong_data = b"pong".to_vec();
        let mut pong_frame = Frame::pong(pong_data.clone());

        ext.encode(&mut pong_frame).unwrap();
        assert!(!pong_frame.rsv1);
        assert_eq!(pong_frame.payload(), &pong_data[..]);

        let mut close_frame = Frame::close(Some(1000), "bye");
        let close_payload = close_frame.payload().to_vec();

        ext.encode(&mut close_frame).unwrap();
        assert!(!close_frame.rsv1);
        assert_eq!(close_frame.payload(), &close_payload[..]);
    }

    #[test]
    fn test_context_takeover_config() {
        let config = DeflateConfig::new()
            .server_no_context_takeover(true)
            .client_no_context_takeover(true);

        assert!(config.server_no_context_takeover);
        assert!(config.client_no_context_takeover);

        let ext = DeflateExtension::new(config, false);
        let params = ext.offer_params();

        assert!(
            params
                .iter()
                .any(|p| p.name == "server_no_context_takeover")
        );
        assert!(
            params
                .iter()
                .any(|p| p.name == "client_no_context_takeover")
        );
    }

    #[test]
    fn test_empty_payload_handling() {
        let mut ext = DeflateExtension::new(DeflateConfig::default(), false);
        ext.negotiated = true;

        let mut frame = Frame::new(true, OpCode::Text, Vec::new());

        ext.encode(&mut frame).unwrap();
        assert!(!frame.rsv1);
        assert!(frame.payload().is_empty());
    }

    #[test]
    fn test_window_bits_validation() {
        assert!(DeflateConfig::new().server_max_window_bits(8).is_ok());
        assert!(DeflateConfig::new().server_max_window_bits(15).is_ok());
        assert!(DeflateConfig::new().server_max_window_bits(7).is_err());
        assert!(DeflateConfig::new().server_max_window_bits(16).is_err());

        assert!(DeflateConfig::new().client_max_window_bits(8).is_ok());
        assert!(DeflateConfig::new().client_max_window_bits(15).is_ok());
        assert!(DeflateConfig::new().client_max_window_bits(7).is_err());
        assert!(DeflateConfig::new().client_max_window_bits(16).is_err());
    }

    #[test]
    fn test_compression_level_validation() {
        assert!(DeflateConfig::new().compression_level(0).is_ok());
        assert!(DeflateConfig::new().compression_level(9).is_ok());
        assert!(DeflateConfig::new().compression_level(10).is_err());
    }

    #[test]
    fn test_rsv1_on_control_frame_error() {
        let mut ext = DeflateExtension::new(DeflateConfig::default(), false);
        ext.negotiated = true;

        let mut frame = Frame::ping(b"test".to_vec());
        frame.rsv1 = true;

        let result = ext.decode(&mut frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_frame_compression() {
        let mut client_ext = DeflateExtension::client(DeflateConfig::default());
        let mut server_ext = DeflateExtension::server(DeflateConfig::default());
        client_ext.negotiated = true;
        server_ext.negotiated = true;

        let original_data: Vec<u8> = (0..256).map(|i| (i % 256) as u8).collect();
        let mut frame = Frame::binary(original_data.clone());

        client_ext.encode(&mut frame).unwrap();
        assert!(frame.rsv1);

        server_ext.decode(&mut frame).unwrap();
        assert_eq!(frame.payload(), &original_data[..]);
    }

    #[test]
    fn test_extension_name_and_rsv_bits() {
        let ext = DeflateExtension::new(DeflateConfig::default(), false);
        assert_eq!(ext.name(), "permessage-deflate");
        assert!(ext.rsv_bits().rsv1);
        assert!(!ext.rsv_bits().rsv2);
        assert!(!ext.rsv_bits().rsv3);
    }

    #[test]
    fn test_unknown_parameter_rejected() {
        let mut ext = DeflateExtension::new(DeflateConfig::default(), true);

        let params = vec![ExtensionParam::flag("unknown_param")];

        let result = ext.negotiate(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_window_bits_applied_to_encoder() {
        let config = DeflateConfig::new().client_max_window_bits(9).unwrap();
        let mut ext = DeflateExtension::client(config);
        ext.negotiated = true;

        // Trigger encoder creation
        let _ = ext.ensure_encoder().unwrap();
        assert!(ext.encoder.is_some());

        let mut frame = Frame::text(b"test data for compression".to_vec());
        ext.encode(&mut frame).unwrap();
        assert!(frame.rsv1);
    }

    #[test]
    fn test_window_bits_applied_to_decoder() {
        let config = DeflateConfig::new()
            .server_max_window_bits(9)
            .unwrap()
            .client_max_window_bits(9)
            .unwrap();

        let mut server_ext = DeflateExtension::server(config.clone());
        server_ext.negotiated = true;

        let mut client_ext = DeflateExtension::client(config);
        client_ext.negotiated = true;

        let data = b"This is some data that will be compressed with 9-bit window".to_vec();
        let mut frame = Frame::text(data.clone());

        server_ext.encode(&mut frame).unwrap();
        client_ext.decode(&mut frame).unwrap();

        assert_eq!(frame.payload(), &data[..]);
    }

    #[test]
    fn test_negotiated_window_bits_used() {
        let mut ext = DeflateExtension::server(DeflateConfig::default());
        let params = vec![
            ExtensionParam::new("server_max_window_bits", "9"),
            ExtensionParam::new("client_max_window_bits", "10"),
        ];
        ext.negotiate(&params).unwrap();

        assert_eq!(ext.config.server_max_window_bits, 9);
        assert_eq!(ext.config.client_max_window_bits, 10);

        let _ = ext.ensure_encoder().unwrap();
        let _ = ext.ensure_decoder().unwrap();

        assert!(ext.encoder.is_some());
        assert!(ext.decoder.is_some());
    }

    #[test]
    fn test_context_takeover_improves_compression() {
        // With context takeover enabled (default), the second identical message
        // should compress smaller because the LZ77 dictionary is preserved
        let mut client_ext = DeflateExtension::client(DeflateConfig::default());
        let mut server_ext = DeflateExtension::server(DeflateConfig::default());
        client_ext.negotiated = true;
        server_ext.negotiated = true;

        // Use a message with repeated patterns that benefits from dictionary reuse
        let message = b"The quick brown fox jumps over the lazy dog. ".repeat(10);

        // Encode first message
        let mut frame1 = Frame::text(message.clone());
        client_ext.encode(&mut frame1).unwrap();
        let first_size = frame1.payload().len();

        // Decoder must see the first message to sync its dictionary
        server_ext.decode(&mut frame1).unwrap();

        // Encode second identical message - should use preserved dictionary
        let mut frame2 = Frame::text(message.clone());
        client_ext.encode(&mut frame2).unwrap();
        let second_size = frame2.payload().len();

        // With context takeover, second message should compress at least as well
        // (usually smaller due to dictionary containing previous message's patterns)
        assert!(
            second_size <= first_size,
            "Context takeover should improve or maintain compression: second={} first={}",
            second_size,
            first_size
        );

        // Verify roundtrip works for second message
        server_ext.decode(&mut frame2).unwrap();
        assert_eq!(frame2.payload(), &message[..]);
    }

    #[test]
    fn test_no_context_takeover_resets_state() {
        // With no_context_takeover, each message starts fresh - no dictionary reuse
        let config = DeflateConfig::new().client_no_context_takeover(true);
        let mut client_ext = DeflateExtension::client(config.clone());
        let mut server_ext = DeflateExtension::server(config);
        client_ext.negotiated = true;
        server_ext.negotiated = true;

        let message = b"The quick brown fox jumps over the lazy dog. ".repeat(10);

        // Encode first message
        let mut frame1 = Frame::text(message.clone());
        client_ext.encode(&mut frame1).unwrap();
        let first_size = frame1.payload().len();
        server_ext.decode(&mut frame1).unwrap();

        // Encode second identical message - state should be reset
        let mut frame2 = Frame::text(message.clone());
        client_ext.encode(&mut frame2).unwrap();
        let second_size = frame2.payload().len();

        // With no context takeover, sizes should be the same
        assert_eq!(
            first_size, second_size,
            "No context takeover should produce same size: first={} second={}",
            first_size, second_size
        );

        // Verify roundtrip still works
        server_ext.decode(&mut frame2).unwrap();
        assert_eq!(frame2.payload(), &message[..]);
    }
}
