use crate::hedging::{
    DeltaHedge, HedgeConfig, HedgeRecommendation, MVHRStrategy, MeanReversionHedge,
};
use crate::market_data::{MarketTick, OrderBook};
use crate::utils::Metrics;
use parking_lot::RwLock;
use std::sync::Arc;

/// Main hedging engine
///
/// Coordinates multiple strategies and manages execution
pub struct HedgeEngine {
    /// Spot orderbook
    spot_orderbook: Arc<OrderBook>,

    /// Futures orderbook
    futures_orderbook: Arc<OrderBook>,

    /// Delta hedging strategy
    delta_hedge: Arc<DeltaHedge>,

    /// MVHR strategy (optional)
    mvhr_strategy: Option<Arc<MVHRStrategy>>,

    /// Mean reversion strategy (optional)
    mean_reversion: Option<Arc<MeanReversionHedge>>,

    /// Performance metrics
    metrics: Arc<RwLock<Metrics>>,
}

impl HedgeEngine {
    /// Create a new hedge engine
    pub fn new(config: HedgeConfig) -> crate::Result<Self> {
        config.validate()?;

        let delta_hedge = Arc::new(DeltaHedge::new(
            config.initial_position,
            config.default_hedge_ratio,
            config.rehedge_threshold_bps,
        ));

        let mvhr_strategy: Option<Arc<MVHRStrategy>> = if config.enable_mvhr {
            Some(Arc::new(MVHRStrategy::new(
                config.statistics_window_hours,
                24, // Recalculate every 24 hours
            )))
        } else {
            None
        };

        let mean_reversion = if config.enable_mean_reversion {
            Some(Arc::new(MeanReversionHedge::new(
                config.statistics_window_hours,
                0.20, // Kappa for energy markets
                2.0,  // Z-score threshold
                0.70, // Hedge strength
            )))
        } else {
            None
        };

        Ok(Self {
            spot_orderbook: Arc::new(OrderBook::new(1)),
            futures_orderbook: Arc::new(OrderBook::new(2)),
            delta_hedge,
            mvhr_strategy,
            mean_reversion,
            metrics: Arc::new(RwLock::new(Metrics::new())),
        })
    }

    /// Process incoming market data tick
    ///
    /// # Performance
    /// Hot path: ~200-400ns
    pub fn on_tick(&self, tick: MarketTick) {
        let start_ns = crate::utils::get_timestamp_ns();

        // Update appropriate orderbook
        match tick.symbol_id {
            1 => {
                // Spot market
                if tick.is_bid() {
                    self.spot_orderbook.update_bid(
                        0,
                        tick.price,
                        tick.quantity as u64,
                        tick.timestamp_ns,
                    );
                } else {
                    self.spot_orderbook.update_ask(
                        0,
                        tick.price,
                        tick.quantity as u64,
                        tick.timestamp_ns,
                    );
                }

                // Update mean reversion if enabled
                if let Some(ref mr) = self.mean_reversion {
                    mr.add_price(tick.price_f64());
                }
            }
            2 => {
                // Futures market
                if tick.is_bid() {
                    self.futures_orderbook.update_bid(
                        0,
                        tick.price,
                        tick.quantity as u64,
                        tick.timestamp_ns,
                    );
                } else {
                    self.futures_orderbook.update_ask(
                        0,
                        tick.price,
                        tick.quantity as u64,
                        tick.timestamp_ns,
                    );
                }

                // Update MVHR if enabled
                if let Some(ref mvhr) = self.mvhr_strategy {
                    let spot_mid = self.spot_orderbook.mid_price();
                    let futures_mid = self.futures_orderbook.mid_price();
                    mvhr.add_observation(spot_mid, futures_mid);
                }
            }
            _ => {}
        }

        // Record latency
        let latency_ns = crate::utils::get_timestamp_ns() - start_ns;
        self.metrics.write().record_tick_latency(latency_ns);
    }

    /// Get hedge recommendation
    pub fn get_hedge_recommendation(&self) -> crate::Result<Option<HedgeRecommendation>> {
        // Calculate base delta hedge
        let recommendation = self.delta_hedge.get_recommendation(&self.futures_orderbook);

        if let Some(mut rec) = recommendation {
            // Adjust with MVHR if enabled
            if let Some(ref mvhr) = self.mvhr_strategy {
                let optimal_ratio = mvhr.get_hedge_ratio();
                self.delta_hedge.update_hedge_ratio(optimal_ratio);
                rec.reason
                    .push_str(&format!(" [MVHR ratio: {:.3}]", optimal_ratio));
            }

            // Adjust with mean reversion if enabled
            if let Some(ref mr) = self.mean_reversion {
                let current_price = self.spot_orderbook.mid_price();
                if let Some(adjustment) = mr.should_adjust_hedge(current_price) {
                    rec.quantity *= adjustment;
                    rec.reason
                        .push_str(&format!(" [MR adjustment: {:.2}]", adjustment));
                }
            }

            Ok(Some(rec))
        } else {
            Ok(None)
        }
    }

    /// Execute hedge (update internal state)
    pub fn execute_hedge(&self, recommendation: &HedgeRecommendation) -> crate::Result<()> {
        self.delta_hedge
            .execute_hedge(recommendation.quantity, recommendation.side);
        self.metrics
            .write()
            .record_hedge_execution(recommendation.quantity);
        Ok(())
    }

    /// Get current position
    pub fn get_position(&self) -> f64 {
        self.delta_hedge.get_position()
    }

    /// Get current hedge position
    pub fn get_hedge_position(&self) -> f64 {
        self.delta_hedge.get_hedge_position()
    }

    /// Get metrics
    pub fn get_metrics(&self) -> Metrics {
        self.metrics.read().clone()
    }

    /// Get spot orderbook
    pub fn spot_orderbook(&self) -> &OrderBook {
        &self.spot_orderbook
    }

    /// Get futures orderbook
    pub fn futures_orderbook(&self) -> &OrderBook {
        &self.futures_orderbook
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::MarketTick;
    use crate::utils::get_timestamp_ns;

    #[test]
    fn test_engine_creation() {
        let config = HedgeConfig::simple(-10_000.0, 1.125);
        let engine = HedgeEngine::new(config);

        assert!(engine.is_ok());

        let engine = engine.unwrap();
        assert_eq!(engine.get_position(), -10_000.0);
    }

    #[test]
    fn test_engine_tick_processing() {
        let config = HedgeConfig::simple(-10_000.0, 1.125);
        let engine = HedgeEngine::new(config).unwrap();

        // Send spot tick
        let tick = MarketTick::bid(get_timestamp_ns(), 45.50, 100, 1);
        engine.on_tick(tick);

        // Send futures tick
        let tick = MarketTick::ask(get_timestamp_ns(), 50.15, 120, 2);
        engine.on_tick(tick);

        // Check orderbooks updated
        let (spot_bid, _) = engine.spot_orderbook().best_bid();
        assert_eq!(spot_bid, 45.50);

        let (futures_ask, _) = engine.futures_orderbook().best_ask();
        assert_eq!(futures_ask, 50.15);
    }

    #[test]
    fn test_hedge_recommendation() {
        let config = HedgeConfig::simple(-10_000.0, 1.125);
        let engine = HedgeEngine::new(config).unwrap();

        // Setup orderbooks
        let spot_tick = MarketTick::bid(get_timestamp_ns(), 45.50, 100, 1);
        engine.on_tick(spot_tick);

        let futures_tick = MarketTick::ask(get_timestamp_ns(), 50.15, 120, 2);
        engine.on_tick(futures_tick);

        // Get recommendation
        let rec = engine.get_hedge_recommendation().unwrap();
        assert!(rec.is_some());

        let rec = rec.unwrap();
        // Should recommend ~11,250 MWh
        assert!((rec.quantity - 11_250.0).abs() < 100.0);
    }
}
