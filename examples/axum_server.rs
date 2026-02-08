//! Example: Integrating rsws with an Axum HTTP server.
//!
//! This demonstrates how to use rsws as the WebSocket protocol handler
//! while letting Axum handle HTTP routing and the upgrade handshake.
//!
//! Run with:
//!   cargo run --example axum_server
//!
//! Test with the built-in client example:
//!   cargo run --example client
//!
//! Or open http://127.0.0.1:9001 in a browser to use the built-in test page.

use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use hyper_util::rt::TokioIo;
use rsws::{CloseCode, Config, Connection, Message, Role, compute_accept_key};
use std::error::Error;

const ADDR: &str = "127.0.0.1:9001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/ws", get(ws_handler));

    println!("Axum + rsws server listening on {}", ADDR);
    println!("  WebSocket endpoint: ws://{}/ws", ADDR);
    println!("  Test page:          http://{}/", ADDR);

    let listener = tokio::net::TcpListener::bind(ADDR).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Routes "/" — serves the HTML test page for normal browser requests,
/// or upgrades to WebSocket when the client sends upgrade headers
/// (e.g. `cargo run --example client` connects to "/").
async fn root_handler(req: Request) -> Response {
    let is_upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));

    if is_upgrade {
        return ws_handler(req).await;
    }

    Html(
        r#"<!DOCTYPE html>
<html>
<head><title>rsws + Axum</title></head>
<body>
  <h2>rsws + Axum WebSocket Demo</h2>
  <div>
    <input id="msg" type="text" value="Hello, WebSocket!" />
    <button onclick="send()">Send</button>
    <button onclick="close_ws()">Close</button>
  </div>
  <pre id="log"></pre>
  <script>
    const log = document.getElementById('log');
    const ws = new WebSocket('ws://' + location.host + '/ws');
    ws.onopen    = () => appendLog('Connected');
    ws.onmessage = (e) => appendLog('Received: ' + e.data);
    ws.onclose   = (e) => appendLog('Closed: code=' + e.code + ' reason=' + e.reason);
    ws.onerror   = (e) => appendLog('Error: ' + e);
    function send() {
      const text = document.getElementById('msg').value;
      ws.send(text);
      appendLog('Sent: ' + text);
    }
    function close_ws() { ws.close(1000, 'user closed'); }
    function appendLog(msg) { log.textContent += msg + '\n'; }
  </script>
</body>
</html>"#,
    )
    .into_response()
}

/// Handles the WebSocket upgrade using rsws.
///
/// The flow:
///   1. Validate WebSocket upgrade headers from the HTTP request.
///   2. Compute `Sec-WebSocket-Accept` using rsws's `compute_accept_key`.
///   3. Return a `101 Switching Protocols` response to complete the handshake.
///   4. Spawn a task that awaits the upgraded raw I/O stream and wraps it in
///      an rsws `Connection` for full RFC 6455 message handling.
async fn ws_handler(mut req: Request) -> Response {
    // --- Step 1: Validate upgrade headers ---
    let headers = req.headers();

    let is_upgrade = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));

    if !is_upgrade {
        return (StatusCode::BAD_REQUEST, "Not a WebSocket upgrade request").into_response();
    }

    let has_connection_upgrade = headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_lowercase().contains("upgrade"));

    if !has_connection_upgrade {
        return (StatusCode::BAD_REQUEST, "Missing Connection: Upgrade").into_response();
    }

    let sec_key = match headers.get("sec-websocket-key").and_then(|v| v.to_str().ok()) {
        Some(key) => key.to_owned(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing Sec-WebSocket-Key").into_response();
        }
    };

    // --- Step 2: Compute the accept key with rsws ---
    let accept_key = compute_accept_key(&sec_key);

    // --- Step 3: Spawn the upgrade task ---
    //
    // `hyper::upgrade::on` consumes the request body and resolves once the
    // HTTP layer hands over the raw bidirectional byte stream (`Upgraded`).
    // We wrap it in `TokioIo` so it implements `tokio::io::AsyncRead + AsyncWrite`.
    tokio::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgraded) => {
                let io = TokioIo::new(upgraded);
                if let Err(e) = handle_websocket(io).await {
                    eprintln!("WebSocket session error: {}", e);
                }
            }
            Err(e) => eprintln!("HTTP upgrade failed: {}", e),
        }
    });

    // --- Step 4: Return 101 to complete the handshake ---
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Accept", accept_key)
        .body(axum::body::Body::empty())
        .unwrap()
}

/// Echo handler powered by rsws.
///
/// Demonstrates the standard rsws message loop: receive a message, inspect
/// its type, and send a response. Ping/Pong is handled automatically by
/// `Connection::recv`.
async fn handle_websocket(
    io: TokioIo<hyper::upgrade::Upgraded>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = Config::server();
    let mut conn = Connection::new(io, Role::Server, config);

    println!("  WebSocket connection established");

    while conn.is_open() {
        match conn.recv().await? {
            Some(Message::Text(text)) => {
                println!("  Received text: {}", text);
                conn.send(Message::text(text)).await?;
            }
            Some(Message::Binary(data)) => {
                println!("  Received binary: {} bytes", data.len());
                conn.send(Message::binary(data)).await?;
            }
            Some(Message::Ping(data)) => {
                println!(
                    "  Received ping ({} bytes) - pong sent automatically",
                    data.len()
                );
            }
            Some(Message::Pong(data)) => {
                println!("  Received pong: {} bytes", data.len());
            }
            Some(Message::Close(frame)) => {
                if let Some(cf) = frame {
                    println!("  Received close: {} - {}", cf.code.as_u16(), cf.reason);
                } else {
                    println!("  Received close (no code)");
                }
                break;
            }
            None => {
                println!("  Connection closed");
                break;
            }
            _ => {}
        }
    }

    // Ensure a graceful close if we haven't already.
    if conn.is_open() {
        let _ = conn.close(CloseCode::Normal, "goodbye").await;
    }

    println!("  Session ended");
    Ok(())
}
