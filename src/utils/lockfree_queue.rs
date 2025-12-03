//! Lock-free queue implementation for inter-thread communication
//!
//! This is a Single-Producer, Single-Consumer (SPSC) queue optimized
//! for low-latency message passing between threads.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Cache-line size (64 bytes on most modern CPUs)
const _CACHE_LINE_SIZE: usize = 64;

/// Padding to prevent false sharing
#[repr(align(64))]
struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    fn new(value: T) -> Self {
        Self { value }
    }
}

/// Lock-free SPSC (Single Producer, Single Consumer) queue
///
/// # Performance
/// - Enqueue: ~20-30ns
/// - Dequeue: ~20-30ns
/// - Zero allocations after initialization
/// - Wait-free for single producer/consumer
///
/// # Example
/// ```
/// use hedging_engine::utils::LockFreeQueue;
///
/// let queue = LockFreeQueue::<i32>::new(1024);
///
/// // Producer thread
/// queue.try_push(42).unwrap();
///
/// // Consumer thread
/// if let Some(value) = queue.try_pop() {
///     println!("Got: {}", value);
/// }
/// ```
pub struct LockFreeQueue<T> {
    /// Ring buffer
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,

    /// Capacity (power of 2)
    capacity: usize,

    /// Mask for fast modulo (capacity - 1)
    mask: usize,

    /// Head index (consumer reads from here)
    /// Cache-line padded to prevent false sharing with tail
    head: CachePadded<AtomicUsize>,

    /// Tail index (producer writes here)
    /// Cache-line padded to prevent false sharing with head
    tail: CachePadded<AtomicUsize>,
}

unsafe impl<T: Send> Send for LockFreeQueue<T> {}
unsafe impl<T: Send> Sync for LockFreeQueue<T> {}

impl<T> LockFreeQueue<T> {
    /// Create a new lock-free queue with a given capacity
    ///
    /// # Panics
    /// Panics if capacity is not a power of 2
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be a power of 2");

        // Allocate buffer
        let buffer: Vec<UnsafeCell<MaybeUninit<T>>> = (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();

        Self {
            buffer: buffer.into_boxed_slice(),
            capacity,
            mask: capacity - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    /// Try to push an item to the queue
    ///
    /// Returns `Ok(())` if successful, `Err(item)` if queue is full
    ///
    /// # Performance
    /// ~20-30ns in uncontended case
    #[inline]
    pub fn try_push(&self, item: T) -> Result<(), T> {
        let tail: usize = self.tail.value.load(Ordering::Relaxed);
        let next_tail: usize = (tail + 1) & self.mask;
        let head: usize = self.head.value.load(Ordering::Acquire);

        if next_tail == head {
            // Queue is full
            return Err(item);
        }

        // Safe: we have exclusive access to this slot
        unsafe {
            (*self.buffer[tail].get()).write(item);
        }

        // Make item visible to consumer
        self.tail.value.store(next_tail, Ordering::Release);

        Ok(())
    }

    /// Try to pop an item from the queue
    ///
    /// Returns `Some(item)` if successful, `None` if queue is empty
    ///
    /// Performance
    /// ~20-30ns in uncontended case
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        let head: usize = self.head.value.load(Ordering::Relaxed);
        let tail: usize = self.tail.value.load(Ordering::Acquire);

        if head == tail {
            // Queue is empty
            return None;
        }

        // Safe: we have exclusive access to this slot
        let item: T = unsafe { (*self.buffer[head].get()).assume_init_read() };

        let next_head = (head + 1) & self.mask;
        self.head.value.store(next_head, Ordering::Release);

        Some(item)
    }

    /// Check if queue is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        let head: usize = self.head.value.load(Ordering::Relaxed);
        let tail: usize = self.tail.value.load(Ordering::Acquire);
        head == tail
    }

    /// Check if the queue is full
    #[inline]
    pub fn is_full(&self) -> bool {
        let tail: usize = self.tail.value.load(Ordering::Relaxed);
        let next_tail: usize = (tail + 1) & self.mask;
        let head: usize = self.head.value.load(Ordering::Acquire);
        next_tail == head
    }

    /// Get an approximate number of items in the queue.
    ///
    /// Note: This is an estimate and may not be exact due to concurrent access
    pub fn len(&self) -> usize {
        let tail: usize = self.tail.value.load(Ordering::Relaxed);
        let head: usize = self.head.value.load(Ordering::Relaxed);

        if tail >= head {
            tail - head
        } else {
            self.capacity - head + tail
        }
    }

    /// Get capacity of the queue
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Drop for LockFreeQueue<T> {
    fn drop(&mut self) {
        // Drop all remaining items
        while self.try_pop().is_some() {
            // Items are dropped automatically
        }
    }
}

/// Multi-Producer, Single-Consumer (MPSC) queue
///
/// Uses atomic operations for thread-safe enqueueing from multiple threads
pub struct MPSCQueue<T> {
    inner: LockFreeQueue<T>,
    /// Atomic flag for producer synchronization
    enqueue_lock: AtomicU64,
}

impl<T> MPSCQueue<T> {
    /// Create new MPSC queue
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: LockFreeQueue::new(capacity),
            enqueue_lock: AtomicU64::new(0),
        }
    }

    /// Try to push an item (thread-safe for multiple producers)
    pub fn try_push(&self, item: T) -> Result<(), T> {
        // Simple spinlock for multiple producers
        loop {
            if self
                .enqueue_lock
                .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                let result = self.inner.try_push(item);
                self.enqueue_lock.store(0, Ordering::Release);
                return result;
            }

            // Yield to other threads
            std::hint::spin_loop();
        }
    }

    /// Try to pop an item (only one consumer allowed)
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.inner.try_pop()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::thread::JoinHandle;

    #[test]
    fn test_basic_operations() {
        let queue: LockFreeQueue<i32> = LockFreeQueue::<i32>::new(4);

        assert!(queue.is_empty());
        assert!(!queue.is_full());

        // Push items
        assert!(queue.try_push(1).is_ok());
        assert!(queue.try_push(2).is_ok());
        assert!(queue.try_push(3).is_ok());

        assert!(!queue.is_empty());
        assert!(queue.is_full());

        // Try to push when full
        assert!(queue.try_push(4).is_err());

        // Pop items
        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), Some(3));
        assert_eq!(queue.try_pop(), None);

        assert!(queue.is_empty());
    }

    #[test]
    fn test_spsc_threaded() {
        let queue: Arc<LockFreeQueue<i32>> = Arc::new(LockFreeQueue::<i32>::new(1024));
        let queue_clone: Arc<LockFreeQueue<i32>> = Arc::clone(&queue);

        // Producer thread
        let producer: JoinHandle<()> = thread::spawn(move || {
            for i in 0..10000 {
                while queue_clone.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });

        // Consumer thread
        let consumer: JoinHandle<Vec<i32>> = thread::spawn(move || {
            let mut received: Vec<i32> = Vec::new();
            while received.len() < 10000 {
                if let Some(item) = queue.try_pop() {
                    received.push(item);
                }
            }
            received
        });

        producer.join().unwrap();
        let received: Vec<i32> = consumer.join().unwrap();

        assert_eq!(received.len(), 10000);
        for (i, &val) in received.iter().enumerate() {
            assert_eq!(val, i as i32);
        }
    }

    #[test]
    fn test_mpsc_threaded() {
        let queue: Arc<MPSCQueue<i32>> = Arc::new(MPSCQueue::<i32>::new(1024));

        // Multiple producer threads
        let mut producers: Vec<JoinHandle<()>> = vec![];
        for thread_id in 0..4 {
            let queue: Arc<MPSCQueue<i32>> = Arc::clone(&queue);
            let handle: JoinHandle<()> = thread::spawn(move || {
                for i in 0..1000 {
                    let value = thread_id * 1000 + i;
                    while queue.try_push(value).is_err() {
                        std::hint::spin_loop();
                    }
                }
            });
            producers.push(handle);
        }

        // Single consumer thread
        let queue_clone: Arc<MPSCQueue<i32>> = Arc::clone(&queue);
        let consumer: JoinHandle<Vec<i32>> = thread::spawn(move || {
            let mut received: Vec<i32> = Vec::new();
            while received.len() < 4000 {
                if let Some(item) = queue_clone.try_pop() {
                    received.push(item);
                }
            }
            received
        });

        for handle in producers {
            handle.join().unwrap();
        }

        let mut received: Vec<i32> = consumer.join().unwrap();
        received.sort();

        assert_eq!(received.len(), 4000);
    }

    #[test]
    fn test_queue_size() {
        let queue: LockFreeQueue<i32> = LockFreeQueue::<i32>::new(8);

        assert_eq!(queue.len(), 0);

        queue.try_push(1).unwrap();
        queue.try_push(2).unwrap();
        assert_eq!(queue.len(), 2);

        queue.try_pop();
        assert_eq!(queue.len(), 1);
    }
}
