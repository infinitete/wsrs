use rsws::{compute_accept_key, CloseCode, Config, Connection, HandshakeResponse, Message, Role};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::timeout;

const HANDSHAKE_TIMEOUT_SECS: u64 = 10;
const MAX_HEADER_SIZE: usize = 8192;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 30;

struct ClientMetrics {
    connections_attempted: AtomicU64,
    connections_successful: AtomicU64,
    connections_failed: AtomicU64,
    connections_active: AtomicUsize,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    latencies_us: Mutex<Vec<u64>>,
    errors: AtomicU64,
    start_time: Mutex<Option<Instant>>,
}

impl ClientMetrics {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            connections_attempted: AtomicU64::new(0),
            connections_successful: AtomicU64::new(0),
            connections_failed: AtomicU64::new(0),
            connections_active: AtomicUsize::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            latencies_us: Mutex::new(Vec::new()),
            errors: AtomicU64::new(0),
            start_time: Mutex::new(None),
        })
    }

    fn start(&self) {
        *self.start_time.lock().unwrap() = Some(Instant::now());
    }

    fn elapsed(&self) -> Duration {
        self.start_time.lock().unwrap().map(|t| t.elapsed()).unwrap_or_default()
    }

    fn connection_attempted(&self) {
        self.connections_attempted.fetch_add(1, Ordering::Relaxed);
    }

    fn connection_success(&self) {
        self.connections_successful.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }

    fn connection_failed(&self) {
        self.connections_failed.fetch_add(1, Ordering::Relaxed);
    }

    fn connection_closed(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }

    fn message_sent(&self, size: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }

    fn message_received(&self, size: usize, latency: Duration) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(size as u64, Ordering::Relaxed);
        if let Ok(mut latencies) = self.latencies_us.lock() {
            latencies.push(latency.as_micros() as u64);
        }
    }

    fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn percentile(&self, p: f64) -> Option<Duration> {
        let latencies = self.latencies_us.lock().ok()?;
        if latencies.is_empty() {
            return None;
        }
        let mut sorted = latencies.clone();
        sorted.sort();
        let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        Some(Duration::from_micros(sorted[idx.min(sorted.len() - 1)]))
    }

    fn report(&self) {
        let elapsed = self.elapsed().as_secs_f64();
        let attempted = self.connections_attempted.load(Ordering::Relaxed);
        let successful = self.connections_successful.load(Ordering::Relaxed);
        let failed = self.connections_failed.load(Ordering::Relaxed);
        let active = self.connections_active.load(Ordering::Relaxed);
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let received = self.messages_received.load(Ordering::Relaxed);
        let tx_bytes = self.bytes_sent.load(Ordering::Relaxed);
        let rx_bytes = self.bytes_received.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);

        let msg_rate = if elapsed > 0.0 { received as f64 / elapsed } else { 0.0 };
        let tx_rate = if elapsed > 0.0 { tx_bytes as f64 / elapsed / 1024.0 / 1024.0 } else { 0.0 };
        let rx_rate = if elapsed > 0.0 { rx_bytes as f64 / elapsed / 1024.0 / 1024.0 } else { 0.0 };

        let p50 = self.percentile(0.50);
        let p95 = self.percentile(0.95);
        let p99 = self.percentile(0.99);

        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                    CLIENT METRICS                            ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Duration:            {:>10.3}s                           ║", elapsed);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  CONNECTIONS                                                 ║");
        println!("║    Attempted:         {:>10}                             ║", attempted);
        println!("║    Successful:        {:>10}                             ║", successful);
        println!("║    Failed:            {:>10}                             ║", failed);
        println!("║    Active:            {:>10}                             ║", active);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  MESSAGES                                                    ║");
        println!("║    Sent:              {:>10}                             ║", sent);
        println!("║    Received:          {:>10}                             ║", received);
        println!("║    Rate:              {:>10.1} msg/s                      ║", msg_rate);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  THROUGHPUT                                                  ║");
        println!("║    TX:                {:>10.2} MB/s                       ║", tx_rate);
        println!("║    RX:                {:>10.2} MB/s                       ║", rx_rate);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  LATENCY (round-trip)                                        ║");
        if let Some(p) = p50 {
            println!("║    P50:               {:>10.3} ms                        ║", p.as_secs_f64() * 1000.0);
        }
        if let Some(p) = p95 {
            println!("║    P95:               {:>10.3} ms                        ║", p.as_secs_f64() * 1000.0);
        }
        if let Some(p) = p99 {
            println!("║    P99:               {:>10.3} ms                        ║", p.as_secs_f64() * 1000.0);
        }
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Errors:              {:>10}                             ║", errors);
        println!("╚══════════════════════════════════════════════════════════════╝");
    }

    fn report_json(&self) {
        let elapsed = self.elapsed().as_secs_f64();
        let attempted = self.connections_attempted.load(Ordering::Relaxed);
        let successful = self.connections_successful.load(Ordering::Relaxed);
        let failed = self.connections_failed.load(Ordering::Relaxed);
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let received = self.messages_received.load(Ordering::Relaxed);
        let tx_bytes = self.bytes_sent.load(Ordering::Relaxed);
        let rx_bytes = self.bytes_received.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);

        let p50_us = self.percentile(0.50).map(|d| d.as_micros()).unwrap_or(0);
        let p95_us = self.percentile(0.95).map(|d| d.as_micros()).unwrap_or(0);
        let p99_us = self.percentile(0.99).map(|d| d.as_micros()).unwrap_or(0);

        println!(
            r#"{{"duration_secs":{:.3},"connections":{{"attempted":{},"successful":{},"failed":{}}},"messages":{{"sent":{},"received":{},"rate":{:.1}}},"bytes":{{"sent":{},"received":{}}},"latency_us":{{"p50":{},"p95":{},"p99":{}}},"errors":{}}}"#,
            elapsed, attempted, successful, failed, sent, received,
            if elapsed > 0.0 { received as f64 / elapsed } else { 0.0 },
            tx_bytes, rx_bytes, p50_us, p95_us, p99_us, errors
        );
    }
}

struct TestConfig {
    server_addr: SocketAddr,
    num_clients: usize,
    messages_per_client: usize,
    message_size: usize,
    max_concurrent: usize,
    connect_timeout_secs: u64,
    warmup_ms: u64,
    json: bool,
}

fn parse_args() -> TestConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 9001;
    let mut num_clients: usize = 1000;
    let mut messages_per_client: usize = 100;
    let mut message_size: usize = 128;
    let mut max_concurrent: usize = 200;
    let mut connect_timeout_secs: u64 = DEFAULT_CONNECT_TIMEOUT_SECS;
    let mut warmup_ms: u64 = 100;
    let mut json = false;

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
            "-c" | "--clients" => {
                if i + 1 < args.len() {
                    num_clients = args[i + 1].parse().unwrap_or(1000);
                    i += 1;
                }
            }
            "-m" | "--messages" => {
                if i + 1 < args.len() {
                    messages_per_client = args[i + 1].parse().unwrap_or(100);
                    i += 1;
                }
            }
            "-s" | "--size" => {
                if i + 1 < args.len() {
                    message_size = args[i + 1].parse().unwrap_or(128);
                    i += 1;
                }
            }
            "--max-concurrent" => {
                if i + 1 < args.len() {
                    max_concurrent = args[i + 1].parse().unwrap_or(200);
                    i += 1;
                }
            }
            "--connect-timeout" => {
                if i + 1 < args.len() {
                    connect_timeout_secs = args[i + 1].parse().unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS);
                    i += 1;
                }
            }
            "--warmup" => {
                if i + 1 < args.len() {
                    warmup_ms = args[i + 1].parse().unwrap_or(100);
                    i += 1;
                }
            }
            "-j" | "--json" => {
                json = true;
            }
            "--help" => {
                println!("WebSocket Stress Test Client");
                println!();
                println!("USAGE:");
                println!("    stress_client [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --host <HOST>          Server address [default: 127.0.0.1]");
                println!("    -p, --port <PORT>          Server port [default: 9001]");
                println!("    -c, --clients <N>          Number of clients [default: 1000]");
                println!("    -m, --messages <N>         Messages per client [default: 100]");
                println!("    -s, --size <BYTES>         Message size in bytes [default: 128]");
                println!("        --max-concurrent <N>   Max concurrent connections [default: 200]");
                println!("        --connect-timeout <S>  Connection timeout in seconds [default: 30]");
                println!("        --warmup <MS>          Warmup delay in ms [default: 100]");
                println!("    -j, --json                 Output results as JSON");
                println!("        --help                 Show this help");
                println!();
                println!("EXAMPLES:");
                println!("    # Test against local server");
                println!("    stress_client -c 1000 -m 100");
                println!();
                println!("    # Test against remote server");
                println!("    stress_client -h 192.168.1.100 -p 9001 -c 5000 -m 50");
                println!();
                println!("    # High throughput test (large messages)");
                println!("    stress_client -c 100 -m 1000 -s 4096");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    let server_addr: SocketAddr = match format!("{}:{}", host, port).parse() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("Error: Invalid address '{}:{}': {}", host, port, e);
            std::process::exit(1);
        }
    };

    TestConfig {
        server_addr,
        num_clients,
        messages_per_client,
        message_size,
        max_concurrent,
        connect_timeout_secs,
        warmup_ms,
        json,
    }
}

async fn run_client(
    client_id: usize,
    config: &TestConfig,
    metrics: Arc<ClientMetrics>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    metrics.connection_attempted();

    let stream = timeout(
        Duration::from_secs(config.connect_timeout_secs),
        TcpStream::connect(config.server_addr),
    )
    .await
    .map_err(|_| "Connection timeout")?
    .map_err(|e| format!("Connection failed: {}", e))?;

    let mut stream = stream;

    let key = generate_websocket_key(client_id);
    let request = format!(
        "GET / HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        config.server_addr, key
    );
    stream.write_all(request.as_bytes()).await?;

    let mut reader = BufReader::new(&mut stream);
    let mut response_bytes = Vec::new();

    let handshake_result = timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS), async {
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            response_bytes.extend_from_slice(line.as_bytes());
            if response_bytes.len() > MAX_HEADER_SIZE {
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

    let response = HandshakeResponse::parse(&response_bytes)?;
    let expected_accept = compute_accept_key(&key);
    if response.accept != expected_accept {
        return Err("Invalid Sec-WebSocket-Accept".into());
    }

    metrics.connection_success();

    let ws_config = Config::client();
    let mut conn = Connection::new(stream, Role::Client, ws_config);

    let payload: String = (0..config.message_size)
        .map(|i| (b'a' + (i % 26) as u8) as char)
        .collect();

    for seq in 0..config.messages_per_client {
        let msg = format!("{}:{}:{}", client_id, seq, payload);
        let msg_len = msg.len();

        let start = Instant::now();
        conn.send(Message::text(&msg)).await?;
        metrics.message_sent(msg_len);

        match conn.recv().await? {
            Some(Message::Text(response)) => {
                let latency = start.elapsed();
                metrics.message_received(response.len(), latency);
            }
            Some(Message::Binary(data)) => {
                let latency = start.elapsed();
                metrics.message_received(data.len(), latency);
            }
            _ => {
                metrics.error();
                break;
            }
        }
    }

    conn.close(CloseCode::Normal, "done").await?;
    while let Ok(Some(msg)) = conn.recv().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }

    metrics.connection_closed();
    Ok(())
}

fn generate_websocket_key(seed: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let combined = time_seed.wrapping_add(seed as u128);
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = ((combined >> (i * 4)) & 0xFF) as u8;
    }
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_args();
    let metrics = ClientMetrics::new();

    if !config.json {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║           WebSocket Stress Test Client                       ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Target:              {:>38} ║", config.server_addr);
        println!("║  Clients:             {:>10}                             ║", config.num_clients);
        println!("║  Messages/client:     {:>10}                             ║", config.messages_per_client);
        println!("║  Message size:        {:>10} bytes                       ║", config.message_size);
        println!("║  Max concurrent:      {:>10}                             ║", config.max_concurrent);
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("Starting stress test...");
    }

    if config.warmup_ms > 0 {
        tokio::time::sleep(Duration::from_millis(config.warmup_ms)).await;
    }

    metrics.start();

    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
    let mut set = JoinSet::new();

    for client_id in 0..config.num_clients {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let metrics = metrics.clone();
        let server_addr = config.server_addr;
        let messages_per_client = config.messages_per_client;
        let message_size = config.message_size;
        let connect_timeout_secs = config.connect_timeout_secs;
        let json = config.json;

        set.spawn(async move {
            let cfg = TestConfig {
                server_addr,
                num_clients: 1,
                messages_per_client,
                message_size,
                max_concurrent: 1,
                connect_timeout_secs,
                warmup_ms: 0,
                json: false,
            };

            if let Err(e) = run_client(client_id, &cfg, metrics.clone()).await {
                let error_count = metrics.errors.load(Ordering::Relaxed);
                if !json && error_count < 5 {
                    eprintln!("Client {} error: {}", client_id, e);
                }
                metrics.connection_failed();
                metrics.error();
            }

            drop(permit);
        });
    }

    while set.join_next().await.is_some() {}

    if config.json {
        metrics.report_json();
    } else {
        metrics.report();

        let successful = metrics.connections_successful.load(Ordering::Relaxed);
        let total_messages = metrics.messages_received.load(Ordering::Relaxed);
        let expected_messages = config.num_clients as u64 * config.messages_per_client as u64;

        println!();
        if successful == config.num_clients as u64 && total_messages == expected_messages {
            println!("✓ Test PASSED: All {} clients completed successfully", config.num_clients);
        } else {
            println!("✗ Test FAILED: Expected {} clients/{} messages, got {}/{}", 
                     config.num_clients, expected_messages, successful, total_messages);
        }
    }

    Ok(())
}
