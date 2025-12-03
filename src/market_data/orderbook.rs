use std::fmt;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Cache-line padded atomic value to prevent false sharing
#[repr(align(64))]
struct CacheLinePadded<T> {
    value: T,
}

impl<T> CacheLinePadded<T> {
    fn new(value: T) -> Self {
        Self { value }
    }
}

/// Lock-free OrderBook for low-latency trading
///
/// Stores top 10 levels for each side using atomic operations.
/// All operations are wait-free for a single writer, a single reader.
#[repr(align(64))]
pub struct OrderBook {
    /// Top 10 bid prices (fixed-point)
    bids: [CacheLinePadded<AtomicI64>; 10],

    /// Top 10 ask prices (fixed-point)
    asks: [CacheLinePadded<AtomicI64>; 10],

    /// Bid sizes
    bid_sizes: [CacheLinePadded<AtomicU64>; 10],

    /// Ask sizes
    ask_sizes: [CacheLinePadded<AtomicU64>; 10],

    /// Timestamp of last update (nanoseconds)
    last_update_ns: CacheLinePadded<AtomicU64>,

    /// Monotonic sequence number
    sequence: CacheLinePadded<AtomicU64>,

    /// Symbol identifier
    symbol_id: u8,
}

impl OrderBook {
    /// Create a new OrderBook
    pub fn new(symbol_id: u8) -> Self {
        Self {
            bids: std::array::from_fn(|_| CacheLinePadded::new(AtomicI64::new(0))),
            asks: std::array::from_fn(|_| CacheLinePadded::new(AtomicI64::new(0))),
            bid_sizes: std::array::from_fn(|_| CacheLinePadded::new(AtomicU64::new(0))),
            ask_sizes: std::array::from_fn(|_| CacheLinePadded::new(AtomicU64::new(0))),
            last_update_ns: CacheLinePadded::new(AtomicU64::new(0)),
            sequence: CacheLinePadded::new(AtomicU64::new(0)),
            symbol_id,
        }
    }

    /// Update a bid level (lock-free)
    ///
    /// # Performance
    /// ~50-60ns on modern hardware
    #[inline(always)]
    pub fn update_bid(&self, level: usize, price: i64, size: u64, timestamp_ns: u64) {
        if level < 10 {
            self.bids[level].value.store(price, Ordering::Release);
            self.bid_sizes[level].value.store(size, Ordering::Release);
            self.last_update_ns
                .value
                .store(timestamp_ns, Ordering::Release);
            self.sequence.value.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Update an ask level (lock-free)
    #[inline(always)]
    pub fn update_ask(&self, level: usize, price: i64, size: u64, timestamp_ns: u64) {
        if level < 10 {
            self.asks[level].value.store(price, Ordering::Release);
            self.ask_sizes[level].value.store(size, Ordering::Release);
            self.last_update_ns
                .value
                .store(timestamp_ns, Ordering::Release);
            self.sequence.value.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Get the best bid (level 0)
    ///
    /// # Performance
    /// ~8-10ns (just atomic load)
    #[inline(always)]
    pub fn best_bid(&self) -> (f64, u64) {
        let price = self.bids[0].value.load(Ordering::Acquire);
        let size = self.bid_sizes[0].value.load(Ordering::Acquire);
        ((price as f64) / 10000.0, size)
    }

    /// Get the best ask (level 0)
    #[inline(always)]
    pub fn best_ask(&self) -> (f64, u64) {
        let price = self.asks[0].value.load(Ordering::Acquire);
        let size = self.ask_sizes[0].value.load(Ordering::Acquire);
        ((price as f64) / 10000.0, size)
    }

    /// Get mid price
    ///
    /// # Performance
    /// ~16-20ns
    #[inline(always)]
    pub fn mid_price(&self) -> f64 {
        let (bid, _) = self.best_bid();
        let (ask, _) = self.best_ask();
        (bid + ask) / 2.0
    }

    /// Get spread in basis points
    #[inline(always)]
    pub fn spread_bps(&self) -> f64 {
        let (bid, _) = self.best_bid();
        let (ask, _) = self.best_ask();
        let mid = (bid + ask) / 2.0;

        if mid > 0.0 {
            ((ask - bid) / mid) * 10000.0
        } else {
            0.0
        }
    }

    /// Get all bid levels
    pub fn get_bids(&self, levels: usize) -> Vec<(f64, u64)> {
        let n = levels.min(10);
        let mut result = Vec::with_capacity(n);

        for i in 0..n {
            let price = self.bids[i].value.load(Ordering::Acquire);
            let size = self.bid_sizes[i].value.load(Ordering::Acquire);

            if price > 0 {
                result.push(((price as f64) / 10000.0, size));
            }
        }

        result
    }

    /// Get all ask levels
    pub fn get_asks(&self, levels: usize) -> Vec<(f64, u64)> {
        let n = levels.min(10);
        let mut result = Vec::with_capacity(n);

        for i in 0..n {
            let price = self.asks[i].value.load(Ordering::Acquire);
            let size = self.ask_sizes[i].value.load(Ordering::Acquire);

            if price > 0 {
                result.push(((price as f64) / 10000.0, size));
            }
        }

        result
    }

    /// Get the current sequence number
    #[inline(always)]
    pub fn sequence(&self) -> u64 {
        self.sequence.value.load(Ordering::Acquire)
    }

    /// Get the last update timestamp
    #[inline(always)]
    pub fn last_update_ns(&self) -> u64 {
        self.last_update_ns.value.load(Ordering::Acquire)
    }

    /// Get symbol ID
    #[inline(always)]
    pub fn symbol_id(&self) -> u8 {
        self.symbol_id
    }
}

impl fmt::Display for OrderBook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "OrderBook (Symbol: {})", self.symbol_id)?;
        writeln!(f, "Sequence: {}", self.sequence())?;
        writeln!(f, "\nAsks:")?;

        let asks = self.get_asks(5);
        for (i, (price, size)) in asks.iter().enumerate().rev() {
            writeln!(f, "  L{}: {:>8.2} x {:>6}", i, price, size)?;
        }

        writeln!(f, "        ---SPREAD: {:.2} bps---", self.spread_bps())?;

        writeln!(f, "\nBids:")?;
        let bids = self.get_bids(5);
        for (i, (price, size)) in bids.iter().enumerate() {
            writeln!(f, "  L{}: {:>8.2} x {:>6}", i, price, size)?;
        }

        writeln!(f, "\nMid: {:.2}", self.mid_price())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orderbook_basic() {
        let ob = OrderBook::new(1);

        ob.update_bid(0, 450000, 100, 1000);
        let (price, size) = ob.best_bid();

        assert_eq!(price, 45.0);
        assert_eq!(size, 100);
    }

    #[test]
    fn test_mid_price() {
        let ob = OrderBook::new(1);

        ob.update_bid(0, 450000, 100, 1000);
        ob.update_ask(0, 460000, 100, 1000);

        assert_eq!(ob.mid_price(), 45.5);
    }

    #[test]
    fn test_spread_calculation() {
        let ob = OrderBook::new(1);

        ob.update_bid(0, 450000, 100, 1000);
        ob.update_ask(0, 460000, 100, 1000);

        let spread = ob.spread_bps();
        assert!(spread > 219.0 && spread < 220.0);
    }

    #[test]
    fn test_sequence_increment() {
        let ob = OrderBook::new(1);

        assert_eq!(ob.sequence(), 0);

        ob.update_bid(0, 450000, 100, 1000);
        assert_eq!(ob.sequence(), 1);

        ob.update_ask(0, 460000, 100, 1000);
        assert_eq!(ob.sequence(), 2);
    }

    #[test]
    fn test_multiple_levels() {
        let ob = OrderBook::new(1);

        ob.update_bid(0, 450000, 100, 1000);
        ob.update_bid(1, 449000, 200, 1000);
        ob.update_bid(2, 448000, 150, 1000);

        let bids = ob.get_bids(3);
        assert_eq!(bids.len(), 3);
        assert_eq!(bids[0].0, 45.0);
        assert_eq!(bids[1].0, 44.9);
        assert_eq!(bids[2].0, 44.8);
    }
}
