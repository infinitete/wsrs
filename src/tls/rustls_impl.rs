use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::rustls::{ClientConfig, ServerConfig};

#[derive(Debug)]
pub enum TlsError {
    Io(std::io::Error),
    Configuration(String),
    NoCertificatesFound,
    NoPrivateKeyFound,
    InvalidPrivateKey,
    InvalidDnsName(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsError::Io(e) => write!(f, "TLS I/O error: {}", e),
            TlsError::Configuration(msg) => write!(f, "TLS configuration error: {}", msg),
            TlsError::NoCertificatesFound => write!(f, "no certificates found in file"),
            TlsError::NoPrivateKeyFound => write!(f, "no private key found in file"),
            TlsError::InvalidPrivateKey => write!(f, "invalid private key format"),
            TlsError::InvalidDnsName(name) => write!(f, "invalid DNS name: {}", name),
        }
    }
}

impl std::error::Error for TlsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TlsError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TlsError {
    fn from(err: std::io::Error) -> Self {
        TlsError::Io(err)
    }
}

pub enum TlsStream<S> {
    Client(tokio_rustls::client::TlsStream<S>),
    Server(tokio_rustls::server::TlsStream<S>),
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsStream<S> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Client(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            TlsStream::Server(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsStream<S> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            TlsStream::Client(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            TlsStream::Server(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Client(s) => std::pin::Pin::new(s).poll_flush(cx),
            TlsStream::Server(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Client(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            TlsStream::Server(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct TlsConnector {
    inner: tokio_rustls::TlsConnector,
}

impl TlsConnector {
    pub fn new(config: Arc<ClientConfig>) -> Self {
        Self {
            inner: tokio_rustls::TlsConnector::from(config),
        }
    }

    pub async fn connect<S>(&self, domain: &str, stream: S) -> Result<TlsStream<S>, TlsError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let server_name = ServerName::try_from(domain.to_string())
            .map_err(|_| TlsError::InvalidDnsName(domain.to_string()))?;

        let tls_stream = self
            .inner
            .connect(server_name, stream)
            .await
            .map_err(TlsError::Io)?;

        Ok(TlsStream::Client(tls_stream))
    }
}

pub struct TlsAcceptor {
    inner: tokio_rustls::TlsAcceptor,
}

impl TlsAcceptor {
    pub fn new(config: Arc<ServerConfig>) -> Self {
        Self {
            inner: tokio_rustls::TlsAcceptor::from(config),
        }
    }

    pub async fn accept<S>(&self, stream: S) -> Result<TlsStream<S>, TlsError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let tls_stream = self.inner.accept(stream).await.map_err(TlsError::Io)?;
        Ok(TlsStream::Server(tls_stream))
    }
}

pub fn load_certs_from_file(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let certs: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    if certs.is_empty() {
        return Err(TlsError::NoCertificatesFound);
    }

    Ok(certs)
}

pub fn load_private_key_from_file(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    for item in rustls_pemfile::read_all(&mut reader) {
        match item? {
            rustls_pemfile::Item::Pkcs1Key(key) => return Ok(PrivateKeyDer::Pkcs1(key)),
            rustls_pemfile::Item::Pkcs8Key(key) => return Ok(PrivateKeyDer::Pkcs8(key)),
            rustls_pemfile::Item::Sec1Key(key) => return Ok(PrivateKeyDer::Sec1(key)),
            _ => continue,
        }
    }

    Err(TlsError::NoPrivateKeyFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tls_error_display() {
        let io_err = TlsError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        assert!(io_err.to_string().contains("TLS I/O error"));

        let config_err = TlsError::Configuration("bad config".to_string());
        assert!(config_err.to_string().contains("TLS configuration error"));
        assert!(config_err.to_string().contains("bad config"));

        let no_certs = TlsError::NoCertificatesFound;
        assert!(no_certs.to_string().contains("no certificates found"));

        let no_key = TlsError::NoPrivateKeyFound;
        assert!(no_key.to_string().contains("no private key found"));

        let invalid_key = TlsError::InvalidPrivateKey;
        assert!(invalid_key.to_string().contains("invalid private key"));

        let invalid_dns = TlsError::InvalidDnsName("bad.name".to_string());
        assert!(invalid_dns.to_string().contains("invalid DNS name"));
        assert!(invalid_dns.to_string().contains("bad.name"));
    }

    #[test]
    fn test_tls_error_source() {
        let io_err = TlsError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert!(io_err.source().is_some());

        let config_err = TlsError::Configuration("test".to_string());
        assert!(config_err.source().is_none());

        let no_certs = TlsError::NoCertificatesFound;
        assert!(no_certs.source().is_none());
    }

    #[test]
    fn test_load_certs_file_not_found() {
        let result = load_certs_from_file(Path::new("/nonexistent/path/cert.pem"));
        assert!(matches!(result, Err(TlsError::Io(_))));
    }

    #[test]
    fn test_load_certs_empty_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"").unwrap();
        temp.flush().unwrap();

        let result = load_certs_from_file(temp.path());
        assert!(matches!(result, Err(TlsError::NoCertificatesFound)));
    }

    #[test]
    fn test_load_certs_no_certs_in_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"not a certificate\njust some text\n").unwrap();
        temp.flush().unwrap();

        let result = load_certs_from_file(temp.path());
        assert!(matches!(result, Err(TlsError::NoCertificatesFound)));
    }

    #[test]
    fn test_load_private_key_file_not_found() {
        let result = load_private_key_from_file(Path::new("/nonexistent/path/key.pem"));
        assert!(matches!(result, Err(TlsError::Io(_))));
    }

    #[test]
    fn test_load_private_key_empty_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"").unwrap();
        temp.flush().unwrap();

        let result = load_private_key_from_file(temp.path());
        assert!(matches!(result, Err(TlsError::NoPrivateKeyFound)));
    }

    #[test]
    fn test_load_private_key_no_key_in_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"not a private key\njust some text\n").unwrap();
        temp.flush().unwrap();

        let result = load_private_key_from_file(temp.path());
        assert!(matches!(result, Err(TlsError::NoPrivateKeyFound)));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let tls_err: TlsError = io_err.into();
        assert!(matches!(tls_err, TlsError::Io(_)));
    }
}
