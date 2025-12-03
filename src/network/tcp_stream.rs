//! Standard TCP/IP networking for market data and orders

use crate::Error;
use crate::market_data::MarketTick;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// TCP-based market data feed
pub struct TcpMarketDataFeed {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl TcpMarketDataFeed {
    /// Connect to market data feed
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self, Error> {
        let stream: TcpStream = TcpStream::connect(addr)
            .map_err(|e| Error::MarketData(format!("Failed to connect: {}", e)))?;

        // Set TCP options for low latency
        stream
            .set_nodelay(true)
            .map_err(|e| Error::MarketData(format!("Failed to set nodelay: {}", e)))?;

        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| Error::MarketData(format!("Failed to set timeout: {}", e)))?;

        Ok(Self {
            stream,
            buffer: vec![0u8; 8192],
        })
    }

    /// Read next tick from stream
    pub fn read_tick(&mut self) -> Result<Option<MarketTick>, Error> {
        // Read exactly 32 bytes (size of MarketTick)
        match self.stream.read_exact(&mut self.buffer[..32]) {
            Ok(_) => {
                // Parse binary tick data
                let tick = unsafe { std::ptr::read(self.buffer.as_ptr() as *const MarketTick) };
                Ok(Some(tick))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(None),
            Err(e) => Err(Error::MarketData(format!("Read error: {}", e))),
        }
    }

    /// Read multiple ticks in batch
    pub fn read_batch(&mut self, max_count: usize) -> Result<Vec<MarketTick>, Error> {
        let mut ticks = Vec::with_capacity(max_count);

        while ticks.len() < max_count {
            match self.read_tick()? {
                Some(tick) => ticks.push(tick),
                None => break,
            }
        }

        Ok(ticks)
    }
}

/// TCP-based order submission
pub struct TcpOrderSubmitter {
    stream: TcpStream,
}

impl TcpOrderSubmitter {
    /// Connect to order submission endpoint
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self, Error> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| Error::MarketData(format!("Failed to connect: {}", e)))?;

        stream
            .set_nodelay(true)
            .map_err(|e| Error::MarketData(format!("Failed to set nodelay: {}", e)))?;

        Ok(Self { stream })
    }

    /// Submit order (binary protocol)
    pub fn submit_order(&mut self, order_data: &[u8]) -> Result<(), Error> {
        self.stream
            .write_all(order_data)
            .map_err(|e| Error::MarketData(format!("Failed to send order: {}", e)))?;

        self.stream
            .flush()
            .map_err(|e| Error::MarketData(format!("Failed to flush: {}", e)))?;

        Ok(())
    }
}
