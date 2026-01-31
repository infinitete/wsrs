//! Stress tests for high-concurrency WebSocket validation.
//!
//! These tests are marked #[ignore] and run via: cargo test -- --ignored

mod harness;

use harness::{Latencies, Metrics, TestClient, TestServer};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

fn get_stress_client_count() -> usize {
    std::env::var("RSWS_STRESS_CLIENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000)
}

const MAX_CONCURRENT: usize = 200;
const MESSAGES_PER_CLIENT: usize = 10;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn test_stress_connections() {
    let num_clients = get_stress_client_count();
    println!(
        "Stress test: {} clients, {} max concurrent",
        num_clients, MAX_CONCURRENT
    );

    let (server, addr) = TestServer::spawn().await;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let metrics = Metrics::new();

    let mut set = JoinSet::new();

    for client_id in 0..num_clients {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let metrics = metrics.clone();

        set.spawn(async move {
            match TestClient::connect_with_id(addr, client_id).await {
                Ok(mut client) => {
                    metrics.record_connection();

                    if client.send_text("stress test").await.is_ok() {
                        metrics.record_message_sent();
                        if client.recv_text().await.is_ok() {
                            metrics.record_message_received();
                        }
                    }

                    let _ = client.close().await;
                    metrics.record_disconnect();
                }
                Err(_) => {
                    metrics.record_connection_failed();
                }
            }

            drop(permit);
        });
    }

    while let Some(result) = set.join_next().await {
        if result.is_err() {
            metrics.record_error();
        }
    }

    metrics.report();

    let total = metrics.connections_total();
    let failed = metrics.connections_failed();
    let success_rate = if num_clients > 0 {
        (total as f64 / num_clients as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "{} clients connected successfully ({:.1}%), {} failed",
        total, success_rate, failed
    );

    assert!(
        success_rate >= 95.0,
        "Expected at least 95% connection success rate, got {:.1}%",
        success_rate
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn test_stress_throughput() {
    let num_clients = get_stress_client_count().min(500);
    println!(
        "Throughput test: {} clients, {} messages each",
        num_clients, MESSAGES_PER_CLIENT
    );

    let (server, addr) = TestServer::spawn().await;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let metrics = Metrics::new();

    let start = Instant::now();
    let mut set = JoinSet::new();

    for client_id in 0..num_clients {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let metrics = metrics.clone();

        set.spawn(async move {
            if let Ok(mut client) = TestClient::connect_with_id(addr, client_id).await {
                metrics.record_connection();

                for seq in 0..MESSAGES_PER_CLIENT {
                    let msg = format!("throughput:{}:{}", client_id, seq);
                    if client.send_text(&msg).await.is_ok() {
                        metrics.record_message_sent();
                        if client.recv_text().await.is_ok() {
                            metrics.record_message_received();
                        }
                    }
                }

                let _ = client.close().await;
                metrics.record_disconnect();
            } else {
                metrics.record_connection_failed();
            }

            drop(permit);
        });
    }

    while let Some(result) = set.join_next().await {
        if result.is_err() {
            metrics.record_error();
        }
    }

    let elapsed = start.elapsed();
    let total_messages = metrics.messages_received();
    let throughput = if elapsed.as_secs_f64() > 0.0 {
        total_messages as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    metrics.report();
    println!("Throughput: {:.2} msg/sec", throughput);
    println!("Elapsed: {:?}", elapsed);

    assert!(total_messages > 0, "Expected some messages to be processed");

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn test_stress_latency() {
    let num_clients = get_stress_client_count().min(500);
    println!(
        "Latency test: {} clients, {} messages each",
        num_clients, MESSAGES_PER_CLIENT
    );

    let (server, addr) = TestServer::spawn().await;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let latencies = Latencies::new();
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut set = JoinSet::new();

    for client_id in 0..num_clients {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let latencies = latencies.clone();
        let success = success_count.clone();

        set.spawn(async move {
            if let Ok(mut client) = TestClient::connect_with_id(addr, client_id).await {
                for seq in 0..MESSAGES_PER_CLIENT {
                    let msg = format!("latency:{}:{}", client_id, seq);

                    let start = Instant::now();
                    if client.send_text(&msg).await.is_ok() {
                        if client.recv_text().await.is_ok() {
                            let latency = start.elapsed();
                            latencies.record(latency);
                            success.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                let _ = client.close().await;
            }

            drop(permit);
        });
    }

    while set.join_next().await.is_some() {}

    latencies.report();

    let total = success_count.load(Ordering::Relaxed);
    println!("{} round-trips measured", total);

    if let Some(p99) = latencies.p99() {
        println!("P99 latency: {:?}", p99);
        assert!(
            p99 < Duration::from_secs(5),
            "P99 latency too high: {:?}",
            p99
        );
    }

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn test_stress_sustained_load() {
    let num_clients = get_stress_client_count().min(200);
    let duration_secs = 10;

    println!(
        "Sustained load test: {} clients for {} seconds",
        num_clients, duration_secs
    );

    let (server, addr) = TestServer::spawn().await;
    let semaphore = Arc::new(Semaphore::new(num_clients));
    let metrics = Metrics::new();
    let stop_signal = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    let mut set = JoinSet::new();

    for client_id in 0..num_clients {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let metrics = metrics.clone();
        let stop = stop_signal.clone();

        set.spawn(async move {
            if let Ok(mut client) = TestClient::connect_with_id(addr, client_id).await {
                metrics.record_connection();

                let mut seq = 0;
                while stop.load(Ordering::Relaxed) == 0 {
                    let msg = format!("sustained:{}:{}", client_id, seq);
                    if client.send_text(&msg).await.is_ok() {
                        metrics.record_message_sent();
                        if client.recv_text().await.is_ok() {
                            metrics.record_message_received();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                    seq += 1;

                    if seq > 1000 {
                        break;
                    }
                }

                let _ = client.close().await;
                metrics.record_disconnect();
            } else {
                metrics.record_connection_failed();
            }

            drop(permit);
        });
    }

    tokio::time::sleep(Duration::from_secs(duration_secs)).await;
    stop_signal.store(1, Ordering::Relaxed);

    while set.join_next().await.is_some() {}

    let elapsed = start.elapsed();
    metrics.report();

    let sent = metrics.messages_sent();
    let received = metrics.messages_received();
    let throughput = received as f64 / elapsed.as_secs_f64();

    println!("Sustained throughput: {:.2} msg/sec", throughput);
    println!("Messages sent: {}, received: {}", sent, received);

    server.shutdown().await;
}
