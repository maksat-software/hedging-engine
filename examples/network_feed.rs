//! Example: TCP market data feed

use hedging_engine::network::TcpMarketDataFeed;
use hedging_engine::*;

fn main() -> Result<()> {
    println!("╔════════════════════════════════════════════════╗");
    println!("║     TCP MARKET DATA FEED EXAMPLE               ║");
    println!("╚════════════════════════════════════════════════╝\n");

    // Note: This requires a running market data server
    // For testing, you can use: nc -l 5555

    let engine_config = HedgeConfig::simple(-10_000.0, 1.125);
    let engine = HedgeEngine::new(engine_config)?;

    println!("Connecting to market data feed...");

    match TcpMarketDataFeed::connect("127.0.0.1:5555") {
        Ok(mut feed) => {
            println!("✓ Connected!\n");
            println!("Receiving market data...\n");

            let mut tick_count = 0;

            loop {
                match feed.read_tick()? {
                    Some(tick) => {
                        engine.on_tick(tick);
                        tick_count += 1;

                        if tick_count % 100 == 0 {
                            println!("Processed {} ticks", tick_count);

                            if let Some(rec) = engine.get_hedge_recommendation()? {
                                println!(
                                    "  HEDGE: {} {:.0} MWh @ €{:.2}",
                                    match rec.side {
                                        Side::Bid => "SELL",
                                        Side::Ask => "BUY",
                                    },
                                    rec.quantity,
                                    rec.price
                                );
                            }
                        }

                        if tick_count >= 1000 {
                            break;
                        }
                    }
                    None => {
                        // No data available, continue
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                }
            }

            println!("\n✓ Processed {} ticks", tick_count);
        }
        Err(e) => {
            println!("✗ Failed to connect: {}", e);
            println!("\nTo test this example:");
            println!("1. Start a TCP server: nc -l 5555");
            println!("2. Run this example in another terminal");
        }
    }

    Ok(())
}
