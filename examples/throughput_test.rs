use hedging_engine::utils::MetricsSummary;
use hedging_engine::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

fn main() {
    println!("╔════════════════════════════════════════════════╗");
    println!("║     THROUGHPUT STRESS TEST                     ║");
    println!("╚════════════════════════════════════════════════╝\n");

    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: Arc<HedgeEngine> = Arc::new(HedgeEngine::new(config).unwrap());

    let tick_counter: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let running: Arc<AtomicU64> = Arc::new(AtomicU64::new(1));

    println!("Configuration:");
    println!("  Threads: 4 (2 spot, 2 futures)");
    println!("  Duration: 10 seconds");
    println!("  Target: 100,000 ticks/second\n");

    println!("Starting stress test...\n");

    let start_time: Instant = Instant::now();

    // Spawn producer threads
    let mut handles: Vec<JoinHandle<()>> = vec![];

    // 2 threads for the spot market
    for _thread_id in 0..5 {
        let engine = Arc::clone(&engine);
        let counter = Arc::clone(&tick_counter);
        let running = Arc::clone(&running);

        let handle = thread::spawn(move || {
            let mut local_count: u64 = 0u64;
            let mut price: f64 = 45.0;

            while running.load(Ordering::Relaxed) == 1 {
                // Generate tick
                price += (local_count % 20) as f64 * 0.01 - 0.1;

                let tick: MarketTick = MarketTick::bid(
                    local_count,
                    price,
                    100 + (local_count % 50) as u32,
                    1, // Spot
                );

                engine.on_tick(tick);

                local_count += 1;

                // Report every 10k ticks
                if local_count % 10_000 == 0 {
                    counter.fetch_add(10_000, Ordering::Relaxed);
                }
            }

            counter.fetch_add(local_count % 10_000, Ordering::Relaxed);
        });

        handles.push(handle);
    }

    // 2 threads for the futures market
    for _thread_id in 0..2 {
        let engine = Arc::clone(&engine);
        let counter = Arc::clone(&tick_counter);
        let running = Arc::clone(&running);

        let handle: JoinHandle<()> = thread::spawn(move || {
            let mut local_count = 0u64;
            let mut price: f64 = 50.0;

            while running.load(Ordering::Relaxed) == 1 {
                price += (local_count % 18) as f64 * 0.012 - 0.11;

                let tick: MarketTick = MarketTick::ask(
                    local_count,
                    price,
                    110 + (local_count % 40) as u32,
                    2, // Futures
                );

                engine.on_tick(tick);
                local_count += 1;

                if local_count % 10_000 == 0 {
                    counter.fetch_add(10_000, Ordering::Relaxed);
                }
            }

            counter.fetch_add(local_count % 10_000, Ordering::Relaxed);
        });

        handles.push(handle);
    }

    // Monitor thread
    let monitor_counter: Arc<AtomicU64> = Arc::clone(&tick_counter);
    thread::spawn(move || {
        let mut last_count: u64 = 0u64;
        let mut last_time: Instant = Instant::now();

        loop {
            thread::sleep(Duration::from_secs(1));

            let current_count: u64 = monitor_counter.load(Ordering::Relaxed);
            let current_time: Instant = Instant::now();

            let delta_count: u64 = current_count - last_count;
            let delta_time: Duration = current_time.duration_since(last_time);

            let tps: f64 = delta_count as f64 / delta_time.as_secs_f64();

            println!("Ticks/Second: {:>10.0} | Total: {:>12}", tps, current_count);

            last_count = current_count;
            last_time = current_time;

            if current_time.duration_since(Instant::now()) > Duration::from_secs(10) {
                break;
            }
        }
    });

    // Run for 10 seconds
    thread::sleep(Duration::from_secs(10));

    // Stop all threads
    running.store(0, Ordering::Relaxed);

    // Wait for threads
    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed: Duration = start_time.elapsed();
    let total_ticks: u64 = tick_counter.load(Ordering::Relaxed);
    let tps: f64 = total_ticks as f64 / elapsed.as_secs_f64();

    println!("\n{}", "═".repeat(60));
    println!("RESULTS");
    println!("\n{}", "═".repeat(60));
    println!("Total Ticks:     {:>12}", total_ticks);
    println!("Duration:        {:>12.2} seconds", elapsed.as_secs_f64());
    println!("Throughput:      {:>12.0} ticks/second", tps);
    println!("Avg Latency:     {:>12.0} ns", 1_000_000_000.0 / tps);

    // Get metrics
    let metrics: Metrics = engine.get_metrics();
    let summary: MetricsSummary = metrics.summary();

    println!("\nEngine Metrics:");
    println!("  Ticks Processed: {}", summary.ticks_processed);
    println!("  Avg Latency:     {} ns", summary.avg_latency_ns);
    println!("  P99 Latency:     {} ns", summary.p99_latency_ns);

    if tps >= 100_000.0 {
        println!("\nSUCCESS: Achieved 100k+ ticks/second!");
    } else {
        println!("\n  Target not met. Achieved {:.0} ticks/second", tps);
        println!("   Recommendations:");
        println!("   1. Enable CPU isolation");
        println!("   2. Use RT kernel");
        println!("   3. Pin threads to specific CPUs");
    }
}
