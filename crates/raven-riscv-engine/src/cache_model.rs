use std::collections::VecDeque;

use crate::capability::{
    CacheHierarchy, CacheHistory, CacheInclusionPolicy, CacheLevelConfig, CacheLevelStats,
    CacheLevelView, CacheLineView, CacheReplacementPolicy, CacheRole, CacheSetView,
    CacheWriteAllocation, CacheWritePolicy,
};

/// Small shared cache model for teaching ISAs that do not need Falcon's MMU or bus.
pub(crate) struct TeachingCache {
    instruction: Level,
    data: Level,
}

impl TeachingCache {
    pub(crate) fn new(address_bits: usize, size: usize, line_size: usize, associativity: usize) -> Self {
        let config = CacheLevelConfig {
            size,
            line_size,
            associativity,
            num_sets: (size / line_size / associativity).max(1),
            address_bits,
            replacement: CacheReplacementPolicy::Lru,
            write_policy: CacheWritePolicy::WriteThrough,
            write_allocation: CacheWriteAllocation::WriteAllocate,
            inclusion: CacheInclusionPolicy::NonInclusive,
            hit_latency: 1,
            miss_penalty: 4,
            associativity_penalty: 0,
            transfer_width: line_size,
            enabled: true,
        };
        Self {
            instruction: Level::new(config),
            data: Level::new(config),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.instruction.reset();
        self.data.reset();
    }

    pub(crate) fn fetch(&mut self, address: usize, memory: &[u8]) {
        self.instruction.access(address, memory, None);
    }

    pub(crate) fn read(&mut self, address: usize, memory: &[u8]) {
        self.data.access(address, memory, None);
    }

    pub(crate) fn write(&mut self, address: usize, bytes: &[u8], memory: &[u8]) {
        self.data.access(address, memory, Some(bytes));
    }

    fn level(&self, role: CacheRole) -> Option<&Level> {
        match role {
            CacheRole::Instruction => Some(&self.instruction),
            CacheRole::Data => Some(&self.data),
            CacheRole::Unified => None,
        }
    }
}

impl CacheHierarchy for TeachingCache {
    fn level_count(&self) -> usize {
        1
    }

    fn cache(&self, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>> {
        if level != 0 {
            return None;
        }
        let cache = self.level(role)?;
        let (first, second) = cache.history.as_slices();
        Some(CacheLevelView {
            name: match role {
                CacheRole::Instruction => "I-Cache",
                CacheRole::Data => "D-Cache",
                CacheRole::Unified => return None,
            }
            .into(),
            level: 0,
            role,
            config: cache.config,
            stats: CacheLevelStats {
                hits: cache.hits,
                misses: cache.misses,
                evictions: cache.evictions,
                writebacks: 0,
                bytes_loaded: cache.bytes_loaded,
                bytes_stored: cache.bytes_stored,
                total_cycles: cache.total_cycles,
                ram_write_bytes: cache.bytes_stored,
                history: CacheHistory::new(first, second),
            },
        })
    }

    fn set(&self, level: usize, role: CacheRole, set: usize) -> Option<CacheSetView<'_>> {
        if level != 0 {
            return None;
        }
        let cache = self.level(role)?;
        let lines = cache.sets.get(set)?;
        let mut order: Vec<_> = (0..lines.len()).collect();
        order.sort_by_key(|&way| std::cmp::Reverse(lines[way].last_used));
        Some(CacheSetView {
            lines: lines
                .iter()
                .map(|line| CacheLineView {
                    valid: line.valid,
                    dirty: false,
                    tag: line.tag,
                    data: &line.data,
                    frequency: line.frequency,
                    referenced: line.referenced,
                })
                .collect(),
            lru_order: order,
            fifo_order: (0..lines.len()).collect(),
            clock_hand: 0,
        })
    }
}

struct Level {
    config: CacheLevelConfig,
    sets: Vec<Vec<Line>>,
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    bytes_loaded: u64,
    bytes_stored: u64,
    total_cycles: u64,
    history: VecDeque<(f64, f64)>,
}

impl Level {
    fn new(config: CacheLevelConfig) -> Self {
        let line = || Line {
            valid: false,
            tag: 0,
            data: vec![0; config.line_size],
            last_used: 0,
            frequency: 0,
            referenced: false,
        };
        Self {
            config,
            sets: (0..config.num_sets)
                .map(|_| (0..config.associativity).map(|_| line()).collect())
                .collect(),
            clock: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            bytes_loaded: 0,
            bytes_stored: 0,
            total_cycles: 0,
            history: VecDeque::with_capacity(256),
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.config);
    }

    fn access(&mut self, address: usize, memory: &[u8], write: Option<&[u8]>) {
        self.clock += 1;
        let block = address / self.config.line_size;
        let set_index = block % self.config.num_sets;
        let tag = (block / self.config.num_sets) as u64;
        let hit = self.sets[set_index]
            .iter()
            .position(|line| line.valid && line.tag == tag);
        let way = if let Some(way) = hit {
            self.hits += 1;
            way
        } else {
            self.misses += 1;
            let way = self.sets[set_index]
                .iter()
                .position(|line| !line.valid)
                .unwrap_or_else(|| {
                    self.evictions += 1;
                    self.sets[set_index]
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, line)| line.last_used)
                        .map(|(way, _)| way)
                        .unwrap_or(0)
                });
            let base = block * self.config.line_size;
            let line = &mut self.sets[set_index][way];
            line.data.fill(0);
            if base < memory.len() {
                let end = (base + self.config.line_size).min(memory.len());
                line.data[..end - base].copy_from_slice(&memory[base..end]);
                self.bytes_loaded += (end - base) as u64;
            }
            line.valid = true;
            line.tag = tag;
            line.frequency = 0;
            way
        };

        let line = &mut self.sets[set_index][way];
        line.last_used = self.clock;
        line.frequency += 1;
        line.referenced = true;
        if let Some(bytes) = write {
            let offset = address % self.config.line_size;
            let count = bytes.len().min(self.config.line_size.saturating_sub(offset));
            line.data[offset..offset + count].copy_from_slice(&bytes[..count]);
            self.bytes_stored += bytes.len() as u64;
        }
        self.total_cycles += self.config.tag_search_cycles()
            + if hit.is_none() {
                self.config.miss_penalty + self.config.line_transfer_cycles()
            } else {
                0
            };
        if self.history.len() == 256 {
            self.history.pop_front();
        }
        let accesses = self.hits + self.misses;
        self.history
            .push_back((accesses as f64, self.hits as f64 / accesses as f64 * 100.0));
    }
}

struct Line {
    valid: bool,
    tag: u64,
    data: Vec<u8>,
    last_used: u64,
    frequency: u64,
    referenced: bool,
}
