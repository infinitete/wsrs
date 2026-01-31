//! TLS support for WebSocket connections.
//!
//! This module provides TLS/SSL support for secure WebSocket (wss://) connections.
//! Multiple TLS backends are supported:
//!
//! - **rustls** (feature `tls-rustls`): Pure Rust TLS implementation
//! - **native-tls** (feature `tls-native`): Platform-native TLS (OpenSSL/Schannel/Security.framework)

#[cfg(feature = "tls-rustls")]
mod rustls_impl;

#[cfg(feature = "tls-native")]
mod native;

#[cfg(feature = "tls-rustls")]
pub use rustls_impl::{
    TlsAcceptor, TlsConnector, TlsError, TlsStream, load_certs_from_file,
    load_private_key_from_file,
};

#[cfg(feature = "tls-native")]
pub use native::{
    NativeTlsAcceptor, NativeTlsConnector, NativeTlsError, NativeTlsStream,
    load_certificate_from_der, load_certificate_from_pem, load_identity_from_pem,
    load_identity_from_pkcs12,
};

#[cfg(feature = "tls-rustls")]
use std::sync::Arc;
#[cfg(feature = "tls-rustls")]
use tokio_rustls::rustls::{ClientConfig, ServerConfig};

#[cfg(feature = "tls-rustls")]
pub fn client_config_with_native_roots() -> Result<Arc<ClientConfig>, TlsError> {
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(Arc::new(config))
}

#[cfg(feature = "tls-rustls")]
pub fn server_config(
    cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
    private_key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, TlsError> {
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| TlsError::Configuration(e.to_string()))?;

    Ok(Arc::new(config))
}
