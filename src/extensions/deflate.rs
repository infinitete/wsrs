//! Permessage-deflate WebSocket compression extension (RFC 7692).

use crate::error::{Error, Result};
use crate::extensions::{Extension, ExtensionParam, RsvBits};
use crate::protocol::Frame;
use flate2::Compression;
use flate2::read::{DeflateDecoder, DeflateEncoder};
use std::io::Read;

const MIN_WINDOW_BITS: u8 = 8;
const MAX_WINDOW_BITS: u8 = 15;
const DEFAULT_WINDOW_BITS: u8 = 15;
const DEFLATE_TRAILER: [u8; 4] = [0x00, 0x00, 0xff, 0xff];

#[derive(Debug, Clone)]
pub struct DeflateConfig {
    pub server_no_context_takeover: bool,
    pub client_no_context_takeover: bool,
    pub server_max_window_bits: u8,
    pub client_max_window_bits: u8,
    pub compression_level: u32,
}

impl Default for DeflateConfig {
    fn default() -> Self {
        Self {
            server_no_context_takeover: false,
            client_no_context_takeover: false,
            server_max_window_bits: DEFAULT_WINDOW_BITS,
            client_max_window_bits: DEFAULT_WINDOW_BITS,
            compression_level: 6,
        }
    }
}

impl DeflateConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn server_no_context_takeover(mut self, value: bool) -> Self {
        self.server_no_context_takeover = value;
        self
    }

    pub fn client_no_context_takeover(mut self, value: bool) -> Self {
        self.client_no_context_takeover = value;
        self
    }

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

pub struct DeflateExtension {
    config: DeflateConfig,
    negotiated: bool,
}

impl DeflateExtension {
    pub fn new(config: DeflateConfig) -> Self {
        Self {
            config,
            negotiated: false,
        }
    }

    pub fn client(config: DeflateConfig) -> Self {
        Self {
            config,
            negotiated: false,
        }
    }

    pub fn server(config: DeflateConfig) -> Self {
        Self {
            config,
            negotiated: false,
        }
    }

    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let compression = Compression::new(self.config.compression_level);
        let mut encoder = DeflateEncoder::new(data, compression);
        let mut compressed = Vec::new();
        encoder
            .read_to_end(&mut compressed)
            .map_err(|e| Error::Extension(format!("Compression failed: {}", e)))?;

        if compressed.len() >= DEFLATE_TRAILER.len()
            && compressed[compressed.len() - 4..] == DEFLATE_TRAILER
        {
            compressed.truncate(compressed.len() - 4);
        }

        Ok(compressed)
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let mut with_trailer = data.to_vec();
        with_trailer.extend_from_slice(&DEFLATE_TRAILER);

        let mut decoder = DeflateDecoder::new(with_trailer.as_slice());
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| Error::Extension(format!("Decompression failed: {}", e)))?;

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

unsafe impl Send for DeflateExtension {}
unsafe impl Sync for DeflateExtension {}

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
                _ => {}
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
        let mut ext = DeflateExtension::new(DeflateConfig::default());
        ext.negotiated = true;

        let original_data = b"Hello, WebSocket compression! This is a test message.".to_vec();
        let mut frame = Frame::text(original_data.clone());

        ext.encode(&mut frame).unwrap();
        assert!(frame.rsv1);
        assert_ne!(frame.payload(), &original_data[..]);

        ext.decode(&mut frame).unwrap();
        assert!(!frame.rsv1);
        assert_eq!(frame.payload(), &original_data[..]);
    }

    #[test]
    fn test_parameter_negotiation() {
        let mut ext = DeflateExtension::new(DeflateConfig::default());

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
        let mut ext = DeflateExtension::new(DeflateConfig::default());
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

        let ext = DeflateExtension::new(config);
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
        let mut ext = DeflateExtension::new(DeflateConfig::default());
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
        let mut ext = DeflateExtension::new(DeflateConfig::default());
        ext.negotiated = true;

        let mut frame = Frame::ping(b"test".to_vec());
        frame.rsv1 = true;

        let result = ext.decode(&mut frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_frame_compression() {
        let mut ext = DeflateExtension::new(DeflateConfig::default());
        ext.negotiated = true;

        let original_data: Vec<u8> = (0..256).map(|i| (i % 256) as u8).collect();
        let mut frame = Frame::binary(original_data.clone());

        ext.encode(&mut frame).unwrap();
        assert!(frame.rsv1);

        ext.decode(&mut frame).unwrap();
        assert_eq!(frame.payload(), &original_data[..]);
    }

    #[test]
    fn test_extension_name_and_rsv_bits() {
        let ext = DeflateExtension::new(DeflateConfig::default());
        assert_eq!(ext.name(), "permessage-deflate");
        assert!(ext.rsv_bits().rsv1);
        assert!(!ext.rsv_bits().rsv2);
        assert!(!ext.rsv_bits().rsv3);
    }

    #[test]
    fn test_unknown_parameter_rejected() {
        let mut ext = DeflateExtension::new(DeflateConfig::default());

        let params = vec![ExtensionParam::flag("unknown_param")];

        let result = ext.negotiate(&params);
        assert!(result.is_err());
    }
}
