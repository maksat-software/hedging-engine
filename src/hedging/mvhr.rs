use parking_lot::lock_api::{RwLockReadGuard, RwLockWriteGuard};
use parking_lot::{RawRwLock, RwLock};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// MVHR (Minimum Variance Hedge Ratio) strategy
///
/// Calculates optimal hedge ratio using historical correlation
pub struct MVHRStrategy {
    /// Historical spot prices
    spot_prices: RwLock<VecDeque<f64>>,

    /// Historical futures prices
    futures_prices: RwLock<VecDeque<f64>>,

    /// Cached optimal ratio (fixed-point: ratio * 10000)
    cached_ratio: AtomicI64,

    /// Last calculation timestamp (nanoseconds)
    last_calc_ns: AtomicU64,

    /// Window size (number of observations)
    window_size: usize,

    /// Recalculation interval (nanoseconds)
    recalc_interval_ns: u64,
}

impl MVHRStrategy {
    /// Create new MVHR strategy
    pub fn new(window_hours: usize, recalc_hours: usize) -> Self {
        Self {
            spot_prices: RwLock::new(VecDeque::with_capacity(window_hours)),
            futures_prices: RwLock::new(VecDeque::with_capacity(window_hours)),
            cached_ratio: AtomicI64::new(10000), // Default 1.0
            last_calc_ns: AtomicU64::new(0),
            window_size: window_hours,
            recalc_interval_ns: (recalc_hours as u64) * 3600 * 1_000_000_000,
        }
    }

    /// Add new price observation
    pub fn add_observation(&self, spot_price: f64, futures_price: f64) {
        let mut spot_prices: RwLockWriteGuard<RawRwLock, VecDeque<f64>> = self.spot_prices.write();
        let mut futures_prices: RwLockWriteGuard<RawRwLock, VecDeque<f64>> =
            self.futures_prices.write();

        // Add new prices
        spot_prices.push_back(spot_price);
        futures_prices.push_back(futures_price);

        // Maintain window size
        if spot_prices.len() > self.window_size {
            spot_prices.pop_front();
            futures_prices.pop_front();
        }
    }

    /// Calculate optimal hedge ratio
    ///
    /// h* = Cov(ΔS, ΔF) / Var(ΔF)
    ///
    /// Requires at least 3 observations (to get 2 returns for variance calculation)
    pub fn calculate_optimal_ratio(&self) -> Option<f64> {
        let spot_prices = self.spot_prices.read();
        let futures_prices = self.futures_prices.read();

        // Need at least 3 observations to calculate meaningful statistics
        // (3 prices → 2 returns → can calculate variance)
        if spot_prices.len() < 3 {
            return None;
        }

        // Calculate returns
        let mut spot_returns = Vec::with_capacity(spot_prices.len() - 1);
        let mut futures_returns = Vec::with_capacity(futures_prices.len() - 1);

        for i in 1..spot_prices.len() {
            let spot_ret = (spot_prices[i] - spot_prices[i - 1]) / spot_prices[i - 1];
            let futures_ret = (futures_prices[i] - futures_prices[i - 1]) / futures_prices[i - 1];

            spot_returns.push(spot_ret);
            futures_returns.push(futures_ret);
        }

        let n = spot_returns.len();

        // Calculate means
        let spot_mean: f64 = spot_returns.iter().sum::<f64>() / n as f64;
        let futures_mean: f64 = futures_returns.iter().sum::<f64>() / n as f64;

        // Calculate covariance and variance
        let mut covariance = 0.0;
        let mut variance = 0.0;

        for i in 0..n {
            let spot_diff = spot_returns[i] - spot_mean;
            let futures_diff = futures_returns[i] - futures_mean;

            covariance += spot_diff * futures_diff;
            variance += futures_diff * futures_diff;
        }

        covariance /= (n - 1) as f64;
        variance /= (n - 1) as f64;

        // Avoid division by zero
        if variance.abs() < 1e-10 {
            return None;
        }

        let ratio = covariance / variance;

        // Sanity check: ratio should be reasonable (-5 to +5)
        // If outside this range, likely numerical issues
        if ratio.abs() > 5.0 {
            return None;
        }

        // Update cached value
        self.cached_ratio
            .store((ratio * 10000.0) as i64, Ordering::Release);
        self.last_calc_ns
            .store(crate::utils::get_timestamp_ns(), Ordering::Release);

        Some(ratio)
    }

    /// Get cached hedge ratio (fast)
    #[inline(always)]
    pub fn get_hedge_ratio(&self) -> f64 {
        (self.cached_ratio.load(Ordering::Acquire) as f64) / 10000.0
    }

    /// Check if recalculation is needed
    pub fn needs_recalculation(&self) -> bool {
        let last_calc: u64 = self.last_calc_ns.load(Ordering::Relaxed);
        let now: u64 = crate::utils::get_timestamp_ns();

        now - last_calc > self.recalc_interval_ns
    }

    /// Get statistics
    pub fn get_statistics(&self) -> Option<MVHRStatistics> {
        let spot_prices: RwLockReadGuard<RawRwLock, VecDeque<f64>> = self.spot_prices.read();
        let futures_prices: RwLockReadGuard<RawRwLock, VecDeque<f64>> = self.futures_prices.read();

        // Need at least 3 observations
        if spot_prices.len() < 3 {
            return None;
        }

        // Calculate returns
        let mut spot_returns: Vec<f64> = Vec::new();
        let mut futures_returns: Vec<f64> = Vec::new();

        for i in 1..spot_prices.len() {
            spot_returns.push((spot_prices[i] - spot_prices[i - 1]) / spot_prices[i - 1]);
            futures_returns
                .push((futures_prices[i] - futures_prices[i - 1]) / futures_prices[i - 1]);
        }

        let n = spot_returns.len();

        // Calculate statistics
        let spot_mean: f64 = spot_returns.iter().sum::<f64>() / n as f64;
        let futures_mean: f64 = futures_returns.iter().sum::<f64>() / n as f64;

        let spot_var: f64 = spot_returns
            .iter()
            .map(|&r| (r - spot_mean).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;

        let futures_var: f64 = futures_returns
            .iter()
            .map(|&r| (r - futures_mean).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;

        let covariance: f64 = spot_returns
            .iter()
            .zip(futures_returns.iter())
            .map(|(&s, &f)| (s - spot_mean) * (f - futures_mean))
            .sum::<f64>()
            / (n - 1) as f64;

        let correlation = if spot_var > 0.0 && futures_var > 0.0 {
            covariance / (spot_var.sqrt() * futures_var.sqrt())
        } else {
            0.0
        };

        Some(MVHRStatistics {
            hedge_ratio: self.get_hedge_ratio(),
            correlation,
            observations: spot_prices.len(),
            spot_volatility: spot_var.sqrt(),
            futures_volatility: futures_var.sqrt(),
        })
    }
}

/// MVHR statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct MVHRStatistics {
    pub hedge_ratio: f64,
    pub correlation: f64,
    pub observations: usize,
    pub spot_volatility: f64,
    pub futures_volatility: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mvhr_calculation() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        // Add correlated observations
        for i in 0..50 {
            let spot = 45.0 + i as f64 * 0.1;
            let futures = 50.0 + i as f64 * 0.12;
            mvhr.add_observation(spot, futures);
        }

        let ratio: Option<f64> = mvhr.calculate_optimal_ratio();
        assert!(
            ratio.is_some(),
            "Should calculate ratio with 50+ observations"
        );

        let ratio: f64 = ratio.unwrap();

        // Ratio should be reasonable
        assert!(
            ratio > 0.3 && ratio < 1.5,
            "Ratio should be between 0.3-1.5, got {}",
            ratio
        );

        // Check cached value matches
        let cached: f64 = mvhr.get_hedge_ratio();
        assert!(
            (cached - ratio).abs() < 0.01,
            "Cached ratio should match calculated, got {} vs {}",
            cached,
            ratio
        );

        // Verify statistics
        let stats: Option<MVHRStatistics> = mvhr.get_statistics();
        assert!(stats.is_some(), "Should have statistics");

        let stats: MVHRStatistics = stats.unwrap();
        assert_eq!(stats.observations, 50);
        assert!(
            stats.correlation > 0.9,
            "Correlation should be high (>0.9), got {}",
            stats.correlation
        );
        assert!(stats.spot_volatility > 0.0);
        assert!(stats.futures_volatility > 0.0);
    }

    #[test]
    fn test_mvhr_window_size() {
        let mvhr = MVHRStrategy::new(10, 1);

        for i in 0..20 {
            let spot = 45.0 + i as f64 * 0.1;
            let futures = 50.0 + i as f64 * 0.12;
            mvhr.add_observation(spot, futures);
        }

        let spot_prices = mvhr.spot_prices.read();
        let futures_prices = mvhr.futures_prices.read();

        assert_eq!(spot_prices.len(), 10);
        assert_eq!(futures_prices.len(), 10);

        let last_spot = spot_prices.back().unwrap();
        let expected_last_spot = 45.0 + 19.0 * 0.1;
        assert!((last_spot - expected_last_spot).abs() < 0.01);
    }

    #[test]
    fn test_mvhr_insufficient_data() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        // No data
        assert!(mvhr.calculate_optimal_ratio().is_none());

        mvhr.add_observation(45.0, 50.0);
        assert!(mvhr.calculate_optimal_ratio().is_none());

        mvhr.add_observation(46.0, 51.0);
        assert!(mvhr.calculate_optimal_ratio().is_none());

        let base_spot: f64 = 45.0;
        let base_futures: f64 = 50.0;

        for i in 0..10 {
            let spot: f64 = base_spot + (i as f64) * 0.5 + (i as f64 % 3f64) * 0.3;
            let futures: f64 = base_futures + (i as f64) * 0.6 + (i as f64 % 3f64) * 0.35;
            mvhr.add_observation(spot, futures);
        }

        let ratio: Option<f64> = mvhr.calculate_optimal_ratio();
        assert!(
            ratio.is_some(),
            "Should calculate ratio with sufficient diverse data"
        );
    }

    #[test]
    fn test_mvhr_negative_correlation() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        // Create PERFECT negative correlation
        // When spot goes up 10%, futures goes down 10%
        let mut spot: f64 = 50.0;
        let mut futures: f64 = 50.0;

        for i in 0..50 {
            mvhr.add_observation(spot, futures);

            // Alternate up and down movements
            if i % 2 == 0 {
                spot *= 1.05; // +5%
                futures *= 0.95; // -5%
            } else {
                spot *= 1.03; // +3%
                futures *= 0.97; // -3%
            }
        }

        let ratio: Option<f64> = mvhr.calculate_optimal_ratio();
        assert!(ratio.is_some(), "Should calculate ratio");

        let ratio: f64 = ratio.unwrap();

        // Negative correlation MUST give negative ratio
        assert!(
            ratio < 0.0,
            "Negative correlation must give negative ratio, got {}",
            ratio
        );

        let stats: MVHRStatistics = mvhr.get_statistics().unwrap();
        assert!(
            stats.correlation < 0.0,
            "Correlation must be negative, got {}",
            stats.correlation
        );
    }

    #[test]
    fn test_mvhr_zero_variance() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        for i in 0..50 {
            let spot = 45.0 + i as f64 * 0.1;
            let futures = 50.0;
            mvhr.add_observation(spot, futures);
        }

        let ratio: Option<f64> = mvhr.calculate_optimal_ratio();
        assert!(ratio.is_none());
    }

    #[test]
    fn test_mvhr_recalculation_interval() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        for i in 0..50 {
            mvhr.add_observation(45.0 + i as f64 * 0.1, 50.0 + i as f64 * 0.12);
        }

        let ratio1: Option<f64> = mvhr.calculate_optimal_ratio();
        assert!(ratio1.is_some());
        assert!(!mvhr.needs_recalculation());
    }

    #[test]
    fn test_mvhr_statistics() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        assert!(mvhr.get_statistics().is_none());

        mvhr.add_observation(45.0, 50.0);
        assert!(mvhr.get_statistics().is_none());

        for i in 1..50 {
            mvhr.add_observation(45.0 + i as f64 * 0.1, 50.0 + i as f64 * 0.12);
        }

        mvhr.calculate_optimal_ratio();

        let stats: Option<MVHRStatistics> = mvhr.get_statistics();
        assert!(stats.is_some());

        let stats = stats.unwrap();
        assert!(stats.hedge_ratio > 0.0 && stats.hedge_ratio < 2.0);
        assert!(stats.correlation >= -1.0 && stats.correlation <= 1.0);
        assert_eq!(stats.observations, 50);
        assert!(stats.spot_volatility > 0.0);
        assert!(stats.futures_volatility > 0.0);
    }

    #[test]
    fn test_mvhr_perfect_correlation() {
        let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

        for i in 0..50 {
            let spot = 45.0 + i as f64 * 0.5;
            let futures = 50.0 + i as f64 * 0.6;
            mvhr.add_observation(spot, futures);
        }

        mvhr.calculate_optimal_ratio();
        let stats: MVHRStatistics = mvhr.get_statistics().unwrap();

        assert!(
            stats.correlation > 0.95,
            "Expected high correlation, got {}",
            stats.correlation
        );
    }
}
