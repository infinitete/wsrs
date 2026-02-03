# rsws

[![CI](https://github.com/infinitete/rust-ws/actions/workflows/ci.yml/badge.svg)](https://github.com/infinitete/rust-ws/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[English](README.md) | 中文

生产级、符合 RFC 6455 标准的 Rust WebSocket 库。

## 特性

- **RFC 6455 完全兼容** — 严格的协议验证
- **Async/Await** — 基于 Tokio 的高性能异步 I/O
- **零拷贝解析** — 热路径最小化内存分配
- **SIMD 加速** — 运行时检测 AVX2/SSE2/NEON，掩码吞吐量 >150 GiB/s
- **TLS 支持** — 通过 rustls 或 native-tls 实现安全 WebSocket (wss://)
- **压缩支持** — Per-message deflate (RFC 7692)
- **可配置限制** — 防止资源耗尽攻击

## 安装

```toml
[dependencies]
rsws = "0.1"
```

### 功能标志

| 功能 | 描述 | 默认 |
|------|------|------|
| `async-tokio` | Tokio 异步 I/O 运行时 | 是 |
| `tls-rustls` | 通过 rustls 启用 TLS（纯 Rust） | 否 |
| `tls-native` | 通过 native-tls 启用 TLS（平台原生） | 否 |
| `compression` | Per-message deflate (RFC 7692) | 否 |

```toml
# 启用 TLS
rsws = { version = "0.1", features = ["tls-rustls"] }

# 启用压缩
rsws = { version = "0.1", features = ["compression"] }

# 完整功能
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```

## 快速开始

### Echo 服务器

```rust
use rsws::{Connection, Config, Role, Message};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    
    loop {
        let (stream, _) = listener.accept().await?;
        
        tokio::spawn(async move {
            // 注意：需要先完成握手再包装连接
            let mut conn = Connection::new(stream, Role::Server, Config::server());
            
            while let Ok(Some(msg)) = conn.recv().await {
                match msg {
                    Message::Text(text) => {
                        conn.send(Message::text(text)).await.ok();
                    }
                    Message::Binary(data) => {
                        conn.send(Message::binary(data)).await.ok();
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });
    }
}
```

### 客户端

```rust
use rsws::{Connection, Config, Role, Message, CloseCode};
use rsws::protocol::handshake::{HandshakeRequest, HandshakeResponse, compute_accept_key};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    
    // 执行 HTTP 升级握手
    let key = rsws::protocol::handshake::generate_key();
    let request = HandshakeRequest::new("127.0.0.1:8080", "/", &key);
    stream.write_all(request.to_string().as_bytes()).await?;
    
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = HandshakeResponse::parse(&buf[..n])?;
    assert_eq!(response.accept, compute_accept_key(&key));
    
    // 创建 WebSocket 连接
    let mut conn = Connection::new(stream, Role::Client, Config::client());
    
    conn.send(Message::text("Hello, WebSocket!")).await?;
    
    if let Ok(Some(msg)) = conn.recv().await {
        println!("收到: {:?}", msg);
    }
    
    conn.close(CloseCode::Normal, "完成").await?;
    Ok(())
}
```

## API 参考

### 核心类型

| 类型 | 描述 |
|------|------|
| `Connection<T>` | 异步流 `T` 上的 WebSocket 连接 |
| `Config` | 连接配置（限制、缓冲、掩码） |
| `Limits` | 资源限制（帧大小、消息大小、分片数） |
| `Message` | WebSocket 消息（Text、Binary、Ping、Pong、Close） |
| `CloseCode` | RFC 6455 关闭状态码 |
| `CloseFrame` | 包含状态码和原因的关闭帧 |
| `Role` | 连接角色（Client 或 Server） |
| `ConnectionState` | 状态机（Open、Closing、Closed） |

### Connection 方法

```rust
// 发送消息（自动刷新）
conn.send(Message::text("你好")).await?;

// 发送但不刷新（用于批量发送）
conn.send_no_flush(Message::text("一")).await?;
conn.send_no_flush(Message::text("二")).await?;
conn.flush().await?;

// 批量发送（最后统一刷新）
conn.send_batch([Message::text("a"), Message::text("b")]).await?;

// 接收下一条消息
while let Some(msg) = conn.recv().await? {
    // 处理消息
}

// 发起关闭握手
conn.close(CloseCode::Normal, "再见").await?;

// 检查连接状态
if conn.is_open() { /* ... */ }
```

### 消息构建器

```rust
let text = Message::text("你好");
let binary = Message::binary(vec![0x01, 0x02, 0x03]);
let ping = Message::ping(vec![]);
let pong = Message::pong(data);
let close = Message::close(CloseCode::Normal, "再见");

// 类型检查
msg.is_text();
msg.is_binary();
msg.is_data();      // text 或 binary
msg.is_control();   // ping、pong 或 close

// 提取数据
msg.as_text();      // Option<&str>
msg.as_binary();    // Option<&[u8]>
msg.into_text();    // Option<String>
msg.into_binary();  // Option<Vec<u8>>
```

### 配置

```rust
// 角色预设
let server_config = Config::server();  // 不掩码，验证客户端帧
let client_config = Config::client();  // 掩码所有发出的帧

// 自定义配置
let config = Config::new()
    .with_limits(Limits::default())
    .with_fragment_size(16 * 1024)
    .with_read_buffer_size(8192)
    .with_write_buffer_size(8192)
    .with_timeouts(Timeouts::default())
    .with_allowed_origins(vec!["https://example.com".into()]);
```

### Limits 预设

| 预设 | 帧大小 | 消息大小 | 分片数 | 适用场景 |
|------|--------|----------|--------|----------|
| `Limits::default()` | 16 MB | 64 MB | 128 | 通用 |
| `Limits::embedded()` | 64 KB | 256 KB | 16 | 资源受限环境 |
| `Limits::unrestricted()` | 1 GB | 4 GB | 1024 | 受信环境 |

### 错误处理

```rust
use rsws::{Error, Result};

match conn.recv().await {
    Ok(Some(msg)) => { /* 处理消息 */ }
    Ok(None) => { /* 连接已关闭 */ }
    Err(Error::ConnectionClosed(_)) => { /* 对端关闭 */ }
    Err(Error::FrameTooLarge { size, max }) => {
        eprintln!("帧 {} 超过限制 {}", size, max);
    }
    Err(Error::InvalidUtf8) => { /* 无效文本帧 */ }
    Err(Error::ProtocolViolation(reason)) => { /* RFC 违规 */ }
    Err(e) => { /* 其他错误 */ }
}
```

## TLS 支持

### rustls 服务端

```rust
use rsws::{Connection, Config, Role};
use rsws::tls::{TlsAcceptor, load_certs_from_file, load_private_key_from_file, server_config};
use std::sync::Arc;

let certs = load_certs_from_file("cert.pem")?;
let key = load_private_key_from_file("key.pem")?;
let tls_config = server_config(certs, key)?;
let acceptor = TlsAcceptor::new(Arc::new(tls_config));

let (tcp_stream, _) = listener.accept().await?;
let tls_stream = acceptor.accept(tcp_stream).await?;
let conn = Connection::new(tls_stream, Role::Server, Config::server());
```

### rustls 客户端

```rust
use rsws::tls::{TlsConnector, client_config_with_native_roots};

let tls_config = client_config_with_native_roots()?;
let connector = TlsConnector::new(tls_config);
let tls_stream = connector.connect("example.com", tcp_stream).await?;
let conn = Connection::new(tls_stream, Role::Client, Config::client());
```

## 性能

rsws 通过 SIMD 加速实现 **>150 GiB/s** 掩码吞吐量：

| 负载大小 | 标量 | SIMD (AVX2/NEON) | 提升 |
|----------|------|------------------|------|
| 64 KB | ~10 GiB/s | 154.9 GiB/s | ~15x |
| 1 MB | 7.07 GiB/s | 101.2 GiB/s | ~14x |

**优化技术：**
- 运行时 CPU 特性检测（AVX2/SSE2/NEON）
- 零拷贝 `Bytes` 解析（非掩码帧）
- 单缓冲区消息重组
- `send_batch()` 批量发送减少系统调用
- 可配置读写缓冲区大小

运行基准测试：
```bash
cargo bench --bench benchmarks
```

## RFC 6455 合规性

| 章节 | 功能 | 状态 |
|------|------|------|
| §4 | 开启握手 | ✅ |
| §5.2 | 帧格式 | ✅ |
| §5.3 | 客户端到服务端掩码 | ✅ |
| §5.4 | 消息分片 | ✅ |
| §5.5 | 控制帧 | ✅ |
| §6 | UTF-8 验证 | ✅ |
| §7 | 关闭握手 | ✅ |
| §7.4 | 状态码 | ✅ |
| §9 | 扩展 | ✅ |
| §10 | 安全性 | ✅ |

### 扩展

| 扩展 | RFC | 状态 |
|------|-----|------|
| permessage-deflate | RFC 7692 | ✅（功能门控） |

### 安全特性

- CSWSH 防护（Origin 验证）
- 头部 CRLF 注入防护
- 可配置大小限制（DoS 防护）
- 按角色强制执行掩码规则

### Autobahn 测试套件

详见 [autobahn/README.md](autobahn/README.md)。

## 示例

```bash
# Echo 服务器
cargo run --example echo_server

# 客户端
cargo run --example client

# WSS 客户端（TLS）
cargo run --example wss_client --features tls-rustls

# 压力测试
cargo run --example stress_server
cargo run --example stress_client

# Autobahn 合规测试
cargo run --example autobahn_server
```

## 许可证

MIT
