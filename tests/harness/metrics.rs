//! Metrics collection for concurrency and stress testing.
//!
//! Provides atomic counters for throughput and latency measurements.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Thread-safe metrics collector for connection and message statistics.
///
/// Uses atomic counters for low-overhead recording on hot paths.
/// Cloneable via internal `Arc` - all clones share the same counters.
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    connections_total: AtomicUsize,
    connections_failed: AtomicUsize,
    disconnections: AtomicUsize,
    messages_sent: AtomicUsize,
    messages_received: AtomicUsize,
    errors: AtomicUsize,
}

impl Metrics {
    /// Create a new metrics collector with all counters at zero.
    #[must_use]
    pub fn new() -> Self {
        Metrics {
            inner: Arc::new(MetricsInner {
                connections_total: AtomicUsize::new(0),
                connections_failed: AtomicUsize::new(0),
                disconnections: AtomicUsize::new(0),
                messages_sent: AtomicUsize::new(0),
                messages_received: AtomicUsize::new(0),
                errors: AtomicUsize::new(0),
            }),
        }
    }

    /// Record a successful connection.
    #[inline]
    pub fn record_connection(&self) {
        self.inner.connections_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a disconnection.
    #[inline]
    pub fn record_disconnect(&self) {
        self.inner.disconnections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed connection attempt.
    #[inline]
    pub fn record_connection_failed(&self) {
        self.inner
            .connections_failed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message sent.
    #[inline]
    pub fn record_message_sent(&self) {
        self.inner.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message received.
    #[inline]
    pub fn record_message_received(&self) {
        self.inner.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error.
    #[inline]
    pub fn record_error(&self) {
        self.inner.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total successful connections.
    #[must_use]
    pub fn connections_total(&self) -> usize {
        self.inner.connections_total.load(Ordering::Relaxed)
    }

    /// Get total failed connections.
    #[must_use]
    pub fn connections_failed(&self) -> usize {
        self.inner.connections_failed.load(Ordering::Relaxed)
    }

    /// Get total messages sent.
    #[must_use]
    pub fn messages_sent(&self) -> usize {
        self.inner.messages_sent.load(Ordering::Relaxed)
    }

    /// Get total messages received.
    #[must_use]
    pub fn messages_received(&self) -> usize {
        self.inner.messages_received.load(Ordering::Relaxed)
    }

    /// Get total errors.
    #[must_use]
    pub fn errors(&self) -> usize {
        self.inner.errors.load(Ordering::Relaxed)
    }

    /// Get total disconnections.
    #[must_use]
    pub fn disconnections(&self) -> usize {
        self.inner.disconnections.load(Ordering::Relaxed)
    }

    /// Print a formatted summary of all metrics.
    pub fn report(&self) {
        println!("=== Metrics Report ===");
        println!(
            "Connections: {} total, {} failed",
            self.connections_total(),
            self.connections_failed()
        );
        println!("Disconnections: {}", self.disconnections());
        println!(
            "Messages: {} sent, {} received",
            self.messages_sent(),
            self.messages_received()
        );
        println!("Errors: {}", self.errors());
        println!("======================");
    }
}

impl Clone for Metrics {
    fn clone(&self) -> Self {
        Metrics {
            inner: self.inner.clone(),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe latency collector for percentile calculations.
///
/// Collects duration samples and provides percentile statistics.
/// Cloneable via internal `Arc` - all clones share the same data.
pub struct Latencies {
    inner: Arc<LatenciesInner>,
}

struct LatenciesInner {
    samples: Mutex<Vec<Duration>>,
}

impl Latencies {
    /// Create a new latency collector.
    #[must_use]
    pub fn new() -> Self {
        Latencies {
            inner: Arc::new(LatenciesInner {
                samples: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Record a latency sample.
    pub fn record(&self, duration: Duration) {
        if let Ok(mut samples) = self.inner.samples.lock() {
            samples.push(duration);
        }
    }

    /// Calculate a percentile from the collected samples.
    ///
    /// Returns `None` if no samples have been collected.
    fn percentile(&self, p: f64) -> Option<Duration> {
        let samples = self.inner.samples.lock().ok()?;
        if samples.is_empty() {
            return None;
        }

        let mut sorted: Vec<Duration> = samples.clone();
        sorted.sort();

        let index = ((sorted.len() as f64) * p).ceil() as usize;
        let index = index.saturating_sub(1).min(sorted.len() - 1);

        Some(sorted[index])
    }

    /// Get the 50th percentile (median) latency.
    #[must_use]
    pub fn p50(&self) -> Option<Duration> {
        self.percentile(0.50)
    }

    /// Get the 95th percentile latency.
    #[must_use]
    pub fn p95(&self) -> Option<Duration> {
        self.percentile(0.95)
    }

    /// Get the 99th percentile latency.
    #[must_use]
    pub fn p99(&self) -> Option<Duration> {
        self.percentile(0.99)
    }

    /// Get the number of samples collected.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inner.samples.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// Print a formatted summary of latency percentiles.
    pub fn report(&self) {
        println!("=== Latency Report ===");
        println!("Samples: {}", self.count());
        if let Some(p50) = self.p50() {
            println!("p50: {:?}", p50);
        }
        if let Some(p95) = self.p95() {
            println!("p95: {:?}", p95);
        }
        if let Some(p99) = self.p99() {
            println!("p99: {:?}", p99);
        }
        println!("======================");
    }
}

impl Clone for Latencies {
    fn clone(&self) -> Self {
        Latencies {
            inner: self.inner.clone(),
        }
    }
}

impl Default for Latencies {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_counting() {
        let metrics = Metrics::new();

        metrics.record_connection();
        metrics.record_connection();
        metrics.record_connection_failed();
        metrics.record_message_sent();
        metrics.record_message_received();
        metrics.record_message_received();
        metrics.record_error();
        metrics.record_disconnect();

        assert_eq!(metrics.connections_total(), 2);
        assert_eq!(metrics.connections_failed(), 1);
        assert_eq!(metrics.messages_sent(), 1);
        assert_eq!(metrics.messages_received(), 2);
        assert_eq!(metrics.errors(), 1);
        assert_eq!(metrics.disconnections(), 1);
    }

    #[test]
    fn test_metrics_clone_shares_state() {
        let metrics1 = Metrics::new();
        let metrics2 = metrics1.clone();

        metrics1.record_connection();
        metrics2.record_connection();

        assert_eq!(metrics1.connections_total(), 2);
        assert_eq!(metrics2.connections_total(), 2);
    }

    #[test]
    fn test_latencies_percentiles() {
        let latencies = Latencies::new();

        // Add 100 samples: 1ms, 2ms, ..., 100ms
        for i in 1..=100 {
            latencies.record(Duration::from_millis(i));
        }

        assert_eq!(latencies.count(), 100);

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

        assert!(latencies.p50().is_none());
        assert!(latencies.p95().is_none());
        assert!(latencies.p99().is_none());
        assert_eq!(latencies.count(), 0);
    }

    #[test]
    fn test_latencies_clone_shares_state() {
        let latencies1 = Latencies::new();
        let latencies2 = latencies1.clone();

        latencies1.record(Duration::from_millis(10));
        latencies2.record(Duration::from_millis(20));

        assert_eq!(latencies1.count(), 2);
        assert_eq!(latencies2.count(), 2);
    }
}
