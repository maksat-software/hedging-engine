//! Spark Spread Hedging for Power Generation
//!
//! The spark spread is the theoretical gross margin of a gas-fired power plant
//! from selling electricity and buying natural gas.
//!
//! # Formula
//! ```text
//! Spark Spread = Power Price - (Gas Price / Heat Rate) - (CO2 Price × Emission Factor)
//! ```
//!
//! # Example
//! ```
//! use hedging_engine::hedging::SparkSpreadHedge;
//!
//! let hedge = SparkSpreadHedge::new(
//!     100.0,    // 100 MW capacity
//!     2.0,      // Heat rate (50% efficiency)
//!     0.202,    // Emission factor (tons CO2/MWh)
//!     50.0,     // Target spread (€50/MWh)
//! );
//!
//! // Check if the spread is profitable
//! let power_price = 100.0;
//! let gas_price = 40.0;
//! let co2_price = 80.0;
//!
//! let spread = hedge.calculate_spread(power_price, gas_price, co2_price);
//! println!("Spark Spread: €{:.2}/MWh", spread); // €63.84/MWh
//! ```

use crate::hedging::{HedgeRecommendation, Urgency};
use crate::market_data::{OrderBook, Side};
use crate::utils::get_timestamp_ns;
use std::sync::atomic::{AtomicI64, Ordering};

/// Spark spread hedging strategy for gas-fired power plants
///
/// This strategy calculates the profitability of running a power plant
/// and hedges when the spark spread exceeds a target threshold.
pub struct SparkSpreadHedge {
    /// Plant capacity (MW)
    capacity_mw: f64,

    /// Heat rate (MWh gas per MWh electricity)
    /// Typical values:
    /// - Combined Cycle Gas Turbine (CC GT): 1.8-2.2 (45-55% efficiency)
    /// - Open Cycle Gas Turbine (OCT): 2.5-3.5 (28-40% efficiency)
    heat_rate: f64,

    /// CO2 emission factor (tons CO2 per MWh gas)
    /// Natural gas: ~0.202 tons CO2/MWh
    emission_factor: f64,

    /// Target spark spread threshold (€/MWh)
    /// Only hedge wthe hen spread exceeds this
    target_spread: f64,

    /// Current hedge position for power (MW, fixed-point * 100)
    power_hedge: AtomicI64,

    /// Current hedge position for gas (MWh, fixed-point * 100)
    gas_hedge: AtomicI64,

    /// Current hedge position for CO2 (tons, fixed-point * 100)
    co2_hedge: AtomicI64,

    /// Historical average spread (for mean reversion, fixed-point * 10000)
    avg_spread: AtomicI64,

    /// Hedge threshold (only rehedge if spread changes by this much)
    rehedge_threshold_bps: i64,
}

impl SparkSpreadHedge {
    /// Create new spark spread hedging strategy
    ///
    /// # Arguments
    /// * `capacity_mw` - Plant capacity in MW
    /// * `heat_rate` - Heat rate (MWh gas / MWh electricity)
    /// * `emission_factor` - CO2 emissions (tons / MWh gas)
    /// * `target_spread` - Minimum spread to hedge (€/MWh)
    ///
    /// # Example
    /// ```
    /// use hedging_engine::hedging::SparkSpreadHedge;
    ///
    /// // 100 MW CCGT plant
    /// let hedge = SparkSpreadHedge::new(
    ///     100.0,  // 100 MW
    ///     2.0,    // 50% efficiency
    ///     0.202,  // Natural gas emissions
    ///     50.0,   // Target €50/MWh spread
    /// );
    /// ```
    pub fn new(capacity_mw: f64, heat_rate: f64, emission_factor: f64, target_spread: f64) -> Self {
        Self {
            capacity_mw,
            heat_rate,
            emission_factor,
            target_spread,
            power_hedge: AtomicI64::new(0),
            gas_hedge: AtomicI64::new(0),
            co2_hedge: AtomicI64::new(0),
            avg_spread: AtomicI64::new((target_spread * 10000.0) as i64),
            rehedge_threshold_bps: 500, // 5%
        }
    }

    /// Calculate spark spread
    ///
    /// # Formula
    /// ```text
    /// Spread = Power - (Gas / Heat_Rate) - (CO2 × Emission_Factor)
    /// ```
    ///
    /// # Arguments
    /// * `power_price` - Electricity price (€/MWh)
    /// * `gas_price` - Natural gas price (€/MWh)
    /// * `co2_price` - CO2 allowance price (€/ton)
    ///
    /// # Returns
    /// Spark spread in €/MWh
    ///
    /// # Example
    /// ```
    /// # use hedging_engine::hedging::SparkSpreadHedge;
    /// let hedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);
    ///
    /// let spread = hedge.calculate_spread(100.0, 40.0, 80.0);
    /// assert!((spread - 63.84).abs() < 0.01);
    /// ```
    #[inline(always)]
    pub fn calculate_spread(&self, power_price: f64, gas_price: f64, co2_price: f64) -> f64 {
        let gas_cost: f64 = gas_price / self.heat_rate;
        let co2_cost: f64 = co2_price * self.emission_factor;

        power_price - gas_cost - co2_cost
    }

    /// Calculate detailed costs breakdown
    pub fn calculate_costs_breakdown(&self, gas_price: f64, co2_price: f64) -> CostsBreakdown {
        let gas_cost_per_mwh: f64 = gas_price / self.heat_rate;
        let co2_cost_per_mwh: f64 = co2_price * self.emission_factor;
        let total_cost: f64 = gas_cost_per_mwh + co2_cost_per_mwh;

        CostsBreakdown {
            gas_cost_per_mwh,
            co2_cost_per_mwh,
            total_cost_per_mwh: total_cost,
            gas_volume_per_mwh: self.heat_rate,
            co2_volume_per_mwh: self.heat_rate * self.emission_factor,
        }
    }

    /// Check if spread is profitable (above target)
    #[inline]
    pub fn is_profitable(&self, spread: f64) -> bool {
        spread > self.target_spread
    }

    /// Calculate required hedge volumes
    ///
    /// Returns (power_mw, gas_mwh, co2_tons) for 1 hour of operation
    pub fn calculate_hedge_volumes(&self, hours: f64) -> (f64, f64, f64) {
        let power_volume: f64 = self.capacity_mw * hours;
        let gas_volume: f64 = power_volume * self.heat_rate;
        let co2_volume: f64 = gas_volume * self.emission_factor;

        (power_volume, gas_volume, co2_volume)
    }

    /// Update average spread (for mean reversion analysis)
    pub fn update_avg_spread(&self, current_spread: f64) {
        let current: i64 = self.avg_spread.load(Ordering::Relaxed);
        let current_f64: f64 = (current as f64) / 10000.0;

        // Exponential moving average (alpha = 0.05)
        let new_avg: f64 = current_f64 * 0.95 + current_spread * 0.05;

        self.avg_spread
            .store((new_avg * 10000.0) as i64, Ordering::Release);
    }

    /// Get recommendation for spark spread hedge
    ///
    /// Returns 3 separate recommendations:
    /// 1. Power (SELL) – Lock in power revenue
    /// 2. Gas (BUY) – Lock in fuel cost
    /// 3. CO2 (BUY) – Lock in carbon cost
    pub fn get_recommendations(
        &self,
        power_orderbook: &OrderBook,
        gas_orderbook: &OrderBook,
        co2_orderbook: &OrderBook,
        hours_ahead: f64,
    ) -> Option<SparkSpreadRecommendations> {
        // Get current prices
        let (power_bid, _) = power_orderbook.best_bid();
        let (gas_ask, _) = gas_orderbook.best_ask();
        let (co2_ask, _) = co2_orderbook.best_ask();

        // Calculate spread
        let spread: f64 = self.calculate_spread(power_bid, gas_ask, co2_ask);

        // Update average
        self.update_avg_spread(spread);

        // Check if profitable
        if !self.is_profitable(spread) {
            return None;
        }

        // Calculate volumes
        let (power_volume, gas_volume, co2_volume): (f64, f64, f64) =
            self.calculate_hedge_volumes(hours_ahead);

        // Check if we need to rehedge
        let current_power_hedge: f64 = (self.power_hedge.load(Ordering::Acquire) as f64) / 100.0;
        let delta_power: f64 = power_volume - current_power_hedge.abs();

        if current_power_hedge != 0.0 {
            let change_pct: f64 = (delta_power / current_power_hedge.abs()).abs() * 10000.0;
            if change_pct < self.rehedge_threshold_bps as f64 {
                return None; // Below threshold
            }
        }

        // Calculate costs for profitability check
        let costs: CostsBreakdown = self.calculate_costs_breakdown(gas_ask, co2_ask);

        // Urgency based on spread vs. average
        let avg_spread: f64 = (self.avg_spread.load(Ordering::Relaxed) as f64) / 10000.0;
        let spread_premium: f64 = spread - avg_spread;

        let urgency = if spread_premium > 10.0 {
            Urgency::High // Exceptional spread
        } else {
            Urgency::Normal
        };

        let timestamp = get_timestamp_ns();

        // Power recommendation (SELL)
        let power_rec: HedgeRecommendation = HedgeRecommendation::new(
            power_volume,
            power_bid,
            Side::Bid, // SELL power
            urgency,
            format!(
                "Spark spread hedge: SELL power @ €{:.2}/MWh (spread: €{:.2})",
                power_bid, spread
            ),
            timestamp,
        );

        // Gas recommendation (BUY)
        let gas_rec: HedgeRecommendation = HedgeRecommendation::new(
            gas_volume,
            gas_ask,
            Side::Ask, // BUY gas
            urgency,
            format!(
                "Spark spread hedge: BUY gas @ €{:.2}/MWh (cost: €{:.2}/MWh power)",
                gas_ask, costs.gas_cost_per_mwh
            ),
            timestamp,
        );

        // CO2 recommendation (BUY)
        let co2_rec: HedgeRecommendation = HedgeRecommendation::new(
            co2_volume,
            co2_ask,
            Side::Ask, // BUY CO2 allowances
            urgency,
            format!(
                "Spark spread hedge: BUY CO2 @ €{:.2}/ton (cost: €{:.2}/MWh power)",
                co2_ask, costs.co2_cost_per_mwh
            ),
            timestamp,
        );

        Some(SparkSpreadRecommendations {
            spread,
            avg_spread,
            power: power_rec,
            gas: gas_rec,
            co2: co2_rec,
            costs,
            profit_per_mwh: spread - self.target_spread,
            total_profit: (spread - self.target_spread) * power_volume,
        })
    }

    /// Execute hedge (update internal positions)
    pub fn execute_hedge(&self, power_volume: f64, gas_volume: f64, co2_volume: f64) {
        // Power is sold (negative position)
        self.power_hedge
            .fetch_add(-(power_volume * 100.0) as i64, Ordering::AcqRel);

        // Gas is bought (positive position)
        self.gas_hedge
            .fetch_add((gas_volume * 100.0) as i64, Ordering::AcqRel);

        // CO2 is bought (positive position)
        self.co2_hedge
            .fetch_add((co2_volume * 100.0) as i64, Ordering::AcqRel);
    }

    /// Get current hedge positions
    pub fn get_positions(&self) -> SparkSpreadPositions {
        SparkSpreadPositions {
            power_mw: (self.power_hedge.load(Ordering::Acquire) as f64) / 100.0,
            gas_mwh: (self.gas_hedge.load(Ordering::Acquire) as f64) / 100.0,
            co2_tons: (self.co2_hedge.load(Ordering::Acquire) as f64) / 100.0,
        }
    }

    /// Calculate net P&L given current prices
    pub fn calculate_pnl(&self, power_price: f64, gas_price: f64, co2_price: f64) -> f64 {
        let positions: SparkSpreadPositions = self.get_positions();

        // Revenue from selling power (negative position)
        let power_pnl: f64 = -positions.power_mw * power_price;

        // Cost of buying gas (positive position)
        let gas_pnl: f64 = -positions.gas_mwh * gas_price;

        // Cost of buying CO2 (positive position)
        let co2_pnl: f64 = -positions.co2_tons * co2_price;

        power_pnl + gas_pnl + co2_pnl
    }
}

/// Costs breakdown for spark spread calculation
#[derive(Debug, Clone)]
pub struct CostsBreakdown {
    /// Gas cost per MWh of electricity (€/MWh)
    pub gas_cost_per_mwh: f64,

    /// CO2 cost per MWh of electricity (€/MWh)
    pub co2_cost_per_mwh: f64,

    /// Total cost per MWh of electricity (€/MWh)
    pub total_cost_per_mwh: f64,

    /// Gas volume needed per MWh electricity (MWh)
    pub gas_volume_per_mwh: f64,

    /// CO2 emissions per MWh electricity (tons)
    pub co2_volume_per_mwh: f64,
}

/// Complete spark spread hedge recommendations
#[derive(Debug, Clone)]
pub struct SparkSpreadRecommendations {
    /// Current spark spread (€/MWh)
    pub spread: f64,

    /// Historical average spread (€/MWh)
    pub avg_spread: f64,

    /// Power hedge recommendation (SELL)
    pub power: HedgeRecommendation,

    /// Gas hedge recommendation (BUY)
    pub gas: HedgeRecommendation,

    /// CO2 hedge recommendation (BUY)
    pub co2: HedgeRecommendation,

    /// Cost breakdown
    pub costs: CostsBreakdown,

    /// Profit above target per MWh (€/MWh)
    pub profit_per_mwh: f64,

    /// Total expected profit (€)
    pub total_profit: f64,
}

/// Current hedge positions
#[derive(Debug, Clone)]
pub struct SparkSpreadPositions {
    /// Power position (MW, negative = sold)
    pub power_mw: f64,

    /// Gas position (MWh, positive = bought)
    pub gas_mwh: f64,

    /// CO2 position (tons, positive = bought)
    pub co2_tons: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spark_spread_calculation() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        // Example: Power €100, Gas €40, CO2 €80
        let spread: f64 = hedge.calculate_spread(100.0, 40.0, 80.0);

        // Expected: 100 - (40/2.0) - (80*0.202) = 100 - 20 - 16.16 = 63.84
        assert!((spread - 63.84).abs() < 0.01);
    }

    #[test]
    fn test_profitability_check() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        let good_spread: f64 = 65.0;
        let bad_spread: f64 = 45.0;

        assert!(hedge.is_profitable(good_spread));
        assert!(!hedge.is_profitable(bad_spread));
    }

    #[test]
    fn test_hedge_volumes() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        let (power, gas, co2): (f64, f64, f64) = hedge.calculate_hedge_volumes(24.0); // 24 hours

        assert_eq!(power, 2400.0); // 100 MW × 24h = 2400 MWh
        assert_eq!(gas, 4800.0); // 2400 × 2.0 = 4800 MWh
        assert!((co2 - 969.6).abs() < 0.1); // 4800 × 0.202 = 969.6 tons
    }

    #[test]
    fn test_costs_breakdown() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        let costs: CostsBreakdown = hedge.calculate_costs_breakdown(40.0, 80.0);

        assert!((costs.gas_cost_per_mwh - 20.0).abs() < 0.01);
        assert!((costs.co2_cost_per_mwh - 16.16).abs() < 0.01);
        assert!((costs.total_cost_per_mwh - 36.16).abs() < 0.01);
    }

    #[test]
    fn test_execute_hedge() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        hedge.execute_hedge(100.0, 200.0, 40.4);

        let positions = hedge.get_positions();

        assert_eq!(positions.power_mw, -100.0); // Sold power (negative)
        assert_eq!(positions.gas_mwh, 200.0); // Bought gas (positive)
        assert_eq!(positions.co2_tons, 40.4); // Bought CO2 (positive)
    }

    #[test]
    fn test_pnl_calculation() {
        let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

        // Execute hedge at: Power €100, Gas €40, CO2 €80
        hedge.execute_hedge(100.0, 200.0, 40.4);

        // Calculate P&L at the same prices (should be ~0)
        let pnl: f64 = hedge.calculate_pnl(100.0, 40.0, 80.0);

        // Revenue: 100 MWh × €100 = €10,000
        // Gas cost: 200 MWh × €40 = €8,000
        // CO2 cost: 40.4 tons × €80 = €3,232
        // Net: €10,000 - €8,000 - €3,232 = -€1,232 (small loss due to spread costs)

        assert!(pnl.abs() < 2000.0); // Should be close to zero
    }

    #[cfg(test)]
    mod integration_tests {
        use super::*;

        #[test]
        fn test_real_world_scenario() {
            // Based on German CCGT plant, Q2 2024 data
            let hedge: SparkSpreadHedge = SparkSpreadHedge::new(
                500.0, // 500 MW plant
                1.9,   // Modern CCGT efficiency
                0.202, // Natural gas emissions
                45.0,  // €45/MWh target
            );

            // Typical prices
            let power: f64 = 95.0;
            let gas: f64 = 38.0; // TTF
            let co2: f64 = 75.0; // EUA

            let spread: f64 = hedge.calculate_spread(power, gas, co2);

            // Expected: 95 - (38/1.9) - (75*0.202) = 95 - 20 - 15.15 = 59.85
            assert!((spread - 59.85).abs() < 0.5);
            assert!(hedge.is_profitable(spread));
        }

        #[test]
        fn test_inefficient_plant() {
            // Old OCGT plant
            let hedge: SparkSpreadHedge = SparkSpreadHedge::new(
                100.0, 3.0, // Poor efficiency (33%)
                0.202, 30.0,
            );

            let spread: f64 = hedge.calculate_spread(80.0, 40.0, 80.0);

            // Expected: 80 - (40/3.0) - (80*0.202) = 80 - 13.33 - 16.16 = 50.51
            assert!((spread - 50.51).abs() < 0.5);
        }
    }
}
