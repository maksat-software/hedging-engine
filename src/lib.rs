//! # Hedging Engine
//!
//! Sub-microsecond algorithmic hedging system for energy derivatives.
//!
//! ## Features
//!
//! - Lock-free data structures
//! - Cache-aware memory layout
//! - Zero-allocation hot paths
//! - Multiple hedging strategies
//! - Network I/O (TCP and optional DPDK)
//!
//! ## Quick Start
//!
//! ```
//! use hedging_engine::*;
//!
//! let engine = HedgeEngine::new(HedgeConfig::default())?;
//! let tick = MarketTick::bid(get_timestamp_ns(), 45.50, 100, 1);
//! engine.on_tick(tick);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod hedging;
pub mod market_data;
pub mod network;
pub mod strategy;
pub mod utils;

// Re-exports
pub use hedging::{
    DeltaHedge, HedgeConfig, HedgeEngine, HedgeRecommendation, MVHRStrategy, MeanReversionHedge,
};
pub use market_data::{MarketTick, OrderBook, Side};
pub use network::{NetworkConfig, TcpMarketDataFeed, TcpOrderSubmitter};
pub use strategy::HedgingStrategy;
pub use utils::{LockFreeQueue, MPSCQueue, Metrics, get_timestamp_ns};

/// Common result type
pub type Result<T> = std::result::Result<T, Error>;

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Market data error: {0}")]
    MarketData(String),

    #[error("Calculation error: {0}")]
    Calculation(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Network error: {0}")]
    Network(String),
}
