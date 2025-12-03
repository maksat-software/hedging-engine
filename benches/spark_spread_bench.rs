use criterion::{Criterion, criterion_group, criterion_main};
use hedging_engine::hedging::SparkSpreadHedge;
use std::hint::black_box;

fn bench_spark_spread(c: &mut Criterion) {
    let hedge: SparkSpreadHedge = SparkSpreadHedge::new(100.0, 2.0, 0.202, 50.0);

    c.bench_function("spark_spread_calculation", |b| {
        b.iter(|| hedge.calculate_spread(black_box(100.0), black_box(40.0), black_box(80.0)));
    });

    c.bench_function("costs_breakdown", |b| {
        b.iter(|| hedge.calculate_costs_breakdown(black_box(40.0), black_box(80.0)));
    });
}

criterion_group!(benches, bench_spark_spread);
criterion_main!(benches);
