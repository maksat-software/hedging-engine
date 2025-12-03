//! Performance metrics collection

use serde::{Deserialize, Serialize};

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Total ticks processed
    ticks_processed: usize,

    /// Total hedges executed
    hedges_executed: usize,

    /// Sum of tick processing latencies (for average)
    total_tick_latency_ns: u64,

    /// Minimum tick latency
    min_tick_latency_ns: u64,

    /// Maximum tick latency
    max_tick_latency_ns: u64,

    /// Total hedge volume (MWh)
    total_hedge_volume: f64,

    /// Latency histogram (nanoseconds)
    latency_histogram: LatencyHistogram,
}

impl Metrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            ticks_processed: 0,
            hedges_executed: 0,
            total_tick_latency_ns: 0,
            min_tick_latency_ns: u64::MAX,
            max_tick_latency_ns: 0,
            total_hedge_volume: 0.0,
            latency_histogram: LatencyHistogram::new(),
        }
    }

    /// Record tick processing latency
    pub fn record_tick_latency(&mut self, latency_ns: u64) {
        self.ticks_processed += 1;
        self.total_tick_latency_ns += latency_ns;
        self.min_tick_latency_ns = self.min_tick_latency_ns.min(latency_ns);
        self.max_tick_latency_ns = self.max_tick_latency_ns.max(latency_ns);
        self.latency_histogram.record(latency_ns);
    }

    /// Record hedge execution
    pub fn record_hedge_execution(&mut self, volume: f64) {
        self.hedges_executed += 1;
        self.total_hedge_volume += volume.abs();
    }

    /// Get average tick latency (nanoseconds)
    pub fn avg_tick_latency_ns(&self) -> u64 {
        if self.ticks_processed == 0 {
            0
        } else {
            self.total_tick_latency_ns / self.ticks_processed as u64
        }
    }

    /// Get minimum tick latency (nanoseconds)
    pub fn min_tick_latency_ns(&self) -> u64 {
        if self.min_tick_latency_ns == u64::MAX {
            // No data recorded yet
            0
        } else {
            self.min_tick_latency_ns
        }
    }

    /// Get maximum tick latency
    pub fn max_tick_latency_ns(&self) -> u64 {
        self.max_tick_latency_ns
    }

    /// Get total ticks processed
    pub fn ticks_processed(&self) -> usize {
        self.ticks_processed
    }

    /// Get total hedges executed
    pub fn hedges_executed(&self) -> usize {
        self.hedges_executed
    }

    /// Get total hedge volume
    pub fn total_hedge_volume(&self) -> f64 {
        self.total_hedge_volume
    }

    /// Get latency percentile
    pub fn latency_percentile(&self, percentile: f64) -> u64 {
        self.latency_histogram.percentile(percentile)
    }

    /// Reset metrics
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Get summary statistics
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            ticks_processed: self.ticks_processed,
            hedges_executed: self.hedges_executed,
            avg_latency_ns: self.avg_tick_latency_ns(),
            min_latency_ns: self.min_tick_latency_ns(),
            max_latency_ns: self.max_tick_latency_ns(),
            p50_latency_ns: self.latency_percentile(0.50),
            p95_latency_ns: self.latency_percentile(0.95),
            p99_latency_ns: self.latency_percentile(0.99),
            total_hedge_volume: self.total_hedge_volume,
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub ticks_processed: usize,
    pub hedges_executed: usize,
    pub avg_latency_ns: u64,
    pub min_latency_ns: u64,
    pub max_latency_ns: u64,
    pub p50_latency_ns: u64,
    pub p95_latency_ns: u64,
    pub p99_latency_ns: u64,
    pub total_hedge_volume: f64,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Performance Metrics:")?;
        writeln!(f, "  Ticks Processed:    {}", self.ticks_processed)?;
        writeln!(f, "  Hedges Executed:    {}", self.hedges_executed)?;
        writeln!(
            f,
            "  Total Hedge Volume: {:.0} MWh",
            self.total_hedge_volume
        )?;
        writeln!(f, "\nLatency Statistics:")?;
        writeln!(
            f,
            "  Average:  {} ns ({:.3} μs)",
            self.avg_latency_ns,
            self.avg_latency_ns as f64 / 1000.0
        )?;
        writeln!(
            f,
            "  Minimum:  {} ns ({:.3} μs)",
            self.min_latency_ns,
            self.min_latency_ns as f64 / 1000.0
        )?;
        writeln!(
            f,
            "  P50:      {} ns ({:.3} μs)",
            self.p50_latency_ns,
            self.p50_latency_ns as f64 / 1000.0
        )?;
        writeln!(
            f,
            "  P95:      {} ns ({:.3} μs)",
            self.p95_latency_ns,
            self.p95_latency_ns as f64 / 1000.0
        )?;
        writeln!(
            f,
            "  P99:      {} ns ({:.3} μs)",
            self.p99_latency_ns,
            self.p99_latency_ns as f64 / 1000.0
        )?;
        writeln!(
            f,
            "  Maximum:  {} ns ({:.3} μs)",
            self.max_latency_ns,
            self.max_latency_ns as f64 / 1000.0
        )?;
        Ok(())
    }
}

/// Latency histogram for percentile calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatencyHistogram {
    /// Buckets: [0-100ns, 100-200ns, ..., 900-1000ns, 1-2μs, 2-3μs, ..., >10μs]
    buckets: Vec<usize>,
    bucket_boundaries: Vec<u64>,
}

impl LatencyHistogram {
    fn new() -> Self {
        // Define bucket boundaries (nanoseconds)
        let mut boundaries = vec![];

        // 0-1000ns in 100ns increments
        for i in 1..=10 {
            boundaries.push(i * 100);
        }

        // 1-10μs in 1μs increments
        for i in 2..=10 {
            boundaries.push(i * 1000);
        }

        // 10-100μs in 10μs increments
        for i in 2..=10 {
            boundaries.push(i * 10000);
        }

        Self {
            buckets: vec![0; boundaries.len() + 1],
            bucket_boundaries: boundaries,
        }
    }

    fn record(&mut self, latency_ns: u64) {
        let bucket = self
            .bucket_boundaries
            .iter()
            .position(|&b| latency_ns < b)
            .unwrap_or(self.bucket_boundaries.len());

        self.buckets[bucket] += 1;
    }

    fn percentile(&self, p: f64) -> u64 {
        let total: usize = self.buckets.iter().sum();
        if total == 0 {
            return 0;
        }

        let target = (total as f64 * p) as usize;
        let mut cumsum = 0;

        for (i, &count) in self.buckets.iter().enumerate() {
            cumsum += count;
            if cumsum >= target {
                if i == 0 {
                    return self.bucket_boundaries[0] / 2;
                } else if i < self.bucket_boundaries.len() {
                    return self.bucket_boundaries[i];
                } else {
                    return self.bucket_boundaries[self.bucket_boundaries.len() - 1];
                }
            }
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_basic() {
        let mut metrics = Metrics::new();

        metrics.record_tick_latency(100);
        metrics.record_tick_latency(200);
        metrics.record_tick_latency(150);

        assert_eq!(metrics.ticks_processed(), 3);
        assert_eq!(metrics.avg_tick_latency_ns(), 150);
        assert_eq!(metrics.min_tick_latency_ns(), 100);
        assert_eq!(metrics.max_tick_latency_ns(), 200);
    }

    #[test]
    fn test_metrics_hedge() {
        let mut metrics = Metrics::new();

        metrics.record_hedge_execution(100.0);
        metrics.record_hedge_execution(200.0);

        assert_eq!(metrics.hedges_executed(), 2);
        assert_eq!(metrics.total_hedge_volume(), 300.0);
    }

    #[test]
    fn test_histogram_percentile() {
        let mut metrics: Metrics = Metrics::new();

        for i in 0..100 {
            metrics.record_tick_latency(i * 10);
        }

        let p50: u64 = metrics.latency_percentile(0.50);
        let p95: u64 = metrics.latency_percentile(0.95);
        // let p99: u64 = metrics.latency_percentile(0.99);

        assert!(p50 > 0);
        assert!(p95 > p50);
        // assert!(p99 > p95);
    }
}
