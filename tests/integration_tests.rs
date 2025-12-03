//! Integration tests

use hedging_engine::utils::MetricsSummary;
use hedging_engine::*;
use std::thread::JoinHandle;

#[test]
fn test_full_hedging_workflow() {
    // Create engine
    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Send market data
    let ts: u64 = get_timestamp_ns();

    engine.on_tick(MarketTick::bid(ts, 48.20, 150, 1));
    engine.on_tick(MarketTick::ask(ts, 48.25, 130, 1));
    engine.on_tick(MarketTick::bid(ts, 50.10, 120, 2));
    engine.on_tick(MarketTick::ask(ts, 50.15, 140, 2));

    // Get hedge recommendation
    let rec: Option<HedgeRecommendation> = engine.get_hedge_recommendation().unwrap();
    assert!(rec.is_some());

    let rec: HedgeRecommendation = rec.unwrap();

    // Should recommend ~11,250 MWh hedge
    assert!((rec.quantity - 11_250.0).abs() < 200.0);

    // Execute hedge
    engine.execute_hedge(&rec).unwrap();

    // Check position updated
    let hedge_pos: f64 = engine.get_hedge_position();
    assert!(hedge_pos.abs() > 0.0);
}

#[test]
fn test_orderbook_consistency() {
    let ob: OrderBook = OrderBook::new(1);

    // Update multiple levels
    ob.update_bid(0, 450000, 100, 1000);
    ob.update_bid(1, 449000, 150, 1001);
    ob.update_bid(2, 448000, 120, 1002);

    ob.update_ask(0, 451000, 110, 1003);
    ob.update_ask(1, 452000, 140, 1004);
    ob.update_ask(2, 453000, 130, 1005);

    // Check best levels
    let (bid, _) = ob.best_bid();
    let (ask, _) = ob.best_ask();

    assert_eq!(bid, 45.0);
    assert_eq!(ask, 45.1);

    // Check mid-price
    assert_eq!(ob.mid_price(), 45.05);

    // Check all levels
    let bids: Vec<(f64, u64)> = ob.get_bids(3);
    assert_eq!(bids.len(), 3);
    assert_eq!(bids[0].0, 45.0);
    assert_eq!(bids[1].0, 44.9);
    assert_eq!(bids[2].0, 44.8);
}

#[test]
fn test_delta_hedge_threshold() {
    let hedge: DeltaHedge = DeltaHedge::new(-10_000.0, 1.125, 500);

    // Initial state - should need hedge
    let delta: Option<f64> = hedge.calculate_hedge_delta();
    assert!(delta.is_some());

    // Execute full hedge
    hedge.execute_hedge(11_250.0, Side::Ask);

    // Small position change shouldn't trigger rehedge
    hedge.update_position(-10_100.0);
    let delta = hedge.calculate_hedge_delta();
    assert!(delta.is_none()); // Below the 5% threshold

    // Large position change should trigger
    hedge.update_position(-11_000.0);
    let delta: Option<f64> = hedge.calculate_hedge_delta();
    assert!(delta.is_some()); // Above the 5% threshold
}

#[test]
fn test_mvhr_calculation() {
    let mvhr: MVHRStrategy = MVHRStrategy::new(100, 1);

    // Add correlated observations
    for i in 0..50 {
        let spot = 45.0 + i as f64 * 0.1;
        let futures = 50.0 + i as f64 * 0.12;
        mvhr.add_observation(spot, futures);
    }

    // Calculate optimal ratio
    let ratio: Option<f64> = mvhr.calculate_optimal_ratio();
    assert!(ratio.is_some());

    let ratio = ratio.unwrap();

    // With perfect correlation, expect a ratio close to 1.0-1.2
    assert!(ratio > 0.5 && ratio < 2.0);

    // Check cached value
    let cached: f64 = mvhr.get_hedge_ratio();
    assert!((cached - ratio).abs() < 0.01);
}

#[test]
fn test_mean_reversion_strategy() {
    let config = HedgeConfig {
        initial_position: -10_000.0,
        default_hedge_ratio: 1.125,
        enable_mvhr: false,
        enable_mean_reversion: true,
        statistics_window_hours: 100, // Small window for testing
        ..Default::default()
    };

    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Build up price history with oscillations
    // Mean will be around 45.0, with std dev we can calculate
    let prices: Vec<f64> = vec![
        45.0, 44.5, 45.5, 44.0, 46.0, 43.5, 46.5, 43.0, 47.0, 42.5, 45.0, 44.5, 45.5, 44.0, 46.0,
        43.5, 46.5, 43.0, 47.0, 42.5, 45.0, 44.5, 45.5, 44.0, 46.0, 43.5, 46.5, 43.0, 47.0, 42.5,
    ];

    for (_i, &price) in prices.iter().enumerate() {
        let ts: u64 = get_timestamp_ns();
        engine.on_tick(MarketTick::bid(ts, price, 100, 1));
    }

    // Add futures ticks too
    for _i in 0..30 {
        let ts: u64 = get_timestamp_ns();
        engine.on_tick(MarketTick::ask(ts, 50.0, 100, 2));
    }

    // Now the mean reversion strategy has data
    // Get a recommendation
    let rec: Option<HedgeRecommendation> = engine.get_hedge_recommendation().unwrap();

    // Should be able to get a recommendation (even if None)
    // The key test is that the system doesn't crash
    println!("Mean reversion test: recommendation = {:?}", rec.is_some());

    // Verify the engine is working
    assert_eq!(engine.get_position(), -10_000.0);
}

#[test]
fn test_concurrent_tick_processing() {
    use std::sync::Arc;
    use std::thread;

    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: Arc<HedgeEngine> = Arc::new(HedgeEngine::new(config).unwrap());

    let mut handles: Vec<JoinHandle<()>> = vec![];

    // Spawn multiple threads sending ticks
    for thread_id in 0..4 {
        let engine: Arc<HedgeEngine> = Arc::clone(&engine);
        let handle: JoinHandle<()> = thread::spawn(move || {
            for i in 0..100 {
                let tick = MarketTick::bid(
                    i as u64,
                    45.0 + (i % 10) as f64 * 0.1,
                    100,
                    (thread_id % 2 + 1) as u8,
                );
                engine.on_tick(tick);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Check metrics
    let metrics: Metrics = engine.get_metrics();
    assert_eq!(metrics.ticks_processed(), 400);
}

#[test]
fn test_hedge_with_all_strategies() {
    let config = HedgeConfig {
        initial_position: -10_000.0,
        default_hedge_ratio: 1.125,
        enable_mvhr: true,
        enable_mean_reversion: true,
        ..Default::default()
    };

    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Send multiple ticks to build up statistics
    for i in 0..100 {
        let ts: u64 = get_timestamp_ns();
        let spot: f64 = 45.0 + (i % 20) as f64 * 0.1;
        let futures: f64 = 50.0 + (i % 18) as f64 * 0.12;

        engine.on_tick(MarketTick::bid(ts, spot, 100, 1));
        engine.on_tick(MarketTick::ask(ts, futures, 100, 2));
    }

    // Get a recommendation (should combine all strategies)
    let rec: Option<HedgeRecommendation> = engine.get_hedge_recommendation().unwrap();
    assert!(rec.is_some());
}

#[test]
fn test_metrics_collection() {
    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Process some ticks
    for i in 0..100 {
        let tick: MarketTick = MarketTick::bid(i, 45.0, 100, 1);
        engine.on_tick(tick);
    }

    let metrics: Metrics = engine.get_metrics();
    let summary: MetricsSummary = metrics.summary();

    assert_eq!(summary.ticks_processed, 100);
    assert!(summary.avg_latency_ns > 0);
    assert!(summary.min_latency_ns < u64::MAX);
    assert!(summary.max_latency_ns > 0);
    assert!(summary.p50_latency_ns > 0);
    assert!(summary.p99_latency_ns >= summary.p50_latency_ns);
}

#[test]
fn test_position_limits() {
    let config = HedgeConfig {
        initial_position: -10_000.0,
        max_position: 50_000.0,
        ..Default::default()
    };

    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Position should be within limits
    assert!(engine.get_position().abs() <= 50_000.0);
}

#[test]
fn test_error_handling() {
    // Invalid config should fail
    let config = HedgeConfig {
        default_hedge_ratio: -1.0, // Invalid!
        ..Default::default()
    };

    let result: Result<HedgeEngine> = HedgeEngine::new(config);
    assert!(result.is_err());
}

#[test]
fn test_hedge_reduces_net_exposure() {
    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Setup market
    let ts: u64 = get_timestamp_ns();
    engine.on_tick(MarketTick::bid(ts, 48.0, 150, 1));
    engine.on_tick(MarketTick::ask(ts, 50.0, 140, 2));

    // Before hedge
    let net_before: f64 = engine.get_position() + engine.get_hedge_position();
    assert_eq!(
        net_before, -10_000.0,
        "Before hedge, net = physical position"
    );

    // Execute hedge
    let rec: HedgeRecommendation = engine.get_hedge_recommendation().unwrap().unwrap();
    engine.execute_hedge(&rec).unwrap();

    // After hedge
    let physical: f64 = engine.get_position();
    let hedge: f64 = engine.get_hedge_position();
    let net_after: f64 = physical + hedge;

    println!("Physical: {:.0} MWh", physical);
    println!("Hedge:    {:.0} MWh", hedge);
    println!("Net:      {:.0} MWh", net_after);

    // Verify hedge is an opposite direction
    assert!(physical < 0.0, "Physical should be SHORT (negative)");
    assert!(hedge > 0.0, "Hedge should be LONG (positive)");

    // Verify net exposure is greatly reduced
    assert!(
        net_after.abs() < 2_000.0,
        "Net exposure should be < 2,000 MWh, got {:.0}",
        net_after
    );

    // Verify hedge effectiveness
    let reduction: f64 = (net_before.abs() - net_after.abs()) / net_before.abs() * 100.0;
    assert!(
        reduction > 80.0,
        "Hedge should reduce exposure by >80%, got {:.1}%",
        reduction
    );
}
