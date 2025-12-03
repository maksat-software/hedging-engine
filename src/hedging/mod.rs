//! Hedging strategies and execution engine

mod config;
mod delta;
mod engine;
mod mean_reversion;
mod mvhr;
mod spark_spread;

pub use config::{HedgeConfig, HedgeRecommendation, Urgency};
pub use delta::DeltaHedge;
pub use engine::HedgeEngine;
pub use mean_reversion::{MeanReversionHedge, MeanReversionStats};
pub use mvhr::{MVHRStatistics, MVHRStrategy};
pub use spark_spread::{
    CostsBreakdown, SparkSpreadHedge, SparkSpreadPositions, SparkSpreadRecommendations,
};
