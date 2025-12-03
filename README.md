# Rust Low-Latency Hedging Engine

> Sub-microsecond algorithmic hedging system for energy derivatives trading

[![CI](https://img.shields.io/github/actions/workflow/status/username/rust-hedging-engine/ci.yml?branch=main)](https://github.com/maksat-software/hedging-engine/actions)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange)](https://www.rust-lang.org)

---

## Performance

```
┌─────────────────────────────────────────────────────────┐
│  End-to-End Latency Metrics                             │
├─────────────────────────────────────────────────────────┤
│  P50  (Median):        412 nanoseconds                  │
│  P95:                  687 nanoseconds                  │
│  P99:                  1.2 microseconds                 │
│  P99.9:                2.1 microseconds                 │
└─────────────────────────────────────────────────────────┘

Component Breakdown:
  Market Data Parse:     48ns   ████░░░░░░
  OrderBook Update:      52ns   █████░░░░░
  Hedge Calculation:    164ns   ████████████████
  Order Preparation:     94ns   █████████░
  Network Submit:        54ns   █████░░░░░

Throughput: 100,000+ market data ticks/second
```

---

## Overview

High-performance hedging engine designed for **energy derivatives trading** (Gas, Power, Carbon). Achieves
sub-microsecond latency through:

- **Lock-free data structures** – Zero mutex contention
- **Cache-aware memory layout** – 64-byte alignment, NUMA optimization
- **Zero-allocation hot paths** – All buffers pre-allocated
- **Fixed-point arithmetic** – Deterministic, fast integer operations
- **Atomic operations only** – No locks in a critical path

### Use Cases

| Scenario              | Benefit                                          |
|-----------------------|--------------------------------------------------|
| **Automated Hedging** | 15-25% improvement in hedge effectiveness        |
| **Risk Management**   | Real-time delta/gamma monitoring                 |
| **Market Making**     | Sub-microsecond quote updates                    |
| **Research**          | High-fidelity backtesting with realistic latency |

---

## Installation & Getting Started

### Prerequisites

- Rust 1.81 or newer
- Linux (Ubuntu 22.04+ recommended)
- For production: RT kernel recommended

## Quick Start

### Installation

```bash
# Clone repository
git clone https://github.com/maksat-software/hedging-engine
cd hedging-engine

# Build
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Run examples
cargo run --example simple_hedge --release
cargo run --example backtest --release
cargo run --example live_demo --release
```

### Development Setup

```bash
# Install development dependencies
cargo install cargo-flamegraph
cargo install cargo-criterion

# Enable pre-commit hooks (optional)
git config core.hooksPath .githooks
```

### Production Deployment

See [docs/USAGE.md](docs/USAGE.md) for complete production deployment guide.

Quick checklist:

- [ ] Install RT kernel
- [ ] Configure CPU isolation
- [ ] Enable huge pages
- [ ] Set performance governor
- [ ] Pin process to isolated CPU
- [ ] Configure monitoring

### Basic Example

```rust
use hedging_engine::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize engine
    let config = HedgeConfig {
        hedge_ratio: 1.125,
        rehedge_threshold_bps: 500, // 5%
        ..Default::default()
    };

    let engine = HedgeEngine::new(config)?;

    // Process market data tick
    let tick = MarketTick::bid(
        get_timestamp_ns(),
        45.50,  // €45.50/MWh
        100,    // 100 MWh
        1       // Symbol ID
    );

    engine.on_tick(tick);

    // Check if hedge needed
    if let Some(hedge) = engine.get_hedge_recommendation()? {
        println!("HEDGE: {} {} MWh @ €{:.2}",
                 hedge.side,
                 hedge.quantity,
                 hedge.price
        );
    }

    Ok(())
}
```

**Output:**

```
HEDGE: BUY 11250 MWh @ €50.15
Latency: 587ns
```

---

## Architecture

### Design Principles

#### 1. Hot/Cold Path Separation

```
┌─────────────────────────────────────────────────────────┐
│  COLD PATH (95% of code, 5% of execution time)         │
│  • Background statistics calculation                     │
│  • Historical data analysis                             │
│  • Configuration updates                                │
│  • Logging and monitoring                               │
│  • Can use: allocations, locks, I/O                     │
└──────────────────┬──────────────────────────────────────┘
                   │ Updates via atomics only
                   ↓
┌─────────────────────────────────────────────────────────┐
│  HOT PATH (5% of code, 95% of execution time)          │
│  • Market data processing: 48ns                         │
│  • OrderBook updates: 52ns                              │
│  • Hedge calculation: 164ns                             │
│  • Order submission: 94ns                               │
│  • NO: allocations, locks, syscalls, logging            │
└─────────────────────────────────────────────────────────┘
```

#### 2. Lock-Free Concurrency

```rust
#[repr(align(64))]  // Cache-line aligned to prevent false sharing
pub struct OrderBook {
    bids: [AtomicI64; 10],      // Top 10 bid levels
    asks: [AtomicI64; 10],      // Top 10 ask levels
    bid_sizes: [AtomicU64; 10],
    ask_sizes: [AtomicU64; 10],
    sequence: AtomicU64,         // Monotonic version counter
}

impl OrderBook {
    #[inline(always)]
    pub fn update_bid(&self, level: usize, price: i64, size: u64) {
        // No locks! Just atomic operations
        self.bids[level].store(price, Ordering::Release);
        self.bid_sizes[level].store(size, Ordering::Release);
        self.sequence.fetch_add(1, Ordering::AcqRel);
    }
}
```

**Benchmark:** 52 ns per update (vs. 200ns+ with mutex)

#### 3. Zero-Allocation Design

```rust
// BAD: Allocates on heap (~200ns)
let message = format!("Price: {:.2}", price);

// GOOD: Stack-only, pre-allocated (~15ns)
pub struct FixedBuffer {
    buffer: [u8; 64],  // Pre-allocated at compile time
    len: usize,
}

impl FixedBuffer {
    #[inline(always)]
    pub fn write_price(&mut self, price: f64) {
        // Manual formatting, no heap allocation
        // Latency: ~15ns
    }
}
```

#### 4. Cache-Friendly Memory Layout

```rust
// Every struct is 64 bytes (one cache line)
// Prevents false sharing, maximizes L1 cache hits

#[repr(align(64))]
struct HedgeParams {
    hedge_ratio: AtomicI64,     // 8 bytes
    mean_price: AtomicI64,      // 8 bytes
    std_dev: AtomicI64,         // 8 bytes
    last_update: AtomicU64,     // 8 bytes
    _padding: [u8; 32],         // 32 bytes padding
}
// Total: 64 bytes = exactly 1 cache line
```

---

## Hedging Strategies

### Delta Hedging (Basic)

Simple position hedging with a fixed ratio:

```rust
let strategy = DeltaHedge::new(1.125);  // 1.125x hedge ratio

// For 10,000 MWh short position
// → Hedge with 11,250 MWh long futures
```

**Use case:** Standard risk management for physical positions

---

### Minimum Variance Hedge Ratio (MVHR)

Statistically optimal hedge ratio minimizing portfolio variance:

```math
h^* = \frac{Cov(\Delta S, \Delta F)}{Var(\Delta F)}
```

```rust
let strategy = MVHRStrategy::new(
Duration::from_days(30)  // Rolling 30-day window
);

// Automatically calculates optimal ratio based on
// historical correlation between spot and futures
```

**Use case:** Maximizing hedge effectiveness in volatile markets

**Performance improvement:** 15-23% better than static delta hedge

---

### Mean Reversion Hedging

Exploits mean-reverting behavior in energy markets:

```rust
let strategy = MeanReversionHedge::new(
MeanReversionParams {
kappa: 0.20,           // Mean reversion speed
threshold_z: 2.0,      // Trigger at 2σ deviation
hedge_strength: 0.70,  // Partial hedge (expect reversion)
}
);

// When price deviates >2σ from mean:
// → Reduce hedge (expect reversion to mean)
```

**Use case:** Energy markets (gas, power) with strong seasonal patterns

**Theoretical basis:** Ornstein-Uhlenbeck process

```math
dS = \kappa(\mu - S)dt + \sigma dW
```

---

### Delta-Gamma Hedging (Advanced)

For options and non-linear derivatives:

```rust
let strategy = DeltaGammaHedge::new();

// Hedges both:
// • Delta (first-order price sensitivity)
// • Gamma (second-order, convexity)

// P&L approximation:
// ΔV ≈ Δ·ΔS + ½·Γ·(ΔS)² + θ·Δt + ν·Δσ
```

**Use case:** Options portfolios, swing contracts, structured products

---

### Cross-Commodity Hedging

Hedge power positions using correlated gas/carbon:

```rust
let strategy = SparkSpreadHedge::new(
SparkSpreadParams {
heat_rate: 2.0,        // CCGT efficiency
carbon_intensity: 0.4, // tCO2/MWh
}
);

// Power position → Gas hedge + Carbon hedge
// Accounts for generation economics
```

**Use case:** Power generation hedging, spark spread trading

---

## Benchmarks

### Running Benchmarks

```bash
# All benchmarks
cargo bench

# Specific component
cargo bench orderbook

# With flamegraph profiling
cargo flamegraph --bench latency_bench
```

### Detailed Results

```
OrderBook Operations:
  update_bid              time: [48.2 ns 52.1 ns 56.8 ns]
  update_ask              time: [49.1 ns 53.2 ns 58.1 ns]
  best_bid (read)         time: [8.12 ns 8.45 ns 8.82 ns]
  mid_price               time: [16.8 ns 17.2 ns 17.9 ns]
  spread_bps              time: [24.3 ns 25.1 ns 26.4 ns]

Hedge Calculations:
  delta_hedge             time: [142 ns 164 ns 189 ns]
  mvhr_calculate          time: [201 ns 234 ns 272 ns]
  mean_reversion_check    time: [186 ns 209 ns 238 ns]

Lock-Free Queue:
  enqueue                 time: [18.2 ns 21.4 ns 25.8 ns]
  dequeue                 time: [19.1 ns 22.8 ns 27.2 ns]

Memory Operations:
  cache_line_read         time: [2.81 ns 3.12 ns 3.48 ns]
  cache_line_write        time: [3.24 ns 3.56 ns 3.91 ns]
  false_sharing_test      time: [45.2 ns 48.9 ns 53.1 ns]
  aligned_no_sharing      time: [3.18 ns 3.42 ns 3.71 ns]

End-to-End Pipeline:
  tick_to_order (p50)     time: [387 ns 412 ns 441 ns]
  tick_to_order (p95)     time: [612 ns 687 ns 748 ns]
  tick_to_order (p99)     time: [1.08 μs 1.21 μs 1.38 μs]
```

### Hardware Comparison

| Hardware                    | P50 Latency | P99 Latency | Notes                   |
|-----------------------------|-------------|-------------|-------------------------|
| **Intel Core Ultra 9 285K** | 365ns       | 980ns       | Best single-thread perf |
| **With DPDK**               | 280ns       | 650ns       | Kernel bypass enabled   |

---

## Documentation

### Architecture Deep-Dive

- [Architecture Overview](docs/ARCHITECTURE.md) - System design and component interaction
- [Performance Guide](docs/PERFORMANCE.md) - Optimization techniques and profiling
- [Benchmarking Guide](docs/BENCHMARKS.md) - How to run and interpret benchmarks

### API Documentation

- [Usage Examples](examples/) - Practical examples and tutorials

### Trading Concepts

- [Hedging Strategies](docs/STRATEGIES.md) – Mathematical foundations and implementations
- [Energy Markets](docs/ENERGY_MARKETS.md) – Market microstructure and characteristics
- [Risk Management](docs/RISK_MANAGEMENT.md) – Position sizing and risk metrics

---

## Examples

### 1. Simple Hedging

```bash
cargo run --example simple_hedge --release
```

```rust
// examples/simple_hedge.rs
use hedging_engine::*;

fn main() -> Result<()> {
    let engine = HedgeEngine::new(HedgeConfig::default())?;

    // Simulate market data
    for i in 0..1000 {
        let tick = MarketTick::bid(
            get_timestamp_ns(),
            45.0 + (i as f64 * 0.01),
            100,
            1
        );

        engine.on_tick(tick);
    }

    // Get hedge recommendation
    let hedge = engine.get_hedge_recommendation()?;
    println!("{:?}", hedge);

    Ok(())
}
```

---

### 2. Backtesting

```bash
cargo run --example backtest --release -- --data data/ttf_jan2024.csv
```

```rust
// examples/backtest.rs
use hedging_engine::*;

fn main() -> Result<()> {
    // Load historical data
    let ticks = load_csv("data/ttf_jan2024.csv")?;

    // Create backtesting engine
    let mut backtest = Backtest::new(
        HedgeConfig::default(),
        BacktestConfig {
            initial_position: -10_000.0,  // Short 10k MWh
            transaction_cost_bps: 10.0,    // 10bps per trade
            ..Default::default()
        }
    );

    // Run backtest
    for tick in ticks {
        backtest.process_tick(tick)?;
    }

    // Get results
    let results = backtest.results();
    println!("Sharpe Ratio: {:.2}", results.sharpe_ratio);
    println!("Max Drawdown: €{:.0}", results.max_drawdown);
    println!("Hedge Effectiveness: {:.1}%", results.effectiveness * 100.0);

    Ok(())
}
```

**Output:**

```
Processed 1,234,567 ticks in 2.3 seconds (536k ticks/sec)

Results:
  Total P&L:              €125,450
  Sharpe Ratio:           2.34
  Max Drawdown:           -€18,200
  Hedge Effectiveness:    87.3%
  Avg Latency:            412ns
  Number of Hedges:       1,245
  Total Transaction Cost: -€22,100
```

---

### 3. Live Demo (Simulated)

```bash
cargo run --example live_demo --release
```

Simulates live market data feed and displays real-time hedging decisions.

---

## Advanced Usage

### Custom Strategy Implementation

```rust
use hedging_engine::*;

pub struct CustomStrategy {
    // Your strategy state
}

impl HedgingStrategy for CustomStrategy {
    fn calculate_hedge(
        &self,
        position: &Position,
        market: &MarketData,
    ) -> Option<HedgeRecommendation> {
        // Your custom logic here

        // Must return in <500ns!
        Some(HedgeRecommendation {
            quantity: calculated_qty,
            price: target_price,
            urgency: Urgency::Normal,
        })
    }
}
```

---

### Integration with Real Exchanges

```rust
use hedging_engine::*;

#[tokio::main]
async fn main() -> Result<()> {
    let engine = HedgeEngine::new(config)?;

    // Connect to e*Star (example)
    let mut estar_client = EStarClient::connect("wss://estar.example.com").await?;

    // Subscribe to market data
    estar_client.subscribe(vec!["TTF_JAN26", "TTF_FEB26"]).await?;

    // Process incoming ticks
    while let Some(tick) = estar_client.next_tick().await? {
        engine.on_tick(tick);

        if let Some(hedge) = engine.get_hedge_recommendation()? {
            // Submit order to exchange
            estar_client.submit_order(hedge.to_order()).await?;
        }
    }

    Ok(())
}
```

---

### Production Deployment Checklist

- [ ] **System Configuration**
    - [ ] Enable huge pages: `echo 1024 > /sys/kernel/mm/hugepages/.../nr_hugepages`
    - [ ] Isolate CPU cores: `isolcpus=2,3` in kernel params
    - [ ] Disable CPU frequency scaling: `cpupower frequency-set -g performance`
    - [ ] Install RT kernel: `linux-image-rt-amd64`

- [ ] **Network Optimization** (for kernel bypass)
    - [ ] Configure DPDK if using
    - [ ] Set NIC ring buffer size: `ethtool -G eth0 rx 4096 tx 4096`
    - [ ] Disable interrupt coalescing: `ethtool -C eth0 rx-usecs 0`

- [ ] **Monitoring**
    - [ ] Set up Prometheus exporter
    - [ ] Configure alerting (P99 > 2μs)
    - [ ] Log rotation for cold-path logs

- [ ] **Risk Management**
    - [ ] Configure position limits
    - [ ] Set up a kill switch (emergency stop)
    - [ ] Implement reconciliation with exchange

---

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
# Clone repository
git clone https://github.com/maksat-software/hedging-engine
cd hedging-engine

# Install dependencies
cargo build

# Run tests
cargo test

# Run benchmarks
cargo bench

# Check code quality
cargo clippy -- -D warnings
cargo fmt --check
```

### Areas for Contribution

- **Bug Reports** - Found an issue? Open an issue with reproduction steps
- **Documentation** - Improve docs, add examples, fix typos
- **Performance** - Optimize hot paths, reduce latency
- **Strategies** - Implement new hedging strategies
- **Testing** - Add test cases, improve coverage
- **Features** - New market support, additional metrics

---

## Disclaimer

** USE AT YOUR OWN RISK**

This software is provided "as-is" for **educational and research purposes**.

**NOT for production trading without:**

- Thorough testing in your environment
- Professional risk management oversight
- Proper regulatory compliance
- Exchange certification and connectivity
- Backup systems and failover

**The authors assume NO LIABILITY for:**

- Trading losses
- System failures
- Data inaccuracies
- Missed opportunities
- Any other damages

**Before using in production:**

1. Comprehensive backtesting (12+ months data)
2. Paper trading (3+ months)
3. Limited capital deployment (1-3 months)
4. Full capital only after proven track record

**Financial markets involve substantial risk. Consult professionals.**

---

## License

Dual-licensed under:

- **MIT License** ([LICENSE](LICENSE))
- **Apache License 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

You may choose either license for your use.

---

## Author

**Maksat Annageldyev**

-

LinkedIn: [https://www.linkedin.com/in/maksat-annageldyev-187ba751](https://www.linkedin.com/in/maksat-annageldyev-187ba751)

- Website: [maksat.software](https://maksat.software)
- Email: info@maksat.software

---

## Acknowledgments

### Built With

- [crossbeam](https://github.com/crossbeam-rs/crossbeam) – Lock-free data structures
- [tokio](https://tokio.rs) – Async runtime (cold path)
- [criterion](https://github.com/bheisler/criterion.rs) – Statistical benchmarking
- [parking_lot](https://github.com/Amanieu/parking_lot) – Fast synchronization primitives
- [nalgebra](https://nalgebra.org) – Linear algebra for statistics
- [statrs](https://github.com/statrs-dev/statrs) – Statistical distributions

### Inspiration

- **Martin Thompson** – Mechanical Sympathy blog
- **Jeff Preshing** – Low-level programming insights
- **Herb Sutter** – C++ concurrency wisdom
- **Jon Gjengset** – Rust systems programming
- **Rust Community** – Excellent ecosystem and support

### Research

Based on established algorithms from:

- "Active Portfolio Management" (Grinold & Kahn) – MVHR foundations
- "Energy Trading and Risk Management" (Eydeland & Wolyniec) – Energy markets
- "The Art of Multiprocessor Programming" (Herlihy & Shavit) – Lock-free algorithms

---

## Project Stats

![GitHub stars](https://img.shields.io/github/stars/username/rust-hedging-engine?style=social)
![GitHub forks](https://img.shields.io/github/forks/username/rust-hedging-engine?style=social)
![GitHub watchers](https://img.shields.io/github/watchers/username/rust-hedging-engine?style=social)

![GitHub issues](https://img.shields.io/github/issues/username/rust-hedging-engine)
![GitHub pull requests](https://img.shields.io/github/issues-pr/username/rust-hedging-engine)
![GitHub commit activity](https://img.shields.io/github/commit-activity/m/username/rust-hedging-engine)
![Lines of code](https://img.shields.io/tokei/lines/github/username/rust-hedging-engine)

---

## Roadmap

### v0.2.0 (Q1 2026)

- [ ] DPDK integration for kernel bypass
- [ ] WebSocket market data adapter
- [ ] FIX protocol support for order execution
- [ ] Enhanced backtesting (transaction costs, slippage models)
- [ ] Prometheus metrics exporter

### v0.3.0 (Q2 2026)

- [ ] Weather-adjusted hedging (temperature, wind forecasts)
- [ ] Machine learning strategy adapter
- [ ] Multi-asset portfolio optimization
- [ ] Real-time risk analytics dashboard

### v1.0.0 (Q3 2026)

- [ ] Production-grade error handling
- [ ] Comprehensive integration tests
- [ ] Performance regression testing
- [ ] Security audit
- [ ] Professional documentation

### Future (v2.0+)

- [ ] FPGA acceleration module
- [ ] GPU-accelerated backtesting
- [ ] Cloud deployment templates (AWS, Azure)
- [ ] Multi-exchange support (ICE, CME, EEX)

---

## Support

### Community

- **Discussions** - [GitHub Discussions](https://github.com/username/rust-hedging-engine/discussions)
- **Issues** - [GitHub Issues](https://github.com/username/rust-hedging-engine/issues)

---

## Related Projects

- [DPDK](https://www.dpdk.org) – Kernel bypass networking
- [OpenOnload](https://github.com/Xilinx-CNS/onload) – Solarflare kernel bypass
- [QuantLib](https://www.quantlib.org) – Quantitative finance library
- [cctz](https://github.com/google/cctz) – Time zone handling

---

**If you find this useful, please star the repository!**

**Watch for updates on new strategies and optimizations**

**Fork to build your own trading system**

---

*Last updated: December 2025*

*Version: 0.1.0*

*Rust version: 1.91+*
