//! Live demo with real-time visualization

use hedging_engine::utils::MetricsSummary;
use hedging_engine::*;
use std::io::{self, Write};
use std::time::Duration;

fn main() -> Result<()> {
    clear_screen();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║         LIVE HEDGING DEMO - Real-Time Simulation           ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Setup
    let config = HedgeConfig {
        initial_position: -10_000.0,
        default_hedge_ratio: 1.125,
        rehedge_threshold_bps: 500,
        enable_mvhr: true,
        enable_mean_reversion: true,
        ..Default::default()
    };

    let engine: HedgeEngine = HedgeEngine::new(config)?;

    println!("Configuration:");
    println!("  Position: -10,000 MWh (SHORT)");
    println!("  Hedge Ratio: 1.125");
    println!("  Strategies: Delta + MVHR + Mean Reversion");
    println!("\nPress Ctrl+C to stop\n");

    std::thread::sleep(Duration::from_secs(2));

    // Simulation state
    let mut spot_price: f64 = 48.0;
    let mut futures_price: f64 = 50.0;
    let mut iteration: usize = 0;
    let mut last_hedge_iter: usize = 0;

    loop {
        iteration += 1;

        // Simulate market movement
        let spot_delta: f64 = simulate_price_change(iteration, 0.15);
        let futures_delta: f64 = simulate_price_change(iteration + 1, 0.18);

        spot_price = (spot_price + spot_delta).max(30.0).min(70.0);
        futures_price = (futures_price + futures_delta).max(35.0).min(75.0);

        // Send market data
        let ts: u64 = get_timestamp_ns();

        engine.on_tick(MarketTick::bid(
            ts,
            spot_price,
            100 + (iteration % 50) as u32,
            1,
        ));
        engine.on_tick(MarketTick::ask(ts, spot_price + 0.05, 120, 1));
        engine.on_tick(MarketTick::bid(ts, futures_price, 110, 2));
        engine.on_tick(MarketTick::ask(ts, futures_price + 0.05, 130, 2));

        // Display every 10 iterations
        if iteration % 10 == 0 {
            clear_screen();
            display_dashboard(&engine, iteration, spot_price, futures_price)?;

            // Check for hedge
            if let Some(rec) = engine.get_hedge_recommendation()? {
                display_hedge_alert(&rec);
                engine.execute_hedge(&rec)?;
                last_hedge_iter = iteration;
            } else if iteration - last_hedge_iter < 50 {
                println!(
                    "\n✓ Last hedge: {} iterations ago",
                    iteration - last_hedge_iter
                );
            }
        }

        std::thread::sleep(Duration::from_millis(100));

        // Stop after 1000 iterations
        if iteration >= 1000 {
            break;
        }
    }

    // Final summary
    clear_screen();
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                    SIMULATION COMPLETE                     ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!("\n");

    let metrics: Metrics = engine.get_metrics();
    println!("{}", metrics.summary());

    println!("\nFinal Position:");
    println!("  Physical:    {:>10.0} MWh", engine.get_position());
    println!("  Hedge:       {:>10.0} MWh", engine.get_hedge_position());
    println!(
        "  Net:         {:>10.0} MWh",
        engine.get_position() + engine.get_hedge_position()
    );

    let hedge_pct: f64 = (engine.get_hedge_position().abs() / engine.get_position().abs()) * 100.0;
    println!("  Hedged:      {:>10.1}%", hedge_pct);

    Ok(())
}

fn display_dashboard(
    engine: &HedgeEngine,
    iteration: usize,
    spot_price: f64,
    futures_price: f64,
) -> Result<()> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║              LIVE HEDGING DASHBOARD                        ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!(
        "║  Iteration: {:>6}                                          ║",
        iteration
    );
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Market prices
    println!("┌─ MARKET PRICES ─────────────────────────────────────────┐");
    println!(
        "│  Spot:     €{:>7.2}/MWh  {}                    │",
        spot_price,
        price_indicator(spot_price, 48.0)
    );
    println!(
        "│  Futures:  €{:>7.2}/MWh  {}                    │",
        futures_price,
        price_indicator(futures_price, 50.0)
    );

    let basis = spot_price - futures_price;
    println!(
        "│  Basis:    €{:>7.2}/MWh                              │",
        basis
    );
    println!("└─────────────────────────────────────────────────────────┘\n");

    // Orderbooks (compact)
    println!("┌─ ORDER BOOKS ───────────────────────────────────────────┐");
    let (spot_bid, spot_bid_size): (f64, u64) = engine.spot_orderbook().best_bid();
    let (spot_ask, spot_ask_size): (f64, u64) = engine.spot_orderbook().best_ask();
    let (fut_bid, fut_bid_size): (f64, u64) = engine.futures_orderbook().best_bid();
    let (fut_ask, fut_ask_size): (f64, u64) = engine.futures_orderbook().best_ask();

    println!(
        "│  Spot:    {:>7.2} x {:>4}  |  {:>4} x {:<7.2}     │",
        spot_bid, spot_bid_size, spot_ask_size, spot_ask
    );
    println!(
        "│  Futures: {:>7.2} x {:>4}  |  {:>4} x {:<7.2}     │",
        fut_bid, fut_bid_size, fut_ask_size, fut_ask
    );
    println!("└─────────────────────────────────────────────────────────┘\n");

    // Position
    let position: f64 = engine.get_position();
    let hedge_pos: f64 = engine.get_hedge_position();
    let net_exposure: f64 = position + hedge_pos;
    let hedge_pct: f64 = if position != 0.0 {
        (hedge_pos.abs() / position.abs()) * 100.0
    } else {
        0.0
    };

    println!("┌─ POSITION ──────────────────────────────────────────────┐");
    println!(
        "│  Physical:   {:>10.0} MWh                          │",
        position
    );
    println!(
        "│  Hedge:      {:>10.0} MWh  ({:>5.1}% hedged)        │",
        hedge_pos, hedge_pct
    );
    println!(
        "│  Net:        {:>10.0} MWh                          │",
        net_exposure
    );
    println!("└─────────────────────────────────────────────────────────┘\n");

    // Performance
    let metrics: Metrics = engine.get_metrics();
    let summary: MetricsSummary = metrics.summary();

    println!("┌─ PERFORMANCE ───────────────────────────────────────────┐");
    println!(
        "│  Ticks:      {:>10}                                │",
        summary.ticks_processed
    );
    println!(
        "│  Hedges:     {:>10}                                │",
        summary.hedges_executed
    );
    println!(
        "│  Avg Latency: {:>8} ns  ({:>6.3} μs)             │",
        summary.avg_latency_ns,
        summary.avg_latency_ns as f64 / 1000.0
    );
    println!(
        "│  P99 Latency: {:>8} ns  ({:>6.3} μs)             │",
        summary.p99_latency_ns,
        summary.p99_latency_ns as f64 / 1000.0
    );
    println!("└─────────────────────────────────────────────────────────┘");

    Ok(())
}

fn display_hedge_alert(rec: &HedgeRecommendation) -> String {
    let side = match rec.side {
        Side::Bid => "SELL",
        Side::Ask => "BUY ",
    };

    format!(
        "
╔════════════════════════════════════════════════════════════╗
║                       HEDGE ALERT                          ║
╠════════════════════════════════════════════════════════════╣
║  Action:   {} {:>8.0} MWh                                  ║
║  Price:    € {:>8.2}/MWh                                   ║
║  Urgency:  {:?}                                            ║
╚════════════════════════════════════════════════════════════╝
",
        side, rec.quantity, rec.price, rec.urgency
    )
}

fn simulate_price_change(seed: usize, volatility: f64) -> f64 {
    // Simple pseudo-random price change
    let x: f64 = (seed as f64 * 1234.5678).sin();
    x * volatility
}

fn price_indicator(current: f64, reference: f64) -> &'static str {
    let diff: f64 = current - reference;
    if diff > 0.5 {
        "↑↑"
    } else if diff > 0.1 {
        "↑ "
    } else if diff < -0.5 {
        "↓↓"
    } else if diff < -0.1 {
        "↓ "
    } else {
        "→ "
    }
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush().unwrap();
}
