#![cfg(feature = "tls-rustls")]

use std::sync::Arc;

use rcgen::{CertifiedKey, generate_simple_self_signed};
use rsws::tls::{TlsAcceptor, TlsConnector, TlsError};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};

fn generate_test_cert() -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    let subject_alt_names = vec!["localhost".to_string()];
    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names).unwrap();

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    (vec![cert_der], key_der)
}

fn create_test_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Arc<ServerConfig> {
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    Arc::new(config)
}

fn create_test_client_config(server_cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut root_store = RootCertStore::empty();
    root_store.add(server_cert).unwrap();

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Arc::new(config)
}

#[tokio::test]
async fn test_tls_client_server_handshake() {
    let (certs, key) = generate_test_cert();
    let server_config = create_test_server_config(certs.clone(), key);
    let client_config = create_test_client_config(certs[0].clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let acceptor = TlsAcceptor::new(server_config);
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls_stream = acceptor.accept(stream).await.unwrap();

        let mut buf = [0u8; 5];
        tls_stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        tls_stream.write_all(b"world").await.unwrap();
    });

    let client_handle = tokio::spawn(async move {
        let connector = TlsConnector::new(client_config);
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut tls_stream = connector.connect("localhost", stream).await.unwrap();

        tls_stream.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 5];
        tls_stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");
    });

    let (server_result, client_result) = tokio::join!(server_handle, client_handle);
    server_result.unwrap();
    client_result.unwrap();
}

#[tokio::test]
async fn test_invalid_dns_name() {
    let config = rsws::tls::client_config_with_native_roots().unwrap();
    let connector = TlsConnector::new(config);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let stream = TcpStream::connect(addr).await.unwrap();
    let result = connector.connect("invalid..name", stream).await;

    assert!(matches!(result, Err(TlsError::InvalidDnsName(_))));
}

#[test]
fn test_tls_error_display() {
    let io_err = TlsError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
    assert!(io_err.to_string().contains("TLS I/O error"));

    let config_err = TlsError::Configuration("bad config".to_string());
    assert!(config_err.to_string().contains("TLS configuration error"));

    let no_certs = TlsError::NoCertificatesFound;
    assert!(no_certs.to_string().contains("no certificates found"));

    let no_key = TlsError::NoPrivateKeyFound;
    assert!(no_key.to_string().contains("no private key found"));

    let invalid_key = TlsError::InvalidPrivateKey;
    assert!(invalid_key.to_string().contains("invalid private key"));

    let invalid_dns = TlsError::InvalidDnsName("bad.name".to_string());
    assert!(invalid_dns.to_string().contains("invalid DNS name"));
}
