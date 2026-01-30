//! Metrics collection for stress testing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Thread-safe metrics collector using atomic counters.
#[derive(Debug, Default)]
pub struct Metrics {
    connections_total: AtomicUsize,
    connections_active: AtomicUsize,
    connections_failed: AtomicUsize,
    messages_sent: AtomicUsize,
    messages_received: AtomicUsize,
    errors: AtomicUsize,
}

impl Metrics {
    /// Create a new metrics collector.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record a new connection attempt.
    pub fn record_connection(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a connection closed.
    pub fn record_disconnect(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a failed connection attempt.
    pub fn record_connection_failed(&self) {
        self.connections_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message sent.
    pub fn record_message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message received.
    pub fn record_message_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error.
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total connections attempted.
    pub fn connections_total(&self) -> usize {
        self.connections_total.load(Ordering::Relaxed)
    }

    /// Get current active connections.
    pub fn connections_active(&self) -> usize {
        self.connections_active.load(Ordering::Relaxed)
    }

    /// Get failed connection count.
    pub fn connections_failed(&self) -> usize {
        self.connections_failed.load(Ordering::Relaxed)
    }

    /// Get total messages sent.
    pub fn messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get total messages received.
    pub fn messages_received(&self) -> usize {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Get error count.
    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    /// Print a summary report.
    pub fn report(&self) {
        println!("\n=== Metrics Report ===");
        println!("Connections total:  {}", self.connections_total());
        println!("Connections active: {}", self.connections_active());
        println!("Connections failed: {}", self.connections_failed());
        println!("Messages sent:      {}", self.messages_sent());
        println!("Messages received:  {}", self.messages_received());
        println!("Errors:             {}", self.errors());
        println!("======================\n");
    }
}

/// Thread-safe latency collector for percentile calculations.
#[derive(Debug, Default)]
pub struct Latencies {
    samples: Mutex<Vec<Duration>>,
}

impl Latencies {
    /// Create a new latency collector.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record a latency sample.
    pub fn record(&self, latency: Duration) {
        if let Ok(mut samples) = self.samples.lock() {
            samples.push(latency);
        }
    }

    /// Get the number of samples.
    pub fn count(&self) -> usize {
        self.samples.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// Calculate a percentile (0.0 to 1.0).
    pub fn percentile(&self, p: f64) -> Option<Duration> {
        let samples = self.samples.lock().ok()?;
        if samples.is_empty() {
            return None;
        }

        let mut sorted: Vec<_> = samples.clone();
        sorted.sort();

        let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        Some(sorted[index.min(sorted.len() - 1)])
    }

    /// Get p50 (median) latency.
    pub fn p50(&self) -> Option<Duration> {
        self.percentile(0.50)
    }

    /// Get p95 latency.
    pub fn p95(&self) -> Option<Duration> {
        self.percentile(0.95)
    }

    /// Get p99 latency.
    pub fn p99(&self) -> Option<Duration> {
        self.percentile(0.99)
    }

    /// Get minimum latency.
    pub fn min(&self) -> Option<Duration> {
        self.samples.lock().ok()?.iter().copied().min()
    }

    /// Get maximum latency.
    pub fn max(&self) -> Option<Duration> {
        self.samples.lock().ok()?.iter().copied().max()
    }

    /// Get mean latency.
    pub fn mean(&self) -> Option<Duration> {
        let samples = self.samples.lock().ok()?;
        if samples.is_empty() {
            return None;
        }

        let total: Duration = samples.iter().sum();
        Some(total / samples.len() as u32)
    }

    /// Print a latency report.
    pub fn report(&self) {
        println!("\n=== Latency Report ===");
        println!("Samples: {}", self.count());
        if let Some(min) = self.min() {
            println!("Min:     {:?}", min);
        }
        if let Some(p50) = self.p50() {
            println!("P50:     {:?}", p50);
        }
        if let Some(p95) = self.p95() {
            println!("P95:     {:?}", p95);
        }
        if let Some(p99) = self.p99() {
            println!("P99:     {:?}", p99);
        }
        if let Some(max) = self.max() {
            println!("Max:     {:?}", max);
        }
        if let Some(mean) = self.mean() {
            println!("Mean:    {:?}", mean);
        }
        println!("======================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_concurrent_increment() {
        let metrics = Metrics::new();

        std::thread::scope(|s| {
            for _ in 0..100 {
                let m = &metrics;
                s.spawn(move || {
                    m.record_message_sent();
                });
            }
        });

        assert_eq!(metrics.messages_sent(), 100);
    }

    #[test]
    fn test_metrics_connection_lifecycle() {
        let metrics = Metrics::new();

        metrics.record_connection();
        assert_eq!(metrics.connections_total(), 1);
        assert_eq!(metrics.connections_active(), 1);

        metrics.record_disconnect();
        assert_eq!(metrics.connections_active(), 0);
    }

    #[test]
    fn test_latencies_percentiles() {
        let latencies = Latencies::new();

        // Add 100 samples: 1ms, 2ms, ..., 100ms
        for i in 1..=100 {
            latencies.record(Duration::from_millis(i));
        }

        assert_eq!(latencies.count(), 100);
        assert_eq!(latencies.min(), Some(Duration::from_millis(1)));
        assert_eq!(latencies.max(), Some(Duration::from_millis(100)));

        // p50 should be around 50ms
        let p50 = latencies.p50().unwrap();
        assert!(p50 >= Duration::from_millis(49) && p50 <= Duration::from_millis(51));

        // p95 should be around 95ms
        let p95 = latencies.p95().unwrap();
        assert!(p95 >= Duration::from_millis(94) && p95 <= Duration::from_millis(96));

        // p99 should be around 99ms
        let p99 = latencies.p99().unwrap();
        assert!(p99 >= Duration::from_millis(98) && p99 <= Duration::from_millis(100));
    }

    #[test]
    fn test_latencies_empty() {
        let latencies = Latencies::new();
        assert_eq!(latencies.count(), 0);
        assert_eq!(latencies.p50(), None);
        assert_eq!(latencies.min(), None);
    }
}
