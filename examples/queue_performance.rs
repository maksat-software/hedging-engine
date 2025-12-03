//! Lock-free queue performance test

use hedging_engine::utils::LockFreeQueue;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

fn main() {
    println!("╔════════════════════════════════════════════════╗");
    println!("║   LOCK-FREE QUEUE PERFORMANCE TEST             ║");
    println!("╚════════════════════════════════════════════════╝\n");

    let queue: Arc<LockFreeQueue<i64>> = Arc::new(LockFreeQueue::<i64>::new(1024));

    let producer_queue: Arc<LockFreeQueue<i64>> = Arc::clone(&queue);
    let consumer_queue: Arc<LockFreeQueue<i64>> = Arc::clone(&queue);

    let count = 1_000_000;

    // Producer thread
    let producer: JoinHandle<()> = thread::spawn(move || {
        let start: Instant = Instant::now();

        for i in 0..count {
            while producer_queue.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }

        let elapsed: Duration = start.elapsed();
        let ops_per_sec: f64 = count as f64 / elapsed.as_secs_f64();
        let ns_per_op: f64 = elapsed.as_nanos() as f64 / count as f64;

        println!("Producer:");
        println!("  Total ops:   {}", count);
        println!("  Duration:    {:?}", elapsed);
        println!("  Ops/sec:     {:.0}", ops_per_sec);
        println!("  Ns/op:       {:.0}", ns_per_op);
    });

    // Consumer thread
    let consumer: JoinHandle<()> = thread::spawn(move || {
        let start: Instant = Instant::now();
        let mut received: i64 = 0;

        while received < count {
            if let Some(_value) = consumer_queue.try_pop() {
                received += 1;
            }
        }

        let elapsed: Duration = start.elapsed();
        let ops_per_sec: f64 = count as f64 / elapsed.as_secs_f64();
        let ns_per_op: f64 = elapsed.as_nanos() as f64 / count as f64;

        println!("\nConsumer:");
        println!("  Total ops:   {}", count);
        println!("  Duration:    {:?}", elapsed);
        println!("  Ops/sec:     {:.0}", ops_per_sec);
        println!("  Ns/op:       {:.0}", ns_per_op);
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    println!("\nTest completed successfully!");
}
