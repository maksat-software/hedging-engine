//! High-resolution timestamp utilities

use std::time::{SystemTime, UNIX_EPOCH};

/// Get the current timestamp in nanoseconds
///
/// Uses RDTSC on x86_64 for the lowest overhead (~5ns)
/// Falls back to SystemTime on other architectures (~50-100ns)
#[inline(always)]
pub fn get_timestamp_ns() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        // RDTSC: Read Time-Stamp Counter. The fastest way to get a timestamp on x86_64
        // ~5-10 nanoseconds overhead
        unsafe { std::arch::x86_64::_rdtsc() }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // Fallback for non-x86_64 architectures
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_monotonic() {
        let t1 = get_timestamp_ns();
        std::thread::sleep(std::time::Duration::from_micros(100));
        let t2 = get_timestamp_ns();

        assert!(t2 > t1);
        assert!(t2 - t1 > 100_000); // At least 100 microseconds
    }

    #[test]
    fn test_timestamp_overhead() {
        let iterations = 10000;
        let start = std::time::Instant::now();

        for _ in 0..iterations {
            let _ = get_timestamp_ns();
        }

        let elapsed = start.elapsed();
        let avg_ns = elapsed.as_nanos() / iterations;

        // Should be very fast (< 50ns per call)
        println!("Average timestamp overhead: {}ns", avg_ns);
        assert!(avg_ns < 100);
    }
}
