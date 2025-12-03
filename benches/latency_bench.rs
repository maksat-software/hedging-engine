//! Latency benchmarks using Criterion

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, criterion_group, criterion_main};
use hedging_engine::*;
use std::hint::black_box;

fn bench_orderbook_update(c: &mut Criterion) {
    let ob: OrderBook = OrderBook::new(1);

    c.bench_function("orderbook_update_bid", |b| {
        b.iter(|| {
            ob.update_bid(
                black_box(0),
                black_box(450000),
                black_box(100),
                black_box(1000),
            );
        });
    });

    c.bench_function("orderbook_update_ask", |b| {
        b.iter(|| {
            ob.update_ask(
                black_box(0),
                black_box(460000),
                black_box(100),
                black_box(1000),
            );
        });
    });
}

fn bench_orderbook_read(c: &mut Criterion) {
    let ob: OrderBook = OrderBook::new(1);
    ob.update_bid(0, 450000, 100, 1000);
    ob.update_ask(0, 460000, 100, 1000);

    c.bench_function("orderbook_best_bid", |b| {
        b.iter(|| {
            black_box(ob.best_bid());
        });
    });

    c.bench_function("orderbook_best_ask", |b| {
        b.iter(|| {
            black_box(ob.best_ask());
        });
    });

    c.bench_function("orderbook_mid_price", |b| {
        b.iter(|| {
            black_box(ob.mid_price());
        });
    });

    c.bench_function("orderbook_spread_bps", |b| {
        b.iter(|| {
            black_box(ob.spread_bps());
        });
    });
}

fn bench_tick_processing(c: &mut Criterion) {
    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    c.bench_function("tick_processing", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let tick = MarketTick::bid(
                black_box(i),
                black_box(45.0 + (i % 100) as f64 * 0.01),
                black_box(100),
                black_box(1),
            );
            engine.on_tick(black_box(tick));
            i += 1;
        });
    });
}

fn bench_hedge_calculation(c: &mut Criterion) {
    let delta_hedge: DeltaHedge = hedging::DeltaHedge::new(-10_000.0, 1.125, 500);

    c.bench_function("hedge_calculation_delta", |b| {
        b.iter(|| {
            black_box(delta_hedge.calculate_hedge_delta());
        });
    });
}

fn bench_end_to_end(c: &mut Criterion) {
    let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
    let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

    // Warm up orderbooks
    engine.on_tick(MarketTick::bid(0, 45.0, 100, 1));
    engine.on_tick(MarketTick::ask(0, 50.0, 100, 2));

    c.bench_function("end_to_end_tick_to_decision", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let tick = MarketTick::bid(i, 45.0, 100, 1);
            engine.on_tick(black_box(tick));
            let _ = engine.get_hedge_recommendation().unwrap();
            i += 1;
        });
    });
}

fn bench_timestamp(c: &mut Criterion) {
    c.bench_function("timestamp_rdtsc", |b| {
        b.iter(|| {
            black_box(get_timestamp_ns());
        });
    });
}

fn bench_throughput(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("throughput");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let config: HedgeConfig = HedgeConfig::simple(-10_000.0, 1.125);
            let engine: HedgeEngine = HedgeEngine::new(config).unwrap();

            b.iter(|| {
                for i in 0..size {
                    let tick: MarketTick =
                        MarketTick::bid(i as u64, 45.0 + (i % 10) as f64 * 0.1, 100, 1);
                    engine.on_tick(black_box(tick));
                }
            });
        });
    }

    group.finish();
}

fn bench_mvhr(c: &mut Criterion) {
    let mvhr: MVHRStrategy = MVHRStrategy::new(720, 24);

    // Add some observations
    for i in 0..100 {
        mvhr.add_observation(45.0 + i as f64 * 0.1, 50.0 + i as f64 * 0.12);
    }

    c.bench_function("mvhr_get_ratio", |b| {
        b.iter(|| {
            black_box(mvhr.get_hedge_ratio());
        });
    });

    c.bench_function("mvhr_calculate_optimal", |b| {
        b.iter(|| {
            black_box(mvhr.calculate_optimal_ratio());
        });
    });
}

fn bench_mean_reversion(c: &mut Criterion) {
    let mr: MeanReversionHedge = MeanReversionHedge::new(720, 0.20, 2.0, 0.70);

    // Add price history
    for i in 0..100 {
        mr.add_price(45.0 + (i % 10) as f64 * 0.5);
    }
    mr.calculate_statistics();

    c.bench_function("mean_reversion_z_score", |b| {
        b.iter(|| {
            black_box(mr.calculate_z_score(black_box(48.5)));
        });
    });

    c.bench_function("mean_reversion_should_adjust", |b| {
        b.iter(|| {
            black_box(mr.should_adjust_hedge(black_box(48.5)));
        });
    });
}

criterion_group!(
    benches,
    bench_orderbook_update,
    bench_orderbook_read,
    bench_tick_processing,
    bench_hedge_calculation,
    bench_end_to_end,
    bench_timestamp,
    bench_throughput,
    bench_mvhr,
    bench_mean_reversion,
);

criterion_main!(benches);
