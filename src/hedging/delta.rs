use crate::hedging::{HedgeRecommendation, Urgency};
use crate::market_data::{OrderBook, Side};
use crate::utils::get_timestamp_ns;
use std::sync::atomic::{AtomicI64, Ordering};

/// Simple delta hedging strategy
///
/// Maintains a fixed hedge ratio relative to position size.
///
/// # Hedge Direction Logic
///
/// ```text
/// Physical Position: -10,000 MWh (SHORT)
/// Hedge Ratio: 1.125
/// Target Hedge: +11,250 MWh (LONG - opposite direction!)
///
/// Action: BUY 11,250 MWh futures (Side::Ask)
/// Result: Net exposure = -10,000 + 11,250 = +1,250 MWh
/// ```
pub struct DeltaHedge {
    /// Current position (fixed-point: actual * 100)
    position: AtomicI64,

    /// Target hedge ratio (fixed-point: ratio * 10000)
    hedge_ratio: AtomicI64,

    /// Current hedge position (fixed-point: actual * 100)
    /// IMPORTANT: This should be an OPPOSITE sign to position
    /// - If position is negative (SHORT), hedge should be positive (LONG)
    /// - If position is positive (LONG), hedge should be negative (SHORT)
    hedge_position: AtomicI64,

    /// Rehedge threshold (basis points)
    threshold_bps: i64,
}

impl DeltaHedge {
    /// Create new delta hedging strategy
    pub fn new(initial_position: f64, hedge_ratio: f64, threshold_bps: i64) -> Self {
        Self {
            position: AtomicI64::new((initial_position * 100.0) as i64),
            hedge_ratio: AtomicI64::new((hedge_ratio * 10000.0) as i64),
            hedge_position: AtomicI64::new(0),
            threshold_bps,
        }
    }

    /// Update a position
    pub fn update_position(&self, new_position: f64) {
        self.position
            .store((new_position * 100.0) as i64, Ordering::Release);
    }

    /// Update hedge ratio
    pub fn update_hedge_ratio(&self, new_ratio: f64) {
        self.hedge_ratio
            .store((new_ratio * 10000.0) as i64, Ordering::Release);
    }

    /// Get the current position
    pub fn get_position(&self) -> f64 {
        (self.position.load(Ordering::Acquire) as f64) / 100.0
    }

    /// Get the current hedge position
    pub fn get_hedge_position(&self) -> f64 {
        (self.hedge_position.load(Ordering::Acquire) as f64) / 100.0
    }

    /// Calculate the required hedge delta
    ///
    /// # Performance
    /// ~100-150ns
    ///
    /// # Returns
    /// Positive delta = need to BUY (increase LONG hedge)
    /// Negative delta = need to SELL (increase SHORT hedge)
    #[inline(always)]
    pub fn calculate_hedge_delta(&self) -> Option<f64> {
        let position: i64 = self.position.load(Ordering::Acquire);
        let ratio: i64 = self.hedge_ratio.load(Ordering::Acquire);
        let current_hedge: i64 = self.hedge_position.load(Ordering::Acquire);

        // Target hedge = (-position * ratio)
        // Why negative? Because hedge is OPPOSITE to position
        // Example: position = -10,000 (SHORT)
        //          ratio = 1.125
        //          target = -(-10,000) * 1.125 = +11,250 (LONG)
        let target_hedge = ((-position as i128) * (ratio as i128)) / 10000;
        let delta = (target_hedge as i64) - current_hedge;

        // Check if the delta exceeds a threshold
        if current_hedge != 0 {
            let delta_pct = ((delta as i128) * 10000) / (current_hedge.abs() as i128);

            if delta_pct.abs() > self.threshold_bps as i128 {
                Some((delta as f64) / 100.0)
            } else {
                None
            }
        } else {
            // No current hedge, any delta triggers rehedge
            if delta.abs() > 0 {
                Some((delta as f64) / 100.0)
            } else {
                None
            }
        }
    }

    /// Get hedge recommendation
    pub fn get_recommendation(&self, orderbook: &OrderBook) -> Option<HedgeRecommendation> {
        let delta: f64 = self.calculate_hedge_delta()?;

        // Determine side based on delta sign
        // Positive delta = need to BUY (add LONG position) = use ASK side
        // Negative delta = need to SELL (add SHORT position) = use BID side
        let (side, price): (Side, f64) = if delta > 0.0 {
            let (ask_price, _) = orderbook.best_ask();
            (Side::Ask, ask_price)
        } else {
            let (bid_price, _) = orderbook.best_bid();
            (Side::Bid, bid_price)
        };

        let urgency: Urgency = if delta.abs() > self.get_position().abs() * 0.10 {
            Urgency::High
        } else {
            Urgency::Normal
        };

        Some(HedgeRecommendation::new(
            delta.abs(),
            price,
            side,
            urgency,
            format!(
                "Delta hedge: position={:.0}, target hedge={:.0}, current hedge={:.0}, delta={:.0}",
                self.get_position(),
                -self.get_position() * (self.hedge_ratio.load(Ordering::Acquire) as f64 / 10000.0),
                self.get_hedge_position(),
                delta
            ),
            get_timestamp_ns(),
        ))
    }

    /// Execute hedge (update internal state)
    ///
    /// # CRITICAL: Hedge Direction Logic
    ///
    /// The hedge position must be OPPOSITE to the physical position:
    /// - Physical SHORT → Hedge LONG (positive)
    /// - Physical LONG → Hedge SHORT (negative)
    ///
    /// # Side Interpretation:
    /// - `Side::Ask` (BUY) → Increases hedge position (more LONG)
    /// - `Side::Bid` (SELL) → Decreases hedge position (more SHORT)
    ///
    /// # Example:
    /// ```text
    /// Physical: -10,000 MWh (SHORT)
    /// Action: BUY 11,250 MWh (Side::Ask)
    /// Result: hedge_position = +11,250 MWh (LONG)
    /// Net: -10,000 + 11,250 = +1,250 MWh
    /// ```
    pub fn execute_hedge(&self, quantity: f64, side: Side) {
        let delta = match side {
            Side::Ask => (quantity * 100.0) as i64, // BUY = add LONG position (positive)
            Side::Bid => -(quantity * 100.0) as i64, // SELL = add SHORT position (negative)
        };

        self.hedge_position.fetch_add(delta, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_hedge_basic() {
        let hedge = DeltaHedge::new(-10_000.0, 1.125, 500);

        assert_eq!(hedge.get_position(), -10_000.0);

        let delta: Option<f64> = hedge.calculate_hedge_delta();
        assert!(delta.is_some());

        // Should recommend hedge of 11,250 MWh (10,000 * 1.125)
        let delta: f64 = delta.unwrap();
        assert!((delta - 11_250.0).abs() < 1.0);
    }

    #[test]
    fn test_delta_hedge_threshold() {
        let hedge: DeltaHedge = DeltaHedge::new(-10_000.0, 1.125, 500);

        // Execute initial hedge
        hedge.execute_hedge(11_250.0, Side::Ask);

        // Verify hedge is now LONG (positive)
        assert_eq!(hedge.get_hedge_position(), 11_250.0);

        // Small position change shouldn't trigger rehedge
        hedge.update_position(-10_100.0);
        let delta = hedge.calculate_hedge_delta();

        // Delta is 112.5 MWh, which is ~1% of hedge
        // Threshold is 5%, so no rehedge
        assert!(delta.is_none());

        // Large position change should trigger
        hedge.update_position(-11_000.0);
        let delta = hedge.calculate_hedge_delta();
        assert!(delta.is_some());
    }

    #[test]
    fn test_hedge_ratio_update() {
        let hedge = DeltaHedge::new(-10_000.0, 1.0, 500);

        hedge.update_hedge_ratio(1.5);

        let delta = hedge.calculate_hedge_delta();
        assert!(delta.is_some());

        // Should recommend 15,000 MWh (10,000 * 1.5)
        let delta = delta.unwrap();
        assert!((delta - 15_000.0).abs() < 1.0);
    }

    #[test]
    fn test_hedge_direction_short_position() {
        // Test: SHORT physical position requires LONG hedge
        let hedge: DeltaHedge = DeltaHedge::new(-10_000.0, 1.125, 500);

        // Initial: should recommend BUY hedge (positive delta)
        let delta: Option<f64> = hedge.calculate_hedge_delta();
        assert!(delta.is_some());
        let delta: f64 = delta.unwrap();
        assert!(
            delta > 0.0,
            "Delta should be positive (need to buy), got {}",
            delta
        );

        // Execute BUY hedge (Side::Ask)
        hedge.execute_hedge(delta, Side::Ask);

        // Check hedge position is now LONG (positive)
        let hedge_pos = hedge.get_hedge_position();
        assert!(
            hedge_pos > 0.0,
            "Hedge position should be LONG (positive), got {}",
            hedge_pos
        );
        assert!(
            (hedge_pos - 11_250.0).abs() < 1.0,
            "Hedge position should be ~11,250, got {}",
            hedge_pos
        );

        // Net exposure should be near zero (slightly positive due to overhedge)
        let net: f64 = hedge.get_position() + hedge_pos;
        assert!(
            net > 0.0 && net < 2_000.0,
            "Net exposure should be slightly positive, got {}",
            net
        );
        assert!(
            (net - 1_250.0).abs() < 1.0,
            "Net exposure should be ~1,250 (overhedge), got {}",
            net
        );
    }

    #[test]
    fn test_hedge_direction_long_position() {
        // Test: LONG physical position requires SHORT hedge
        let hedge = DeltaHedge::new(10_000.0, 1.125, 500);

        let delta: Option<f64> = hedge.calculate_hedge_delta();
        assert!(delta.is_some());
        let delta = delta.unwrap();

        // Should recommend SELL (negative delta)
        assert!(
            delta < 0.0,
            "Delta should be negative (need to sell), got {}",
            delta
        );

        // Execute SELL hedge (Side::Bid)
        hedge.execute_hedge(delta.abs(), Side::Bid);

        // Hedge position should be SHORT (negative)
        let hedge_pos: f64 = hedge.get_hedge_position();
        assert!(
            hedge_pos < 0.0,
            "Hedge position should be SHORT (negative), got {}",
            hedge_pos
        );
        assert!(
            (hedge_pos + 11_250.0).abs() < 1.0,
            "Hedge position should be ~-11,250, got {}",
            hedge_pos
        );

        // Net exposure should be slightly negative (overhedge)
        let net: f64 = hedge.get_position() + hedge_pos;
        assert!(
            net < 0.0 && net > -2_000.0,
            "Net exposure should be slightly negative, got {}",
            net
        );
    }

    #[test]
    fn test_hedge_execution_adds_correctly() {
        let hedge: DeltaHedge = DeltaHedge::new(-10_000.0, 1.0, 500);

        // Execute first hedge: BUY 5,000
        hedge.execute_hedge(5_000.0, Side::Ask);
        assert_eq!(hedge.get_hedge_position(), 5_000.0);

        // Execute second hedge: BUY 5,000 more
        hedge.execute_hedge(5_000.0, Side::Ask);
        assert_eq!(hedge.get_hedge_position(), 10_000.0);

        // Execute partial unwind: SELL 2,000
        hedge.execute_hedge(2_000.0, Side::Bid);
        assert_eq!(hedge.get_hedge_position(), 8_000.0);
    }

    #[test]
    fn test_hedge_recommendation_content() {
        let hedge: DeltaHedge = DeltaHedge::new(-10_000.0, 1.125, 500);
        let ob = OrderBook::new(2);

        // Setup orderbook
        ob.update_ask(0, 500000, 100, 1000); // €50.00
        ob.update_bid(0, 499000, 100, 1000); // €49.90

        let rec: Option<HedgeRecommendation> = hedge.get_recommendation(&ob);
        assert!(rec.is_some());

        let rec: HedgeRecommendation = rec.unwrap();

        // Should recommend BUY (Side::Ask)
        assert!(matches!(rec.side, Side::Ask));

        // Should use ask price
        assert_eq!(rec.price, 50.00);

        // Should recommend ~11,250 MWh
        assert!((rec.quantity - 11_250.0).abs() < 100.0);

        // Reason should contain position info
        assert!(rec.reason.contains("position=-10000"));
    }

    #[test]
    fn test_net_exposure_calculation() {
        // Verify net exposure formula: physical + hedge
        let hedge = DeltaHedge::new(-10_000.0, 1.0, 500);

        // Perfect hedge: 1:1 ratio
        hedge.execute_hedge(10_000.0, Side::Ask);

        let net = hedge.get_position() + hedge.get_hedge_position();
        assert!(
            net.abs() < 1.0,
            "Net should be ~0 with 1:1 hedge, got {}",
            net
        );

        // Now with 1.125 ratio (over hedge)
        let hedge2 = DeltaHedge::new(-10_000.0, 1.125, 500);
        hedge2.execute_hedge(11_250.0, Side::Ask);

        let net2: f64 = hedge2.get_position() + hedge2.get_hedge_position();
        assert!(
            (net2 - 1_250.0).abs() < 1.0,
            "Net should be ~1,250 with 1.125 hedge, got {}",
            net2
        );
    }
}
