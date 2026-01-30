//! TLS WebSocket client example using rustls.
//!
//! Run with: cargo run --example wss_client --features tls-rustls
//!
//! Connects to a public WebSocket echo server over TLS.

#[cfg(not(feature = "tls-rustls"))]
fn main() {
    eprintln!("This example requires the 'tls-rustls' feature.");
    eprintln!("Run with: cargo run --example wss_client --features tls-rustls");
}

#[cfg(feature = "tls-rustls")]
mod inner {
    use rsws::tls::TlsConnector;
    use rsws::{
        compute_accept_key, CloseCode, Config, Connection, HandshakeResponse, Message, Role,
    };
    use rustls::ClientConfig;
    use std::error::Error;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    const WSS_HOST: &str = "echo.websocket.org";
    const WSS_PORT: u16 = 443;

    pub async fn run() -> Result<(), Box<dyn Error>> {
        println!("Connecting to wss://{}:{}", WSS_HOST, WSS_PORT);

        let tcp_stream = TcpStream::connect((WSS_HOST, WSS_PORT)).await?;

        let tls_config = build_tls_config()?;
        let connector = TlsConnector::new(Arc::new(tls_config));
        let mut tls_stream = connector.connect(WSS_HOST, tcp_stream).await?;

        println!("TLS connection established");

        let key = base64_encode_random_key();
        let request = format!(
            "GET / HTTP/1.1\r\n\
             Host: {}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n",
            WSS_HOST, key
        );
        tls_stream.write_all(request.as_bytes()).await?;

        let mut reader = BufReader::new(&mut tls_stream);
        let mut response_bytes = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            response_bytes.extend_from_slice(line.as_bytes());
            if line == "\r\n" {
                break;
            }
        }

        let response = HandshakeResponse::parse(&response_bytes)?;
        let expected_accept = compute_accept_key(&key);
        if response.accept != expected_accept {
            return Err("Invalid Sec-WebSocket-Accept".into());
        }
        println!("WebSocket handshake complete");

        let config = Config::client();
        let mut conn = Connection::new(tls_stream, Role::Client, config);

        let message = "Hello from wss client!";
        println!("Sending: {}", message);
        conn.send(Message::text(message)).await?;

        if let Some(msg) = conn.recv().await? {
            match msg {
                Message::Text(text) => println!("Received: {}", text),
                Message::Binary(data) => println!("Received binary: {} bytes", data.len()),
                _ => println!("Received: {:?}", msg),
            }
        }

        println!("Closing connection...");
        conn.close(CloseCode::Normal, "goodbye").await?;

        while let Some(msg) = conn.recv().await? {
            if matches!(msg, Message::Close(_)) {
                println!("Received close confirmation");
                break;
            }
        }

        println!("Done");
        Ok(())
    }

    fn build_tls_config() -> Result<ClientConfig, Box<dyn Error>> {
        let root_store =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Ok(config)
    }

    fn base64_encode_random_key() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((seed >> (i * 4)) & 0xFF) as u8;
        }

        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    }
}

#[cfg(feature = "tls-rustls")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    inner::run().await
}
