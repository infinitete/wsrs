//! WebSocket Extension Framework (RFC 6455 Section 9).
//!
//! This module provides a trait-based extension system for WebSocket connections.
//! Extensions can modify frames during encoding (before sending) and decoding (after receiving),
//! and participate in the handshake negotiation process.
//!
//! # Example
//!
//! ```rust,ignore
//! use rsws::extensions::{Extension, ExtensionParam, ExtensionRegistry};
//!
//! // Create a registry and add extensions
//! let mut registry = ExtensionRegistry::new();
//! registry.add(Box::new(MyExtension::new()));
//!
//! // During handshake, negotiate extensions
//! let offered = registry.offer_params();
//! // ... send in Sec-WebSocket-Extensions header ...
//!
//! // After receiving server response, configure extensions
//! registry.configure(&server_params)?;
//! ```

#[cfg(feature = "compression")]
pub mod deflate;

use crate::error::{Error, Result};
use crate::protocol::Frame;
use std::fmt;

/// Represents a single extension parameter.
///
/// Extension parameters follow the format: `name; param1=value1; param2`
/// For example: `permessage-deflate; client_max_window_bits=15; server_no_context_takeover`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionParam {
    /// Parameter name (e.g., "client_max_window_bits").
    pub name: String,
    /// Optional parameter value. None for boolean parameters.
    pub value: Option<String>,
}

impl ExtensionParam {
    /// Create a new parameter with a value.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(value.into()),
        }
    }

    /// Create a boolean/flag parameter (no value).
    pub fn flag(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: None,
        }
    }

    /// Parse a single parameter from a string (e.g., "param=value" or "param").
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if let Some((name, value)) = s.split_once('=') {
            Self {
                name: name.trim().to_string(),
                value: Some(value.trim().trim_matches('"').to_string()),
            }
        } else {
            Self::flag(s)
        }
    }
}

impl fmt::Display for ExtensionParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            Some(v) => write!(f, "{}={}", self.name, v),
            None => write!(f, "{}", self.name),
        }
    }
}

/// Parsed extension offer/response from Sec-WebSocket-Extensions header.
///
/// Represents a single extension with its name and parameters.
/// For example: `permessage-deflate; client_max_window_bits=15`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionOffer {
    /// Extension name (e.g., "permessage-deflate").
    pub name: String,
    /// Extension parameters.
    pub params: Vec<ExtensionParam>,
}

impl ExtensionOffer {
    /// Create a new extension offer with no parameters.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
        }
    }

    /// Create a new extension offer with parameters.
    pub fn with_params(name: impl Into<String>, params: Vec<ExtensionParam>) -> Self {
        Self {
            name: name.into(),
            params,
        }
    }

    /// Parse a single extension offer from a string.
    ///
    /// Format: `extension-name; param1=value1; param2`
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if the extension string or name is empty.
    pub fn parse(s: &str) -> Result<Self> {
        let mut parts = s.split(';');
        let name = parts
            .next()
            .ok_or_else(|| Error::InvalidExtension("Empty extension string".into()))?
            .trim()
            .to_string();

        if name.is_empty() {
            return Err(Error::InvalidExtension("Empty extension name".into()));
        }

        let params: Vec<ExtensionParam> = parts.map(ExtensionParam::parse).collect();

        Ok(Self { name, params })
    }

    /// Parse multiple extension offers from a Sec-WebSocket-Extensions header value.
    ///
    /// Extensions are comma-separated, parameters are semicolon-separated.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if any extension offer in the header is invalid.
    pub fn parse_header(header: &str) -> Result<Vec<Self>> {
        header.split(',').map(|s| Self::parse(s.trim())).collect()
    }

    /// Get a parameter value by name.
    pub fn get_param(&self, name: &str) -> Option<&ExtensionParam> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Check if a boolean parameter is present.
    pub fn has_param(&self, name: &str) -> bool {
        self.params.iter().any(|p| p.name == name)
    }
}

impl fmt::Display for ExtensionOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        for param in &self.params {
            write!(f, "; {}", param)?;
        }
        Ok(())
    }
}

/// RSV bit usage declaration for extensions.
///
/// Extensions must declare which RSV bits they use to prevent conflicts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RsvBits {
    /// Extension uses RSV1 bit (e.g., permessage-deflate).
    pub rsv1: bool,
    /// Extension uses RSV2 bit.
    pub rsv2: bool,
    /// Extension uses RSV3 bit.
    pub rsv3: bool,
}

impl RsvBits {
    /// No RSV bits used.
    pub const NONE: Self = Self {
        rsv1: false,
        rsv2: false,
        rsv3: false,
    };

    /// RSV1 only (used by permessage-deflate).
    pub const RSV1: Self = Self {
        rsv1: true,
        rsv2: false,
        rsv3: false,
    };

    /// Check if any bits conflict with another RsvBits declaration.
    pub fn conflicts_with(&self, other: &RsvBits) -> bool {
        (self.rsv1 && other.rsv1) || (self.rsv2 && other.rsv2) || (self.rsv3 && other.rsv3)
    }
}

/// WebSocket extension trait.
///
/// Extensions can:
/// - Negotiate parameters during the handshake
/// - Transform frames before sending (encode) and after receiving (decode)
/// - Declare which RSV bits they use
///
/// # Thread Safety
///
/// Extensions must be `Send + Sync` to work with async runtimes.
///
/// # Example Implementation
///
/// ```rust,ignore
/// struct NoOpExtension;
///
/// impl Extension for NoOpExtension {
///     fn name(&self) -> &str { "x-noop" }
///
///     fn rsv_bits(&self) -> RsvBits { RsvBits::NONE }
///
///     fn negotiate(&mut self, params: &[ExtensionParam]) -> Result<Vec<ExtensionParam>, Error> {
///         Ok(vec![]) // Accept with no parameters
///     }
///
///     fn encode(&mut self, frame: &mut Frame) -> Result<(), Error> {
///         Ok(()) // No-op
///     }
///
///     fn decode(&mut self, frame: &mut Frame) -> Result<(), Error> {
///         Ok(()) // No-op
///     }
/// }
/// ```
pub trait Extension: Send + Sync {
    /// Returns the extension name as used in Sec-WebSocket-Extensions header.
    ///
    /// This must match the registered extension name (e.g., "permessage-deflate").
    fn name(&self) -> &str;

    /// Returns which RSV bits this extension uses.
    ///
    /// Extensions must not use bits already claimed by other extensions.
    fn rsv_bits(&self) -> RsvBits {
        RsvBits::NONE
    }

    /// Negotiate extension parameters during handshake.
    ///
    /// Called when the extension is offered by the peer. The extension should:
    /// - Validate the offered parameters
    /// - Return accepted parameters to include in the response
    /// - Return an error to reject the extension offer
    ///
    /// # Arguments
    ///
    /// * `params` - Parameters offered by the peer
    ///
    /// # Returns
    ///
    /// * `Ok(params)` - Extension accepted with these response parameters
    /// * `Err(_)` - Extension rejected
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if the offered parameters are invalid or
    /// incompatible with this extension.
    fn negotiate(&mut self, params: &[ExtensionParam]) -> Result<Vec<ExtensionParam>>;

    /// Configure the extension after successful negotiation.
    ///
    /// Called with the final negotiated parameters. Use this to set up
    /// internal state based on the agreed parameters.
    ///
    /// Default implementation does nothing.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if the negotiated parameters are invalid
    /// for this extension's configuration.
    fn configure(&mut self, _params: &[ExtensionParam]) -> Result<()> {
        Ok(())
    }

    /// Encode a frame before sending.
    ///
    /// Extensions are applied in registration order for encoding.
    /// This method may modify the frame's payload and RSV bits.
    ///
    /// # Arguments
    ///
    /// * `frame` - The frame to encode (mutable)
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extension`] if an error occurs during frame transformation
    /// (e.g., compression failure).
    ///
    /// # Notes
    ///
    /// - Control frames (Close, Ping, Pong) should typically not be modified
    /// - Set appropriate RSV bits when compressing/transforming data
    fn encode(&mut self, frame: &mut Frame) -> Result<()>;

    /// Decode a frame after receiving.
    ///
    /// Extensions are applied in reverse registration order for decoding.
    /// This method may modify the frame's payload and should clear RSV bits it handles.
    ///
    /// # Arguments
    ///
    /// * `frame` - The frame to decode (mutable)
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extension`] if an error occurs during frame transformation
    /// (e.g., decompression failure).
    ///
    /// # Notes
    ///
    /// - Check RSV bits to determine if extension processing is needed
    /// - Clear RSV bits after processing to prevent validation errors
    fn decode(&mut self, frame: &mut Frame) -> Result<()>;

    /// Generate parameters to offer during client handshake.
    ///
    /// Returns the parameters to include in the Sec-WebSocket-Extensions
    /// header when initiating a connection.
    ///
    /// Default returns empty (no parameters offered).
    fn offer_params(&self) -> Vec<ExtensionParam> {
        Vec::new()
    }
}

/// Registry for managing multiple WebSocket extensions.
///
/// The registry handles:
/// - Extension registration and ordering
/// - RSV bit conflict detection
/// - Handshake negotiation
/// - Frame encoding/decoding pipeline
#[derive(Default)]
pub struct ExtensionRegistry {
    /// Registered extensions in order.
    extensions: Vec<Box<dyn Extension>>,
    /// Combined RSV bit usage.
    used_rsv_bits: RsvBits,
    /// Extensions that were successfully negotiated.
    negotiated: Vec<usize>,
}

impl ExtensionRegistry {
    /// Create a new empty extension registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an extension to the registry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if the extension's RSV bits conflict with already
    /// registered extensions.
    pub fn add(&mut self, extension: Box<dyn Extension>) -> Result<()> {
        let rsv = extension.rsv_bits();

        if self.used_rsv_bits.conflicts_with(&rsv) {
            return Err(Error::InvalidExtension(format!(
                "Extension '{}' RSV bits conflict with existing extensions",
                extension.name()
            )));
        }

        self.used_rsv_bits.rsv1 |= rsv.rsv1;
        self.used_rsv_bits.rsv2 |= rsv.rsv2;
        self.used_rsv_bits.rsv3 |= rsv.rsv3;

        self.extensions.push(extension);
        Ok(())
    }

    /// Get the number of registered extensions.
    pub fn len(&self) -> usize {
        self.extensions.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    /// Get the number of successfully negotiated extensions.
    pub fn negotiated_count(&self) -> usize {
        self.negotiated.len()
    }

    /// Generate the Sec-WebSocket-Extensions header value for client handshake.
    ///
    /// Returns a comma-separated list of extension offers.
    pub fn offer_header(&self) -> String {
        self.extensions
            .iter()
            .map(|ext| {
                let params = ext.offer_params();
                let mut offer = ext.name().to_string();
                for param in params {
                    offer.push_str("; ");
                    offer.push_str(&param.to_string());
                }
                offer
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Negotiate extensions based on offers from the peer (server-side).
    ///
    /// Processes each offer and returns the accepted extensions.
    ///
    /// # Arguments
    ///
    /// * `offers` - Extension offers from the client's Sec-WebSocket-Extensions header
    ///
    /// # Returns
    ///
    /// A list of accepted extension responses to include in the server's response.
    pub fn negotiate(&mut self, offers: &[ExtensionOffer]) -> Vec<ExtensionOffer> {
        let mut accepted = Vec::new();
        self.negotiated.clear();

        for offer in offers {
            // Find matching registered extension
            if let Some((idx, ext)) = self
                .extensions
                .iter_mut()
                .enumerate()
                .find(|(_, e)| e.name() == offer.name)
            {
                // Try to negotiate
                if let Ok(response_params) = ext.negotiate(&offer.params) {
                    // Configure with final params
                    if ext.configure(&response_params).is_ok() {
                        self.negotiated.push(idx);
                        accepted.push(ExtensionOffer::with_params(
                            offer.name.clone(),
                            response_params,
                        ));
                    }
                }
            }
        }

        accepted
    }

    /// Configure extensions based on server response (client-side).
    ///
    /// # Arguments
    ///
    /// * `responses` - Extension responses from server's Sec-WebSocket-Extensions header
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidExtension`] if any extension fails to configure with
    /// the provided parameters.
    pub fn configure(&mut self, responses: &[ExtensionOffer]) -> Result<()> {
        self.negotiated.clear();

        for response in responses {
            if let Some((idx, ext)) = self
                .extensions
                .iter_mut()
                .enumerate()
                .find(|(_, e)| e.name() == response.name)
            {
                ext.configure(&response.params)?;
                self.negotiated.push(idx);
            }
        }

        Ok(())
    }

    /// Encode a frame through all negotiated extensions.
    ///
    /// Extensions are applied in registration order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extension`] if any extension fails to encode the frame.
    pub fn encode(&mut self, frame: &mut Frame) -> Result<()> {
        for &idx in &self.negotiated {
            self.extensions[idx].encode(frame)?;
        }
        Ok(())
    }

    /// Decode a frame through all negotiated extensions.
    ///
    /// Extensions are applied in reverse registration order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extension`] if any extension fails to decode the frame.
    pub fn decode(&mut self, frame: &mut Frame) -> Result<()> {
        for &idx in self.negotiated.iter().rev() {
            self.extensions[idx].decode(frame)?;
        }
        Ok(())
    }

    /// Format accepted extensions for Sec-WebSocket-Extensions response header.
    pub fn response_header(&self, accepted: &[ExtensionOffer]) -> String {
        accepted
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Debug for ExtensionRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionRegistry")
            .field(
                "extensions",
                &self.extensions.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .field("used_rsv_bits", &self.used_rsv_bits)
            .field("negotiated", &self.negotiated)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OpCode;

    // ==========================================================================
    // Mock/NoOp Extension for testing
    // ==========================================================================

    /// A no-op extension for testing the framework.
    struct NoOpExtension {
        name: String,
        rsv_bits: RsvBits,
        encode_called: std::cell::Cell<usize>,
        decode_called: std::cell::Cell<usize>,
    }

    impl NoOpExtension {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                rsv_bits: RsvBits::NONE,
                encode_called: std::cell::Cell::new(0),
                decode_called: std::cell::Cell::new(0),
            }
        }

        fn with_rsv1(mut self) -> Self {
            self.rsv_bits = RsvBits::RSV1;
            self
        }
    }

    // NoOpExtension is not Sync due to Cell, but for tests we wrap it
    unsafe impl Sync for NoOpExtension {}

    impl Extension for NoOpExtension {
        fn name(&self) -> &str {
            &self.name
        }

        fn rsv_bits(&self) -> RsvBits {
            self.rsv_bits
        }

        fn negotiate(&mut self, _params: &[ExtensionParam]) -> Result<Vec<ExtensionParam>> {
            Ok(vec![])
        }

        fn encode(&mut self, _frame: &mut Frame) -> Result<()> {
            self.encode_called.set(self.encode_called.get() + 1);
            Ok(())
        }

        fn decode(&mut self, _frame: &mut Frame) -> Result<()> {
            self.decode_called.set(self.decode_called.get() + 1);
            Ok(())
        }

        fn offer_params(&self) -> Vec<ExtensionParam> {
            vec![ExtensionParam::flag("test-param")]
        }
    }

    // ==========================================================================
    // ExtensionParam Tests
    // ==========================================================================

    #[test]
    fn test_extension_param_new() {
        let param = ExtensionParam::new("client_max_window_bits", "15");
        assert_eq!(param.name, "client_max_window_bits");
        assert_eq!(param.value, Some("15".to_string()));
    }

    #[test]
    fn test_extension_param_flag() {
        let param = ExtensionParam::flag("server_no_context_takeover");
        assert_eq!(param.name, "server_no_context_takeover");
        assert_eq!(param.value, None);
    }

    #[test]
    fn test_extension_param_parse_with_value() {
        let param = ExtensionParam::parse("client_max_window_bits=15");
        assert_eq!(param.name, "client_max_window_bits");
        assert_eq!(param.value, Some("15".to_string()));
    }

    #[test]
    fn test_extension_param_parse_flag() {
        let param = ExtensionParam::parse("server_no_context_takeover");
        assert_eq!(param.name, "server_no_context_takeover");
        assert_eq!(param.value, None);
    }

    #[test]
    fn test_extension_param_parse_quoted_value() {
        let param = ExtensionParam::parse("param=\"quoted value\"");
        assert_eq!(param.name, "param");
        assert_eq!(param.value, Some("quoted value".to_string()));
    }

    #[test]
    fn test_extension_param_display() {
        let param = ExtensionParam::new("bits", "15");
        assert_eq!(param.to_string(), "bits=15");

        let flag = ExtensionParam::flag("no_context");
        assert_eq!(flag.to_string(), "no_context");
    }

    // ==========================================================================
    // ExtensionOffer Tests
    // ==========================================================================

    #[test]
    fn test_extension_offer_parse_simple() {
        let offer = ExtensionOffer::parse("permessage-deflate").unwrap();
        assert_eq!(offer.name, "permessage-deflate");
        assert!(offer.params.is_empty());
    }

    #[test]
    fn test_extension_offer_parse_with_params() {
        let offer = ExtensionOffer::parse("permessage-deflate; client_max_window_bits=15").unwrap();
        assert_eq!(offer.name, "permessage-deflate");
        assert_eq!(offer.params.len(), 1);
        assert_eq!(offer.params[0].name, "client_max_window_bits");
        assert_eq!(offer.params[0].value, Some("15".to_string()));
    }

    #[test]
    fn test_extension_offer_parse_multiple_params() {
        let offer = ExtensionOffer::parse(
            "permessage-deflate; client_max_window_bits=15; server_no_context_takeover",
        )
        .unwrap();
        assert_eq!(offer.name, "permessage-deflate");
        assert_eq!(offer.params.len(), 2);
        assert_eq!(offer.params[0].name, "client_max_window_bits");
        assert_eq!(offer.params[1].name, "server_no_context_takeover");
        assert_eq!(offer.params[1].value, None);
    }

    #[test]
    fn test_extension_offer_parse_header() {
        let offers = ExtensionOffer::parse_header(
            "permessage-deflate; client_max_window_bits, x-webkit-deflate-frame",
        )
        .unwrap();
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].name, "permessage-deflate");
        assert_eq!(offers[1].name, "x-webkit-deflate-frame");
    }

    #[test]
    fn test_extension_offer_get_param() {
        let offer = ExtensionOffer::parse("ext; param1=value1; param2").unwrap();

        let param = offer.get_param("param1").unwrap();
        assert_eq!(param.value, Some("value1".to_string()));

        assert!(offer.has_param("param2"));
        assert!(!offer.has_param("param3"));
    }

    #[test]
    fn test_extension_offer_display() {
        let offer = ExtensionOffer::with_params(
            "permessage-deflate",
            vec![
                ExtensionParam::new("client_max_window_bits", "15"),
                ExtensionParam::flag("server_no_context_takeover"),
            ],
        );
        assert_eq!(
            offer.to_string(),
            "permessage-deflate; client_max_window_bits=15; server_no_context_takeover"
        );
    }

    #[test]
    fn test_extension_offer_parse_empty_name_error() {
        let result = ExtensionOffer::parse("");
        assert!(result.is_err());
    }

    // ==========================================================================
    // RsvBits Tests
    // ==========================================================================

    #[test]
    fn test_rsv_bits_none() {
        let bits = RsvBits::NONE;
        assert!(!bits.rsv1);
        assert!(!bits.rsv2);
        assert!(!bits.rsv3);
    }

    #[test]
    fn test_rsv_bits_rsv1() {
        let bits = RsvBits::RSV1;
        assert!(bits.rsv1);
        assert!(!bits.rsv2);
        assert!(!bits.rsv3);
    }

    #[test]
    fn test_rsv_bits_conflicts() {
        let rsv1 = RsvBits::RSV1;
        let rsv1_again = RsvBits::RSV1;
        let none = RsvBits::NONE;

        assert!(rsv1.conflicts_with(&rsv1_again));
        assert!(!rsv1.conflicts_with(&none));
        assert!(!none.conflicts_with(&none));
    }

    // ==========================================================================
    // Extension Trait Tests (via NoOpExtension)
    // ==========================================================================

    #[test]
    fn test_extension_trait_name() {
        let ext = NoOpExtension::new("test-extension");
        assert_eq!(ext.name(), "test-extension");
    }

    #[test]
    fn test_extension_trait_rsv_bits() {
        let ext = NoOpExtension::new("test").with_rsv1();
        assert!(ext.rsv_bits().rsv1);
    }

    #[test]
    fn test_extension_trait_negotiate() {
        let mut ext = NoOpExtension::new("test");
        let params = ext.negotiate(&[]).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_extension_trait_encode_decode() {
        let mut ext = NoOpExtension::new("test");
        let mut frame = Frame::text(b"hello".to_vec());

        ext.encode(&mut frame).unwrap();
        assert_eq!(ext.encode_called.get(), 1);

        ext.decode(&mut frame).unwrap();
        assert_eq!(ext.decode_called.get(), 1);
    }

    #[test]
    fn test_extension_offer_params() {
        let ext = NoOpExtension::new("test");
        let params = ext.offer_params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "test-param");
    }

    // ==========================================================================
    // ExtensionRegistry Tests
    // ==========================================================================

    #[test]
    fn test_registry_new() {
        let registry = ExtensionRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_add_extension() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("test"))).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_registry_add_multiple_extensions() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("ext1"))).unwrap();
        registry.add(Box::new(NoOpExtension::new("ext2"))).unwrap();
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_registry_rsv_conflict_detection() {
        let mut registry = ExtensionRegistry::new();
        registry
            .add(Box::new(NoOpExtension::new("ext1").with_rsv1()))
            .unwrap();

        // Adding another extension that uses RSV1 should fail
        let result = registry.add(Box::new(NoOpExtension::new("ext2").with_rsv1()));
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_offer_header() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("ext1"))).unwrap();
        registry.add(Box::new(NoOpExtension::new("ext2"))).unwrap();

        let header = registry.offer_header();
        assert!(header.contains("ext1"));
        assert!(header.contains("ext2"));
        assert!(header.contains("test-param"));
    }

    #[test]
    fn test_registry_negotiate() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("ext1"))).unwrap();
        registry.add(Box::new(NoOpExtension::new("ext2"))).unwrap();

        let offers = vec![
            ExtensionOffer::new("ext1"),
            ExtensionOffer::new("unknown"),
            ExtensionOffer::new("ext2"),
        ];

        let accepted = registry.negotiate(&offers);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0].name, "ext1");
        assert_eq!(accepted[1].name, "ext2");
        assert_eq!(registry.negotiated_count(), 2);
    }

    #[test]
    fn test_registry_negotiate_unknown_extension() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("known"))).unwrap();

        let offers = vec![ExtensionOffer::new("unknown")];
        let accepted = registry.negotiate(&offers);
        assert!(accepted.is_empty());
        assert_eq!(registry.negotiated_count(), 0);
    }

    #[test]
    fn test_registry_encode_decode_pipeline() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("ext1"))).unwrap();
        registry.add(Box::new(NoOpExtension::new("ext2"))).unwrap();

        // Negotiate to activate extensions
        let offers = vec![ExtensionOffer::new("ext1"), ExtensionOffer::new("ext2")];
        registry.negotiate(&offers);

        let mut frame = Frame::text(b"test".to_vec());

        registry.encode(&mut frame).unwrap();
        registry.decode(&mut frame).unwrap();

        // Both extensions should have been called
        assert_eq!(registry.negotiated_count(), 2);
    }

    #[test]
    fn test_registry_configure_client_side() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("ext1"))).unwrap();

        let responses = vec![ExtensionOffer::new("ext1")];
        registry.configure(&responses).unwrap();

        assert_eq!(registry.negotiated_count(), 1);
    }

    #[test]
    fn test_registry_response_header() {
        let registry = ExtensionRegistry::new();
        let accepted = vec![ExtensionOffer::with_params(
            "permessage-deflate",
            vec![ExtensionParam::new("client_max_window_bits", "15")],
        )];

        let header = registry.response_header(&accepted);
        assert_eq!(header, "permessage-deflate; client_max_window_bits=15");
    }

    #[test]
    fn test_registry_debug() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("test"))).unwrap();

        let debug = format!("{:?}", registry);
        assert!(debug.contains("ExtensionRegistry"));
        assert!(debug.contains("test"));
    }

    // ==========================================================================
    // Integration Tests
    // ==========================================================================

    #[test]
    fn test_full_negotiation_flow() {
        // Simulate client-server extension negotiation

        // Client side: create registry and generate offer
        let mut client_registry = ExtensionRegistry::new();
        client_registry
            .add(Box::new(NoOpExtension::new("permessage-deflate")))
            .unwrap();

        let offer_header = client_registry.offer_header();
        assert!(offer_header.contains("permessage-deflate"));

        // Server side: parse offers and negotiate
        let mut server_registry = ExtensionRegistry::new();
        server_registry
            .add(Box::new(NoOpExtension::new("permessage-deflate")))
            .unwrap();

        let client_offers = ExtensionOffer::parse_header(&offer_header).unwrap();
        let accepted = server_registry.negotiate(&client_offers);
        assert_eq!(accepted.len(), 1);

        // Client side: configure with server response
        client_registry.configure(&accepted).unwrap();
        assert_eq!(client_registry.negotiated_count(), 1);
    }

    #[test]
    fn test_frame_not_modified_by_noop() {
        let mut registry = ExtensionRegistry::new();
        registry.add(Box::new(NoOpExtension::new("noop"))).unwrap();

        let offers = vec![ExtensionOffer::new("noop")];
        registry.negotiate(&offers);

        let mut frame = Frame::new(true, OpCode::Text, b"original".to_vec());
        let original_payload = frame.payload().to_vec();

        registry.encode(&mut frame).unwrap();
        registry.decode(&mut frame).unwrap();

        // NoOp should not change the payload
        assert_eq!(frame.payload(), &original_payload[..]);
    }
}
