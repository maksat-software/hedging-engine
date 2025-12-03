use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, Throughput, criterion_group, criterion_main};
use hedging_engine::*;
use std::hint::black_box;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn stress_test_throughput(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("stress_throughput");

    // Test with different thread counts
    for thread_count in [1, 2, 4, 8].iter() {
        group.throughput(Throughput::Elements(100_000));

        group.bench_with_input(
            format!("{}_threads", thread_count),
            thread_count,
            |b, &threads| {
                b.iter_custom(|iters| {
                    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
                    let engine: Arc<HedgeEngine> = Arc::new(HedgeEngine::new(config).unwrap());

                    let start: Instant = Instant::now();

                    let handles: Vec<_> = (0..threads)
                        .map(|thread_id| {
                            let engine = Arc::clone(&engine);
                            thread::spawn(move || {
                                let ticks_per_thread = (iters as usize) / threads;
                                for i in 0..ticks_per_thread {
                                    let tick = MarketTick::bid(
                                        i as u64,
                                        45.0 + (i % 100) as f64 * 0.01,
                                        100,
                                        (thread_id % 2 + 1) as u8,
                                    );
                                    engine.on_tick(black_box(tick));
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

fn sustained_throughput_test(c: &mut Criterion) {
    c.bench_function("sustained_1sec_100k_ticks", |b| {
        let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
        let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

        b.iter(|| {
            let start: Instant = Instant::now();
            let target_duration: Duration = Duration::from_secs(1);
            let mut count: u64 = 0u64;

            while start.elapsed() < target_duration {
                let tick: MarketTick =
                    MarketTick::bid(count, 45.0 + (count % 100) as f64 * 0.01, 100, 1);
                engine.on_tick(black_box(tick));
                count += 1;
            }

            println!("Processed {} ticks in 1 second", count);
            count
        });
    });
}

criterion_group!(benches, stress_test_throughput, sustained_throughput_test);
criterion_main!(benches);
