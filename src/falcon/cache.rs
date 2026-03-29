// falcon/cache.rs — Cache simulation (I-cache + D-cache)
use std::collections::{HashMap, VecDeque};

use crate::falcon::{
    errors::FalconError,
    memory::{Bus, Ram},
};

// ── Policies ────────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementPolicy {
    /// Least Recently Used — evicts the way not accessed for longest
    Lru,
    /// First In First Out — evicts oldest installed line
    Fifo,
    /// Pseudo-random via LCG
    Random,
    /// Least Frequently Used — evicts way with fewest accesses
    Lfu,
    /// Clock (Second Chance) — circular pointer with reference bit
    Clock,
    /// Most Recently Used — evicts most recently accessed (good for scans)
    Mru,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteAllocPolicy {
    WriteAllocate,
    NoWriteAllocate,
}

/// Inclusion policy between this cache level and the NEXT level below it.
/// Applies to all levels except the last (which has no level below it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InclusionPolicy {
    /// No constraint — data may or may not appear in both levels (NINE).
    #[default]
    NonInclusive,
    /// Every line in this level is guaranteed to also exist in the level below.
    Inclusive,
    /// Lines in this level are guaranteed NOT to exist in the level below.
    Exclusive,
}

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub size: usize,          // total bytes
    pub line_size: usize,     // bytes per line
    pub associativity: usize, // ways per set
    pub replacement: ReplacementPolicy,
    pub write_policy: WritePolicy,
    pub write_alloc: WriteAllocPolicy,
    /// Inclusion/exclusion policy with respect to the next cache level.
    /// Ignored for the last (lowest) cache level.
    pub inclusion: InclusionPolicy,
    /// Cycles consumed on a cache hit
    pub hit_latency: u64,
    /// Extra cycles added on a cache miss (stall waiting for RAM)
    pub miss_penalty: u64,
    /// Extra cycles per additional way beyond the first during tag search
    pub assoc_penalty: u64,
    /// Bus width in bytes; transfer cost = ceil(line_size / transfer_width)
    pub transfer_width: u32,
}

impl CacheConfig {
    pub fn num_sets(&self) -> usize {
        self.size / (self.line_size * self.associativity)
    }
    pub fn offset_bits(&self) -> u32 {
        self.line_size.trailing_zeros()
    }
    pub fn index_bits(&self) -> u32 {
        self.num_sets().trailing_zeros()
    }
    pub fn addr_tag(&self, addr: u32) -> u32 {
        addr >> (self.offset_bits() + self.index_bits())
    }
    pub fn addr_index(&self, addr: u32) -> usize {
        let mask = (self.num_sets() as u32).saturating_sub(1);
        ((addr >> self.offset_bits()) & mask) as usize
    }
    pub fn addr_offset(&self, addr: u32) -> usize {
        (addr & (self.line_size as u32 - 1)) as usize
    }
    pub fn line_base(&self, addr: u32) -> u32 {
        addr & !(self.line_size as u32 - 1)
    }
    /// Returns Ok(()) if the config is usable, or an Err with a human-readable reason.
    pub fn validate(&self) -> Result<(), String> {
        if self.size == 0 {
            return Err("Size must be > 0".to_string());
        }
        if self.line_size < 4 {
            return Err(format!("Line size must be ≥ 4 B (got {})", self.line_size));
        }
        if self.associativity == 0 {
            return Err("Associativity must be ≥ 1".to_string());
        }
        if !self.line_size.is_power_of_two() {
            return Err(format!(
                "Line size {} is not a power of 2 (try {})",
                self.line_size,
                self.line_size.next_power_of_two()
            ));
        }
        let bytes_per_set = match self.line_size.checked_mul(self.associativity) {
            Some(v) => v,
            None => return Err("assoc × line_size overflows usize".to_string()),
        };
        if bytes_per_set > self.size {
            return Err(format!(
                "assoc({}) × line({}) = {} B > size({} B): need ≥ 1 set",
                self.associativity, self.line_size, bytes_per_set, self.size
            ));
        }
        if self.size % bytes_per_set != 0 {
            return Err(format!(
                "size({}) not divisible by assoc({}) × line({})",
                self.size, self.associativity, self.line_size
            ));
        }
        let sets = self.size / bytes_per_set;
        if !sets.is_power_of_two() {
            let next = sets.next_power_of_two();
            return Err(format!(
                "{sets} sets is not a power of 2 (try size={})",
                bytes_per_set * next
            ));
        }
        if !matches!(self.inclusion, InclusionPolicy::NonInclusive) {
            return Err(
                "Inclusive and Exclusive policies are not yet implemented; use NonInclusive"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn is_valid_config(&self) -> bool {
        self.validate().is_ok()
    }

    /// Cycles consumed for a tag lookup: hit_latency + (ways-1) * assoc_penalty
    pub fn tag_search_cycles(&self) -> u64 {
        self.hit_latency + (self.associativity as u64).saturating_sub(1) * self.assoc_penalty
    }

    /// Cycles to transfer one cache line over the bus: ceil(line_size / transfer_width)
    pub fn line_transfer_cycles(&self) -> u64 {
        let w = self.transfer_width.max(1) as u64;
        ((self.line_size as u64) + w - 1) / w
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            size: 1024,
            line_size: 16,
            associativity: 2,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 1,
            miss_penalty: 50,
            assoc_penalty: 1,
            transfer_width: 8,
        }
    }
}

// ── Internal cache structures ────────────────────────────────────────────────

#[derive(Clone)]
struct CacheLine {
    valid: bool,
    tag: u32,
    dirty: bool,
    data: Vec<u8>,
    freq: u64,     // LFU: access frequency counter
    ref_bit: bool, // Clock: reference bit
}

impl CacheLine {
    fn new(line_size: usize) -> Self {
        Self {
            valid: false,
            tag: 0,
            dirty: false,
            data: vec![0; line_size],
            freq: 0,
            ref_bit: false,
        }
    }
}

struct CacheSet {
    lines: Vec<CacheLine>,
    lru_order: VecDeque<usize>,  // front=MRU, back=LRU (LRU & MRU)
    fifo_order: VecDeque<usize>, // front=newest, back=oldest (FIFO)
    rand_state: u32,
    clock_hand: usize, // Clock: current sweep position
}

impl CacheSet {
    fn new(ways: usize, line_size: usize) -> Self {
        Self {
            lines: (0..ways).map(|_| CacheLine::new(line_size)).collect(),
            lru_order: (0..ways).collect(),
            fifo_order: (0..ways).collect(),
            rand_state: 0xDEAD_BEEF,
            clock_hand: 0,
        }
    }

    fn lookup(&self, tag: u32) -> Option<usize> {
        self.lines.iter().position(|l| l.valid && l.tag == tag)
    }

    fn find_victim(&mut self, policy: ReplacementPolicy) -> usize {
        // Always prefer invalid lines
        if let Some(idx) = self.lines.iter().position(|l| !l.valid) {
            return idx;
        }
        let n = self.lines.len();
        match policy {
            ReplacementPolicy::Lru => *self.lru_order.back().unwrap_or(&0),
            ReplacementPolicy::Mru => *self.lru_order.front().unwrap_or(&0),
            ReplacementPolicy::Fifo => *self.fifo_order.back().unwrap_or(&0),
            ReplacementPolicy::Random => {
                self.rand_state = self
                    .rand_state
                    .wrapping_mul(1664525)
                    .wrapping_add(1013904223);
                (self.rand_state as usize) % n
            }
            ReplacementPolicy::Lfu => {
                // Evict way with lowest frequency (ties broken by LRU order)
                let min_freq = self.lines.iter().map(|l| l.freq).min().unwrap_or(0);
                // Among min-freq lines, take the LRU one
                self.lru_order
                    .iter()
                    .rev()
                    .find(|&&w| self.lines[w].freq == min_freq)
                    .copied()
                    .unwrap_or(0)
            }
            ReplacementPolicy::Clock => {
                // Sweep clock hand; skip lines with ref_bit=true (give second chance)
                loop {
                    let way = self.clock_hand % n;
                    self.clock_hand = (self.clock_hand + 1) % n;
                    if self.lines[way].ref_bit {
                        self.lines[way].ref_bit = false; // clear and give second chance
                    } else {
                        return way;
                    }
                }
            }
        }
    }

    fn touch(&mut self, way: usize, policy: ReplacementPolicy) {
        match policy {
            ReplacementPolicy::Lru | ReplacementPolicy::Mru => {
                if let Some(pos) = self.lru_order.iter().position(|&w| w == way) {
                    self.lru_order.remove(pos);
                    self.lru_order.push_front(way);
                }
            }
            ReplacementPolicy::Lfu => {
                self.lines[way].freq = self.lines[way].freq.saturating_add(1);
                // Also update LRU order for tie-breaking
                if let Some(pos) = self.lru_order.iter().position(|&w| w == way) {
                    self.lru_order.remove(pos);
                    self.lru_order.push_front(way);
                }
            }
            ReplacementPolicy::Clock => {
                self.lines[way].ref_bit = true;
            }
            _ => {}
        }
    }

    fn install(&mut self, way: usize, tag: u32, data: Vec<u8>, policy: ReplacementPolicy) {
        self.lines[way] = CacheLine {
            valid: true,
            tag,
            dirty: false,
            data,
            freq: 1,
            ref_bit: true,
        };
        match policy {
            ReplacementPolicy::Lru | ReplacementPolicy::Mru | ReplacementPolicy::Lfu => {
                if let Some(pos) = self.lru_order.iter().position(|&w| w == way) {
                    self.lru_order.remove(pos);
                }
                self.lru_order.push_front(way);
            }
            ReplacementPolicy::Fifo => {
                if let Some(pos) = self.fifo_order.iter().position(|&w| w == way) {
                    self.fifo_order.remove(pos);
                }
                self.fifo_order.push_front(way);
            }
            ReplacementPolicy::Clock | ReplacementPolicy::Random => {}
        }
    }
}

// ── Stats ────────────────────────────────────────────────────────────────────

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

// ── Cache ────────────────────────────────────────────────────────────────────

pub struct Cache {
    pub config: CacheConfig,
    sets: Vec<CacheSet>,
    pub stats: CacheStats,
}

impl Cache {
    pub fn new(config: CacheConfig) -> Self {
        let num_sets = if config.is_valid_config() {
            config.num_sets()
        } else {
            1
        };
        let ways = config.associativity.max(1);
        let line_size = config.line_size.max(4);
        Self {
            sets: (0..num_sets)
                .map(|_| CacheSet::new(ways, line_size))
                .collect(),
            config,
            stats: CacheStats::default(),
        }
    }

    pub fn invalidate(&mut self) {
        for set in &mut self.sets {
            for line in &mut set.lines {
                line.valid = false;
                line.dirty = false;
            }
        }
    }

    /// Invalidate (without writeback) the single line covering `addr`, if present.
    pub fn invalidate_line(&mut self, addr: u32) {
        if !self.config.is_valid_config() {
            return;
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        if let Some(way) = self.sets[idx].lookup(tag) {
            self.sets[idx].lines[way].valid = false;
            self.sets[idx].lines[way].dirty = false;
        }
    }

    /// Write all dirty lines back to RAM, then invalidate.
    pub fn flush_to_ram(&mut self, ram: &mut Ram) {
        if !self.config.is_valid_config() {
            return;
        }
        let offset_bits = self.config.offset_bits();
        let index_bits = self.config.index_bits();
        for (set_idx, set) in self.sets.iter_mut().enumerate() {
            for line in &mut set.lines {
                if line.valid && line.dirty {
                    // Reconstruct line base: tag bits above (offset+index), set index in the middle
                    let base = (line.tag << (offset_bits + index_bits))
                        | ((set_idx as u32) << offset_bits);
                    for (i, &byte) in line.data.iter().enumerate() {
                        // Best-effort: ignore write errors (out-of-bounds addresses)
                        let _ = ram.store8(base.wrapping_add(i as u32), byte);
                    }
                }
                line.valid = false;
                line.dirty = false;
            }
        }
    }

    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    /// If this address is covered by a dirty cache line, return the cached byte.
    pub fn peek_dirty(&self, addr: u32) -> Option<u8> {
        if !self.config.is_valid_config() {
            return None;
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);
        let way = self.sets[idx].lookup(tag)?;
        let line = &self.sets[idx].lines[way];
        if line.dirty {
            Some(line.data[offset])
        } else {
            None
        }
    }

    /// Returns Some(dirty) if this address is present in the cache, None if not.
    pub fn has_line(&self, addr: u32) -> Option<bool> {
        if !self.config.is_valid_config() {
            return None;
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let way = self.sets[idx].lookup(tag)?;
        Some(self.sets[idx].lines[way].dirty)
    }

    /// Allocate a line (read-only path — I-cache, no dirty eviction writes).
    fn allocate_ro(&mut self, addr: u32, ram: &Ram) -> Result<(), FalconError> {
        if !self.config.is_valid_config() {
            return Ok(());
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let line_base = self.config.line_base(addr);
        let way = self.sets[idx].find_victim(self.config.replacement);

        let evicted = &self.sets[idx].lines[way];
        if evicted.valid {
            self.stats.evictions += 1;
        }

        let line_size = self.config.line_size;
        let mut data = vec![0u8; line_size];
        for (i, slot) in data.iter_mut().enumerate() {
            *slot = ram.load8(line_base + i as u32)?;
        }
        self.stats.bytes_loaded += line_size as u64;
        self.sets[idx].install(way, tag, data, self.config.replacement);
        Ok(())
    }

    /// Allocate a line (read-write path — D-cache, may write back dirty eviction).
    fn allocate_rw(&mut self, addr: u32, ram: &mut Ram) -> Result<(), FalconError> {
        if !self.config.is_valid_config() {
            return Ok(());
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let line_base = self.config.line_base(addr);
        let way = self.sets[idx].find_victim(self.config.replacement);

        let evicted = self.sets[idx].lines[way].clone();
        if evicted.valid {
            self.stats.evictions += 1;
            if evicted.dirty {
                let offset_bits = self.config.offset_bits();
                let index_bits = self.config.index_bits();
                let evict_base =
                    (evicted.tag << (offset_bits + index_bits)) | ((idx as u32) << offset_bits);
                for (i, &b) in evicted.data.iter().enumerate() {
                    ram.store8(evict_base + i as u32, b)?;
                    self.stats.ram_write_bytes += 1;
                }
                self.stats.writebacks += 1;
                self.stats.total_cycles +=
                    self.config.miss_penalty + self.config.line_transfer_cycles(); // dirty eviction cost
            }
        }

        let line_size = self.config.line_size;
        let mut data = vec![0u8; line_size];
        for (i, slot) in data.iter_mut().enumerate() {
            *slot = ram.load8(line_base + i as u32)?;
        }
        self.stats.bytes_loaded += line_size as u64;
        self.sets[idx].install(way, tag, data, self.config.replacement);
        Ok(())
    }

    /// Read byte via I-cache (read-only, no dirty evictions). Charges cycles.
    #[allow(dead_code)]
    pub fn read_byte_ro(&mut self, addr: u32, ram: &Ram) -> Result<u8, FalconError> {
        if !self.config.is_valid_config() {
            return ram.load8(addr);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);

        if let Some(way) = self.sets[idx].lookup(tag) {
            self.stats.hits += 1;
            self.stats.total_cycles += self.config.tag_search_cycles();
            self.sets[idx].touch(way, self.config.replacement);
            return Ok(self.sets[idx].lines[way].data[offset]);
        }
        self.stats.misses += 1;
        self.stats.total_cycles += self.config.tag_search_cycles()
            + self.config.miss_penalty
            + self.config.line_transfer_cycles();
        self.allocate_ro(addr, ram)?;
        let way = self.sets[idx].lookup(tag).unwrap();
        Ok(self.sets[idx].lines[way].data[offset])
    }

    /// Write byte via D-cache.
    pub fn write_byte(&mut self, addr: u32, val: u8, ram: &mut Ram) -> Result<(), FalconError> {
        if !self.config.is_valid_config() {
            return ram.store8(addr, val);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);
        let hit_way = self.sets[idx].lookup(tag);

        match self.config.write_policy {
            WritePolicy::WriteThrough => {
                ram.store8(addr, val)?;
                self.stats.ram_write_bytes += 1; // write-through always writes RAM
                self.stats.bytes_stored += 1;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    // tag_search (cache lookup) + miss_penalty (write-through RAM write)
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    self.sets[idx].lines[way].data[offset] = val;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    // tag_search (tag lookup) + miss_penalty (RAM write)
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        self.stats.total_cycles +=
                            self.config.miss_penalty + self.config.line_transfer_cycles(); // fill extra
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = val;
                        }
                    }
                }
            }
            WritePolicy::WriteBack => {
                self.stats.bytes_stored += 1;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles();
                    self.sets[idx].lines[way].data[offset] = val;
                    self.sets[idx].lines[way].dirty = true;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles()
                        + self.config.miss_penalty
                        + self.config.line_transfer_cycles();
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = val;
                            self.sets[idx].lines[way].dirty = true;
                        }
                    } else {
                        ram.store8(addr, val)?;
                        self.stats.ram_write_bytes += 1; // WB + no-alloc miss: goes straight to RAM
                    }
                }
            }
        }
        Ok(())
    }

    /// Write 16-bit halfword via D-cache — single tag lookup (correct stats for `sh`).
    pub fn write_halfword(
        &mut self,
        addr: u32,
        val: u16,
        ram: &mut Ram,
    ) -> Result<(), FalconError> {
        if !self.config.is_valid_config() {
            return ram.store16(addr, val);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);
        let hit_way = self.sets[idx].lookup(tag);
        let [b0, b1] = val.to_le_bytes();

        // Misaligned halfword stores can straddle cache lines.
        if offset + 1 >= self.config.line_size {
            self.write_byte(addr, b0, ram)?;
            self.write_byte(addr.wrapping_add(1), b1, ram)?;
            return Ok(());
        }

        match self.config.write_policy {
            WritePolicy::WriteThrough => {
                ram.store16(addr, val)?;
                self.stats.ram_write_bytes += 2;
                self.stats.bytes_stored += 2;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    self.sets[idx].lines[way].data[offset] = b0;
                    self.sets[idx].lines[way].data[offset + 1] = b1;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        self.stats.total_cycles +=
                            self.config.miss_penalty + self.config.line_transfer_cycles();
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = b0;
                            self.sets[idx].lines[way].data[offset + 1] = b1;
                        }
                    }
                }
            }
            WritePolicy::WriteBack => {
                self.stats.bytes_stored += 2;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles();
                    self.sets[idx].lines[way].data[offset] = b0;
                    self.sets[idx].lines[way].data[offset + 1] = b1;
                    self.sets[idx].lines[way].dirty = true;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles()
                        + self.config.miss_penalty
                        + self.config.line_transfer_cycles();
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = b0;
                            self.sets[idx].lines[way].data[offset + 1] = b1;
                            self.sets[idx].lines[way].dirty = true;
                        }
                    } else {
                        ram.store16(addr, val)?;
                        self.stats.ram_write_bytes += 2;
                    }
                }
            }
        }
        Ok(())
    }

    /// Write 32-bit word via D-cache — single tag lookup (correct stats for `sw`).
    pub fn write_word(&mut self, addr: u32, val: u32, ram: &mut Ram) -> Result<(), FalconError> {
        if !self.config.is_valid_config() {
            return ram.store32(addr, val);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);
        let hit_way = self.sets[idx].lookup(tag);
        let [b0, b1, b2, b3] = val.to_le_bytes();

        // Misaligned word stores can straddle cache lines.
        if offset + 3 >= self.config.line_size {
            self.write_byte(addr, b0, ram)?;
            self.write_byte(addr.wrapping_add(1), b1, ram)?;
            self.write_byte(addr.wrapping_add(2), b2, ram)?;
            self.write_byte(addr.wrapping_add(3), b3, ram)?;
            return Ok(());
        }

        match self.config.write_policy {
            WritePolicy::WriteThrough => {
                ram.store32(addr, val)?;
                self.stats.ram_write_bytes += 4;
                self.stats.bytes_stored += 4;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    self.sets[idx].lines[way].data[offset] = b0;
                    self.sets[idx].lines[way].data[offset + 1] = b1;
                    self.sets[idx].lines[way].data[offset + 2] = b2;
                    self.sets[idx].lines[way].data[offset + 3] = b3;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles +=
                        self.config.tag_search_cycles() + self.config.miss_penalty;
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        self.stats.total_cycles +=
                            self.config.miss_penalty + self.config.line_transfer_cycles();
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = b0;
                            self.sets[idx].lines[way].data[offset + 1] = b1;
                            self.sets[idx].lines[way].data[offset + 2] = b2;
                            self.sets[idx].lines[way].data[offset + 3] = b3;
                        }
                    }
                }
            }
            WritePolicy::WriteBack => {
                self.stats.bytes_stored += 4;
                if let Some(way) = hit_way {
                    self.stats.hits += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles();
                    self.sets[idx].lines[way].data[offset] = b0;
                    self.sets[idx].lines[way].data[offset + 1] = b1;
                    self.sets[idx].lines[way].data[offset + 2] = b2;
                    self.sets[idx].lines[way].data[offset + 3] = b3;
                    self.sets[idx].lines[way].dirty = true;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles += self.config.tag_search_cycles()
                        + self.config.miss_penalty
                        + self.config.line_transfer_cycles();
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
                        if let Some(way) = self.sets[idx].lookup(tag) {
                            self.sets[idx].lines[way].data[offset] = b0;
                            self.sets[idx].lines[way].data[offset + 1] = b1;
                            self.sets[idx].lines[way].data[offset + 2] = b2;
                            self.sets[idx].lines[way].data[offset + 3] = b3;
                            self.sets[idx].lines[way].dirty = true;
                        }
                    } else {
                        ram.store32(addr, val)?;
                        self.stats.ram_write_bytes += 4;
                    }
                }
            }
        }
        Ok(())
    }
}

// ── UI introspection structs ─────────────────────────────────────────────────

/// Snapshot of a single cache line for UI display (no stats side-effects).
pub struct CacheLineView {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u32,
    pub data: Vec<u8>,
    pub freq: u64,     // LFU frequency counter
    pub ref_bit: bool, // Clock reference bit
}

/// Snapshot of a single cache set for UI display.
pub struct CacheSetView {
    pub lines: Vec<CacheLineView>,
    /// LRU order: index 0 = MRU (most recent), last = LRU (eviction candidate).
    pub lru_order: Vec<usize>,
    /// FIFO order: index 0 = newest, last = oldest (eviction candidate).
    pub fifo_order: Vec<usize>,
    /// Clock algorithm sweep position (next line to inspect on eviction).
    pub clock_hand: usize,
}

impl Cache {
    /// Returns a read-only snapshot of all sets for UI rendering.
    /// Does not touch stats or any mutable state.
    pub fn view(&self) -> Vec<CacheSetView> {
        self.sets
            .iter()
            .map(|set| CacheSetView {
                lines: set
                    .lines
                    .iter()
                    .map(|l| CacheLineView {
                        valid: l.valid,
                        dirty: l.dirty,
                        tag: l.tag,
                        data: l.data.clone(),
                        freq: l.freq,
                        ref_bit: l.ref_bit,
                    })
                    .collect(),
                lru_order: set.lru_order.iter().copied().collect(),
                fifo_order: set.fifo_order.iter().copied().collect(),
                clock_hand: set.clock_hand,
            })
            .collect()
    }
}

// ── Presets ──────────────────────────────────────────────────────────────────

/// Returns [Small, Medium, Large] preset configs for I-cache or D-cache.
pub fn cache_presets(icache: bool) -> [CacheConfig; 3] {
    if icache {
        [
            CacheConfig {
                size: 256,
                line_size: 16,
                associativity: 1,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 1024,
                line_size: 16,
                associativity: 2,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 4096,
                line_size: 32,
                associativity: 4,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 50,
                assoc_penalty: 1,
                transfer_width: 8,
            },
        ]
    } else {
        [
            CacheConfig {
                size: 256,
                line_size: 16,
                associativity: 1,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 1024,
                line_size: 16,
                associativity: 2,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
            CacheConfig {
                size: 8192,
                line_size: 32,
                associativity: 4,
                replacement: ReplacementPolicy::Lru,
                write_policy: WritePolicy::WriteBack,
                write_alloc: WriteAllocPolicy::WriteAllocate,
                inclusion: InclusionPolicy::NonInclusive,
                hit_latency: 1,
                miss_penalty: 100,
                assoc_penalty: 1,
                transfer_width: 8,
            },
        ]
    }
}

// ── Extra-level presets ───────────────────────────────────────────────────────

/// Returns [Small, Medium, Large] preset configs for a unified L2/L3 cache.
pub fn extra_level_presets() -> [CacheConfig; 3] {
    [
        CacheConfig {
            size: 8192,
            line_size: 64,
            associativity: 4,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 5,
            miss_penalty: 200,
            assoc_penalty: 1,
            transfer_width: 8,
        },
        CacheConfig {
            size: 65536,
            line_size: 64,
            associativity: 8,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 10,
            miss_penalty: 400,
            assoc_penalty: 1,
            transfer_width: 8,
        },
        CacheConfig {
            size: 524288,
            line_size: 128,
            associativity: 16,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            inclusion: InclusionPolicy::NonInclusive,
            hit_latency: 20,
            miss_penalty: 600,
            assoc_penalty: 1,
            transfer_width: 8,
        },
    ]
}

// ── CacheController ──────────────────────────────────────────────────────────

pub struct CacheController {
    pub ram: Ram,
    pub icache: Cache,
    pub dcache: Cache,
    /// Extra unified cache levels: extra_levels[0]=L2, extra_levels[1]=L3, …
    pub extra_levels: Vec<Cache>,
    pub instruction_count: u64,
    /// Base instruction-execution cycles (not cache): set via add_instruction_cycles().
    pub extra_cycles: u64,
    step_count: u64,
    /// When true, all cache lookups are skipped and RAM is accessed directly (no stats, no latency).
    pub bypass: bool,
}

impl CacheController {
    pub fn new(
        icfg: CacheConfig,
        dcfg: CacheConfig,
        extra_cfgs: Vec<CacheConfig>,
        mem_size: usize,
    ) -> Self {
        Self {
            ram: Ram::new(mem_size),
            icache: Cache::new(icfg),
            dcache: Cache::new(dcfg),
            extra_levels: extra_cfgs.into_iter().map(Cache::new).collect(),
            instruction_count: 0,
            extra_cycles: 0,
            step_count: 0,
            bypass: false,
        }
    }

    /// Accumulate base instruction-execution cycles (not cache latency).
    pub fn add_instruction_cycles(&mut self, cycles: u64) {
        self.extra_cycles += cycles;
    }

    /// Called once per executed instruction to record history snapshots.
    pub fn snapshot_stats(&mut self) {
        let step = self.step_count as f64;
        self.step_count += 1;

        let i_rate = self.icache.stats.hit_rate();
        let d_rate = self.dcache.stats.hit_rate();

        const MAX_HISTORY: usize = 300;
        if self.icache.stats.history.len() >= MAX_HISTORY {
            self.icache.stats.history.pop_front();
        }
        self.icache.stats.history.push_back((step, i_rate));

        if self.dcache.stats.history.len() >= MAX_HISTORY {
            self.dcache.stats.history.pop_front();
        }
        self.dcache.stats.history.push_back((step, d_rate));

        for level in &mut self.extra_levels {
            let rate = level.stats.hit_rate();
            if level.stats.history.len() >= MAX_HISTORY {
                level.stats.history.pop_front();
            }
            level.stats.history.push_back((step, rate));
        }
    }

    pub fn reset_stats(&mut self) {
        self.icache.reset_stats();
        self.dcache.reset_stats();
        for level in &mut self.extra_levels {
            level.reset_stats();
        }
        self.instruction_count = 0;
        self.extra_cycles = 0;
        self.step_count = 0;
    }

    pub fn apply_config(
        &mut self,
        icfg: CacheConfig,
        dcfg: CacheConfig,
        extra_cfgs: Vec<CacheConfig>,
    ) {
        self.icache = Cache::new(icfg);
        self.dcache = Cache::new(dcfg);
        self.extra_levels = extra_cfgs.into_iter().map(Cache::new).collect();
        self.reset_stats();
    }

    pub fn total_program_cycles(&self) -> u64 {
        let mut total =
            self.icache.stats.total_cycles + self.dcache.stats.total_cycles + self.extra_cycles;
        for level in &self.extra_levels {
            total += level.stats.total_cycles;
        }
        total
    }

    pub fn total_cache_cycles(&self) -> u64 {
        let mut total = self.icache.stats.total_cycles + self.dcache.stats.total_cycles;
        for level in &self.extra_levels {
            total += level.stats.total_cycles;
        }
        total
    }

    fn dcache_store_bytes(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FalconError> {
        if self.bypass {
            for (i, &b) in bytes.iter().enumerate() {
                self.ram.store8(addr.wrapping_add(i as u32), b)?;
            }
            return Ok(());
        }
        if !self.dcache.config.is_valid_config() {
            for (i, &b) in bytes.iter().enumerate() {
                self.ram.store8(addr.wrapping_add(i as u32), b)?;
            }
            return Ok(());
        }

        let line_size = self.dcache.config.line_size;
        let offset = self.dcache.config.addr_offset(addr);
        if offset + bytes.len().saturating_sub(1) >= line_size {
            for (i, &b) in bytes.iter().enumerate() {
                self.dcache_store_bytes(addr.wrapping_add(i as u32), &[b])?;
            }
            return Ok(());
        }

        let tag_search = self.dcache.config.tag_search_cycles();
        let miss_penalty = self.dcache.config.miss_penalty;
        let transfer_cyc = self.dcache.config.line_transfer_cycles();
        let replacement = self.dcache.config.replacement;
        let offset_bits = self.dcache.config.offset_bits();
        let index_bits = self.dcache.config.index_bits();
        let line_base = self.dcache.config.line_base(addr);
        let tag = self.dcache.config.addr_tag(addr);
        let idx = self.dcache.config.addr_index(addr);
        let hit_way = self.dcache.sets[idx].lookup(tag);

        match self.dcache.config.write_policy {
            WritePolicy::WriteThrough => {
                for (i, &b) in bytes.iter().enumerate() {
                    self.ram.store8(addr.wrapping_add(i as u32), b)?;
                }
                self.dcache.stats.ram_write_bytes += bytes.len() as u64;
                self.dcache.stats.bytes_stored += bytes.len() as u64;
                if let Some(way) = hit_way {
                    self.dcache.stats.hits += 1;
                    self.dcache.stats.total_cycles += tag_search + miss_penalty;
                    for (i, &b) in bytes.iter().enumerate() {
                        self.dcache.sets[idx].lines[way].data[offset + i] = b;
                    }
                    self.dcache.sets[idx].touch(way, replacement);
                } else {
                    self.dcache.stats.misses += 1;
                    self.dcache.stats.total_cycles += tag_search + miss_penalty;
                    if let WriteAllocPolicy::WriteAllocate = self.dcache.config.write_alloc {
                        let line_data = self.fetch_line(line_base, line_size, 0)?;
                        Self::install_dcache_line(
                            &mut self.dcache,
                            &mut self.ram,
                            idx,
                            tag,
                            replacement,
                            offset_bits,
                            index_bits,
                            miss_penalty,
                            transfer_cyc,
                            line_data,
                        )?;
                        if let Some(way) = self.dcache.sets[idx].lookup(tag) {
                            for (i, &b) in bytes.iter().enumerate() {
                                self.dcache.sets[idx].lines[way].data[offset + i] = b;
                            }
                        }
                    }
                }
            }
            WritePolicy::WriteBack => {
                self.dcache.stats.bytes_stored += bytes.len() as u64;
                if let Some(way) = hit_way {
                    self.dcache.stats.hits += 1;
                    self.dcache.stats.total_cycles += tag_search;
                    for (i, &b) in bytes.iter().enumerate() {
                        self.dcache.sets[idx].lines[way].data[offset + i] = b;
                    }
                    self.dcache.sets[idx].lines[way].dirty = true;
                    self.dcache.sets[idx].touch(way, replacement);
                } else {
                    self.dcache.stats.misses += 1;
                    self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;
                    if let WriteAllocPolicy::WriteAllocate = self.dcache.config.write_alloc {
                        let line_data = self.fetch_line(line_base, line_size, 0)?;
                        Self::install_dcache_line(
                            &mut self.dcache,
                            &mut self.ram,
                            idx,
                            tag,
                            replacement,
                            offset_bits,
                            index_bits,
                            miss_penalty,
                            transfer_cyc,
                            line_data,
                        )?;
                        if let Some(way) = self.dcache.sets[idx].lookup(tag) {
                            for (i, &b) in bytes.iter().enumerate() {
                                self.dcache.sets[idx].lines[way].data[offset + i] = b;
                            }
                            self.dcache.sets[idx].lines[way].dirty = true;
                        }
                    } else {
                        for (i, &b) in bytes.iter().enumerate() {
                            self.ram.store8(addr.wrapping_add(i as u32), b)?;
                        }
                        self.dcache.stats.ram_write_bytes += bytes.len() as u64;
                    }
                }
            }
        }

        // After any L1 D-cache write, invalidate the matching line in lower
        // unified levels so they don't serve stale data on a future L1 miss.
        // We only do this when the cache is actually active (not bypassed/invalid).
        if !self.bypass && self.dcache.config.is_valid_config() {
            for level in &mut self.extra_levels {
                level.invalidate_line(addr);
            }
        }

        Ok(())
    }

    fn measure_cache_latency<T, F>(&mut self, access: F) -> (Result<T, FalconError>, u64)
    where
        F: FnOnce(&mut Self) -> Result<T, FalconError>,
    {
        let before = self.total_cache_cycles();
        let result = access(self);
        let after = self.total_cache_cycles();
        (result, after.saturating_sub(before))
    }

    pub fn fetch32_timed(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::fetch32(mem, addr))
    }

    pub fn fetch32_timed_no_count(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        let before_instr = self.instruction_count;
        let (result, latency) = self.measure_cache_latency(|mem| <Self as Bus>::fetch32(mem, addr));
        self.instruction_count = before_instr;
        (result, latency)
    }

    pub fn dcache_read8_timed(&mut self, addr: u32) -> (Result<u8, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::dcache_read8(mem, addr))
    }

    pub fn dcache_read16_timed(&mut self, addr: u32) -> (Result<u16, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::dcache_read16(mem, addr))
    }

    pub fn dcache_read32_timed(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::dcache_read32(mem, addr))
    }

    pub fn store8_timed(&mut self, addr: u32, val: u8) -> (Result<(), FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::store8(mem, addr, val))
    }

    pub fn store16_timed(&mut self, addr: u32, val: u16) -> (Result<(), FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::store16(mem, addr, val))
    }

    pub fn store32_timed(&mut self, addr: u32, val: u32) -> (Result<(), FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::store32(mem, addr, val))
    }

    pub fn extra_level_name(n: usize) -> String {
        format!("L{}", n + 2)
    }

    pub fn add_extra_level(&mut self, cfg: CacheConfig) {
        self.extra_levels.push(Cache::new(cfg));
    }

    pub fn remove_extra_level(&mut self) -> Option<CacheConfig> {
        self.extra_levels.pop().map(|c| c.config)
    }

    /// Invalidate all cache levels (icache, dcache, and all extra levels).
    pub fn invalidate_all(&mut self) {
        self.icache.invalidate();
        self.dcache.invalidate();
        for level in &mut self.extra_levels {
            level.invalidate();
        }
    }

    /// Write-back all dirty D-cache lines to RAM, then invalidate all caches.
    /// Use this before disabling the cache to keep RAM consistent.
    pub fn flush_all(&mut self) {
        // I-cache is read-only — just invalidate
        self.icache.invalidate();
        // D-cache and extra levels may have dirty data — write back first
        self.dcache.flush_to_ram(&mut self.ram);
        // Borrow checker: ram and extra_levels are disjoint fields, use raw ptr trick
        let ram_ptr = &mut self.ram as *mut Ram;
        for level in &mut self.extra_levels {
            // SAFETY: `ram` and `extra_levels` are distinct fields, no aliasing.
            level.flush_to_ram(unsafe { &mut *ram_ptr });
        }
    }

    /// Fetch `needed_size` bytes starting at `addr` (aligned to `needed_size`)
    /// from extra cache level `from_level` onwards, falling back to RAM.
    /// Returns a Vec<u8> of length `needed_size`.
    fn fetch_line(
        &mut self,
        addr: u32,
        needed_size: usize,
        from_level: usize,
    ) -> Result<Vec<u8>, FalconError> {
        if from_level >= self.extra_levels.len() {
            // Base case: load from RAM
            let mut data = vec![0u8; needed_size];
            for (i, slot) in data.iter_mut().enumerate() {
                *slot = self.ram.load8(addr.wrapping_add(i as u32))?;
            }
            return Ok(data);
        }

        // Skip invalid (disabled) levels
        if !self.extra_levels[from_level].config.is_valid_config() {
            return self.fetch_line(addr, needed_size, from_level + 1);
        }

        // Extract Copy values before any mutable borrows
        let level_tag_search = self.extra_levels[from_level].config.tag_search_cycles();
        let level_miss_penalty = self.extra_levels[from_level].config.miss_penalty;
        let level_transfer_cyc = self.extra_levels[from_level].config.line_transfer_cycles();
        let level_replacement = self.extra_levels[from_level].config.replacement;
        let level_line_size = self.extra_levels[from_level].config.line_size;
        let level_line_base = self.extra_levels[from_level].config.line_base(addr);
        let level_offset_bits = self.extra_levels[from_level].config.offset_bits();
        let level_index_bits = self.extra_levels[from_level].config.index_bits();
        let tag = self.extra_levels[from_level].config.addr_tag(addr);
        let idx = self.extra_levels[from_level].config.addr_index(addr);
        // byte offset of addr within the level's line
        let byte_offset = (addr.wrapping_sub(level_line_base)) as usize;

        // Check for hit (extract data with limited borrow scope)
        let way_opt = self.extra_levels[from_level].sets[idx].lookup(tag);
        if let Some(way) = way_opt {
            let data: Vec<u8> = {
                let line_data = &self.extra_levels[from_level].sets[idx].lines[way].data;
                let end = (byte_offset + needed_size).min(line_data.len());
                line_data[byte_offset..end].to_vec()
            };
            self.extra_levels[from_level].stats.hits += 1;
            self.extra_levels[from_level].stats.total_cycles += level_tag_search;
            self.extra_levels[from_level].sets[idx].touch(way, level_replacement);
            // When this level's line is smaller than needed_size (L2 < L1 line size),
            // fetch the remainder from the same level recursively.
            if data.len() < needed_size {
                let mut result = data;
                while result.len() < needed_size {
                    let next_addr = addr.wrapping_add(result.len() as u32);
                    let chunk =
                        self.fetch_line(next_addr, needed_size - result.len(), from_level)?;
                    if chunk.is_empty() {
                        break;
                    }
                    result.extend_from_slice(&chunk);
                }
                return Ok(result);
            }
            return Ok(data);
        }

        // Miss: record stats then recurse to fetch the level's full line
        self.extra_levels[from_level].stats.misses += 1;
        self.extra_levels[from_level].stats.total_cycles +=
            level_tag_search + level_miss_penalty + level_transfer_cyc;

        // Recurse: fetch the full line for THIS level from the next level/RAM.
        // The next level may have smaller lines; assemble enough to fill ours.
        let mut line_data = Vec::with_capacity(level_line_size);
        let mut fill_addr = level_line_base;
        while line_data.len() < level_line_size {
            let chunk =
                self.fetch_line(fill_addr, level_line_size - line_data.len(), from_level + 1)?;
            if chunk.is_empty() {
                break; // should not happen; guard against infinite loop
            }
            fill_addr = fill_addr.wrapping_add(chunk.len() as u32);
            line_data.extend_from_slice(&chunk);
        }

        // Install into this level (handle dirty eviction to RAM)
        let way = self.extra_levels[from_level].sets[idx].find_victim(level_replacement);
        let evicted_valid = self.extra_levels[from_level].sets[idx].lines[way].valid;
        let evicted_dirty = self.extra_levels[from_level].sets[idx].lines[way].dirty;
        let evicted_tag = self.extra_levels[from_level].sets[idx].lines[way].tag;
        let evicted_data: Vec<u8> = if evicted_valid && evicted_dirty {
            self.extra_levels[from_level].sets[idx].lines[way]
                .data
                .clone()
        } else {
            Vec::new()
        };

        if evicted_valid {
            self.extra_levels[from_level].stats.evictions += 1;
            if evicted_dirty {
                let evict_base = (evicted_tag << (level_offset_bits + level_index_bits))
                    | ((idx as u32) << level_offset_bits);
                for (i, &b) in evicted_data.iter().enumerate() {
                    self.ram.store8(evict_base.wrapping_add(i as u32), b)?;
                    self.extra_levels[from_level].stats.ram_write_bytes += 1;
                }
                self.extra_levels[from_level].stats.writebacks += 1;
            }
        }
        self.extra_levels[from_level].stats.bytes_loaded += level_line_size as u64;
        self.extra_levels[from_level].sets[idx].install(
            way,
            tag,
            line_data.clone(),
            level_replacement,
        );

        // Return only the needed_size bytes the caller asked for.
        // When this level's line is smaller than needed_size (L2 < L1 line size),
        // fetch the remainder from the same level recursively.
        let end = (byte_offset + needed_size).min(line_data.len());
        let mut result = line_data[byte_offset..end].to_vec();
        while result.len() < needed_size {
            let next_addr = addr.wrapping_add(result.len() as u32);
            let chunk = self.fetch_line(next_addr, needed_size - result.len(), from_level)?;
            if chunk.is_empty() {
                break;
            }
            result.extend_from_slice(&chunk);
        }
        Ok(result)
    }

    pub fn overall_cpi(&self) -> f64 {
        if self.instruction_count == 0 {
            return 0.0;
        }
        self.total_program_cycles() as f64 / self.instruction_count as f64
    }

    pub fn ipc(&self) -> f64 {
        let cpi = self.overall_cpi();
        if cpi == 0.0 { 0.0 } else { 1.0 / cpi }
    }

    /// AMAT for I-cache hierarchy (hierarchical formula).
    pub fn icache_amat(&self) -> f64 {
        let hit_lat = self.icache.config.hit_latency as f64;
        let total = self.icache.stats.total_accesses();
        let miss_rate = if total == 0 {
            0.0
        } else {
            self.icache.stats.misses as f64 / total as f64
        };
        let miss_cost = (self.icache.config.miss_penalty
            + self.icache.config.line_transfer_cycles()) as f64;
        if self.extra_levels.is_empty() {
            hit_lat + miss_rate * miss_cost
        } else {
            // miss_cost is the L1→L2 access overhead; add L2 AMAT on top
            hit_lat + miss_rate * (miss_cost + self.extra_level_amat(0))
        }
    }

    /// AMAT for D-cache hierarchy (hierarchical formula).
    pub fn dcache_amat(&self) -> f64 {
        let hit_lat = self.dcache.config.hit_latency as f64;
        let total = self.dcache.stats.total_accesses();
        let miss_rate = if total == 0 {
            0.0
        } else {
            self.dcache.stats.misses as f64 / total as f64
        };
        let miss_cost = (self.dcache.config.miss_penalty
            + self.dcache.config.line_transfer_cycles()) as f64;
        if self.extra_levels.is_empty() {
            hit_lat + miss_rate * miss_cost
        } else {
            // miss_cost is the L1→L2 access overhead; add L2 AMAT on top
            hit_lat + miss_rate * (miss_cost + self.extra_level_amat(0))
        }
    }

    /// Recursive AMAT helper for extra cache levels.
    pub fn extra_level_amat(&self, idx: usize) -> f64 {
        let level = &self.extra_levels[idx];
        let hit_lat = level.config.hit_latency as f64;
        let total = level.stats.total_accesses();
        let miss_rate = if total == 0 {
            0.0
        } else {
            level.stats.misses as f64 / total as f64
        };
        // Miss cost includes this level's penalty + transfer, then recurse
        let miss_cost =
            (level.config.miss_penalty + level.config.line_transfer_cycles()) as f64;
        let next_penalty = if idx + 1 < self.extra_levels.len() {
            miss_cost + self.extra_level_amat(idx + 1)
        } else {
            miss_cost
        };
        hit_lat + miss_rate * next_penalty
    }

    /// Direct RAM read (no cache tracking) — used by UI rendering.
    #[allow(dead_code)]
    pub fn peek8(&self, addr: u32) -> Result<u8, FalconError> {
        self.ram.load8(addr)
    }
    #[allow(dead_code)]
    pub fn peek16(&self, addr: u32) -> Result<u16, FalconError> {
        self.ram.load16(addr)
    }
    pub fn peek32(&self, addr: u32) -> Result<u32, FalconError> {
        self.ram.load32(addr)
    }

    /// Effective read: returns the most-recent dirty byte from the cache hierarchy,
    /// falling back to RAM. Checks L1 D-cache first, then extra_levels in order.
    /// No stats side-effects. Use for syscalls and the Run-tab memory view.
    pub fn effective_read8(&self, addr: u32) -> Result<u8, FalconError> {
        if self.bypass {
            return self.ram.load8(addr);
        }
        if let Some(v) = self.dcache.peek_dirty(addr) {
            return Ok(v);
        }
        for level in &self.extra_levels {
            if let Some(v) = level.peek_dirty(addr) {
                return Ok(v);
            }
        }
        self.ram.load8(addr)
    }
    pub fn effective_read16(&self, addr: u32) -> Result<u16, FalconError> {
        let lo = self.effective_read8(addr)?;
        let hi = self.effective_read8(addr + 1)?;
        Ok(u16::from_le_bytes([lo, hi]))
    }
    pub fn effective_read32(&self, addr: u32) -> Result<u32, FalconError> {
        let b0 = self.effective_read8(addr)?;
        let b1 = self.effective_read8(addr + 1)?;
        let b2 = self.effective_read8(addr + 2)?;
        let b3 = self.effective_read8(addr + 3)?;
        Ok(u32::from_le_bytes([b0, b1, b2, b3]))
    }

    /// True if any byte in [addr, addr+bytes) is dirty in D-cache.
    pub fn is_dirty_cached(&self, addr: u32, bytes: u32) -> bool {
        if self.bypass {
            return false;
        }
        (0..bytes).any(|i| self.dcache.peek_dirty(addr.wrapping_add(i)).is_some())
    }

    /// Returns `Some((level, dirty))` if `addr`'s cache line is present in the D-cache hierarchy.
    /// `level` is 1-based (1 = L1 D-cache, 2 = L2, …). No side effects.
    pub fn data_cache_location(&self, addr: u32) -> Option<(u8, bool)> {
        if self.bypass {
            return None;
        }
        if let Some(dirty) = self.dcache.has_line(addr) {
            return Some((1, dirty));
        }
        for (i, level) in self.extra_levels.iter().enumerate() {
            if let Some(dirty) = level.has_line(addr) {
                return Some((i as u8 + 2, dirty));
            }
        }
        None
    }

    /// Returns `Some(level)` if `addr`'s cache line is present in the I-cache hierarchy.
    /// `level` is 1-based (1 = L1 I-cache, 2 = L2, …). No side effects.
    pub fn instruction_cache_location(&self, addr: u32) -> Option<u8> {
        if self.bypass {
            return None;
        }
        if self.icache.has_line(addr).is_some() {
            return Some(1);
        }
        for (i, level) in self.extra_levels.iter().enumerate() {
            if level.has_line(addr).is_some() {
                return Some(i as u8 + 2);
            }
        }
        None
    }
}

impl Bus for CacheController {
    // load* = cache-aware reads: dirty D-cache lines take priority over RAM.
    // This is the correct view for all runtime code (syscalls, decoders, etc.).
    // For raw RAM (UI diff display), use peek8/peek16/peek32 directly on CacheController.
    fn load8(&self, addr: u32) -> Result<u8, FalconError> {
        self.effective_read8(addr)
    }
    fn load16(&self, addr: u32) -> Result<u16, FalconError> {
        self.effective_read16(addr)
    }
    fn load32(&self, addr: u32) -> Result<u32, FalconError> {
        self.effective_read32(addr)
    }

    // store* = D-cache tracked writes (bypasses L2+ — write-through-to-RAM for evictions)
    fn store8(&mut self, addr: u32, val: u8) -> Result<(), FalconError> {
        self.dcache_store_bytes(addr, &[val])
    }
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError> {
        self.dcache_store_bytes(addr, &val.to_le_bytes())
    }
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError> {
        self.dcache_store_bytes(addr, &val.to_le_bytes())
    }

    // I-cache tracked fetch — hierarchical: L1 hit → return; miss → L2+/RAM fill
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        if self.bypass {
            self.instruction_count += 1;
            return self.ram.load32(addr);
        }
        if !self.icache.config.is_valid_config() {
            self.instruction_count += 1;
            return self.ram.load32(addr);
        }

        // Extract config values (all Copy)
        let tag_search = self.icache.config.tag_search_cycles();
        let miss_penalty = self.icache.config.miss_penalty;
        let transfer_cyc = self.icache.config.line_transfer_cycles();
        let line_base = self.icache.config.line_base(addr);
        let line_size = self.icache.config.line_size;
        let replacement = self.icache.config.replacement;
        let tag = self.icache.config.addr_tag(addr);
        let idx = self.icache.config.addr_index(addr);
        let offset = self.icache.config.addr_offset(addr);

        // Misaligned fetch can straddle cache lines (e.g. addr near the end of a line).
        // RAM can always handle it byte-by-byte, so fall back instead of panicking.
        if offset + 3 >= line_size {
            self.instruction_count += 1;
            return self.ram.load32(addr);
        }

        // L1 hit check
        let way_opt = self.icache.sets[idx].lookup(tag);
        if let Some(way) = way_opt {
            let (d0, d1, d2, d3) = {
                let d = &self.icache.sets[idx].lines[way].data;
                (d[offset], d[offset + 1], d[offset + 2], d[offset + 3])
            };
            self.icache.stats.hits += 1;
            self.icache.stats.total_cycles += tag_search;
            self.icache.sets[idx].touch(way, replacement);
            self.instruction_count += 1;
            return Ok(u32::from_le_bytes([d0, d1, d2, d3]));
        }

        // L1 miss — record and fill from L2+/RAM
        self.icache.stats.misses += 1;
        self.icache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;
        *self.icache.stats.miss_pcs.entry(addr).or_insert(0) += 1;

        let line_data = self.fetch_line(line_base, line_size, 0)?;

        // Install into L1 I-cache (read-only — no dirty eviction)
        let way = self.icache.sets[idx].find_victim(replacement);
        if self.icache.sets[idx].lines[way].valid {
            self.icache.stats.evictions += 1;
        }
        self.icache.stats.bytes_loaded += line_size as u64;
        self.icache.sets[idx].install(way, tag, line_data.clone(), replacement);

        self.instruction_count += 1;
        Ok(u32::from_le_bytes([
            line_data[offset],
            line_data[offset + 1],
            line_data[offset + 2],
            line_data[offset + 3],
        ]))
    }

    // D-cache tracked reads — hierarchical: L1 hit → return; miss → L2+/RAM fill
    fn dcache_read8(&mut self, addr: u32) -> Result<u8, FalconError> {
        if self.bypass {
            return self.ram.load8(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load8(addr);
        }
        let tag_search = self.dcache.config.tag_search_cycles();
        let miss_penalty = self.dcache.config.miss_penalty;
        let transfer_cyc = self.dcache.config.line_transfer_cycles();
        let line_base = self.dcache.config.line_base(addr);
        let line_size = self.dcache.config.line_size;
        let replacement = self.dcache.config.replacement;
        let offset_bits = self.dcache.config.offset_bits();
        let index_bits = self.dcache.config.index_bits();
        let tag = self.dcache.config.addr_tag(addr);
        let idx = self.dcache.config.addr_index(addr);
        let offset = self.dcache.config.addr_offset(addr);

        let way_opt = self.dcache.sets[idx].lookup(tag);
        if let Some(way) = way_opt {
            let b = self.dcache.sets[idx].lines[way].data[offset];
            self.dcache.stats.hits += 1;
            self.dcache.stats.total_cycles += tag_search;
            self.dcache.sets[idx].touch(way, replacement);
            return Ok(b);
        }

        self.dcache.stats.misses += 1;
        self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;

        let line_data = self.fetch_line(line_base, line_size, 0)?;
        Self::install_dcache_line(
            &mut self.dcache,
            &mut self.ram,
            idx,
            tag,
            replacement,
            offset_bits,
            index_bits,
            miss_penalty,
            transfer_cyc,
            line_data.clone(),
        )?;
        Ok(line_data[offset])
    }

    fn dcache_read16(&mut self, addr: u32) -> Result<u16, FalconError> {
        if self.bypass {
            return self.ram.load16(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load16(addr);
        }
        let tag_search = self.dcache.config.tag_search_cycles();
        let miss_penalty = self.dcache.config.miss_penalty;
        let transfer_cyc = self.dcache.config.line_transfer_cycles();
        let line_base = self.dcache.config.line_base(addr);
        let line_size = self.dcache.config.line_size;
        let replacement = self.dcache.config.replacement;
        let offset_bits = self.dcache.config.offset_bits();
        let index_bits = self.dcache.config.index_bits();
        let tag = self.dcache.config.addr_tag(addr);
        let idx = self.dcache.config.addr_index(addr);
        let offset = self.dcache.config.addr_offset(addr);

        // Misaligned halfword reads can straddle cache lines (addr at last byte of a line).
        if offset + 1 >= line_size {
            let b0 = self.dcache_read8(addr)?;
            let b1 = self.dcache_read8(addr.wrapping_add(1))?;
            return Ok(u16::from_le_bytes([b0, b1]));
        }

        let way_opt = self.dcache.sets[idx].lookup(tag);
        if let Some(way) = way_opt {
            let (b0, b1) = {
                let d = &self.dcache.sets[idx].lines[way].data;
                (d[offset], d[offset + 1])
            };
            self.dcache.stats.hits += 1;
            self.dcache.stats.total_cycles += tag_search;
            self.dcache.sets[idx].touch(way, replacement);
            return Ok(u16::from_le_bytes([b0, b1]));
        }

        self.dcache.stats.misses += 1;
        self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;

        let line_data = self.fetch_line(line_base, line_size, 0)?;
        Self::install_dcache_line(
            &mut self.dcache,
            &mut self.ram,
            idx,
            tag,
            replacement,
            offset_bits,
            index_bits,
            miss_penalty,
            transfer_cyc,
            line_data.clone(),
        )?;
        Ok(u16::from_le_bytes([
            line_data[offset],
            line_data[offset + 1],
        ]))
    }

    fn dcache_read32(&mut self, addr: u32) -> Result<u32, FalconError> {
        if self.bypass {
            return self.ram.load32(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load32(addr);
        }
        let tag_search = self.dcache.config.tag_search_cycles();
        let miss_penalty = self.dcache.config.miss_penalty;
        let transfer_cyc = self.dcache.config.line_transfer_cycles();
        let line_base = self.dcache.config.line_base(addr);
        let line_size = self.dcache.config.line_size;
        let replacement = self.dcache.config.replacement;
        let offset_bits = self.dcache.config.offset_bits();
        let index_bits = self.dcache.config.index_bits();
        let tag = self.dcache.config.addr_tag(addr);
        let idx = self.dcache.config.addr_index(addr);
        let offset = self.dcache.config.addr_offset(addr);

        // Misaligned word reads can straddle cache lines (addr near the end of a line).
        if offset + 3 >= line_size {
            let b0 = self.dcache_read8(addr)?;
            let b1 = self.dcache_read8(addr.wrapping_add(1))?;
            let b2 = self.dcache_read8(addr.wrapping_add(2))?;
            let b3 = self.dcache_read8(addr.wrapping_add(3))?;
            return Ok(u32::from_le_bytes([b0, b1, b2, b3]));
        }

        let way_opt = self.dcache.sets[idx].lookup(tag);
        if let Some(way) = way_opt {
            let (d0, d1, d2, d3) = {
                let d = &self.dcache.sets[idx].lines[way].data;
                (d[offset], d[offset + 1], d[offset + 2], d[offset + 3])
            };
            self.dcache.stats.hits += 1;
            self.dcache.stats.total_cycles += tag_search;
            self.dcache.sets[idx].touch(way, replacement);
            return Ok(u32::from_le_bytes([d0, d1, d2, d3]));
        }

        self.dcache.stats.misses += 1;
        self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;

        let line_data = self.fetch_line(line_base, line_size, 0)?;
        Self::install_dcache_line(
            &mut self.dcache,
            &mut self.ram,
            idx,
            tag,
            replacement,
            offset_bits,
            index_bits,
            miss_penalty,
            transfer_cyc,
            line_data.clone(),
        )?;
        Ok(u32::from_le_bytes([
            line_data[offset],
            line_data[offset + 1],
            line_data[offset + 2],
            line_data[offset + 3],
        ]))
    }

    fn total_cycles(&self) -> u64 {
        self.total_program_cycles()
    }
}

impl CacheController {
    /// Install a pre-fetched line into the D-cache, handling any dirty eviction writeback to RAM.
    fn install_dcache_line(
        dcache: &mut Cache,
        ram: &mut Ram,
        idx: usize,
        tag: u32,
        replacement: ReplacementPolicy,
        offset_bits: u32,
        index_bits: u32,
        miss_penalty: u64,
        transfer_cyc: u64,
        line_data: Vec<u8>,
    ) -> Result<(), FalconError> {
        let way = dcache.sets[idx].find_victim(replacement);
        let evicted_valid = dcache.sets[idx].lines[way].valid;
        let evicted_dirty = dcache.sets[idx].lines[way].dirty;
        let evicted_tag = dcache.sets[idx].lines[way].tag;
        let evicted_data: Vec<u8> = if evicted_valid && evicted_dirty {
            dcache.sets[idx].lines[way].data.clone()
        } else {
            Vec::new()
        };

        if evicted_valid {
            dcache.stats.evictions += 1;
            if evicted_dirty {
                let evict_base =
                    (evicted_tag << (offset_bits + index_bits)) | ((idx as u32) << offset_bits);
                for (i, &b) in evicted_data.iter().enumerate() {
                    ram.store8(evict_base.wrapping_add(i as u32), b)?;
                    dcache.stats.ram_write_bytes += 1;
                }
                dcache.stats.writebacks += 1;
                dcache.stats.total_cycles += miss_penalty + transfer_cyc;
            }
        }
        dcache.stats.bytes_loaded += line_data.len() as u64;
        dcache.sets[idx].install(way, tag, line_data, replacement);
        Ok(())
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/support/falcon_cache.rs"]
mod tests;
