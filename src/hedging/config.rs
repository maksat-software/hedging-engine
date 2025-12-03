use crate::market_data::Side;
use serde::{Deserialize, Serialize};

/// Hedge urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    /// Normal priority
    Normal,
    /// High priority (large delta)
    High,
    /// Emergency (risk limit breach)
    Emergency,
}

/// Hedge recommendation from strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeRecommendation {
    /// Quantity to hedge (MWh or lots)
    pub quantity: f64,

    /// Target price (€/MWh)
    pub price: f64,

    /// Side (buy or sell)
    pub side: Side,

    /// Urgency level
    pub urgency: Urgency,

    /// Reason for hedge
    pub reason: String,

    /// Timestamp when recommendation made
    pub timestamp_ns: u64,
}

impl HedgeRecommendation {
    /// Create a new hedge recommendation
    pub fn new(
        quantity: f64,
        price: f64,
        side: Side,
        urgency: Urgency,
        reason: String,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            quantity,
            price,
            side,
            urgency,
            reason,
            timestamp_ns,
        }
    }
}

/// Hedge engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeConfig {
    /// Initial position (MWh, negative = short)
    pub initial_position: f64,

    /// Default hedge ratio (e.g., 1.125)
    pub default_hedge_ratio: f64,

    /// Rehedge threshold (basis points)
    /// e.g., 500 = rehedge when delta > 5%
    pub rehedge_threshold_bps: i64,

    /// Maximum position size (MWh)
    pub max_position: f64,

    /// Enable MVHR calculation
    pub enable_mvhr: bool,

    /// Enable mean reversion
    pub enable_mean_reversion: bool,

    /// Look back window for statistics (hours)
    pub statistics_window_hours: usize,
}

impl Default for HedgeConfig {
    fn default() -> Self {
        Self {
            initial_position: 0.0,
            default_hedge_ratio: 1.0,
            rehedge_threshold_bps: 500,
            max_position: 100_000.0,
            enable_mvhr: true,
            enable_mean_reversion: false,
            statistics_window_hours: 720, // 30 days
        }
    }
}

impl HedgeConfig {
    /// Create a simple configuration
    pub fn simple(position: f64, ratio: f64) -> Self {
        Self {
            initial_position: position,
            default_hedge_ratio: ratio,
            ..Default::default()
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> crate::Result<()> {
        if self.default_hedge_ratio <= 0.0 {
            return Err(crate::Error::Config(
                "Hedge ratio must be positive".to_string(),
            ));
        }

        if self.rehedge_threshold_bps < 0 {
            return Err(crate::Error::Config(
                "Rehedge threshold must be non-negative".to_string(),
            ));
        }

        if self.max_position <= 0.0 {
            return Err(crate::Error::Config(
                "Max position must be positive".to_string(),
            ));
        }

        Ok(())
    }
}
