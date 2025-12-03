//! Network communication module
//!
//! Provides both standard TCP/IP and high-performance DPDK networking

mod tcp_stream;

#[cfg(feature = "dpdk")]
mod dpdk_wrapper;

pub use tcp_stream::{TcpMarketDataFeed, TcpOrderSubmitter};

#[cfg(feature = "dpdk")]
pub use dpdk_wrapper::{DpdkConfig, DpdkPort};

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub host: String,
    pub port: u16,
    pub use_dpdk: bool,
    pub buffer_size: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5555,
            use_dpdk: false,
            buffer_size: 4096,
        }
    }
}
