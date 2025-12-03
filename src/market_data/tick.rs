use serde::{Deserialize, Serialize};
use std::fmt;

/// Market data side (bid or ask)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    Bid = 0,
    Ask = 1,
}

/// Compact market data tick (32 bytes)
///
/// Optimized for cache efficiency and minimal memory footprint.
#[repr(C)]
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct MarketTick {
    /// Timestamp in nanoseconds (epoch)
    pub timestamp_ns: u64,

    pub price: i64,

    /// Quantity in lots
    pub quantity: u32,

    /// Side (0=bid, 1=ask)
    pub side: u8,

    /// Symbol identifier
    pub symbol_id: u8,

    /// Padding to align to 32 bytes
    _padding: [u8; 6],
}

impl MarketTick {
    /// Create a BID tick
    #[inline]
    pub fn bid(timestamp_ns: u64, price: f64, quantity: u32, symbol_id: u8) -> Self {
        Self {
            timestamp_ns,
            price: (price * 10000.0) as i64,
            quantity,
            side: Side::Bid as u8,
            symbol_id,
            _padding: [0; 6],
        }
    }

    /// Create an ASK tick
    #[inline]
    pub fn ask(timestamp_ns: u64, price: f64, quantity: u32, symbol_id: u8) -> Self {
        Self {
            timestamp_ns,
            price: (price * 10000.0) as i64,
            quantity,
            side: Side::Ask as u8,
            symbol_id,
            _padding: [0; 6],
        }
    }

    /// Convert fixed-point price to f64
    #[inline(always)]
    pub fn price_f64(&self) -> f64 {
        (self.price as f64) / 10000.0
    }

    /// Check if this is a BID
    #[inline(always)]
    pub fn is_bid(&self) -> bool {
        self.side == Side::Bid as u8
    }

    /// Check if this is an ASK
    #[inline(always)]
    pub fn is_ask(&self) -> bool {
        self.side == Side::Ask as u8
    }

    /// Calculate latency in microseconds from given timestamp
    #[inline]
    pub fn latency_micros(&self, current_ns: u64) -> u64 {
        (current_ns.saturating_sub(self.timestamp_ns)) / 1000
    }
}

impl fmt::Display for MarketTick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:>8.2} @ {:>6} ({})",
            if self.is_bid() { "BID" } else { "ASK" },
            self.price_f64(),
            self.quantity,
            self.timestamp_ns
        )
    }
}

impl fmt::Debug for MarketTick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarketTick")
            .field("timestamp_ns", &self.timestamp_ns)
            .field("price", &self.price_f64())
            .field("quantity", &self.quantity)
            .field("side", if self.is_bid() { &"BID" } else { &"ASK" })
            .field("symbol_id", &self.symbol_id)
            .finish()
    }
}

// Ensure the size is exactly 32 bytes
static_assertions::const_assert_eq!(std::mem::size_of::<MarketTick>(), 32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_creation() {
        let tick: MarketTick = MarketTick::bid(1000000, 45.50, 100, 1);

        assert_eq!(tick.price_f64(), 45.50);
        assert_eq!(tick.quantity, 100);
        assert!(tick.is_bid());
        assert!(!tick.is_ask());
        assert_eq!(tick.symbol_id, 1);
    }

    #[test]
    fn test_tick_size() {
        assert_eq!(std::mem::size_of::<MarketTick>(), 32);
    }

    #[test]
    fn test_latency_calculation() {
        let tick = MarketTick::bid(1_000_000, 45.0, 100, 1);
        let current = 1_010_000; // +10 microseconds

        assert_eq!(tick.latency_micros(current), 10);
    }

    #[test]
    fn test_fixed_point_conversion() {
        let tick = MarketTick::bid(1000000, 45.5555, 100, 1);
        assert!((tick.price_f64() - 45.5555).abs() < 0.0001);
    }
}
