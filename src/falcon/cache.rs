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

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub size: usize,           // total bytes
    pub line_size: usize,      // bytes per line
    pub associativity: usize,  // ways per set
    pub replacement: ReplacementPolicy,
    pub write_policy: WritePolicy,
    pub write_alloc: WriteAllocPolicy,
    /// Cycles consumed on a cache hit
    pub hit_latency: u64,
    /// Extra cycles added on a cache miss (stall waiting for RAM)
    pub miss_penalty: u64,
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
    pub fn is_valid_config(&self) -> bool {
        if self.size == 0 || self.line_size < 4 || self.associativity == 0 {
            return false;
        }
        if !self.line_size.is_power_of_two() {
            return false;
        }
        let bytes_per_set = match self.line_size.checked_mul(self.associativity) {
            Some(v) => v,
            None => return false,
        };
        if bytes_per_set == 0 || bytes_per_set > self.size {
            return false;
        }
        if self.size % bytes_per_set != 0 {
            return false;
        }
        let sets = self.size / bytes_per_set;
        sets.is_power_of_two()
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
            hit_latency: 1,
            miss_penalty: 50,
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
        Self { valid: false, tag: 0, dirty: false, data: vec![0; line_size], freq: 0, ref_bit: false }
    }
}

struct CacheSet {
    lines: Vec<CacheLine>,
    lru_order: VecDeque<usize>,  // front=MRU, back=LRU (LRU & MRU)
    fifo_order: VecDeque<usize>, // front=newest, back=oldest (FIFO)
    rand_state: u32,
    clock_hand: usize,           // Clock: current sweep position
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
                self.rand_state = self.rand_state.wrapping_mul(1664525).wrapping_add(1013904223);
                (self.rand_state as usize) % n
            }
            ReplacementPolicy::Lfu => {
                // Evict way with lowest frequency (ties broken by LRU order)
                let min_freq = self.lines.iter().map(|l| l.freq).min().unwrap_or(0);
                // Among min-freq lines, take the LRU one
                self.lru_order.iter().rev()
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
        self.lines[way] = CacheLine { valid: true, tag, dirty: false, data, freq: 1, ref_bit: true };
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
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 * 100.0 }
    }
    #[allow(dead_code)]
    pub fn miss_rate(&self) -> f64 {
        100.0 - self.hit_rate()
    }
    pub fn total_accesses(&self) -> u64 {
        self.hits + self.misses
    }
    pub fn mpki(&self, instructions: u64) -> f64 {
        if instructions == 0 { 0.0 } else { self.misses as f64 / instructions as f64 * 1000.0 }
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
        let num_sets = if config.is_valid_config() { config.num_sets() } else { 1 };
        let ways = config.associativity.max(1);
        let line_size = config.line_size.max(4);
        Self {
            sets: (0..num_sets).map(|_| CacheSet::new(ways, line_size)).collect(),
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

    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    /// If this address is covered by a dirty cache line, return the cached byte.
    pub fn peek_dirty(&self, addr: u32) -> Option<u8> {
        if !self.config.is_valid_config() { return None; }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);
        let way = self.sets[idx].lookup(tag)?;
        let line = &self.sets[idx].lines[way];
        if line.dirty { Some(line.data[offset]) } else { None }
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
                let evict_base = (evicted.tag << (offset_bits + index_bits))
                    | ((idx as u32) << offset_bits);
                for (i, &b) in evicted.data.iter().enumerate() {
                    ram.store8(evict_base + i as u32, b)?;
                    self.stats.ram_write_bytes += 1;
                }
                self.stats.writebacks += 1;
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
    pub fn read_byte_ro(&mut self, addr: u32, ram: &Ram) -> Result<u8, FalconError> {
        if !self.config.is_valid_config() {
            return ram.load8(addr);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);

        if let Some(way) = self.sets[idx].lookup(tag) {
            self.stats.hits += 1;
            self.stats.total_cycles += self.config.hit_latency;
            self.sets[idx].touch(way, self.config.replacement);
            return Ok(self.sets[idx].lines[way].data[offset]);
        }
        self.stats.misses += 1;
        self.stats.total_cycles += self.config.hit_latency + self.config.miss_penalty;
        self.allocate_ro(addr, ram)?;
        let way = self.sets[idx].lookup(tag).unwrap();
        Ok(self.sets[idx].lines[way].data[offset])
    }

    /// Read byte via D-cache (read-write, may write back dirty eviction). Charges cycles.
    pub fn read_byte_rw(&mut self, addr: u32, ram: &mut Ram) -> Result<u8, FalconError> {
        if !self.config.is_valid_config() {
            return ram.load8(addr);
        }
        let tag = self.config.addr_tag(addr);
        let idx = self.config.addr_index(addr);
        let offset = self.config.addr_offset(addr);

        if let Some(way) = self.sets[idx].lookup(tag) {
            self.stats.hits += 1;
            self.stats.total_cycles += self.config.hit_latency;
            self.sets[idx].touch(way, self.config.replacement);
            return Ok(self.sets[idx].lines[way].data[offset]);
        }
        self.stats.misses += 1;
        self.stats.total_cycles += self.config.hit_latency + self.config.miss_penalty;
        self.allocate_rw(addr, ram)?;
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
                    self.stats.total_cycles += self.config.hit_latency;
                    self.sets[idx].lines[way].data[offset] = val;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles += self.config.miss_penalty;
                    if let WriteAllocPolicy::WriteAllocate = self.config.write_alloc {
                        self.allocate_rw(addr, ram)?;
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
                    self.stats.total_cycles += self.config.hit_latency;
                    self.sets[idx].lines[way].data[offset] = val;
                    self.sets[idx].lines[way].dirty = true;
                    self.sets[idx].touch(way, self.config.replacement);
                } else {
                    self.stats.misses += 1;
                    self.stats.total_cycles += self.config.hit_latency + self.config.miss_penalty;
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
}

// ── Presets ──────────────────────────────────────────────────────────────────

/// Returns [Small, Medium, Large] preset configs for I-cache or D-cache.
pub fn cache_presets(icache: bool) -> [CacheConfig; 3] {
    if icache {
        [
            CacheConfig { size: 256,  line_size: 16, associativity: 1, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 50 },
            CacheConfig { size: 1024, line_size: 16, associativity: 2, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 50 },
            CacheConfig { size: 4096, line_size: 32, associativity: 4, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 50 },
        ]
    } else {
        [
            CacheConfig { size: 256,  line_size: 16, associativity: 1, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 100 },
            CacheConfig { size: 1024, line_size: 16, associativity: 2, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 100 },
            CacheConfig { size: 8192, line_size: 32, associativity: 4, replacement: ReplacementPolicy::Lru, write_policy: WritePolicy::WriteBack, write_alloc: WriteAllocPolicy::WriteAllocate, hit_latency: 1, miss_penalty: 100 },
        ]
    }
}

// ── CacheController ──────────────────────────────────────────────────────────

pub struct CacheController {
    pub ram: Ram,
    pub icache: Cache,
    pub dcache: Cache,
    pub instruction_count: u64,
    step_count: u64,
}

impl CacheController {
    pub fn new(icfg: CacheConfig, dcfg: CacheConfig, mem_size: usize) -> Self {
        Self {
            ram: Ram::new(mem_size),
            icache: Cache::new(icfg),
            dcache: Cache::new(dcfg),
            instruction_count: 0,
            step_count: 0,
        }
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
    }

    pub fn reset_stats(&mut self) {
        self.icache.reset_stats();
        self.dcache.reset_stats();
        self.instruction_count = 0;
        self.step_count = 0;
    }

    pub fn apply_config(&mut self, icfg: CacheConfig, dcfg: CacheConfig) {
        self.icache = Cache::new(icfg);
        self.dcache = Cache::new(dcfg);
        self.reset_stats();
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

    /// Effective read: returns dirty D-cache value if present, else RAM.
    /// Use this in the RUN tab memory view so write-back stores are visible.
    pub fn effective_read8(&self, addr: u32) -> Result<u8, FalconError> {
        if let Some(v) = self.dcache.peek_dirty(addr) { return Ok(v); }
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
        (0..bytes).any(|i| self.dcache.peek_dirty(addr.wrapping_add(i)).is_some())
    }
}

impl Bus for CacheController {
    // load* = direct RAM reads (no cache tracking) — safe from &self for UI rendering
    fn load8(&self, addr: u32) -> Result<u8, FalconError> {
        self.ram.load8(addr)
    }
    fn load16(&self, addr: u32) -> Result<u16, FalconError> {
        self.ram.load16(addr)
    }
    fn load32(&self, addr: u32) -> Result<u32, FalconError> {
        self.ram.load32(addr)
    }

    // store* = D-cache tracked writes
    fn store8(&mut self, addr: u32, val: u8) -> Result<(), FalconError> {
        self.dcache.write_byte(addr, val, &mut self.ram)
    }
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError> {
        let [b0, b1] = val.to_le_bytes();
        self.dcache.write_byte(addr, b0, &mut self.ram)?;
        self.dcache.write_byte(addr + 1, b1, &mut self.ram)
    }
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError> {
        let [b0, b1, b2, b3] = val.to_le_bytes();
        self.dcache.write_byte(addr, b0, &mut self.ram)?;
        self.dcache.write_byte(addr + 1, b1, &mut self.ram)?;
        self.dcache.write_byte(addr + 2, b2, &mut self.ram)?;
        self.dcache.write_byte(addr + 3, b3, &mut self.ram)
    }

    // I-cache tracked fetch (override default)
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        let misses_before = self.icache.stats.misses;
        let b0 = self.icache.read_byte_ro(addr, &self.ram)?;
        let b1 = self.icache.read_byte_ro(addr + 1, &self.ram)?;
        let b2 = self.icache.read_byte_ro(addr + 2, &self.ram)?;
        let b3 = self.icache.read_byte_ro(addr + 3, &self.ram)?;
        let delta = self.icache.stats.misses - misses_before;
        if delta > 0 {
            *self.icache.stats.miss_pcs.entry(addr).or_insert(0) += delta;
        }
        self.instruction_count += 1;
        Ok(u32::from_le_bytes([b0, b1, b2, b3]))
    }

    // D-cache tracked reads (override default)
    fn dcache_read8(&mut self, addr: u32) -> Result<u8, FalconError> {
        self.dcache.read_byte_rw(addr, &mut self.ram)
    }
    fn dcache_read16(&mut self, addr: u32) -> Result<u16, FalconError> {
        let lo = self.dcache.read_byte_rw(addr, &mut self.ram)?;
        let hi = self.dcache.read_byte_rw(addr + 1, &mut self.ram)?;
        Ok(u16::from_le_bytes([lo, hi]))
    }
    fn dcache_read32(&mut self, addr: u32) -> Result<u32, FalconError> {
        let b0 = self.dcache.read_byte_rw(addr, &mut self.ram)?;
        let b1 = self.dcache.read_byte_rw(addr + 1, &mut self.ram)?;
        let b2 = self.dcache.read_byte_rw(addr + 2, &mut self.ram)?;
        let b3 = self.dcache.read_byte_rw(addr + 3, &mut self.ram)?;
        Ok(u32::from_le_bytes([b0, b1, b2, b3]))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::falcon::memory::Bus;

    fn icfg_small() -> CacheConfig {
        CacheConfig {
            size: 64, line_size: 16, associativity: 1,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_alloc: WriteAllocPolicy::WriteAllocate,
            hit_latency: 1, miss_penalty: 10,
        }
    }

    fn dcfg(write_policy: WritePolicy, write_alloc: WriteAllocPolicy, size: usize, line_size: usize, assoc: usize) -> CacheConfig {
        CacheConfig {
            size, line_size, associativity: assoc,
            replacement: ReplacementPolicy::Lru,
            write_policy, write_alloc,
            hit_latency: 1, miss_penalty: 10,
        }
    }

    // ── Caso 1: miss_pcs incrementa em misses de fetch ─────────────────────

    #[test]
    fn miss_pcs_increment_on_fetch_miss() {
        let mut ctrl = CacheController::new(icfg_small(), CacheConfig::default(), 256);

        // 1ª busca no addr 0 → cold miss; miss_pcs[0] deve ser 1
        ctrl.fetch32(0).unwrap();
        assert_eq!(*ctrl.icache.stats.miss_pcs.get(&0).unwrap_or(&0), 1,
            "first fetch at 0 should record 1 miss");

        // 2ª busca no mesmo addr 0 → hit (mesma linha); miss_pcs[0] não cresce
        ctrl.fetch32(0).unwrap();
        assert_eq!(*ctrl.icache.stats.miss_pcs.get(&0).unwrap_or(&0), 1,
            "second fetch at 0 (hit) should not increment miss_pcs");

        // busca no addr 16 → nova linha, cold miss; miss_pcs[16] == 1
        ctrl.fetch32(16).unwrap();
        assert_eq!(*ctrl.icache.stats.miss_pcs.get(&16).unwrap_or(&0), 1,
            "fetch at addr 16 should record 1 miss");
    }

    // ── Caso 2A: ram_write_bytes — write-through ────────────────────────────

    #[test]
    fn ram_write_bytes_write_through() {
        let d = dcfg(WritePolicy::WriteThrough, WriteAllocPolicy::WriteAllocate, 64, 16, 1);
        let mut ctrl = CacheController::new(CacheConfig::default(), d, 256);

        ctrl.store8(0, 42).unwrap();
        assert_eq!(ctrl.dcache.stats.ram_write_bytes, 1,
            "write-through store8 should write 1 byte to RAM immediately");
        assert_eq!(ctrl.dcache.stats.bytes_stored, 1);
    }

    // ── Caso 2B: ram_write_bytes — write-back writeback on eviction ─────────

    #[test]
    fn ram_write_bytes_write_back_writeback() {
        // 1 set, 1 way, line_size=16 → eviction happens when switching between lines
        let d = dcfg(WritePolicy::WriteBack, WriteAllocPolicy::WriteAllocate, 16, 16, 1);
        let mut ctrl = CacheController::new(CacheConfig::default(), d, 256);

        // store8(0): miss → alloca linha 0, marca dirty; RAM NÃO é escrita ainda
        ctrl.store8(0, 1).unwrap();
        assert_eq!(ctrl.dcache.stats.ram_write_bytes, 0,
            "write-back miss should not write to RAM immediately");

        // store8(16): miss → evict linha 0 (dirty) → writeback de 16 bytes para RAM
        ctrl.store8(16, 2).unwrap();
        assert_eq!(ctrl.dcache.stats.writebacks, 1);
        assert_eq!(ctrl.dcache.stats.ram_write_bytes, 16,
            "writeback should write exactly line_size bytes to RAM");
    }
}
