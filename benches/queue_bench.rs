use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, criterion_group, criterion_main};
use hedging_engine::utils::LockFreeQueue;
use std::hint::black_box;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

fn bench_queue_operations(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("lockfree_queue");

    group.bench_function("push_pop_single_thread", |b| {
        let queue: LockFreeQueue<i64> = LockFreeQueue::<i64>::new(1024);

        b.iter(|| {
            queue.try_push(black_box(42)).unwrap();
            queue.try_pop().unwrap();
        });
    });

    group.bench_function("push_single_thread", |b| {
        let queue: LockFreeQueue<i64> = LockFreeQueue::<i64>::new(1024);
        let mut i = 0;

        b.iter(|| {
            if queue.try_push(black_box(i)).is_ok() {
                i += 1;
            }
            if queue.is_full() {
                while queue.try_pop().is_some() {}
                i = 0;
            }
        });
    });

    group.bench_function("spsc_threaded", |b| {
        b.iter(|| {
            let queue: Arc<LockFreeQueue<i64>> = Arc::new(LockFreeQueue::<i64>::new(1024));
            let producer_queue: Arc<LockFreeQueue<i64>> = Arc::clone(&queue);
            let consumer_queue: Arc<LockFreeQueue<i64>> = Arc::clone(&queue);

            let producer = thread::spawn(move || {
                for i in 0..1000 {
                    while producer_queue.try_push(i).is_err() {
                        std::hint::spin_loop();
                    }
                }
            });

            let consumer: JoinHandle<()> = thread::spawn(move || {
                let mut count = 0;
                while count < 1000 {
                    if consumer_queue.try_pop().is_some() {
                        count += 1;
                    }
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_queue_operations);
criterion_main!(benches);
