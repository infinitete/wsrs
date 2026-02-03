use rsws::{Config, Connection, HandshakeRequest, HandshakeResponse, Message, Role};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const HANDSHAKE_TIMEOUT_SECS: u64 = 10;
const MAX_HEADER_SIZE: usize = 8192;

struct ServerMetrics {
    connections_total: AtomicU64,
    connections_active: AtomicUsize,
    messages_echoed: AtomicU64,
    bytes_received: AtomicU64,
    bytes_sent: AtomicU64,
    errors: AtomicU64,
    start_time: Instant,
}

impl ServerMetrics {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            connections_total: AtomicU64::new(0),
            connections_active: AtomicUsize::new(0),
            messages_echoed: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            start_time: Instant::now(),
        })
    }

    fn connection_opened(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }

    fn connection_closed(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }

    fn message_echoed(&self, size: usize) {
        self.messages_echoed.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(size as u64, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }

    fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn report(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let total = self.connections_total.load(Ordering::Relaxed);
        let active = self.connections_active.load(Ordering::Relaxed);
        let messages = self.messages_echoed.load(Ordering::Relaxed);
        let rx_bytes = self.bytes_received.load(Ordering::Relaxed);
        let tx_bytes = self.bytes_sent.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);

        let msg_rate = if elapsed > 0.0 {
            messages as f64 / elapsed
        } else {
            0.0
        };
        let rx_rate = if elapsed > 0.0 {
            rx_bytes as f64 / elapsed / 1024.0 / 1024.0
        } else {
            0.0
        };
        let tx_rate = if elapsed > 0.0 {
            tx_bytes as f64 / elapsed / 1024.0 / 1024.0
        } else {
            0.0
        };

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                    SERVER METRICS                            ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║  Uptime:              {:>10.1}s                           ║",
            elapsed
        );
        println!(
            "║  Connections total:   {:>10}                             ║",
            total
        );
        println!(
            "║  Connections active:  {:>10}                             ║",
            active
        );
        println!(
            "║  Messages echoed:     {:>10}                             ║",
            messages
        );
        println!(
            "║  Message rate:        {:>10.1} msg/s                      ║",
            msg_rate
        );
        println!(
            "║  RX throughput:       {:>10.2} MB/s                       ║",
            rx_rate
        );
        println!(
            "║  TX throughput:       {:>10.2} MB/s                       ║",
            tx_rate
        );
        println!(
            "║  Errors:              {:>10}                             ║",
            errors
        );
        println!("╚══════════════════════════════════════════════════════════════╝\n");
    }

    fn report_json(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let total = self.connections_total.load(Ordering::Relaxed);
        let active = self.connections_active.load(Ordering::Relaxed);
        let messages = self.messages_echoed.load(Ordering::Relaxed);
        let rx_bytes = self.bytes_received.load(Ordering::Relaxed);
        let tx_bytes = self.bytes_sent.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);

        println!(
            r#"{{"uptime_secs":{:.3},"connections_total":{},"connections_active":{},"messages_echoed":{},"bytes_received":{},"bytes_sent":{},"errors":{},"msg_per_sec":{:.1}}}"#,
            elapsed,
            total,
            active,
            messages,
            rx_bytes,
            tx_bytes,
            errors,
            if elapsed > 0.0 {
                messages as f64 / elapsed
            } else {
                0.0
            }
        );
    }
}

fn parse_args() -> (String, u16, bool, u64) {
    let args: Vec<String> = std::env::args().collect();
    let mut host = "0.0.0.0".to_string();
    let mut port: u16 = 9001;
    let mut json = false;
    let mut report_interval: u64 = 5;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--host" => {
                if i + 1 < args.len() {
                    host = args[i + 1].clone();
                    i += 1;
                }
            }
            "-p" | "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(9001);
                    i += 1;
                }
            }
            "-j" | "--json" => {
                json = true;
            }
            "-i" | "--interval" => {
                if i + 1 < args.len() {
                    report_interval = args[i + 1].parse().unwrap_or(5);
                    i += 1;
                }
            }
            "--help" => {
                println!("WebSocket Stress Test Server");
                println!();
                println!("USAGE:");
                println!("    stress_server [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --host <HOST>      Bind address [default: 0.0.0.0]");
                println!("    -p, --port <PORT>      Bind port [default: 9001]");
                println!("    -i, --interval <SECS>  Metrics report interval [default: 5]");
                println!("    -j, --json             Output metrics as JSON");
                println!("        --help             Show this help");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    (host, port, json, report_interval)
}

async fn handle_connection(stream: TcpStream, metrics: Arc<ServerMetrics>) {
    metrics.connection_opened();

    if let Err(_) = handle_connection_inner(stream, &metrics).await {
        metrics.error();
    }

    metrics.connection_closed();
}

async fn handle_connection_inner(
    mut stream: TcpStream,
    metrics: &ServerMetrics,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut reader = BufReader::new(&mut stream);
    let mut request_bytes = Vec::new();

    let handshake_result = timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS), async {
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            request_bytes.extend_from_slice(line.as_bytes());
            if request_bytes.len() > MAX_HEADER_SIZE {
                return Err("Header too large".into());
            }
            if line == "\r\n" {
                break;
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await;

    match handshake_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Handshake timeout".into()),
    }

    let request = HandshakeRequest::parse(&request_bytes)?;
    request.validate()?;

    let response = HandshakeResponse::from_request(&request);
    let mut response_bytes = Vec::new();
    let _ = response.write(&mut response_bytes);
    stream.write_all(&response_bytes).await?;

    let config = Config::server();
    let mut conn = Connection::new(stream, Role::Server, config);

    while conn.is_open() {
        match conn.recv().await? {
            Some(Message::Text(text)) => {
                let len = text.len();
                conn.send(Message::text(text)).await?;
                metrics.message_echoed(len);
            }
            Some(Message::Binary(data)) => {
                let len = data.len();
                conn.send(Message::binary(data)).await?;
                metrics.message_echoed(len);
            }
            Some(Message::Ping(_)) | Some(Message::Pong(_)) => {}
            Some(Message::Close(_)) | None => break,
            _ => {}
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (host, port, json, report_interval) = parse_args();
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    let listener = TcpListener::bind(addr).await?;
    let metrics = ServerMetrics::new();

    if !json {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║           WebSocket Stress Test Server                       ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Listening on: {:>44} ║", addr);
        println!(
            "║  Report interval: {:>4}s                                     ║",
            report_interval
        );
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("Waiting for connections... (Ctrl+C to stop)");
    }

    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(report_interval));
        loop {
            interval.tick().await;
            if json {
                metrics_clone.report_json();
            } else {
                metrics_clone.report();
            }
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let metrics = metrics.clone();
        tokio::spawn(handle_connection(stream, metrics));
    }
}
