//! WebSocket handshake implementation (RFC 6455).
//!
//! This module handles the HTTP Upgrade mechanism for establishing WebSocket connections.

use crate::error::{Error, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use sha1::{Digest, Sha1};
use std::collections::HashMap;

/// The WebSocket GUID used in the Sec-WebSocket-Accept calculation (RFC 6455).
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Computes the Sec-WebSocket-Accept value from the client's Sec-WebSocket-Key.
///
/// The accept key is calculated as: Base64(SHA-1(key + GUID))
///
/// # Example
///
/// ```
/// use rsws::protocol::handshake::compute_accept_key;
///
/// let key = "dGhlIHNhbXBsZSBub25jZQ==";
/// let accept = compute_accept_key(key);
/// assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
/// ```
pub fn compute_accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let hash = hasher.finalize();
    BASE64.encode(hash)
}

/// Parsed WebSocket handshake request from client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeRequest {
    /// The request path (e.g., "/chat").
    pub path: String,
    /// The Host header value.
    pub host: String,
    /// The Sec-WebSocket-Key header value.
    pub key: String,
    /// The Sec-WebSocket-Version (should be 13).
    pub version: u8,
    /// The Origin header value (optional).
    pub origin: Option<String>,
    /// The Sec-WebSocket-Protocol values (optional).
    pub protocols: Vec<String>,
    /// The Sec-WebSocket-Extensions values (optional).
    pub extensions: Vec<String>,
}

impl HandshakeRequest {
    /// Parse a WebSocket handshake request from raw HTTP data.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidHandshake`] if:
    /// - The data is not valid UTF-8.
    /// - The request line is malformed or missing.
    /// - The HTTP method is not `GET`.
    /// - The HTTP version is not `HTTP/1.1`.
    /// - Any required headers are missing: `Upgrade`, `Connection`, `Host`, `Sec-WebSocket-Key`, `Sec-WebSocket-Version`.
    /// - The `Upgrade` header is not `websocket`.
    /// - The `Connection` header does not contain `upgrade`.
    /// - The `Sec-WebSocket-Version` is not a valid integer.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::InvalidHandshake("Invalid UTF-8".into()))?;

        let mut lines = text.lines();

        // Parse request line: "GET /path HTTP/1.1"
        let request_line = lines
            .next()
            .ok_or_else(|| Error::InvalidHandshake("Empty request".into()))?;

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(Error::InvalidHandshake("Invalid request line".into()));
        }

        if parts[0] != "GET" {
            return Err(Error::InvalidHandshake(format!(
                "Expected GET method, got {}",
                parts[0]
            )));
        }

        if !parts[2].starts_with("HTTP/1.1") {
            return Err(Error::InvalidHandshake(format!(
                "Expected HTTP/1.1, got {}",
                parts[2]
            )));
        }

        let path = parts[1].to_string();

        // Parse headers into a case-insensitive map
        let mut headers: HashMap<String, String> = HashMap::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_lowercase(), value.trim().to_string());
            }
        }

        // Validate Upgrade header
        let upgrade = headers
            .get("upgrade")
            .ok_or_else(|| Error::InvalidHandshake("Missing Upgrade header".into()))?;
        if !upgrade.eq_ignore_ascii_case("websocket") {
            return Err(Error::InvalidHandshake(format!(
                "Invalid Upgrade header: {}",
                upgrade
            )));
        }

        // Validate Connection header
        let connection = headers
            .get("connection")
            .ok_or_else(|| Error::InvalidHandshake("Missing Connection header".into()))?;
        if !connection.to_lowercase().contains("upgrade") {
            return Err(Error::InvalidHandshake(format!(
                "Invalid Connection header: {}",
                connection
            )));
        }

        // Extract Host header
        let host = headers
            .get("host")
            .ok_or_else(|| Error::InvalidHandshake("Missing Host header".into()))?
            .clone();

        // Extract Sec-WebSocket-Key
        let key = headers
            .get("sec-websocket-key")
            .ok_or_else(|| Error::InvalidHandshake("Missing Sec-WebSocket-Key header".into()))?
            .clone();

        // Extract Sec-WebSocket-Version
        let version_str = headers.get("sec-websocket-version").ok_or_else(|| {
            Error::InvalidHandshake("Missing Sec-WebSocket-Version header".into())
        })?;
        let version: u8 = version_str
            .parse()
            .map_err(|_| Error::InvalidHandshake(format!("Invalid version: {}", version_str)))?;

        // Extract optional Origin
        let origin = headers.get("origin").cloned();

        // Extract optional Sec-WebSocket-Protocol (comma-separated)
        let protocols = headers
            .get("sec-websocket-protocol")
            .map(|p| p.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        // Extract optional Sec-WebSocket-Extensions (comma-separated)
        let extensions = headers
            .get("sec-websocket-extensions")
            .map(|e| e.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        Ok(Self {
            path,
            host,
            key,
            version,
            origin,
            protocols,
            extensions,
        })
    }

    /// Validate the handshake request according to RFC 6455.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidHandshake`] if:
    /// - The WebSocket version is not 13.
    /// - The `Sec-WebSocket-Key` is not valid Base64.
    /// - The decoded `Sec-WebSocket-Key` is not exactly 16 bytes.
    /// - The `Host` header is empty.
    pub fn validate(&self) -> Result<()> {
        // Version must be 13
        if self.version != 13 {
            return Err(Error::InvalidHandshake(format!(
                "Unsupported WebSocket version: {} (expected 13)",
                self.version
            )));
        }

        // Key must be 16 bytes when decoded (24 chars base64 with padding)
        match BASE64.decode(&self.key) {
            Ok(decoded) => {
                if decoded.len() != 16 {
                    return Err(Error::InvalidHandshake(format!(
                        "Sec-WebSocket-Key must be 16 bytes, got {}",
                        decoded.len()
                    )));
                }
            }
            Err(_) => {
                return Err(Error::InvalidHandshake(
                    "Invalid Sec-WebSocket-Key: not valid Base64".into(),
                ));
            }
        }

        // Host must not be empty
        if self.host.is_empty() {
            return Err(Error::InvalidHandshake(
                "Host header cannot be empty".into(),
            ));
        }

        Ok(())
    }
}

/// WebSocket handshake response from server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResponse {
    /// The Sec-WebSocket-Accept value.
    pub accept: String,
    /// The selected Sec-WebSocket-Protocol (optional).
    pub protocol: Option<String>,
    /// The negotiated Sec-WebSocket-Extensions (optional).
    pub extensions: Vec<String>,
}

impl HandshakeResponse {
    /// Create a handshake response from a validated request.
    pub fn from_request(req: &HandshakeRequest) -> Self {
        Self {
            accept: compute_accept_key(&req.key),
            protocol: req.protocols.first().cloned(),
            extensions: Vec::new(), // No extensions supported yet
        }
    }

    /// Write the HTTP response to a buffer.
    pub fn write(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(b"HTTP/1.1 101 Switching Protocols\r\n");
        buf.extend_from_slice(b"Upgrade: websocket\r\n");
        buf.extend_from_slice(b"Connection: Upgrade\r\n");
        buf.extend_from_slice(format!("Sec-WebSocket-Accept: {}\r\n", self.accept).as_bytes());

        if let Some(ref proto) = self.protocol {
            buf.extend_from_slice(format!("Sec-WebSocket-Protocol: {}\r\n", proto).as_bytes());
        }

        for ext in &self.extensions {
            buf.extend_from_slice(format!("Sec-WebSocket-Extensions: {}\r\n", ext).as_bytes());
        }

        buf.extend_from_slice(b"\r\n");
    }

    /// Parse a WebSocket handshake response from raw HTTP data.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidHandshake`] if:
    /// - The data is not valid UTF-8.
    /// - The response is empty or the status line is missing.
    /// - The status code is not `101 Switching Protocols`.
    /// - Any required headers are missing: `Upgrade`, `Connection`, `Sec-WebSocket-Accept`.
    /// - The `Upgrade` header is not `websocket`.
    /// - The `Connection` header does not contain `upgrade`.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::InvalidHandshake("Invalid UTF-8".into()))?;

        let mut lines = text.lines();

        // Parse status line: "HTTP/1.1 101 Switching Protocols"
        let status_line = lines
            .next()
            .ok_or_else(|| Error::InvalidHandshake("Empty response".into()))?;

        if !status_line.starts_with("HTTP/1.1 101") {
            return Err(Error::InvalidHandshake(format!(
                "Expected 101 status, got: {}",
                status_line
            )));
        }

        // Parse headers
        let mut headers: HashMap<String, String> = HashMap::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_lowercase(), value.trim().to_string());
            }
        }

        // Validate Upgrade header
        let upgrade = headers
            .get("upgrade")
            .ok_or_else(|| Error::InvalidHandshake("Missing Upgrade header in response".into()))?;
        if !upgrade.eq_ignore_ascii_case("websocket") {
            return Err(Error::InvalidHandshake(format!(
                "Invalid Upgrade header: {}",
                upgrade
            )));
        }

        // Validate Connection header
        let connection = headers.get("connection").ok_or_else(|| {
            Error::InvalidHandshake("Missing Connection header in response".into())
        })?;
        if !connection.to_lowercase().contains("upgrade") {
            return Err(Error::InvalidHandshake(format!(
                "Invalid Connection header: {}",
                connection
            )));
        }

        // Extract Sec-WebSocket-Accept
        let accept = headers
            .get("sec-websocket-accept")
            .ok_or_else(|| Error::InvalidHandshake("Missing Sec-WebSocket-Accept header".into()))?
            .clone();

        // Extract optional protocol
        let protocol = headers.get("sec-websocket-protocol").cloned();

        // Extract optional extensions
        let extensions = headers
            .get("sec-websocket-extensions")
            .map(|e| e.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        Ok(Self {
            accept,
            protocol,
            extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: RFC 6455 example verification
    #[test]
    fn test_compute_accept_key_rfc_example() {
        // RFC 6455 Section 1.3 example
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let expected = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";
        assert_eq!(compute_accept_key(key), expected);
    }

    // Test 2: Full client request parsing
    #[test]
    fn test_parse_valid_request() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            Origin: http://example.com\r\n\
            Sec-WebSocket-Protocol: chat, superchat\r\n\
            \r\n";

        let req = HandshakeRequest::parse(request).unwrap();
        assert_eq!(req.path, "/chat");
        assert_eq!(req.host, "server.example.com");
        assert_eq!(req.key, "dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(req.version, 13);
        assert_eq!(req.origin, Some("http://example.com".to_string()));
        assert_eq!(req.protocols, vec!["chat", "superchat"]);
    }

    // Test 3: Missing Sec-WebSocket-Key
    #[test]
    fn test_parse_request_missing_key() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let result = HandshakeRequest::parse(request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("Sec-WebSocket-Key")));
    }

    // Test 4: Missing Upgrade header
    #[test]
    fn test_parse_request_missing_upgrade() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let result = HandshakeRequest::parse(request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("Upgrade")));
    }

    // Test 5: Wrong WebSocket version
    #[test]
    fn test_parse_request_wrong_version() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 8\r\n\
            \r\n";

        let req = HandshakeRequest::parse(request).unwrap();
        let result = req.validate();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("version")));
    }

    // Test 6: Validation rules
    #[test]
    fn test_validate_request() {
        // Valid request
        let valid_req = HandshakeRequest {
            path: "/chat".to_string(),
            host: "example.com".to_string(),
            key: "dGhlIHNhbXBsZSBub25jZQ==".to_string(), // 16 bytes when decoded
            version: 13,
            origin: None,
            protocols: vec![],
            extensions: vec![],
        };
        assert!(valid_req.validate().is_ok());

        // Invalid key length
        let invalid_key_req = HandshakeRequest {
            key: "c2hvcnQ=".to_string(), // "short" - only 5 bytes
            ..valid_req.clone()
        };
        assert!(invalid_key_req.validate().is_err());

        // Invalid version
        let invalid_version_req = HandshakeRequest {
            version: 12,
            ..valid_req.clone()
        };
        assert!(invalid_version_req.validate().is_err());
    }

    // Test 7: Generate response from request
    #[test]
    fn test_response_from_request() {
        let req = HandshakeRequest {
            path: "/chat".to_string(),
            host: "example.com".to_string(),
            key: "dGhlIHNhbXBsZSBub25jZQ==".to_string(),
            version: 13,
            origin: None,
            protocols: vec!["chat".to_string(), "superchat".to_string()],
            extensions: vec![],
        };

        let resp = HandshakeResponse::from_request(&req);
        assert_eq!(resp.accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(resp.protocol, Some("chat".to_string()));
    }

    // Test 8: Serialize response to bytes
    #[test]
    fn test_response_write() {
        let resp = HandshakeResponse {
            accept: "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=".to_string(),
            protocol: Some("chat".to_string()),
            extensions: vec![],
        };

        let mut buf = Vec::new();
        resp.write(&mut buf);
        let response_str = String::from_utf8(buf).unwrap();

        assert!(response_str.contains("HTTP/1.1 101 Switching Protocols"));
        assert!(response_str.contains("Upgrade: websocket"));
        assert!(response_str.contains("Connection: Upgrade"));
        assert!(response_str.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
        assert!(response_str.contains("Sec-WebSocket-Protocol: chat"));
        assert!(response_str.ends_with("\r\n\r\n"));
    }

    // Test 9: Parse server response
    #[test]
    fn test_parse_response() {
        let response = b"HTTP/1.1 101 Switching Protocols\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\
            Sec-WebSocket-Protocol: chat\r\n\
            \r\n";

        let resp = HandshakeResponse::parse(response).unwrap();
        assert_eq!(resp.accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(resp.protocol, Some("chat".to_string()));
    }

    // Test 10: Request → Response → Validate accept key
    #[test]
    fn test_roundtrip() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        // Parse request
        let req = HandshakeRequest::parse(request).unwrap();
        assert!(req.validate().is_ok());

        // Generate response
        let resp = HandshakeResponse::from_request(&req);

        // Write response
        let mut buf = Vec::new();
        resp.write(&mut buf);

        // Parse response
        let parsed_resp = HandshakeResponse::parse(&buf).unwrap();

        // Verify accept key matches
        let expected_accept = compute_accept_key(&req.key);
        assert_eq!(parsed_resp.accept, expected_accept);
        assert_eq!(parsed_resp.accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    // Test 11: Case-insensitive header parsing
    #[test]
    fn test_case_insensitive_headers() {
        let request = b"GET /chat HTTP/1.1\r\n\
            HOST: server.example.com\r\n\
            UPGRADE: WebSocket\r\n\
            CONNECTION: upgrade\r\n\
            SEC-WEBSOCKET-KEY: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            SEC-WEBSOCKET-VERSION: 13\r\n\
            \r\n";

        let req = HandshakeRequest::parse(request).unwrap();
        assert_eq!(req.host, "server.example.com");
        assert_eq!(req.key, "dGhlIHNhbXBsZSBub25jZQ==");
        assert!(req.validate().is_ok());
    }

    // Test 12: Invalid HTTP method
    #[test]
    fn test_invalid_http_method() {
        let request = b"POST /chat HTTP/1.1\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let result = HandshakeRequest::parse(request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("GET")));
    }

    // Test 13: Invalid HTTP version
    #[test]
    fn test_invalid_http_version() {
        let request = b"GET /chat HTTP/1.0\r\n\
            Host: server.example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let result = HandshakeRequest::parse(request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("HTTP/1.1")));
    }

    // Test 14: Missing Host header
    #[test]
    fn test_missing_host_header() {
        let request = b"GET /chat HTTP/1.1\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let result = HandshakeRequest::parse(request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidHandshake(msg) if msg.contains("Host")));
    }

    // Test 15: Response missing accept header
    #[test]
    fn test_response_missing_accept() {
        let response = b"HTTP/1.1 101 Switching Protocols\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            \r\n";

        let result = HandshakeResponse::parse(response);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::InvalidHandshake(msg) if msg.contains("Sec-WebSocket-Accept"))
        );
    }
}
