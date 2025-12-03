//! Hedging engine binary

use hedging_engine::*;
use std::io::{self, Write};

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("╔════════════════════════════════════════════════╗");
    println!("║   Rust Low-Latency Hedging Engine v0.1.0       ║");
    println!("╚════════════════════════════════════════════════╝\n");

    // Get user input
    print!("Enter initial position (MWh, negative for short): ");
    io::stdout().flush().unwrap();

    let mut input: String = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let position: f64 = input.trim().parse().unwrap_or(-10_000.0);

    print!("Enter hedge ratio (e.g., 1.125): ");
    io::stdout().flush().unwrap();

    let mut input: String = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let ratio: f64 = input.trim().parse().unwrap_or(1.125);

    // Create engine
    let config: HedgeConfig = HedgeConfig::simple(position, ratio);
    let engine: HedgeEngine = HedgeEngine::new(config)?;

    println!("\n✓ Engine initialized");
    println!("  Position: {:.0} MWh", position);
    println!("  Hedge Ratio: {:.3}", ratio);
    println!("\nSimulating market data...\n");

    // Simulate market data
    let mut spot_price = 45.0;
    let mut futures_price = 50.0;

    for i in 0..100 {
        // Simulate price movements
        spot_price += (i % 10) as f64 * 0.1 - 0.5;
        futures_price += (i % 8) as f64 * 0.12 - 0.48;

        // Send ticks
        let spot_tick: MarketTick = MarketTick::bid(utils::get_timestamp_ns(), spot_price, 100, 1);
        engine.on_tick(spot_tick);

        let futures_tick: MarketTick =
            MarketTick::ask(utils::get_timestamp_ns(), futures_price, 120, 2);
        engine.on_tick(futures_tick);

        // Check for hedge every 10 ticks
        if i % 10 == 0
            && let Some(rec) = engine.get_hedge_recommendation()?
        {
            println!("HEDGE RECOMMENDATION:");
            println!(
                "   {} {:.0} MWh @ €{:.2}",
                match rec.side {
                    Side::Bid => "SELL",
                    Side::Ask => "BUY",
                },
                rec.quantity,
                rec.price
            );
            println!("   Reason: {}", rec.reason);
            println!("   Urgency: {:?}\n", rec.urgency);

            engine.execute_hedge(&rec)?;
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Print final statistics
    println!("\n{}", "═".repeat(50));
    println!("FINAL STATISTICS");
    println!("\n{}", "═".repeat(50));

    let metrics: Metrics = engine.get_metrics();
    println!("{}", metrics.summary());

    println!("\nPosition Summary:");
    println!("  Physical Position:  {:.0} MWh", engine.get_position());
    println!(
        "  Hedge Position:     {:.0} MWh",
        engine.get_hedge_position()
    );
    println!(
        "  Net Exposure:       {:.0} MWh",
        engine.get_position() + engine.get_hedge_position()
    );

    println!("\nSimulation complete");

    Ok(())
}
