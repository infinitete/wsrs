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
rsws = "0.2"
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
rsws = { version = "0.2", features = ["tls-rustls"] }

# 启用压缩
rsws = { version = "0.2", features = ["compression"] }

# 完整功能
rsws = { version = "0.2", features = ["tls-rustls", "compression"] }
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
| ~~`Limits::unrestricted()`~~ | 1 GB | 4 GB | 1024 | ⚠️ **已废弃** - 请使用 `default()` |

> **注意**: `Limits::unrestricted()` 自 v0.2.0 起已废弃，因存在安全隐患（内存耗尽攻击）。

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
- 运行时 CPU 特性检测（AVX2/SSE2/NEON/SVE）
- 零拷贝 `Bytes` 解析（非掩码帧）
- 单缓冲区消息重组
- `send_batch()` 批量发送减少系统调用
- 可配置读写缓冲区大小

### aarch64 (ARM64) 专项优化

rsws 针对 ARM64 平台（Apple M1/M2、AWS Graviton 等）进行了专项优化：

| 特性 | 实现方式 | 详情 |
|------|----------|------|
| **NEON 掩码** | 64字节循环展开 | 每次迭代处理 4x 128-bit 向量 |
| **SVE 掩码** | 内联汇编 | 谓词循环，自动处理尾部字节 |
| **UTF-8 验证** | NEON 快速路径 | SIMD ASCII 检测 + 标量回退 |

**运行时调度优先级：**
```
SVE (Graviton 3+) → NEON (所有 ARM64) → 标量 (回退)
```

运行基准测试：
```bash
cargo bench --bench benchmarks  # 掩码吞吐量
cargo bench --bench utf8        # UTF-8 验证吞吐量
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
- 加密安全的掩码生成（v0.2.2+）
- 解压缩炸弹防护，带比率限制（v0.2.2+）

### Autobahn 测试套件

详见 [autobahn/README.md](autobahn/README.md)。

## 框架集成

### Axum

rsws 可以作为 [Axum](https://github.com/tokio-rs/axum) HTTP 服务器中的 WebSocket 协议处理器。核心思路：让 Axum 处理 HTTP 路由，手动完成升级握手，然后将原始 I/O 流传递给 rsws。

```rust
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use hyper_util::rt::TokioIo;
use rsws::{Config, Connection, Message, Role, compute_accept_key};

async fn ws_handler(mut req: Request) -> Response {
    // 1. 从升级请求中提取客户端密钥
    let sec_key = req.headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_owned();

    // 2. 使用 rsws 计算 accept key
    let accept_key = compute_accept_key(&sec_key);

    // 3. 启动任务处理升级后的连接
    tokio::spawn(async move {
        let upgraded = hyper::upgrade::on(&mut req).await.unwrap();
        let io = TokioIo::new(upgraded);

        let mut conn = Connection::new(io, Role::Server, Config::server());
        while let Ok(Some(msg)) = conn.recv().await {
            match msg {
                Message::Text(text) => { conn.send(Message::text(text)).await.ok(); }
                Message::Binary(data) => { conn.send(Message::binary(data)).await.ok(); }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // 4. 返回 101 Switching Protocols 完成握手
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Accept", accept_key)
        .body(axum::body::Body::empty())
        .unwrap()
}
```

完整示例参见 [`examples/axum_server.rs`](examples/axum_server.rs)，包含浏览器测试页面。

## 示例

### 基础示例

```bash
# Echo 服务器
cargo run --example echo_server

# 客户端
cargo run --example client

# Axum 集成（浏览器测试页面 http://127.0.0.1:9001）
cargo run --example axum_server

# WSS 客户端（TLS）
cargo run --example wss_client --features tls-rustls

# 压力测试
cargo run --example stress_server
cargo run --example stress_client

# Autobahn 合规测试
cargo run --example autobahn_server
```

### 完整示例

包含 React 前端的完整示例，展示真实应用场景：

| 示例 | 描述 | 特性 |
|------|------|------|
| [聊天室](examples/chat_room/) | 多用户实时聊天 | WebSocket 消息、用户在线状态、广播 |
| [屏幕共享](examples/screen_share/) | WebRTC 屏幕共享 | WebSocket 信令、点对点流媒体 |
| [文件传输](examples/file_transfer/) | P2P 风格文件传输 | 二进制 WebSocket、分块传输、进度跟踪 |

#### 聊天室

![聊天室截图](examples/chat_room/screenshot.png)

```bash
cd examples/chat_room/server && cargo run
cd examples/chat_room/frontend && npm install && npm run dev
```

#### 屏幕共享

![屏幕共享截图](examples/screen_share/screenshot.png)

```bash
cd examples/screen_share/server && cargo run
cd examples/screen_share/frontend && npm install && npm run dev
```

#### 文件传输

![文件传输截图](examples/file_transfer/screenshot.png)

```bash
cd examples/file_transfer/server && cargo run
cd examples/file_transfer/frontend && npm install && npm run dev
```

## 许可证

MIT
