// falcon/cache/stats.rs — CacheStats: per-level hit/miss/cycle counters

use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub writebacks: u64,
    pub bytes_loaded: u64,
    pub bytes_stored: u64,
    /// Accumulated cycle cost (hit_latency per hit, hit_latency+miss_penalty per miss)
    pub total_cycles: u64,
    /// Bytes actually written to RAM: write-through stores + write-back dirty evictions + WB+NoAlloc misses
    pub ram_write_bytes: u64,
    pub history: VecDeque<(f64, f64)>, // (step_f64, hit_rate_pct)
    pub miss_pcs: HashMap<u32, u64>,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64 * 100.0
        }
    }
    #[allow(dead_code)]
    pub fn miss_rate(&self) -> f64 {
        100.0 - self.hit_rate()
    }
    pub fn total_accesses(&self) -> u64 {
        self.hits + self.misses
    }
    pub fn mpki(&self, instructions: u64) -> f64 {
        if instructions == 0 {
            0.0
        } else {
            self.misses as f64 / instructions as f64 * 1000.0
        }
    }
}
