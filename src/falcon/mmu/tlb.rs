// falcon/mmu/tlb.rs — unified TLB shared by I- and D-side translations.
//
// Phase 1 carries the full Phase 2/3 struct shapes so downstream code (cache
// controller, settings, UI) can be wired without churn. A handful of fields
// are unread until the walker arrives — `#[allow(dead_code)]` reflects that.

#![allow(dead_code)]

use crate::falcon::cache::ReplacementPolicy;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PtePerms {
    pub r: bool,
    pub w: bool,
    pub x: bool,
    pub u: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TlbEntry {
    pub valid: bool,
    pub vpn: u32,
    pub ppn: u32,
    pub asid: u16,
    pub perms: PtePerms,
    pub global: bool,
    pub accessed: bool,
    pub dirty: bool,
    /// L1 leaf — masks vpn[0] (10 LSB) during lookup so a single entry covers
    /// the whole 4 MiB superpage.
    pub megapage: bool,
    /// LRU/FIFO age counter — bumped on access (LRU) or install (FIFO).
    pub age: u32,
}

#[derive(Clone, Debug)]
pub struct TlbConfig {
    /// Total entries (power of two, ≥ associativity).
    pub entry_count: u16,
    pub associativity: u8,
    pub replacement: ReplacementPolicy,
    pub hit_latency: u8,
    pub miss_penalty: u8,
}

impl Default for TlbConfig {
    fn default() -> Self {
        Self {
            entry_count: 32,
            associativity: 4,
            replacement: ReplacementPolicy::Lru,
            hit_latency: 1,
            miss_penalty: 20,
        }
    }
}

#[derive(Default)]
pub struct TlbStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub page_faults: u64,
    pub total_cycles: u64,
    /// (step, hit_rate_pct) — 300-cycle rolling window for the Stats UI.
    pub history: VecDeque<(f64, f64)>,
}

impl TlbStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64 * 100.0
        }
    }
}

pub struct Tlb {
    pub config: TlbConfig,
    pub entries: Vec<TlbEntry>,
    pub stats: TlbStats,
    age_counter: u32,
}

impl Tlb {
    pub fn new(config: TlbConfig) -> Self {
        let n = (config.entry_count.max(1) as usize).next_power_of_two();
        Self {
            entries: vec![TlbEntry::default(); n],
            stats: TlbStats::default(),
            age_counter: 0,
            config,
        }
    }

    pub fn flush(&mut self) {
        for e in self.entries.iter_mut() {
            e.valid = false;
        }
    }

    /// Reconfigure the TLB. Resets entries; preserves stats unless `reset_stats`.
    pub fn reconfigure(&mut self, cfg: TlbConfig, reset_stats: bool) {
        *self = Self::new(cfg);
        if !reset_stats {
            // Caller can swap stats back in if they want; current API resets.
        }
    }
}
