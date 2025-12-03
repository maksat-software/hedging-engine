//! Backtesting example with historical data

use hedging_engine::*;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    println!("=== Backtesting Example ===\n");

    // Load historical data
    println!("Loading historical data...");
    let ticks: Vec<MarketTick> = load_sample_data()?;
    println!("Loaded {} ticks\n", ticks.len());

    // Create engine
    let config = HedgeConfig {
        initial_position: -10_000.0,
        default_hedge_ratio: 1.125,
        enable_mvhr: true,
        enable_mean_reversion: false,
        ..Default::default()
    };

    let engine: HedgeEngine = HedgeEngine::new(config)?;

    println!("Configuration:");
    println!("  Initial Position: -10,000 MWh");
    println!("  Hedge Ratio: 1.125");
    println!("  MVHR: Enabled");
    println!("  Mean Reversion: Disabled\n");

    println!("Running backtest...\n");

    let start_time: Instant = Instant::now();
    let mut hedge_count: i32 = 0;
    let mut total_hedge_volume: f64 = 0.0;

    // Process ticks
    for (i, tick) in ticks.iter().enumerate() {
        engine.on_tick(*tick);

        // Check for hedge every 100 ticks
        if i % 100 == 0 {
            if let Some(rec) = engine.get_hedge_recommendation()? {
                hedge_count += 1;
                total_hedge_volume += rec.quantity;
                engine.execute_hedge(&rec)?;

                if hedge_count <= 5 {
                    println!(
                        "Hedge #{}: {} {:.0} MWh @ €{:.2}",
                        hedge_count,
                        match rec.side {
                            Side::Bid => "SELL",
                            Side::Ask => "BUY",
                        },
                        rec.quantity,
                        rec.price
                    );
                }
            }
        }

        // Progress indicator
        if i % 10000 == 0 && i > 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }

    let elapsed: Duration = start_time.elapsed();
    println!("\n\n✓ Backtest complete in {:.2}s", elapsed.as_secs_f64());

    // Results
    println!("\n{}", "═".repeat(60));

    println!("BACKTEST RESULTS");
    println!("\n{}", "═".repeat(60));

    let metrics: Metrics = engine.get_metrics();
    println!("Processing Statistics:");
    println!("  Total Ticks:        {}", ticks.len());
    println!(
        "  Ticks/Second:       {:.0}",
        ticks.len() as f64 / elapsed.as_secs_f64()
    );
    println!("  Hedges Executed:    {}", hedge_count);
    println!("  Total Hedge Volume: {:.0} MWh", total_hedge_volume);
    println!(
        "  Avg Hedge Size:     {:.0} MWh\n",
        total_hedge_volume / hedge_count as f64
    );

    println!("{}", metrics.summary());

    println!("\nFinal Position:");
    println!("  Physical:  {:.0} MWh", engine.get_position());
    println!("  Hedge:     {:.0} MWh", engine.get_hedge_position());
    println!(
        "  Net:       {:.0} MWh",
        engine.get_position() + engine.get_hedge_position()
    );

    // Calculate hedge effectiveness
    let hedge_ratio: f64 = engine.get_hedge_position().abs() / engine.get_position().abs();
    println!("\nHedge Effectiveness:");
    println!("  Actual Ratio:   {:.3}", hedge_ratio);
    println!("  Target Ratio:   1.125");
    println!(
        "  Deviation:      {:.1}%",
        (hedge_ratio - 1.125) / 1.125 * 100.0
    );

    Ok(())
}

/// Load sample data from CSV file
fn load_sample_data() -> Result<Vec<MarketTick>> {
    // Try to load from a file or generate if not found
    match load_from_csv("data/sample_ticks.csv") {
        Ok(ticks) => Ok(ticks),
        Err(_) => {
            println!("  Sample data file not found, generating synthetic data...");
            Ok(generate_synthetic_data(100_000))
        }
    }
}

fn load_from_csv(path: &str) -> Result<Vec<MarketTick>> {
    let file: File =
        File::open(path).map_err(|e| Error::MarketData(format!("Failed to open file: {}", e)))?;

    let reader: BufReader<File> = BufReader::new(file);
    let mut ticks: Vec<MarketTick> = Vec::new();

    for line in reader.lines().skip(1) {
        let line = line.map_err(|e| Error::MarketData(format!("Failed to read line: {}", e)))?;
        let parts: Vec<&str> = line.split(',').collect();

        if parts.len() < 5 {
            continue;
        }

        let timestamp: u64 = parts[0].parse().unwrap_or(0);
        let symbol_id: u8 = parts[1].parse().unwrap_or(1);
        let price: f64 = parts[2].parse().unwrap_or(45.0);
        let quantity: u32 = parts[3].parse().unwrap_or(100);

        let tick: MarketTick = if parts[4] == "bid" {
            MarketTick::bid(timestamp, price, quantity, symbol_id)
        } else {
            MarketTick::ask(timestamp, price, quantity, symbol_id)
        };

        ticks.push(tick);
    }

    Ok(ticks)
}

fn generate_synthetic_data(count: usize) -> Vec<MarketTick> {
    let mut ticks: Vec<MarketTick> = Vec::with_capacity(count);
    let mut spot_price: f64 = 45.0;
    let mut futures_price: f64 = 50.0;
    let mut ts: u64 = get_timestamp_ns();

    for i in 0..count {
        // Random walk
        spot_price += (i % 20) as f64 * 0.05 - 0.5;
        futures_price += (i % 18) as f64 * 0.06 - 0.54;

        // Alternate between a spot and futures
        let (price, symbol_id) = if i % 2 == 0 {
            (spot_price, 1)
        } else {
            (futures_price, 2)
        };

        let is_bid: bool = i % 2 == 0;
        let qty: u32 = 100 + (i % 50) as u32;

        let tick: MarketTick = if is_bid {
            MarketTick::bid(ts, price, qty, symbol_id)
        } else {
            MarketTick::ask(ts, price, qty, symbol_id)
        };

        ticks.push(tick);

        ts += 1000; // 1 microsecond apart
    }

    ticks
}
