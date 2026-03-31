// falcon/cache/cache.rs — Single cache level: CacheLine, CacheSet, Cache

use std::collections::VecDeque;

use super::{CacheConfig, CacheStats, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::falcon::{
    errors::FalconError,
    memory::{Bus, Ram},
};

// ── Internal structures ──────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct CacheLine {
    pub(crate) valid: bool,
    pub(crate) tag: u32,
    pub(crate) dirty: bool,
    pub(crate) data: Vec<u8>,
    pub(crate) freq: u64,     // LFU: access frequency counter
    pub(crate) ref_bit: bool, // Clock: reference bit
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

pub(crate) struct CacheSet {
    pub(crate) lines: Vec<CacheLine>,
    pub(crate) lru_order: VecDeque<usize>, // front=MRU, back=LRU (LRU & MRU)
    pub(crate) fifo_order: VecDeque<usize>, // front=newest, back=oldest (FIFO)
    rand_state: u32,
    pub(crate) clock_hand: usize, // Clock: current sweep position
}

impl CacheSet {
    pub(crate) fn new(ways: usize, line_size: usize) -> Self {
        Self {
            lines: (0..ways).map(|_| CacheLine::new(line_size)).collect(),
            lru_order: (0..ways).collect(),
            fifo_order: (0..ways).collect(),
            rand_state: 0xDEAD_BEEF,
            clock_hand: 0,
        }
    }

    pub(crate) fn lookup(&self, tag: u32) -> Option<usize> {
        self.lines.iter().position(|l| l.valid && l.tag == tag)
    }

    pub(crate) fn find_victim(&mut self, policy: ReplacementPolicy) -> usize {
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

    pub(crate) fn touch(&mut self, way: usize, policy: ReplacementPolicy) {
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

    pub(crate) fn install(
        &mut self,
        way: usize,
        tag: u32,
        data: Vec<u8>,
        policy: ReplacementPolicy,
    ) {
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

// ── Cache ────────────────────────────────────────────────────────────────────

pub struct Cache {
    pub config: CacheConfig,
    pub(crate) sets: Vec<CacheSet>,
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

    /// Write-back all dirty lines to RAM without invalidating them.
    /// The cache lines remain valid; dirty bits are cleared since RAM is now in sync.
    pub fn writeback_dirty(&mut self, ram: &mut Ram) {
        if !self.config.is_valid_config() {
            return;
        }
        let offset_bits = self.config.offset_bits();
        let index_bits = self.config.index_bits();
        for (set_idx, set) in self.sets.iter_mut().enumerate() {
            for line in &mut set.lines {
                if line.valid && line.dirty {
                    let base = (line.tag << (offset_bits + index_bits))
                        | ((set_idx as u32) << offset_bits);
                    for (i, &byte) in line.data.iter().enumerate() {
                        let _ = ram.store8(base.wrapping_add(i as u32), byte);
                    }
                    line.dirty = false;
                }
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
    pub(crate) fn allocate_ro(&mut self, addr: u32, ram: &Ram) -> Result<(), FalconError> {
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
    pub(crate) fn allocate_rw(&mut self, addr: u32, ram: &mut Ram) -> Result<(), FalconError> {
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
