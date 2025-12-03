use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, criterion_group, criterion_main};
use hedging_engine::market_data::OrderBook;
use std::hint::black_box;

fn bench_orderbook_update(c: &mut Criterion) {
    let ob: OrderBook = OrderBook::new(1);

    c.bench_function("orderbook_single_update", |b| {
        b.iter(|| {
            ob.update_bid(
                black_box(0),
                black_box(450000),
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

    c.bench_function("orderbook_mid_price", |b| {
        b.iter(|| {
            black_box(ob.mid_price());
        });
    });
}

fn bench_orderbook_throughput(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("orderbook_throughput");

    for updates in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(updates),
            updates,
            |b, &updates| {
                let ob = OrderBook::new(1);
                b.iter(|| {
                    for i in 0..updates {
                        ob.update_bid(0, 450000 + i, 100, 1000 + i as u64);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_orderbook_update,
    bench_orderbook_read,
    bench_orderbook_throughput
);
criterion_main!(benches);
