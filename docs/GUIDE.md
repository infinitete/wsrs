# rsws 用户指南

本指南帮助你快速上手 rsws WebSocket 库。

## 目录

- [安装](#安装)
- [快速开始](#快速开始)
- [客户端示例](#客户端示例)
- [服务端示例](#服务端示例)
- [消息处理](#消息处理)
- [配置选项](#配置选项)
- [TLS 安全连接](#tls-安全连接)
- [压缩扩展](#压缩扩展)
- [错误处理](#错误处理)
- [性能优化](#性能优化)
- [最佳实践](#最佳实践)

---

## 安装

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
rsws = "0.1"
```

启用可选功能：

```toml
# TLS 支持 (推荐 rustls)
rsws = { version = "0.1", features = ["tls-rustls"] }

# 压缩支持
rsws = { version = "0.1", features = ["compression"] }

# 全部功能
rsws = { version = "0.1", features = ["tls-rustls", "compression"] }
```

---

## 快速开始

### 最简客户端

```rust
use rsws::{Connection, Config, Role, Message, CloseCode};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 建立 TCP 连接
    let stream = TcpStream::connect("127.0.0.1:8080").await?;
    
    // 2. 创建 WebSocket 连接
    let config = Config::client();
    let mut conn = Connection::new(stream, Role::Client, config);
    
    // 3. 发送消息
    conn.send(Message::text("Hello, WebSocket!")).await?;
    
    // 4. 接收响应
    if let Some(msg) = conn.recv().await? {
        println!("收到: {:?}", msg);
    }
    
    // 5. 关闭连接
    conn.close(CloseCode::Normal, "done").await?;
    
    Ok(())
}
```

---

## 客户端示例

### 带 HTTP 握手的客户端

```rust
use rsws::{Connection, Config, Role, Message, HandshakeRequest, HandshakeResponse};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn connect_with_handshake(host: &str, port: u16) -> Result<Connection<TcpStream>, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(format!("{}:{}", host, port)).await?;
    
    // 构建握手请求
    let request = HandshakeRequest::new(format!("{}:{}", host, port), "/ws");
    let mut buf = Vec::new();
    request.write(&mut buf)?;
    
    // 发送握手
    stream.write_all(&buf).await?;
    
    // 读取响应
    let mut response_buf = vec![0u8; 1024];
    let n = stream.read(&mut response_buf).await?;
    
    // 验证响应
    let response = HandshakeResponse::parse(&response_buf[..n])?;
    response.validate(&request)?;
    
    // 创建连接
    Ok(Connection::new(stream, Role::Client, Config::client()))
}
```

### 持续接收消息

```rust
async fn message_loop(mut conn: Connection<TcpStream>) -> Result<(), rsws::Error> {
    loop {
        match conn.recv().await? {
            Some(Message::Text(text)) => {
                println!("文本消息: {}", text);
            }
            Some(Message::Binary(data)) => {
                println!("二进制消息: {} bytes", data.len());
            }
            Some(Message::Close(frame)) => {
                println!("连接关闭: {:?}", frame);
                break;
            }
            None => {
                println!("连接已断开");
                break;
            }
            _ => {} // Ping/Pong 由库自动处理
        }
    }
    Ok(())
}
```

---

## 服务端示例

### Echo 服务器

```rust
use rsws::{Connection, Config, Role, Message, HandshakeRequest, HandshakeResponse};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("WebSocket 服务器运行在 ws://0.0.0.0:8080");
    
    loop {
        let (mut stream, addr) = listener.accept().await?;
        println!("新连接: {}", addr);
        
        tokio::spawn(async move {
            // 处理握手
            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap();
            
            let request = HandshakeRequest::parse(&buf[..n]).unwrap();
            request.validate().unwrap();
            
            let response = HandshakeResponse::from_request(&request);
            let mut response_buf = Vec::new();
            response.write(&mut response_buf).unwrap();
            stream.write_all(&response_buf).await.unwrap();
            
            // 创建连接
            let config = Config::server();
            let mut conn = Connection::new(stream, Role::Server, config);
            
            // Echo 循环
            while let Some(msg) = conn.recv().await.unwrap() {
                match msg {
                    Message::Text(text) => {
                        conn.send(Message::text(text)).await.unwrap();
                    }
                    Message::Binary(data) => {
                        conn.send(Message::binary(data)).await.unwrap();
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });
    }
}
```

---

## 消息处理

### 消息类型

```rust
use rsws::{Message, CloseCode};

// 创建消息
let text = Message::text("Hello");
let binary = Message::binary(vec![0x01, 0x02, 0x03]);
let ping = Message::ping(vec![]);
let close = Message::close(CloseCode::Normal, "goodbye");

// 检查消息类型
if msg.is_text() { /* ... */ }
if msg.is_binary() { /* ... */ }
if msg.is_data() { /* 文本或二进制 */ }
if msg.is_control() { /* Ping, Pong, Close */ }

// 提取内容
if let Some(text) = msg.as_text() {
    println!("文本: {}", text);
}
if let Some(data) = msg.as_binary() {
    println!("二进制: {} bytes", data.len());
}
```

### 批量发送

```rust
// 多条消息，单次 flush（高效）
let messages = vec![
    Message::text("消息1"),
    Message::text("消息2"),
    Message::text("消息3"),
];
conn.send_batch(messages).await?;

// 或手动控制 flush
conn.send_no_flush(Message::text("消息1")).await?;
conn.send_no_flush(Message::text("消息2")).await?;
conn.flush().await?;
```

---

## 配置选项

### 预设配置

```rust
use rsws::{Config, Limits};

// 客户端 (自动添加掩码)
let client_config = Config::client();

// 服务端 (验证客户端掩码)
let server_config = Config::server();
```

### 自定义配置

```rust
let config = Config::new()
    .with_limits(Limits {
        max_frame_size: 1024 * 1024,       // 1 MB
        max_message_size: 10 * 1024 * 1024, // 10 MB
        max_fragment_count: 100,
        max_handshake_size: 8192,
    })
    .with_fragment_size(65536)     // 64 KB 分片
    .with_read_buffer_size(8192)   // 8 KB 读缓冲
    .with_write_buffer_size(8192); // 8 KB 写缓冲
```

### 资源限制预设

```rust
// 嵌入式/受限环境
let limits = Limits::embedded();  // 64KB frame, 256KB message

// 无限制（仅限可信环境）
let limits = Limits::unrestricted();

// 默认
let limits = Limits::default();  // 16MB frame, 64MB message
```

---

## TLS 安全连接

### 客户端 TLS

```rust
use rsws::tls::{TlsConnector, client_config_with_native_roots};

let config = client_config_with_native_roots()?;
let connector = TlsConnector::new(config);

let tcp_stream = TcpStream::connect("example.com:443").await?;
let tls_stream = connector.connect("example.com", tcp_stream).await?;

let mut conn = Connection::new(tls_stream, Role::Client, Config::client());
```

### 服务端 TLS

```rust
use rsws::tls::{TlsAcceptor, load_certs_from_file, load_private_key_from_file, server_config};
use std::sync::Arc;

let certs = load_certs_from_file("cert.pem")?;
let key = load_private_key_from_file("key.pem")?;
let config = server_config(certs, key)?;
let acceptor = TlsAcceptor::new(config);

let (tcp_stream, _) = listener.accept().await?;
let tls_stream = acceptor.accept(tcp_stream).await?;

let mut conn = Connection::new(tls_stream, Role::Server, Config::server());
```

---

## 压缩扩展

启用 `compression` 功能后可使用 permessage-deflate：

```rust
use rsws::extensions::deflate::{DeflateConfig, DeflateExtension};
use rsws::extensions::ExtensionRegistry;

let config = DeflateConfig::new()
    .compression_level(6)
    .server_no_context_takeover(true);

let extension = DeflateExtension::server(config);

let mut registry = ExtensionRegistry::new();
registry.add(Box::new(extension))?;

// 在握手时协商扩展
// 然后 registry.encode/decode 自动处理压缩
```

---

## 错误处理

### 常见错误

```rust
use rsws::Error;

match result {
    Err(Error::ConnectionClosed(code)) => {
        println!("连接已关闭: {:?}", code);
    }
    Err(Error::FrameTooLarge { size, max }) => {
        println!("帧太大: {} > {}", size, max);
    }
    Err(Error::InvalidUtf8) => {
        println!("无效的 UTF-8 文本");
    }
    Err(Error::Io(msg)) => {
        println!("I/O 错误: {}", msg);
    }
    Err(e) => {
        println!("其他错误: {:?}", e);
    }
    Ok(_) => {}
}
```

---

## 性能优化

### 1. 使用批量发送

```rust
// ❌ 每条消息都 flush
for msg in messages {
    conn.send(msg).await?;
}

// ✅ 批量发送，单次 flush
conn.send_batch(messages).await?;
```

### 2. 配置合适的缓冲区大小

```rust
let config = Config::new()
    .with_read_buffer_size(65536)   // 大负载时增加
    .with_write_buffer_size(65536);
```

### 3. 使用零拷贝解析

库内部已使用 `Bytes` 进行零拷贝优化，无需额外配置。

---

## 最佳实践

### 1. 始终处理关闭帧

```rust
while let Some(msg) = conn.recv().await? {
    match msg {
        Message::Close(frame) => {
            // 响应关闭
            if let Some(f) = frame {
                conn.close(f.code, &f.reason).await?;
            }
            break;
        }
        // ...
    }
}
```

### 2. 设置合理的资源限制

```rust
// 生产环境使用默认限制
let config = Config::server();

// 不要使用 Limits::unrestricted()，除非完全信任客户端
```

### 3. 优雅关闭

```rust
// 发送关闭帧
conn.close(CloseCode::Normal, "shutting down").await?;

// 等待对方确认（可选）
while let Some(msg) = conn.recv().await? {
    if matches!(msg, Message::Close(_)) {
        break;
    }
}
```

### 4. 处理 Ping/Pong

库会自动响应 Ping 帧，你不需要手动处理。但如果需要发送心跳：

```rust
// 客户端主动发送 Ping
conn.send(Message::ping(vec![])).await?;
```

---

## 更多资源

- [API 参考文档](API.md)
- [RFC 6455 规范](https://tools.ietf.org/html/rfc6455)
- [示例代码](../examples/)
