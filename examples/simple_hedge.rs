use hedging_engine::market_data::OrderBook;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn get_timestamp_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn main() {
    println!("=== Hedging Engine - Simple Example ===\n");

    // Creating order books for a spot and futures
    let spot_ob: Arc<OrderBook> = Arc::new(OrderBook::new(1)); // Symbol ID 1 = TTF Spot
    let futures_ob: Arc<OrderBook> = Arc::new(OrderBook::new(2)); // Symbol ID 2 = TTF Jan26

    println!("Initial state:");
    println!("Spot OrderBook:");
    println!("{}", spot_ob);
    println!("\nFutures OrderBook:");
    println!("{}", futures_ob);

    // simulate market data ticks
    println!("\n--- Receiving market data ---\n");

    let ts: u64 = get_timestamp_ns();

    // Spot market updates
    let spot_bids: Vec<(f64, u64)> = vec![(48.20, 150), (48.15, 200), (48.10, 180)];

    let spot_asks: Vec<(f64, u64)> = vec![(48.25, 130), (48.30, 220), (48.35, 190)];

    for (level, (price, size)) in spot_bids.iter().enumerate() {
        spot_ob.update_bid(level, (price * 10000.0) as i64, *size, ts);
        println!("Updated SPOT BID L{}: {} @ {}", level, price, size);
    }

    for (level, (price, size)) in spot_asks.iter().enumerate() {
        spot_ob.update_ask(level, (price * 10000.0) as i64, *size, ts);
        println!("Updated SPOT ASK L{}: {} @ {}", level, price, size);
    }

    // Futures market updates
    let futures_bids: Vec<(f64, u64)> = vec![(50.10, 120), (50.05, 180), (50.00, 160)];

    let futures_asks: Vec<(f64, u64)> = vec![(50.15, 140), (50.20, 200), (50.25, 170)];

    for (level, (price, size)) in futures_bids.iter().enumerate() {
        futures_ob.update_bid(level, (price * 10000.0) as i64, *size, ts);
        println!("Updated FUTURES BID L{}: {} @ {}", level, price, size);
    }

    for (level, (price, size)) in futures_asks.iter().enumerate() {
        futures_ob.update_ask(level, (price * 10000.0) as i64, *size, ts);
        println!("Updated FUTURES ASK L{}: {} @ {}", level, price, size);
    }

    // Display orderbooks
    println!("\n--- Current Market State ---\n");
    println!("SPOT Market:");
    println!("{}", spot_ob);
    println!("\nFUTURES Market:");
    println!("{}", futures_ob);

    // Calculate hedge parameters
    println!("\n--- Hedge Calculation ---\n");

    let physical_position: f64 = -10_000.0f64; // Short 10,000 MWh
    let hedge_ratio: f64 = 1.125f64;
    let required_futures: f64 = physical_position.abs() * hedge_ratio;

    println!("Physical Position: {:.0} MWh (SHORT)", physical_position);
    println!("Hedge Ratio: {:.3}", hedge_ratio);
    println!("Required Futures: {:.0} MWh (LONG)", required_futures);

    let (futures_price, futures_size): (f64, u64) = futures_ob.best_ask();
    let cost: f64 = required_futures * futures_price;

    println!("\nHedge Execution:");
    println!("  BUY {:.0} MWh", required_futures);
    println!("  @ €{:.2}/MWh", futures_price);
    println!("  Total Cost: €{:.2}", cost);
    println!("  Available Liquidity: {} MWh", futures_size);

    // Basis Risiko
    let spot_mid: f64 = spot_ob.mid_price();
    let futures_mid: f64 = futures_ob.mid_price();
    let basis: f64 = spot_mid - futures_mid;

    println!("\n--- Risk Metrics ---\n");
    println!("Spot Mid: €{:.2}", spot_mid);
    println!("Futures Mid: €{:.2}", futures_mid);
    println!("Basis: €{:.2} ({:.2}%)", basis, (basis / spot_mid) * 100.0);
    println!("Spot Spread: {:.2} bps", spot_ob.spread_bps());
    println!("Futures Spread: {:.2} bps", futures_ob.spread_bps());
}
