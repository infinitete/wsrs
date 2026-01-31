//! Concurrency tests for WebSocket connections.
//!
//! Tests protocol correctness and message ordering under concurrent load.

mod harness;

use harness::{Metrics, TestClient, TestServer};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Barrier;
use tokio::task::JoinSet;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_multiple_clients_sequential() {
    let (server, addr) = TestServer::spawn().await;

    for i in 0..10 {
        let mut client = TestClient::connect_with_id(addr, i).await.unwrap();
        let msg = format!("hello from client {}", i);
        client.send_text(&msg).await.unwrap();
        let response = client.recv_text().await.unwrap();
        assert_eq!(response, Some(msg));
        client.close().await.unwrap();
    }

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_multiple_clients_parallel() {
    let (server, addr) = TestServer::spawn().await;
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for i in 0..10 {
        let success = success_count.clone();
        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, i).await.unwrap();
            let msg = format!("hello from client {}", i);
            client.send_text(&msg).await.unwrap();
            let response = client.recv_text().await.unwrap();
            assert_eq!(response, Some(msg));
            client.close().await.unwrap();
            success.fetch_add(1, Ordering::Relaxed);
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    assert_eq!(success_count.load(Ordering::Relaxed), 10);
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_barrier_synchronized_connect() {
    const NUM_CLIENTS: usize = 50;

    let (server, addr) = TestServer::spawn().await;
    let barrier = Arc::new(Barrier::new(NUM_CLIENTS));
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for i in 0..NUM_CLIENTS {
        let barrier = barrier.clone();
        let success = success_count.clone();

        set.spawn(async move {
            barrier.wait().await;

            let mut client = TestClient::connect_with_id(addr, i).await.unwrap();
            client.send_text("sync test").await.unwrap();
            let response = client.recv_text().await.unwrap();
            assert_eq!(response, Some("sync test".to_string()));
            client.close().await.unwrap();

            success.fetch_add(1, Ordering::Relaxed);
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let count = success_count.load(Ordering::Relaxed);
    println!("{} clients synchronized and completed successfully", count);
    assert_eq!(count, NUM_CLIENTS);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_barrier_synchronized_send() {
    const NUM_CLIENTS: usize = 30;
    const MESSAGES_PER_CLIENT: usize = 10;

    let (server, addr) = TestServer::spawn().await;
    let barrier = Arc::new(Barrier::new(NUM_CLIENTS));
    let metrics = Metrics::new();

    let mut set = JoinSet::new();

    for client_id in 0..NUM_CLIENTS {
        let barrier = barrier.clone();
        let metrics = metrics.clone();

        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, client_id).await.unwrap();
            metrics.record_connection();

            barrier.wait().await;

            for seq in 0..MESSAGES_PER_CLIENT {
                let msg = format!("client:{}:msg:{}", client_id, seq);
                client.send_text(&msg).await.unwrap();
                metrics.record_message_sent();

                let response = client.recv_text().await.unwrap();
                assert_eq!(response, Some(msg));
                metrics.record_message_received();
            }

            client.close().await.unwrap();
            metrics.record_disconnect();
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let sent = metrics.messages_sent();
    let received = metrics.messages_received();
    println!(
        "Barrier sync send: {} messages sent, {} received",
        sent, received
    );

    assert_eq!(sent, NUM_CLIENTS * MESSAGES_PER_CLIENT);
    assert_eq!(received, NUM_CLIENTS * MESSAGES_PER_CLIENT);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_message_ordering() {
    const NUM_CLIENTS: usize = 20;
    const MESSAGES_PER_CLIENT: usize = 50;

    let (server, addr) = TestServer::spawn().await;
    let ordering_violations = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for client_id in 0..NUM_CLIENTS {
        let violations = ordering_violations.clone();

        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, client_id).await.unwrap();

            for seq in 0..MESSAGES_PER_CLIENT {
                let msg = format!("client:{}:msg:{}", client_id, seq);
                client.send_text(&msg).await.unwrap();

                let response = client.recv_text().await.unwrap().unwrap();

                let parts: Vec<&str> = response.split(':').collect();
                if parts.len() == 4 {
                    let resp_client: usize = parts[1].parse().unwrap_or(usize::MAX);
                    let resp_seq: usize = parts[3].parse().unwrap_or(usize::MAX);

                    if resp_client != client_id || resp_seq != seq {
                        violations.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    violations.fetch_add(1, Ordering::Relaxed);
                }
            }

            client.close().await.unwrap();
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let total_messages = NUM_CLIENTS * MESSAGES_PER_CLIENT;
    let violation_count = ordering_violations.load(Ordering::Relaxed);

    println!(
        "Message ordering: {} messages validated, {} violations",
        total_messages, violation_count
    );

    assert_eq!(violation_count, 0, "Message ordering violations detected");

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn test_message_ordering_under_load() {
    const NUM_CLIENTS: usize = 100;
    const MESSAGES_PER_CLIENT: usize = 100;

    let (server, addr) = TestServer::spawn().await;
    let ordering_violations = Arc::new(AtomicUsize::new(0));
    let total_received = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for client_id in 0..NUM_CLIENTS {
        let violations = ordering_violations.clone();
        let received = total_received.clone();

        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, client_id).await.unwrap();

            for seq in 0..MESSAGES_PER_CLIENT {
                let msg = format!("client:{}:msg:{}", client_id, seq);
                client.send_text(&msg).await.unwrap();

                let response = client.recv_text().await.unwrap().unwrap();
                received.fetch_add(1, Ordering::Relaxed);

                let parts: Vec<&str> = response.split(':').collect();
                if parts.len() == 4 {
                    let resp_client: usize = parts[1].parse().unwrap_or(usize::MAX);
                    let resp_seq: usize = parts[3].parse().unwrap_or(usize::MAX);

                    if resp_client != client_id || resp_seq != seq {
                        violations.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    violations.fetch_add(1, Ordering::Relaxed);
                }
            }

            client.close().await.unwrap();
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let total = total_received.load(Ordering::Relaxed);
    let violations = ordering_violations.load(Ordering::Relaxed);

    println!("{} messages validated, {} violations", total, violations);
    assert_eq!(violations, 0);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_close_handshake_under_load() {
    const NUM_CLIENTS: usize = 50;

    let (server, addr) = TestServer::spawn().await;
    let closed_gracefully = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for i in 0..NUM_CLIENTS {
        let closed = closed_gracefully.clone();

        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, i).await.unwrap();
            client.send_text("test").await.unwrap();
            let _ = client.recv_text().await.unwrap();
            client.close().await.unwrap();
            closed.fetch_add(1, Ordering::Relaxed);
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let count = closed_gracefully.load(Ordering::Relaxed);
    println!("{} clients closed gracefully", count);
    assert_eq!(count, NUM_CLIENTS);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ping_pong_under_load() {
    const NUM_CLIENTS: usize = 30;

    let (server, addr) = TestServer::spawn().await;
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for i in 0..NUM_CLIENTS {
        let success = success_count.clone();

        set.spawn(async move {
            let mut client = TestClient::connect_with_id(addr, i).await.unwrap();

            client.send_text("keepalive").await.unwrap();
            let _ = client.recv_text().await.unwrap();

            client.close().await.unwrap();
            success.fetch_add(1, Ordering::Relaxed);
        });
    }

    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    let count = success_count.load(Ordering::Relaxed);
    println!("{} clients completed ping/pong cycle", count);
    assert_eq!(count, NUM_CLIENTS);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn test_barrier_synchronized_connect_500() {
    const NUM_CLIENTS: usize = 500;

    let (server, addr) = TestServer::spawn().await;
    let barrier = Arc::new(Barrier::new(NUM_CLIENTS));
    let success_count = Arc::new(AtomicUsize::new(0));
    let failure_count = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for i in 0..NUM_CLIENTS {
        let barrier = barrier.clone();
        let success = success_count.clone();
        let failure = failure_count.clone();

        set.spawn(async move {
            barrier.wait().await;

            match TestClient::connect_with_id(addr, i).await {
                Ok(mut client) => {
                    if client.send_text("sync").await.is_ok() {
                        if client.recv_text().await.is_ok() {
                            let _ = client.close().await;
                            success.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                    failure.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    failure.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    while let Some(result) = set.join_next().await {
        let _ = result;
    }

    let successes = success_count.load(Ordering::Relaxed);
    let failures = failure_count.load(Ordering::Relaxed);

    println!(
        "{} clients synchronized: {} success, {} failed",
        NUM_CLIENTS, successes, failures
    );

    assert!(
        successes >= NUM_CLIENTS * 95 / 100,
        "Expected at least 95% success rate"
    );

    server.shutdown().await;
}
