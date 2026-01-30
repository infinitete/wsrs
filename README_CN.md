# rsws - 生产级 WebSocket 库

[English](README.md) | 中文

`rsws` 是一个高性能、符合 RFC 6455 标准的 Rust WebSocket 协议库。专为生产环境设计，具有零拷贝解析、异步优先架构和全面的安全特性。

## 特性

- **零拷贝帧解析** - 最小化内存分配，优化吞吐量
- **异步优先设计** - 运行时无关的核心，支持 Tokio
- **完全符合 RFC 6455** - 严格的验证和协议正确性
- **TLS/HTTPS 支持** - 通过 rustls 或 native-tls 实现安全 WebSocket (wss://)
- **消息级 deflate 压缩** - 通过协商扩展减少带宽使用
- **生产级限制** - 可配置的帧/消息大小限制，防止资源耗尽
- **全面的错误处理** - 详细的错误类型便于调试

## 安装

在 `Cargo.toml` 中添加 `rsws`：

```toml
[dependencies]
rsws = "0.1"
```

## 快速开始

### 客户端示例

```rust
use rsws::{Connection, Config, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stream = tokio::net::TcpStream::connect("echo.websocket.org:80").await?;
    let config = Config::client();
    let mut conn = Connection::new(stream, Role::Client, config);

    conn.send(rsws::Message::text("Hello, WebSocket!")).await?;
    
    if let Some(msg) = conn.recv().await? {
        println!("收到: {:?}", msg);
    }
    
    conn.close(rsws::CloseCode::Normal, "完成").await?;
    Ok(())
}
```

### 服务端示例

```rust
use rsws::{Connection, Config, Role, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    
    loop {
        let (stream, _) = listener.accept().await?;
        let config = Config::server();
        
        tokio::spawn(async move {
            let mut conn = Connection::new(stream, Role::Server, config);
            
            while let Some(msg) = conn.recv().await.unwrap() {
                // 回显消息
                match msg {
                    Message::Text(text) => {
                        conn.send(Message::text(text)).await.unwrap();
                    }
                    Message::Binary(data) => {
                        conn.send(Message::binary(data)).await.unwrap();
                    }
                    _ => { /* 处理控制帧 */ }
                }
            }
        });
    }
}
```

### TLS 服务端示例

```rust
use rsws::{tls::TlsAcceptor, Connection, Config, Role};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let certs = rsws::tls::load_certs_from_file("cert.pem")?;
    let key = rsws::tls::load_private_key_from_file("key.pem")?;
    let tls_config = rsws::tls::server_config(certs, key)?;
    let tls_acceptor = TlsAcceptor::new(Arc::new(tls_config));
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8443").await?;
    
    loop {
        let stream = listener.accept().await?;
        let tls_stream = tls_acceptor.accept(stream).await?;
        let config = Config::server();
        let mut conn = Connection::new(tls_stream, Role::Server, config);
        
        // 处理连接...
    }
}
```

## 功能特性标志

| 特性 | 描述 | 默认 |
|------|------|------|
| `async-tokio` | 启用 Tokio 异步 I/O | 是 |
| `tls-rustls` | 通过 rustls 启用 TLS（纯 Rust） | 否 |
| `tls-native` | 通过 native-tls 启用 TLS（平台原生） | 否 |
| `compression` | 启用消息级 deflate 压缩 | 否 |

### 推荐配置

```toml
# 最小配置（无 TLS，无压缩）
[dependencies]
rsws = "0.1"

# 使用 rustls TLS
[dependencies]
rsws = { version = "0.1", features = ["tls-rustls"] }

# 使用压缩
[dependencies]
rsws = { version = "0.1", features = ["compression"] }

# 完整功能
[dependencies]
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```

## 性能表现 🚀

`rsws` 经过底层重构，利用 SIMD 加速实现了超过 **150 GiB/s** 的惊人吞吐量。

### 基准测试结果

| 负载大小 | 标量 (基准) | SIMD (AVX2/NEON) | 提升幅度 |
|---------|------------|------------------|---------|
| **64 KB** | ~10.0 GiB/s | **154.9 GiB/s** | **~15倍** 🚀 |
| **1 MB**  | 7.07 GiB/s  | **101.2 GiB/s** | **~14倍** 🚀 |

### 核心优化技术

- **SIMD 加速**: 运行时自动检测并使用 AVX2/SSE2/NEON 指令集，大幅提升掩码操作效率。
- **零拷贝架构**: 
  - 基于 `Bytes` 的非掩码帧解析实现 **0 内存分配**。
  - 单缓冲区消息重组，彻底消除了 N+1 次的内存分配开销。
- **高效 I/O**: `send_batch()` 显著减少系统调用，配合直接缓冲区 I/O 消除中间拷贝。
- **可配置缓冲区**: 通过 `read_buffer_size` 和 `write_buffer_size` 针对您的工作负载进行调优。

在您的硬件上运行基准测试：

```bash
cargo bench --bench benchmarks
```

## API 概览

### 核心类型

- **`Connection<T>`** - 主 WebSocket 连接类型，包装异步 I/O 流
- **`Config`** - 连接配置，包括限制和分片大小
- **`Limits`** - 帧大小、消息大小和分片数量的资源限制
- **`Message`** - 表示 WebSocket 消息的枚举（Text、Binary、Ping、Pong、Close）
- **`Frame`** - 用于直接协议操作的低级帧类型

### 握手函数

- **`compute_accept_key`** - 计算 Sec-WebSocket-Accept 头值
- **`HandshakeRequest`** / **`HandshakeResponse`** - HTTP 升级握手的类型

### 消息构建器

```rust
// 创建消息
let text = Message::text("你好");
let binary = Message::binary(vec![0x01, 0x02, 0x03]);
let ping = Message::ping(vec![]);
let pong = Message::pong(data);
let close = Message::close(CloseCode::Normal, "再见");

// 检查消息类型
if msg.is_text() { /* ... */ }
if msg.is_binary() { /* ... */ }
if msg.is_data() { /* ... */ }
if msg.is_control() { /* ... */ }

// 提取数据
if let Some(text) = msg.into_text() { /* ... */ }
if let Some(data) = msg.into_binary() { /* ... */ }
```

### 配置

```rust
// 默认配置
let config = Config::default();

// 服务端角色（不掩码，验证客户端帧）
let server_config = Config::server();

// 客户端角色（掩码所有帧）
let client_config = Config::client();

// 自定义限制
let config = Config::new()
    .with_limits(Limits::embedded())  // 用于资源受限环境
    .with_limits(Limits::unrestricted())  // 用于受信环境
    .with_fragment_size(4096);  // 分片大消息
```

## 错误处理

```rust
use rsws::{Error, Result};

match connection.send(Message::text("你好")).await {
    Ok(()) => println!("发送成功"),
    Err(Error::ConnectionClosed(None)) => println!("连接已关闭"),
    Err(Error::FrameTooLarge { size, max }) => {
        println!("帧太大: {} > {}", size, max)
    }
    Err(e) => println!("错误: {:?}", e),
}
```

## 协议合规性

`rsws` 实现了 [RFC 6455](https://tools.ietf.org/html/rfc6455) 规定的 WebSocket 协议：

- 帧格式和掩码
- 消息分片和重组
- 文本消息的 UTF-8 验证
- 控制帧处理（Close、Ping、Pong）
- HTTP 升级握手
- 扩展协商框架

压缩扩展实现符合 [RFC 7692](https://tools.ietf.org/html/rfc7692)（permessage-deflate）。

## 示例

查看 [examples](examples/) 目录获取更多示例：

- [`echo_server.rs`](examples/echo_server.rs) - WebSocket 回显服务器
- [`client.rs`](examples/client.rs) - WebSocket 客户端
- [`wss_client.rs`](examples/wss_client.rs) - TLS WebSocket 客户端
- [`autobahn_server.rs`](examples/autobahn_server.rs) - Autobahn 测试服务器

## Autobahn 测试

本库包含 [Autobahn WebSocket 测试套件](https://github.com/crossbario/autobahn-testsuite) 集成。查看 [autobahn/README.md](autobahn/README.md) 了解如何运行合规性测试。

## 许可证

[MIT 许可证](LICENSE)
