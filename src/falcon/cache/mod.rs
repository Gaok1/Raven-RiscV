// falcon/cache/mod.rs — Cache simulation module

mod cache;
mod config;
mod controller;
mod policies;
mod presets;
mod stats;

pub use cache::{Cache, CacheLineView, CacheSetView};
pub use config::CacheConfig;
pub use controller::CacheController;
pub use policies::{InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
pub use presets::{cache_presets, extra_level_presets};
pub use stats::CacheStats;

#[cfg(test)]
#[path = "../../../tests/support/falcon_cache.rs"]
mod tests;
