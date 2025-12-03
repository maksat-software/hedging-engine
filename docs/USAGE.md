# Usage Guide

## Quick Start

### Installation

Clone from source:

```bash
git clone https://github.com/maksat-software/hedging-engine
cd hedging-engine
cargo build --release
```

### Basic Example

```rust
use hedging_engine::*;

fn main() -> Result<()> {
    // 1. Create configuration
    let config = HedgeConfig {
        initial_position: -10_000.0,  // Short 10,000 MWh
        default_hedge_ratio: 1.125,
        rehedge_threshold_bps: 500,   // 5% threshold
        ..Default::default()
    };

    // 2. Initialize engine
    let engine = HedgeEngine::new(config)?;

    // 3. Send market data
    let tick = MarketTick::bid(
        utils::get_timestamp_ns(),
        48.20,  // €48.20/MWh
        150,    // 150 MWh
        1       // Symbol ID (1=spot, 2=futures)
    );
    engine.on_tick(tick);

    // 4. Get hedge recommendation
    if let Some(rec) = engine.get_hedge_recommendation()? {
        println!("Hedge: {} {:.0} MWh @ €{:.2}",
                 match rec.side {
                     Side::Bid => "SELL",
                     Side::Ask => "BUY",
                 },
                 rec.quantity,
                 rec.price
        );

        // 5. Execute hedge
        engine.execute_hedge(&rec)?;
    }

    Ok(())
}
```

## Configuration

### HedgeConfig Options

```rust
pub struct HedgeConfig {
    /// Initial position (MWh, negative = short)
    pub initial_position: f64,

    /// Default hedge ratio
    /// Examples:
    ///   1.0   = 1:1 hedge
    ///   1.125 = 12.5% over hedge
    ///   0.9   = 90% hedge
    pub default_hedge_ratio: f64,

    /// Rehedge threshold (basis points)
    /// Examples:
    ///   100 = 1% deviation triggers rehedge
    ///   500 = 5% deviation triggers rehedge
    pub rehedge_threshold_bps: i64,

    /// Maximum position size (MWh)
    pub max_position: f64,

    /// Enable MVHR calculation
    pub enable_mvhr: bool,

    /// Enable mean reversion strategy
    pub enable_mean_reversion: bool,

    /// Lookback window for statistics (hours)
    pub statistics_window_hours: usize,
}
```

### Simple Configuration

```rust
// Quick setup
let config = HedgeConfig::simple(- 10_000.0, 1.125);
```

### Advanced Configuration

```rust
let config = HedgeConfig {
initial_position: - 50_000.0,
default_hedge_ratio: 1.10,
rehedge_threshold_bps: 300,
max_position: 100_000.0,
enable_mvhr: true,
enable_mean_reversion: true,
statistics_window_hours: 720,  // 30 days
};
```

## Market Data Integration

### Symbol IDs

- **1**: Spot market
- **2**: Futures market
- **3+**: Custom symbols

### Creating Ticks

```rust
// BID tick
let bid = MarketTick::bid(
timestamp_ns,
price,      // €/MWh
quantity,   // MWh
symbol_id
);

// ASK tick
let ask = MarketTick::ask(
timestamp_ns,
price,
quantity,
symbol_id
);
```

### Real-Time Data Feed Example

```rust
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    let engine = HedgeEngine::new(config)?;

    // Connect to market data feed
    let stream = TcpStream::connect("market-data.example.com:5555").await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        // Parse market data
        let tick = parse_market_data(&line)?;

        // Process tick
        engine.on_tick(tick);

        // Check for hedge
        if let Some(rec) = engine.get_hedge_recommendation()? {
            submit_order_to_exchange(&rec).await?;
            engine.execute_hedge(&rec)?;
        }
    }

    Ok(())
}
```

## Hedging Strategies

### 1. Delta Hedging (Default)

Simple position-based hedging:

```rust
let config = HedgeConfig {
enable_mvhr: false,
enable_mean_reversion: false,
default_hedge_ratio: 1.0,
..Default::default ()
};
```

**When to use:**

- Simple risk management
- Stable markets
- Small positions (<10,000 MWh)

### 2. MVHR (Minimum Variance Hedge Ratio)

Statistically optimal hedging:

```rust
let config = HedgeConfig {
enable_mvhr: true,
statistics_window_hours: 720,  // 30 days
..Default::default ()
};
```

**When to use:**

- Medium to large positions
- Historical data available
- Optimization important

**Formula:**

```
h* = Cov(ΔS, ΔF) / Var(ΔF)
```

### 3. Mean Reversion

Energy-specific strategy:

```rust
let config = HedgeConfig {
enable_mean_reversion: true,
statistics_window_hours: 720,
..Default::default ()
};
```

**When to use:**

- Energy markets (gas, power)
- High volatility
- Short-term positions

**How it works:**

```
If price > mean + 2σ:
    Reduce hedge (expect reversion)
    
If price < mean - 2σ:
    Reduce hedge (expect reversion)
```

### 4. Combined Strategies

```rust
let config = HedgeConfig {
enable_mvhr: true,
enable_mean_reversion: true,
..Default::default ()
};
```

All strategies work together:

1. MVHR calculates optimal ratio
2. Mean reversion adjusts for extremes
3. Delta hedge executes

## Backtesting

### Loading Historical Data

```rust
use std::fs::File;
use std::io::{BufRead, BufReader};

fn load_csv(path: &str) -> Result<Vec<MarketTick>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut ticks = Vec::new();

    for line in reader.lines().skip(1) {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();

        let tick = MarketTick {
            timestamp_ns: parts[0].parse()?,
            price: (parts[2].parse::<f64>()? * 10000.0) as i64,
            quantity: parts[3].parse()?,
            side: if parts[4] == "bid" { 0 } else { 1 },
            symbol_id: parts[1].parse()?,
            _padding: [0; 6],
        };

        ticks.push(tick);
    }

    Ok(ticks)
}
```

### Running Backtest

```rust
fn run_backtest(ticks: Vec<MarketTick>) -> Result<BacktestResults> {
    let config = HedgeConfig::simple(-10_000.0, 1.125);
    let engine = HedgeEngine::new(config)?;

    let mut results = BacktestResults::default();

    for (i, tick) in ticks.iter().enumerate() {
        engine.on_tick(*tick);

        // Check hedge every 100 ticks
        if i % 100 == 0 {
            if let Some(rec) = engine.get_hedge_recommendation()? {
                results.record_hedge(&rec);
                engine.execute_hedge(&rec)?;
            }
        }
    }

    results.finalize(&engine);
    Ok(results)
}

#[derive(Default)]
struct BacktestResults {
    hedges_executed: usize,
    total_volume: f64,
    total_cost: f64,
}

impl BacktestResults {
    fn record_hedge(&mut self, rec: &HedgeRecommendation) {
        self.hedges_executed += 1;
        self.total_volume += rec.quantity;
        self.total_cost += rec.quantity * rec.price;
    }

    fn finalize(&mut self, engine: &HedgeEngine) {
        println!("Backtest Results:");
        println!("  Hedges: {}", self.hedges_executed);
        println!("  Volume: {:.0} MWh", self.total_volume);
        println!("  Cost: €{:.0}", self.total_cost);
    }
}
```

## Performance Monitoring

### Getting Metrics

```rust
let metrics = engine.get_metrics();
let summary = metrics.summary();

println!("Performance:");
println!("  Ticks: {}", summary.ticks_processed);
println!("  Hedges: {}", summary.hedges_executed);
println!("  Avg Latency: {} ns", summary.avg_latency_ns);
println!("  P99 Latency: {} ns", summary.p99_latency_ns);
```

### Continuous Monitoring

```rust
use std::time::Duration;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<()> {
    let engine = Arc::new(HedgeEngine::new(config)?);
    let engine_clone = Arc::clone(&engine);

    // Spawn metrics reporter
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            let metrics = engine_clone.get_metrics();
            let summary = metrics.summary();

            log::info!("Metrics: ticks={}, hedges={}, p99={}ns",
                summary.ticks_processed,
                summary.hedges_executed,
                summary.p99_latency_ns
            );
        }
    });

    // Main trading loop
    // ...

    Ok(())
}
```

## Production Deployment

### System Requirements

```bash
# Install RT kernel
sudo apt install linux-image-rt-amd64

# Configure huge pages
echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Isolate CPUs
# Add to /etc/default/grub:
GRUB_CMDLINE_LINUX="isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3"

sudo update-grub
sudo reboot
```

### CPU Pinning

```bash
# Pin to isolated CPU
taskset -c 2 ./hedging-engine
```

Or in Rust:

```rust
use std::thread;

fn main() {
    // Pin to CPU 2
    #[cfg(target_os = "linux")]
    unsafe {
        let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(2, &mut cpuset);
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpuset);
    }

    // code here
}
```

### Performance Governor

```bash
# Set CPU governor to performance
sudo cpupower frequency-set -g performance

# Disable turbo boost (for consistency)
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo
```

### Running in Production

```bash
# Build optimized
cargo build --release

# Run with logging
RUST_LOG=info ./target/release/hedging-engine

# Run as daemon
sudo systemctl enable hedging-engine
sudo systemctl start hedging-engine
```

## Troubleshooting

### High P99 Latency

**Symptoms:** P99 > 10μs

**Causes:**

1. CPU isn’t isolated
2. Background processes
3. Network jitter
4. Swap enabled

**Solutions:**

```bash
# Check CPU isolation
cat /sys/devices/system/cpu/isolated

# Check running processes on isolated CPU
ps -eLo pid,comm,psr | grep " 2$"

# Disable swap
sudo swapoff -a

# Check network settings
ethtool -C eth0 rx-usecs 0 tx-usecs 0
```

### Low Throughput

**Symptoms:** <10k ticks/second

**Causes:**

1. Logging overhead
2. Disk I/O
3. Lock contention

**Solutions:**

```rust
// Use minimal logging
RUST_LOG=error . / hedging-engine

// Disable metrics in hot path
let config = HedgeConfig {
enable_detailed_metrics: false,
..Default::default ()
};
```

### Memory Leaks

**Check with Valgrind:**

```bash
valgrind --leak-check=full ./target/release/hedging-engine
```

**Expected output:**

```
LEAK SUMMARY:
   definitely lost: 0 bytes in 0 blocks
   indirectly lost: 0 bytes in 0 blocks
```

## API Reference

### Core Types

```rust
// Market data
pub struct MarketTick {
    ...
}
pub struct OrderBook {
    ...
}
pub enum Side { Bid, Ask }

// Hedging
pub struct HedgeEngine {
    ...
}
pub struct HedgeConfig {
    ...
}
pub struct HedgeRecommendation {
    ...
}

// Strategies
pub struct DeltaHedge {
    ...
}
pub struct MVHRStrategy {
    ...
}
pub struct MeanReversionHedge {
    ...
}
```

### Key Methods

```rust
// Engine
impl HedgeEngine {
    pub fn new(config: HedgeConfig) -> Result<Self>;
    pub fn on_tick(&self, tick: MarketTick);
    pub fn get_hedge_recommendation(&self) -> Result<Option<HedgeRecommendation>>;
    pub fn execute_hedge(&self, rec: &HedgeRecommendation) -> Result<()>;
    pub fn get_metrics(&self) -> Metrics;
}

// OrderBook
impl OrderBook {
    pub fn new(symbol_id: u8) -> Self;
    pub fn update_bid(&self, level: usize, price: i64, size: u64, ts: u64);
    pub fn best_bid(&self) -> (f64, u64);
    pub fn mid_price(&self) -> f64;
}
```

## Examples

Run the included examples:

```bash
# Simple hedging
cargo run --example simple_hedge --release

# Backtesting
cargo run --example backtest --release

# Live demo
cargo run --example live_demo --release
```

## Further Reading

- [Architecture Documentation](ARCHITECTURE.md)
- [Benchmark Results](BENCHMARKS.md)
- [API Documentation](https://docs.rs/hedging-engine)
- [GitHub Repository](https://github.com/maksat/rust-hedging-engine)

## Support

- **Issues:** [GitHub Issues](https://github.com/maksat/rust-hedging-engine/issues)
- **Discussions:** [GitHub Discussions](https://github.com/maksat/rust-hedging-engine/discussions)
- **Email:** contact@maksat.software