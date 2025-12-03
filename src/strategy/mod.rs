//! Strategy trait and implementations

use crate::hedging::HedgeRecommendation;
use crate::market_data::OrderBook;

/// Trait for hedging strategies
///
/// All strategies must implement this trait to be used by the engine
pub trait HedgingStrategy: Send + Sync {
    /// Calculate hedge recommendation based on the current market state
    ///
    /// # Performance Requirements
    /// This method is called in the hot path and should complete in <500ns
    fn calculate_hedge(
        &self,
        position: f64,
        spot_orderbook: &OrderBook,
        futures_orderbook: &OrderBook,
    ) -> Option<HedgeRecommendation>;

    /// Update strategy parameters (cold path)
    ///
    /// Called periodically in the background thread
    fn update_parameters(&mut self) {}

    /// Get a strategy name
    fn name(&self) -> &str;

    /// Get strategy description
    fn description(&self) -> &str {
        "No description available"
    }
}

/// Strategy builder for composing multiple strategies
pub struct StrategyBuilder {
    strategies: Vec<Box<dyn HedgingStrategy>>,
    weights: Vec<f64>,
}

impl StrategyBuilder {
    /// Create a new strategy builder
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            weights: Vec::new(),
        }
    }

    /// Add a strategy with weight
    pub fn add_strategy(mut self, strategy: Box<dyn HedgingStrategy>, weight: f64) -> Self {
        self.strategies.push(strategy);
        self.weights.push(weight);
        self
    }

    /// Build composite strategy
    pub fn build(self) -> CompositeStrategy {
        CompositeStrategy {
            strategies: self.strategies,
            weights: self.weights,
        }
    }
}

impl Default for StrategyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Composite strategy that combines multiple strategies
#[derive(Default)]
pub struct CompositeStrategy {
    strategies: Vec<Box<dyn HedgingStrategy>>,
    weights: Vec<f64>,
}

impl CompositeStrategy {
    /// Create a new composite strategy
    pub fn builder() -> StrategyBuilder {
        StrategyBuilder::new()
    }
}

impl HedgingStrategy for CompositeStrategy {
    fn calculate_hedge(
        &self,
        position: f64,
        spot_orderbook: &OrderBook,
        futures_orderbook: &OrderBook,
    ) -> Option<HedgeRecommendation> {
        if self.strategies.is_empty() {
            return None;
        }

        let mut total_quantity = 0.0;
        let mut total_weight = 0.0;
        let mut any_hedge = false;

        // Get recommendations from all strategies
        for (strategy, &weight) in self.strategies.iter().zip(self.weights.iter()) {
            if let Some(rec) = strategy.calculate_hedge(position, spot_orderbook, futures_orderbook)
            {
                total_quantity += rec.quantity * weight;
                total_weight += weight;
                any_hedge = true;
            }
        }

        if !any_hedge {
            return None;
        }

        // Weighted average
        let avg_quantity: f64 = total_quantity / total_weight;
        let (price, _) = futures_orderbook.best_ask();

        Some(HedgeRecommendation::new(
            avg_quantity,
            price,
            crate::market_data::Side::Ask,
            crate::hedging::Urgency::Normal,
            format!("Composite strategy ({} strategies)", self.strategies.len()),
            crate::utils::get_timestamp_ns(),
        ))
    }

    fn update_parameters(&mut self) {
        for strategy in &mut self.strategies {
            strategy.update_parameters();
        }
    }

    fn name(&self) -> &str {
        "Composite"
    }

    fn description(&self) -> &str {
        "Combines multiple strategies with weighted average"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStrategy {
        quantity: f64,
    }

    impl HedgingStrategy for MockStrategy {
        fn calculate_hedge(
            &self,
            _position: f64,
            _spot: &OrderBook,
            _futures: &OrderBook,
        ) -> Option<HedgeRecommendation> {
            Some(HedgeRecommendation::new(
                self.quantity,
                50.0,
                crate::market_data::Side::Ask,
                crate::hedging::Urgency::Normal,
                "Mock".to_string(),
                0,
            ))
        }

        fn name(&self) -> &str {
            "Mock"
        }
    }

    #[test]
    fn test_composite_strategy() {
        let strategy1 = Box::new(MockStrategy { quantity: 100.0 });
        let strategy2 = Box::new(MockStrategy { quantity: 200.0 });

        let composite = CompositeStrategy::builder()
            .add_strategy(strategy1, 1.0)
            .add_strategy(strategy2, 1.0)
            .build();

        let spot = OrderBook::new(1);
        let futures = OrderBook::new(2);

        let rec: Option<HedgeRecommendation> = composite.calculate_hedge(-1000.0, &spot, &futures);
        assert!(rec.is_some());

        let rec = rec.unwrap();
        // Should be an average of 100 and 200 = 150
        assert!((rec.quantity - 150.0).abs() < 1.0);
    }
}
