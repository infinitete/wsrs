# 最佳实践

[English](BEST_PRACTICES.md)

本文档介绍使用 rsws 的推荐模式。

---

## 目录

1. [连接管理](#连接管理)
2. [性能优化](#性能优化)
3. [安全性](#安全性)
4. [配置](#配置)
5. [错误处理](#错误处理)
6. [资源管理](#资源管理)

---

## 连接管理

### 优雅关闭

**推荐**: 使用 `close()` 发起符合 RFC 6455 的关闭握手。

```rust
// ✅ 正确: 正常关闭握手
conn.close(CloseCode::Normal, "goodbye").await?;

// ❌ 错误: 直接丢弃连接
drop(conn);  // 对端可能报告协议错误
```

### 消息循环模式

**推荐**: 使用 `while let` 配合 `is_open()` 检查实现健壮的消息处理。

```rust
// ✅ 推荐模式
while conn.is_open() {
    match conn.recv().await? {
        Some(Message::Text(text)) => {
            conn.send(Message::text(process(text))).await?;
        }
        Some(Message::Binary(data)) => {
            conn.send(Message::binary(process(data))).await?;
        }
        Some(Message::Ping(_)) => {
            // Pong 由 recv() 自动发送
        }
        Some(Message::Close(_)) => break,
        None => break,  // 连接已正常关闭
        _ => {}
    }
}
```

### 握手处理

**推荐**: 在创建 `Connection` 之前完成 HTTP 升级握手。

```rust
// 服务端
let request = HandshakeRequest::parse(&request_bytes)?;
request.validate()?;
let response = HandshakeResponse::from_request(&request);
stream.write_all(&response.to_bytes()).await?;

// 现在创建连接
let conn = Connection::new(stream, Role::Server, Config::server());
```

---

## 性能优化

### 批量发送

**推荐**: 对多条消息使用 `send_batch()` 或 `send_no_flush()` + `flush()`。

```rust
// ✅ 最佳: 多条消息只需一次系统调用
conn.send_batch([
    Message::text("one"),
    Message::text("two"),
    Message::text("three"),
]).await?;

// ✅ 替代方案: 手动批量处理
conn.send_no_flush(Message::text("one")).await?;
conn.send_no_flush(Message::text("two")).await?;
conn.send_no_flush(Message::text("three")).await?;
conn.flush().await?;

// ❌ 低效: 三次系统调用
conn.send(Message::text("one")).await?;
conn.send(Message::text("two")).await?;
conn.send(Message::text("three")).await?;
```

### 缓冲区大小

**推荐**: 根据网络特性调整缓冲区大小。

```rust
// 高带宽链路 (10GbE+)
let config = Config::server()
    .with_read_buffer_size(64 * 1024)   // 64 KB
    .with_write_buffer_size(64 * 1024);

// 低延迟应用
let config = Config::server()
    .with_read_buffer_size(4 * 1024)    // 4 KB - 更快处理
    .with_write_buffer_size(4 * 1024);

// 默认: 8 KB - 平衡
```

### 分片大小

**推荐**: 根据消息模式选择分片大小。

```rust
// 大文件传输
let config = Config::client()
    .with_fragment_size(64 * 1024);  // 64 KB 分片

// 频繁的小消息
let config = Config::client()
    .with_fragment_size(4 * 1024);   // 4 KB 分片

// 默认: 16 KB
```

### SIMD 加速

库自动使用 SIMD（AVX2/SSE2/NEON）进行掩码操作。无需配置 - 运行时检测会选择最优实现。

**吞吐量**: 现代 CPU 上超过 150 GiB/s

---

## 安全性

### Origin 验证（CSWSH 防护）

**推荐**: 生产环境中始终配置 `allowed_origins`。

```rust
// ✅ 生产环境: 白名单允许的来源
let config = Config::server()
    .with_allowed_origins(vec![
        "https://app.example.com".into(),
        "https://admin.example.com".into(),
    ]);

// ❌ 仅开发环境: 接受任何来源
let config = Config::server();  // 无来源检查
```

### 大小限制

**推荐**: 选择适当的限制以防止 DoS 攻击。

```rust
// Web 应用（聊天、通知）
let config = Config::server()
    .with_limits(Limits::default());  // 16 MB 帧, 64 MB 消息

// 内存受限的嵌入式/IoT
let config = Config::server()
    .with_limits(Limits::embedded());  // 64 KB 帧, 256 KB 消息

// 内部微服务（受信网络）
let config = Config::server()
    .with_limits(Limits::unrestricted());  // 1 GB 帧, 4 GB 消息
```

### 帧掩码

库自动强制执行 RFC 6455 掩码规则：

- **客户端** (`Config::client()`): 所有帧都被掩码
- **服务端** (`Config::server()`): 拒绝未掩码的客户端帧

```rust
// ⚠️ 仅测试: 接受未掩码帧
let config = Config {
    accept_unmasked_frames: true,  // 不安全
    ..Config::server()
};
```

---

## 配置

### 基于角色的预设

**推荐**: 使用 `Config::server()` 和 `Config::client()` 作为起点。

```rust
// 服务端: 不掩码，验证客户端帧
let config = Config::server();

// 客户端: 掩码所有发出的帧
let config = Config::client();
```

### 限制对比

| 预设 | 帧大小 | 消息大小 | 分片数 | 使用场景 |
|------|--------|----------|--------|----------|
| `default()` | 16 MB | 64 MB | 128 | 通用 Web 应用 |
| `embedded()` | 64 KB | 256 KB | 16 | IoT、内存受限 |
| `unrestricted()` | 1 GB | 4 GB | 1024 | 受信内部服务 |

### 超时配置

**推荐**: 配置超时以处理僵尸连接。

```rust
use std::time::Duration;
use rsws::config::Timeouts;

let config = Config::server()
    .with_timeouts(Timeouts {
        handshake: Duration::from_secs(10),   // 握手超时
        read: Duration::from_secs(30),        // 读取超时
        write: Duration::from_secs(30),       // 写入超时
        idle: Duration::from_secs(300),       // 空闲超时
    });
```

---

## 错误处理

### 错误分类

```rust
match conn.recv().await {
    Ok(Some(msg)) => { /* 处理消息 */ }
    
    Ok(None) => {
        // 正常关闭 - 连接正常结束
    }
    
    Err(Error::ConnectionClosed(_)) => {
        // 对端断开（可能是非正常断开）
    }
    
    Err(Error::ProtocolViolation(reason)) => {
        // ⚠️ 致命: 连接可能已失去同步
        // 立即断开连接
        return Err(e);
    }
    
    Err(Error::FrameTooLarge { size, max }) => {
        // 对端发送超大帧 - 以错误关闭
        conn.close(CloseCode::MessageTooBig, "frame too large").await?;
    }
    
    Err(Error::InvalidUtf8) => {
        // 文本帧包含无效 UTF-8
        conn.close(CloseCode::InvalidPayload, "invalid utf-8").await?;
    }
    
    Err(e) => {
        // 其他错误（I/O 等）
        eprintln!("Error: {}", e);
    }
}
```

### 协议违规

**推荐**: 将 `ProtocolViolation` 视为致命错误 - 立即断开连接。

```rust
// ❌ 不要尝试从协议违规中恢复
if let Err(Error::ProtocolViolation(_)) = result {
    // 连接状态未定义 - 立即关闭
    return Err(e);
}
```

---

## 资源管理

### 每任务一个连接

**推荐**: 每个连接生成一个任务。

```rust
loop {
    let (stream, addr) = listener.accept().await?;
    
    tokio::spawn(async move {
        if let Err(e) = handle_connection(stream).await {
            eprintln!("Connection {} error: {}", addr, e);
        }
    });
}
```

### Ping/Pong 处理

Pong 由 `recv()` 自动发送。您无需手动处理。

```rust
// ✅ 自动发送 pong - 无需操作
match conn.recv().await? {
    Some(Message::Ping(data)) => {
        // Pong 已排队等待下次 recv() 调用发送
        println!("收到 Ping，Pong 将自动发送");
    }
    // ...
}

// 手动 ping 用于保活
conn.ping(vec![]).await?;
```

### 压缩的内存使用

使用 `compression` 功能时，每个连接维护一个 LZ77 字典（约 32 KB）。

```rust
// 高并发: 考虑内存开销
// 10,000 连接 × 32 KB = 仅压缩状态就需要 320 MB
```

---

## 反模式

### ❌ 在异步上下文中阻塞

```rust
// ❌ 错误: 在异步任务中阻塞调用
std::thread::sleep(Duration::from_secs(1));

// ✅ 正确: 使用异步 sleep
tokio::time::sleep(Duration::from_secs(1)).await;
```

### ❌ 忽略关闭帧

```rust
// ❌ 错误: 忽略关闭，导致对端超时
while let Some(msg) = conn.recv().await? {
    if msg.is_text() { /* ... */ }
}

// ✅ 正确: 明确处理关闭
while let Some(msg) = conn.recv().await? {
    match msg {
        Message::Close(_) => break,
        Message::Text(t) => { /* ... */ }
        _ => {}
    }
}
```

### ❌ 无限制的消息累积

```rust
// ❌ 错误: 队列可能无限增长
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

// ✅ 正确: 有界通道实现背压
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
```

---

## 总结

| 领域 | 建议 |
|------|------|
| 关闭 | 始终使用 `close()` 优雅关闭 |
| 批量 | 对多条消息使用 `send_batch()` |
| 安全 | 生产环境配置 `allowed_origins` |
| 限制 | 资源受限环境使用 `Limits::embedded()` |
| 错误 | 将 `ProtocolViolation` 视为致命错误 |
| Pong | 由 `recv()` 自动处理 |
