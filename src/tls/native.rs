use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug)]
pub enum NativeTlsError {
    Io(std::io::Error),
    Tls(native_tls::Error),
    NoCertificatesFound,
    NoPrivateKeyFound,
    InvalidIdentity(String),
}

impl std::fmt::Display for NativeTlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NativeTlsError::Io(e) => write!(f, "TLS I/O error: {}", e),
            NativeTlsError::Tls(e) => write!(f, "TLS error: {}", e),
            NativeTlsError::NoCertificatesFound => write!(f, "no certificates found in file"),
            NativeTlsError::NoPrivateKeyFound => write!(f, "no private key found in file"),
            NativeTlsError::InvalidIdentity(msg) => write!(f, "invalid identity: {}", msg),
        }
    }
}

impl std::error::Error for NativeTlsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NativeTlsError::Io(e) => Some(e),
            NativeTlsError::Tls(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for NativeTlsError {
    fn from(err: std::io::Error) -> Self {
        NativeTlsError::Io(err)
    }
}

impl From<native_tls::Error> for NativeTlsError {
    fn from(err: native_tls::Error) -> Self {
        NativeTlsError::Tls(err)
    }
}

pub enum NativeTlsStream<S> {
    Client(tokio_native_tls::TlsStream<S>),
    Server(tokio_native_tls::TlsStream<S>),
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for NativeTlsStream<S> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            NativeTlsStream::Client(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            NativeTlsStream::Server(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for NativeTlsStream<S> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            NativeTlsStream::Client(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            NativeTlsStream::Server(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            NativeTlsStream::Client(s) => std::pin::Pin::new(s).poll_flush(cx),
            NativeTlsStream::Server(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            NativeTlsStream::Client(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            NativeTlsStream::Server(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct NativeTlsConnector {
    inner: tokio_native_tls::TlsConnector,
}

impl NativeTlsConnector {
    pub fn new(connector: native_tls::TlsConnector) -> Self {
        Self {
            inner: tokio_native_tls::TlsConnector::from(connector),
        }
    }

    pub async fn connect<S>(
        &self,
        domain: &str,
        stream: S,
    ) -> Result<NativeTlsStream<S>, NativeTlsError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let tls_stream = self
            .inner
            .connect(domain, stream)
            .await
            .map_err(NativeTlsError::Tls)?;

        Ok(NativeTlsStream::Client(tls_stream))
    }
}

pub struct NativeTlsAcceptor {
    inner: tokio_native_tls::TlsAcceptor,
}

impl NativeTlsAcceptor {
    pub fn new(acceptor: native_tls::TlsAcceptor) -> Self {
        Self {
            inner: tokio_native_tls::TlsAcceptor::from(acceptor),
        }
    }

    pub async fn accept<S>(&self, stream: S) -> Result<NativeTlsStream<S>, NativeTlsError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let tls_stream = self
            .inner
            .accept(stream)
            .await
            .map_err(NativeTlsError::Tls)?;

        Ok(NativeTlsStream::Server(tls_stream))
    }
}

/// Load a PKCS#12 identity from a file for use with native-tls.
/// The PKCS#12 file should contain both the certificate chain and private key.
pub fn load_identity_from_pkcs12(
    path: &Path,
    password: &str,
) -> Result<native_tls::Identity, NativeTlsError> {
    let mut file = File::open(path)?;
    let mut der = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut der)?;

    native_tls::Identity::from_pkcs12(&der, password)
        .map_err(|e| NativeTlsError::InvalidIdentity(e.to_string()))
}

/// Load a certificate from a PEM file for use with native-tls.
/// This is typically used for adding root certificates.
pub fn load_certificate_from_pem(path: &Path) -> Result<native_tls::Certificate, NativeTlsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut pem = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut pem)?;

    native_tls::Certificate::from_pem(&pem).map_err(NativeTlsError::Tls)
}

/// Load a certificate from a DER file for use with native-tls.
pub fn load_certificate_from_der(path: &Path) -> Result<native_tls::Certificate, NativeTlsError> {
    let mut file = File::open(path)?;
    let mut der = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut der)?;

    native_tls::Certificate::from_der(&der).map_err(NativeTlsError::Tls)
}

/// Load an identity from separate PEM certificate and key files.
/// This combines them into a PKCS#8 format for native-tls.
pub fn load_identity_from_pem(
    cert_path: &Path,
    key_path: &Path,
) -> Result<native_tls::Identity, NativeTlsError> {
    let cert_file = File::open(cert_path)?;
    let mut cert_reader = BufReader::new(cert_file);
    let mut cert_pem = Vec::new();
    std::io::Read::read_to_end(&mut cert_reader, &mut cert_pem)?;

    let key_file = File::open(key_path)?;
    let mut key_reader = BufReader::new(key_file);
    let mut key_pem = Vec::new();
    std::io::Read::read_to_end(&mut key_reader, &mut key_pem)?;

    native_tls::Identity::from_pkcs8(&cert_pem, &key_pem)
        .map_err(|e| NativeTlsError::InvalidIdentity(e.to_string()))
}
