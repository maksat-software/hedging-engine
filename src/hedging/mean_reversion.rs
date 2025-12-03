use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Mean reversion hedging strategy
///
/// Based on an Ornstein-Uhlenbeck process:
/// dS = κ(μ - S)dt + σdW
///
/// Where:
/// - κ (kappa) = speed of mean reversion
/// - μ (mu) = long-term mean
/// - σ (sigma) = volatility
pub struct MeanReversionHedge {
    /// Historical prices for mean calculation
    price_history: RwLock<VecDeque<f64>>,

    /// Cached mean price (fixed-point: price * 10000)
    mean_price: AtomicI64,

    /// Cached standard deviation (fixed-point: std * 10000)
    std_dev: AtomicI64,

    /// Kappa (mean reversion speed) (fixed-point: kappa * 10000)
    kappa: AtomicI64,

    /// Last calculation timestamp
    last_calc_ns: AtomicU64,

    /// Z-score threshold for hedging
    z_threshold: f64,

    /// Window size
    window_size: usize,

    /// Hedge strength factor (0.0 - 1.0)
    hedge_strength: f64,
}

impl MeanReversionHedge {
    /// Create new mean reversion strategy
    pub fn new(window_size: usize, kappa: f64, z_threshold: f64, hedge_strength: f64) -> Self {
        Self {
            price_history: RwLock::new(VecDeque::with_capacity(window_size)),
            mean_price: AtomicI64::new(0),
            std_dev: AtomicI64::new(0),
            kappa: AtomicI64::new((kappa * 10000.0) as i64),
            last_calc_ns: AtomicU64::new(0),
            z_threshold,
            window_size,
            hedge_strength,
        }
    }

    /// Add price observation
    pub fn add_price(&self, price: f64) {
        let mut history = self.price_history.write();
        history.push_back(price);

        if history.len() > self.window_size {
            history.pop_front();
        }
    }

    /// Calculate statistics (mean, std dev)
    ///
    /// Runs in background thread (cold path)
    pub fn calculate_statistics(&self) -> Option<(f64, f64)> {
        let history = self.price_history.read();

        if history.len() < 30 {
            return None;
        }

        // Calculate mean
        let mean: f64 = history.iter().sum::<f64>() / history.len() as f64;

        // Calculate standard deviation
        let variance: f64 =
            history.iter().map(|&p| (p - mean).powi(2)).sum::<f64>() / (history.len() - 1) as f64;
        let std_dev = variance.sqrt();

        // Update cached values
        self.mean_price
            .store((mean * 10000.0) as i64, Ordering::Release);
        self.std_dev
            .store((std_dev * 10000.0) as i64, Ordering::Release);
        self.last_calc_ns
            .store(crate::utils::get_timestamp_ns(), Ordering::Release);

        Some((mean, std_dev))
    }

    /// Calculate z-score for current price
    ///
    /// # Performance
    /// ~50ns (just arithmetic)
    #[inline(always)]
    pub fn calculate_z_score(&self, current_price: f64) -> f64 {
        let mean = (self.mean_price.load(Ordering::Acquire) as f64) / 10000.0;
        let std = (self.std_dev.load(Ordering::Acquire) as f64) / 10000.0;

        if std == 0.0 {
            return 0.0;
        }

        (current_price - mean) / std
    }

    /// Check if hedge adjustment is needed
    ///
    /// Returns adjusted hedge strength based on z-score
    pub fn should_adjust_hedge(&self, current_price: f64) -> Option<f64> {
        let z_score = self.calculate_z_score(current_price);

        if z_score.abs() > self.z_threshold {
            // Strong deviation from mean
            // Reduce hedge strength (expect reversion)
            let adjustment = match z_score.abs() {
                z if z > 3.0 => 0.3, // Very strong deviation, minimal hedge
                z if z > 2.5 => 0.5, // Strong deviation, partial hedge
                z if z > 2.0 => 0.7, // Moderate deviation, most of hedge
                _ => 1.0,            // Normal hedge
            };

            Some(adjustment * self.hedge_strength)
        } else {
            // Price in normal range, full hedge
            Some(1.0)
        }
    }

    /// Get half-life of mean reversion (in days)
    pub fn half_life_days(&self) -> f64 {
        let kappa = (self.kappa.load(Ordering::Acquire) as f64) / 10000.0;
        if kappa == 0.0 {
            return f64::INFINITY;
        }
        (2.0_f64.ln()) / kappa
    }

    /// Get current statistics
    pub fn get_statistics(&self) -> MeanReversionStats {
        MeanReversionStats {
            mean_price: (self.mean_price.load(Ordering::Acquire) as f64) / 10000.0,
            std_dev: (self.std_dev.load(Ordering::Acquire) as f64) / 10000.0,
            kappa: (self.kappa.load(Ordering::Acquire) as f64) / 10000.0,
            half_life_days: self.half_life_days(),
            observations: self.price_history.read().len(),
        }
    }
}

/// Mean reversion statistics
#[derive(Debug, Clone, Default)]
pub struct MeanReversionStats {
    pub mean_price: f64,
    pub std_dev: f64,
    pub kappa: f64,
    pub half_life_days: f64,
    pub observations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean_reversion_basic() {
        let strategy = MeanReversionHedge::new(100, 0.20, 2.0, 0.7);

        // Add prices around mean of 45.0
        for i in 0..50 {
            strategy.add_price(45.0 + (i % 10) as f64 * 0.5);
        }

        let stats = strategy.calculate_statistics();
        assert!(stats.is_some());

        let (mean, std) = stats.unwrap();
        assert!((mean - 47.0).abs() < 3.0);
        assert!(std > 0.0);
    }

    #[test]
    fn test_z_score_calculation() {
        let strategy = MeanReversionHedge::new(100, 0.20, 2.0, 0.7);

        // Add prices with mean 45.0
        for _ in 0..50 {
            strategy.add_price(45.0);
        }

        strategy.calculate_statistics();

        // Price at mean should have z-score ~0
        let z = strategy.calculate_z_score(45.0);
        assert!(z.abs() < 0.1);
    }

    #[test]
    fn test_hedge_adjustment() {
        let strategy = MeanReversionHedge::new(100, 0.20, 2.0, 1.0);

        // Add prices with mean 45.0, std 2.0
        for i in 0..50 {
            strategy.add_price(43.0 + (i % 5) as f64);
        }

        strategy.calculate_statistics();

        // Price far from mean (high z-score) should reduce hedge
        let adjustment = strategy.should_adjust_hedge(55.0);
        assert!(adjustment.is_some());

        let adj = adjustment.unwrap();
        assert!(adj < 1.0); // Should reduce hedge
    }

    #[test]
    fn test_half_life() {
        let strategy = MeanReversionHedge::new(100, 0.20, 2.0, 1.0);

        // κ = 0.20 → half-life = ln(2)/0.20 ≈ 3.47 days
        let half_life = strategy.half_life_days();
        assert!((half_life - 3.47).abs() < 0.1);
    }
}
