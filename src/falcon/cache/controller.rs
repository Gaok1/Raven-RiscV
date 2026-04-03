// falcon/cache/controller.rs — Multi-level CacheController and Bus impl

use super::{Cache, CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::falcon::{
    errors::FalconError,
    memory::{AmoOp, Bus, Ram},
};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Reservation {
    addr: u32,
}

fn reservation_addr(addr: u32) -> u32 {
    addr & !0x3
}

fn overlaps_reserved_word(addr: u32, size: usize, reserved_addr: u32) -> bool {
    let start = addr as u64;
    let end = start.saturating_add(size as u64);
    let rstart = reserved_addr as u64;
    let rend = rstart + 4;
    start < rend && rstart < end
}

pub struct CacheController {
    pub(crate) ram: Ram,
    pub(crate) icache: Cache,
    pub(crate) dcache: Cache,
    /// Extra unified cache levels: extra_levels[0]=L2, extra_levels[1]=L3, …
    pub(crate) extra_levels: Vec<Cache>,
    pub(crate) instruction_count: u64,
    /// Base instruction-execution cycles (not cache): set via add_instruction_cycles().
    pub(crate) extra_cycles: u64,
    step_count: u64,
    /// When true, all cache lookups are skipped and RAM is accessed directly (no stats, no latency).
    pub(crate) bypass: bool,
    reservations: HashMap<u32, Reservation>,
}

#[derive(Clone, Copy)]
enum WritebackSource {
    L1D,
    Extra(usize),
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
            reservations: HashMap::new(),
        }
    }

    fn invalidate_reservations(&mut self, addr: u32, size: usize) {
        self.reservations
            .retain(|_, res| !overlaps_reserved_word(addr, size, res.addr));
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

    fn source_next_level(source: WritebackSource) -> usize {
        match source {
            WritebackSource::L1D => 0,
            WritebackSource::Extra(level) => level + 1,
        }
    }

    fn source_writeback_cost(&self, source: WritebackSource) -> (u64, u64) {
        match source {
            WritebackSource::L1D => (
                self.dcache.config.miss_penalty,
                self.dcache.config.line_transfer_cycles(),
            ),
            WritebackSource::Extra(level) => (
                self.extra_levels[level].config.miss_penalty,
                self.extra_levels[level].config.line_transfer_cycles(),
            ),
        }
    }

    fn record_writeback_event(&mut self, source: WritebackSource) {
        let (miss_penalty, transfer_cyc) = self.source_writeback_cost(source);
        match source {
            WritebackSource::L1D => {
                self.dcache.stats.writebacks += 1;
                self.dcache.stats.total_cycles += miss_penalty + transfer_cyc;
            }
            WritebackSource::Extra(level) => {
                self.extra_levels[level].stats.writebacks += 1;
                self.extra_levels[level].stats.total_cycles += miss_penalty + transfer_cyc;
            }
        }
    }

    fn record_ram_write_bytes(&mut self, source: WritebackSource, count: u64) {
        match source {
            WritebackSource::L1D => self.dcache.stats.ram_write_bytes += count,
            WritebackSource::Extra(level) => {
                self.extra_levels[level].stats.ram_write_bytes += count
            }
        }
    }

    fn propagate_line_to_next_level(
        &mut self,
        source: WritebackSource,
        line_base: u32,
        line_data: &[u8],
    ) -> Result<(), FalconError> {
        self.record_writeback_event(source);
        self.writeback_bytes_to_level(
            source,
            Self::source_next_level(source),
            line_base,
            line_data,
        )
    }

    fn writeback_bytes_to_level(
        &mut self,
        source: WritebackSource,
        level_idx: usize,
        addr: u32,
        bytes: &[u8],
    ) -> Result<(), FalconError> {
        if bytes.is_empty() {
            return Ok(());
        }

        if level_idx >= self.extra_levels.len() {
            for (i, &byte) in bytes.iter().enumerate() {
                self.ram.store8(addr.wrapping_add(i as u32), byte)?;
            }
            self.record_ram_write_bytes(source, bytes.len() as u64);
            return Ok(());
        }

        if !self.extra_levels[level_idx].config.is_valid_config() {
            return self.writeback_bytes_to_level(source, level_idx + 1, addr, bytes);
        }

        let line_size = self.extra_levels[level_idx].config.line_size;
        let line_base = self.extra_levels[level_idx].config.line_base(addr);
        let offset = self.extra_levels[level_idx].config.addr_offset(addr);
        let first_len = (line_size - offset).min(bytes.len());
        if first_len < bytes.len() {
            self.writeback_bytes_to_level(source, level_idx, addr, &bytes[..first_len])?;
            return self.writeback_bytes_to_level(
                source,
                level_idx,
                addr.wrapping_add(first_len as u32),
                &bytes[first_len..],
            );
        }

        let tag_search = self.extra_levels[level_idx].config.tag_search_cycles();
        let miss_penalty = self.extra_levels[level_idx].config.miss_penalty;
        let transfer_cyc = self.extra_levels[level_idx].config.line_transfer_cycles();
        let replacement = self.extra_levels[level_idx].config.replacement;
        let offset_bits = self.extra_levels[level_idx].config.offset_bits();
        let index_bits = self.extra_levels[level_idx].config.index_bits();
        let tag = self.extra_levels[level_idx].config.addr_tag(addr);
        let idx = self.extra_levels[level_idx].config.addr_index(addr);
        let hit_way = self.extra_levels[level_idx].sets[idx].lookup(tag);

        if let Some(way) = hit_way {
            self.extra_levels[level_idx].stats.hits += 1;
            self.extra_levels[level_idx].stats.total_cycles += tag_search;
            self.extra_levels[level_idx].stats.bytes_stored += bytes.len() as u64;
            for (i, &byte) in bytes.iter().enumerate() {
                self.extra_levels[level_idx].sets[idx].lines[way].data[offset + i] = byte;
            }
            self.extra_levels[level_idx].sets[idx].lines[way].dirty = true;
            self.extra_levels[level_idx].sets[idx].touch(way, replacement);
            return Ok(());
        }

        self.extra_levels[level_idx].stats.misses += 1;
        self.extra_levels[level_idx].stats.total_cycles += tag_search + miss_penalty + transfer_cyc;

        let mut line_data = if offset == 0 && bytes.len() == line_size {
            bytes.to_vec()
        } else {
            self.fetch_line(line_base, line_size, level_idx + 1)?
        };
        line_data[offset..offset + bytes.len()].copy_from_slice(bytes);

        let way = self.extra_levels[level_idx].sets[idx].find_victim(replacement);
        let evicted_valid = self.extra_levels[level_idx].sets[idx].lines[way].valid;
        let evicted_dirty = self.extra_levels[level_idx].sets[idx].lines[way].dirty;
        let evicted_tag = self.extra_levels[level_idx].sets[idx].lines[way].tag;
        let evicted_data = if evicted_valid && evicted_dirty {
            self.extra_levels[level_idx].sets[idx].lines[way]
                .data
                .clone()
        } else {
            Vec::new()
        };

        if evicted_valid {
            self.extra_levels[level_idx].stats.evictions += 1;
            if evicted_dirty {
                let evict_base =
                    (evicted_tag << (offset_bits + index_bits)) | ((idx as u32) << offset_bits);
                self.propagate_line_to_next_level(
                    WritebackSource::Extra(level_idx),
                    evict_base,
                    &evicted_data,
                )?;
            }
        }

        self.extra_levels[level_idx].stats.bytes_loaded += line_data.len() as u64;
        self.extra_levels[level_idx].stats.bytes_stored += bytes.len() as u64;
        self.extra_levels[level_idx].sets[idx].install(way, tag, line_data, replacement);
        self.extra_levels[level_idx].sets[idx].lines[way].dirty = true;
        Ok(())
    }

    fn load_icache_line(&mut self, addr: u32) -> Result<Vec<u8>, FalconError> {
        let tag_search = self.icache.config.tag_search_cycles();
        let miss_penalty = self.icache.config.miss_penalty;
        let transfer_cyc = self.icache.config.line_transfer_cycles();
        let line_base = self.icache.config.line_base(addr);
        let line_size = self.icache.config.line_size;
        let replacement = self.icache.config.replacement;
        let tag = self.icache.config.addr_tag(addr);
        let idx = self.icache.config.addr_index(addr);

        if let Some(way) = self.icache.sets[idx].lookup(tag) {
            let line = self.icache.sets[idx].lines[way].data.clone();
            self.icache.stats.hits += 1;
            self.icache.stats.total_cycles += tag_search;
            self.icache.sets[idx].touch(way, replacement);
            return Ok(line);
        }

        self.icache.stats.misses += 1;
        self.icache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;
        *self.icache.stats.miss_pcs.entry(addr).or_insert(0) += 1;

        let line_data = self.fetch_line(line_base, line_size, 0)?;
        let way = self.icache.sets[idx].find_victim(replacement);
        if self.icache.sets[idx].lines[way].valid {
            self.icache.stats.evictions += 1;
        }
        self.icache.stats.bytes_loaded += line_size as u64;
        self.icache.sets[idx].install(way, tag, line_data.clone(), replacement);
        Ok(line_data)
    }

    fn load_dcache_line(&mut self, addr: u32) -> Result<Vec<u8>, FalconError> {
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

        if let Some(way) = self.dcache.sets[idx].lookup(tag) {
            let line = self.dcache.sets[idx].lines[way].data.clone();
            self.dcache.stats.hits += 1;
            self.dcache.stats.total_cycles += tag_search;
            self.dcache.sets[idx].touch(way, replacement);
            return Ok(line);
        }

        self.dcache.stats.misses += 1;
        self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;

        let line_data = self.fetch_line(line_base, line_size, 0)?;
        self.install_dcache_line(
            idx,
            tag,
            replacement,
            offset_bits,
            index_bits,
            line_data.clone(),
        )?;
        Ok(line_data)
    }

    fn dcache_store_bytes(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FalconError> {
        self.invalidate_reservations(addr, bytes.len());

        if self.bypass {
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
                        self.install_dcache_line(
                            idx,
                            tag,
                            replacement,
                            offset_bits,
                            index_bits,
                            line_data,
                        )?;
                        if let Some(way) = self.dcache.sets[idx].lookup(tag) {
                            for (i, &b) in bytes.iter().enumerate() {
                                self.dcache.sets[idx].lines[way].data[offset + i] = b;
                            }
                        }
                    }
                }
                // Write-through: RAM is now authoritative for `addr`.  Any copy
                // in L2+ is stale and must be dropped so that a future D-cache
                // miss fetches the updated value from RAM rather than L2.
                // (Write-back does NOT need this: the D-cache dirty line is
                // always checked before L2, and L2 is updated on eviction.)
                for level in &mut self.extra_levels {
                    level.invalidate_line(addr);
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
                        self.install_dcache_line(
                            idx,
                            tag,
                            replacement,
                            offset_bits,
                            index_bits,
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

        // Note: do NOT invalidate extra_levels (L2+) here.
        // The D-cache is always checked before L2 for reads, so stale L2 data
        // is never served while the D-cache holds a dirty copy.  When a dirty
        // D-cache line is eventually evicted, writeback_bytes_to_level() writes
        // the authoritative data into L2.  Invalidating L2 after every store
        // can erase data that was just installed by a dirty eviction writeback
        // — e.g., when the evicted line and the newly-written line share the
        // same L2 cache line — causing silent data loss.

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

    pub fn lr_w_timed(&mut self, hart_id: u32, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::lr_w(mem, hart_id, addr))
    }

    pub fn sc_w_timed(
        &mut self,
        hart_id: u32,
        addr: u32,
        val: u32,
    ) -> (Result<bool, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::sc_w(mem, hart_id, addr, val))
    }

    pub fn amo_w_timed(
        &mut self,
        hart_id: u32,
        addr: u32,
        op: AmoOp,
        operand: u32,
    ) -> (Result<u32, FalconError>, u64) {
        self.measure_cache_latency(|mem| <Self as Bus>::amo_w(mem, hart_id, addr, op, operand))
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

    /// Write-back all dirty lines to RAM without invalidating the cache.
    /// Use this on program exit to keep RAM consistent while preserving cache state.
    pub fn sync_to_ram(&mut self) {
        // Flush outer levels first (L3, L2, ...) then D-cache (L1).
        // D-cache is always authoritative: any address dirty in D-cache is
        // newer than whatever L2/L3 holds (L2 only receives data through
        // D-cache evictions).  Writing L2 after D-cache would overwrite the
        // correct D-cache data with stale L2 data in RAM.
        let ram_ptr = &mut self.ram as *mut Ram;
        for level in self.extra_levels.iter_mut().rev() {
            // SAFETY: `ram` and `extra_levels` are distinct fields, no aliasing.
            level.writeback_dirty(unsafe { &mut *ram_ptr });
        }
        self.dcache.writeback_dirty(&mut self.ram);
    }

    /// Write-back all dirty D-cache lines to RAM, then invalidate all caches.
    /// Use this before disabling the cache to keep RAM consistent.
    pub fn flush_all(&mut self) {
        // I-cache is read-only — just invalidate
        self.icache.invalidate();
        // Flush outer levels first (L3, L2, ...) then D-cache (L1).
        // D-cache is always authoritative: writing L2 after D-cache would
        // overwrite the correct D-cache data with stale L2 data in RAM.
        let ram_ptr = &mut self.ram as *mut Ram;
        for level in self.extra_levels.iter_mut().rev() {
            // SAFETY: `ram` and `extra_levels` are distinct fields, no aliasing.
            level.flush_to_ram(unsafe { &mut *ram_ptr });
        }
        self.dcache.flush_to_ram(&mut self.ram);
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
                self.propagate_line_to_next_level(
                    WritebackSource::Extra(from_level),
                    evict_base,
                    &evicted_data,
                )?;
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
        let miss_cost =
            (self.icache.config.miss_penalty + self.icache.config.line_transfer_cycles()) as f64;
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
        let miss_cost =
            (self.dcache.config.miss_penalty + self.dcache.config.line_transfer_cycles()) as f64;
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
        let miss_cost = (level.config.miss_penalty + level.config.line_transfer_cycles()) as f64;
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

        let line_size = self.icache.config.line_size;
        let offset = self.icache.config.addr_offset(addr);
        let first_line = self.load_icache_line(addr)?;
        self.instruction_count += 1;
        if offset + 3 < line_size {
            return Ok(u32::from_le_bytes([
                first_line[offset],
                first_line[offset + 1],
                first_line[offset + 2],
                first_line[offset + 3],
            ]));
        }

        let split = line_size - offset;
        let second_line = self.load_icache_line(addr.wrapping_add(split as u32))?;
        let mut bytes = [0u8; 4];
        bytes[..split].copy_from_slice(&first_line[offset..]);
        bytes[split..].copy_from_slice(&second_line[..4 - split]);
        Ok(u32::from_le_bytes(bytes))
    }

    // D-cache tracked reads — hierarchical: L1 hit → return; miss → L2+/RAM fill
    fn dcache_read8(&mut self, addr: u32) -> Result<u8, FalconError> {
        if self.bypass {
            return self.ram.load8(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load8(addr);
        }
        let offset = self.dcache.config.addr_offset(addr);
        let line_data = self.load_dcache_line(addr)?;
        Ok(line_data[offset])
    }

    fn dcache_read16(&mut self, addr: u32) -> Result<u16, FalconError> {
        if self.bypass {
            return self.ram.load16(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load16(addr);
        }
        let line_size = self.dcache.config.line_size;
        let offset = self.dcache.config.addr_offset(addr);
        let first_line = self.load_dcache_line(addr)?;
        if offset + 1 < line_size {
            return Ok(u16::from_le_bytes([
                first_line[offset],
                first_line[offset + 1],
            ]));
        }

        let second_line = self.load_dcache_line(addr.wrapping_add(1))?;
        Ok(u16::from_le_bytes([first_line[offset], second_line[0]]))
    }

    fn dcache_read32(&mut self, addr: u32) -> Result<u32, FalconError> {
        if self.bypass {
            return self.ram.load32(addr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load32(addr);
        }
        let line_size = self.dcache.config.line_size;
        let offset = self.dcache.config.addr_offset(addr);
        let first_line = self.load_dcache_line(addr)?;
        if offset + 3 < line_size {
            return Ok(u32::from_le_bytes([
                first_line[offset],
                first_line[offset + 1],
                first_line[offset + 2],
                first_line[offset + 3],
            ]));
        }

        let split = line_size - offset;
        let second_line = self.load_dcache_line(addr.wrapping_add(split as u32))?;
        let mut bytes = [0u8; 4];
        bytes[..split].copy_from_slice(&first_line[offset..]);
        bytes[split..].copy_from_slice(&second_line[..4 - split]);
        Ok(u32::from_le_bytes(bytes))
    }

    fn total_cycles(&self) -> u64 {
        self.total_program_cycles()
    }

    fn fence_i(&mut self) -> Result<(), FalconError> {
        self.icache.invalidate();
        Ok(())
    }

    fn lr_w(&mut self, hart_id: u32, addr: u32) -> Result<u32, FalconError> {
        let aligned = reservation_addr(addr);
        let val = self.dcache_read32(aligned)?;
        self.reservations
            .insert(hart_id, Reservation { addr: aligned });
        Ok(val)
    }

    fn sc_w(&mut self, hart_id: u32, addr: u32, val: u32) -> Result<bool, FalconError> {
        let aligned = reservation_addr(addr);
        let success = self
            .reservations
            .get(&hart_id)
            .is_some_and(|res| res.addr == aligned);
        self.reservations.remove(&hart_id);
        if success {
            self.store32(aligned, val)?;
        }
        Ok(success)
    }

    fn amo_w(
        &mut self,
        _hart_id: u32,
        addr: u32,
        op: AmoOp,
        operand: u32,
    ) -> Result<u32, FalconError> {
        let aligned = reservation_addr(addr);
        let old = self.dcache_read32(aligned)?;
        let new = match op {
            AmoOp::Swap => operand,
            AmoOp::Add => old.wrapping_add(operand),
            AmoOp::Xor => old ^ operand,
            AmoOp::And => old & operand,
            AmoOp::Or => old | operand,
            AmoOp::Max => (old as i32).max(operand as i32) as u32,
            AmoOp::Min => (old as i32).min(operand as i32) as u32,
            AmoOp::MaxU => old.max(operand),
            AmoOp::MinU => old.min(operand),
        };
        self.store32(aligned, new)?;
        Ok(old)
    }
}

impl CacheController {
    /// Install a pre-fetched line into the D-cache, handling any dirty eviction writeback to RAM.
    fn install_dcache_line(
        &mut self,
        idx: usize,
        tag: u32,
        replacement: ReplacementPolicy,
        offset_bits: u32,
        index_bits: u32,
        line_data: Vec<u8>,
    ) -> Result<(), FalconError> {
        let way = self.dcache.sets[idx].find_victim(replacement);
        let evicted_valid = self.dcache.sets[idx].lines[way].valid;
        let evicted_dirty = self.dcache.sets[idx].lines[way].dirty;
        let evicted_tag = self.dcache.sets[idx].lines[way].tag;
        let evicted_data: Vec<u8> = if evicted_valid && evicted_dirty {
            self.dcache.sets[idx].lines[way].data.clone()
        } else {
            Vec::new()
        };

        if evicted_valid {
            self.dcache.stats.evictions += 1;
            if evicted_dirty {
                let evict_base =
                    (evicted_tag << (offset_bits + index_bits)) | ((idx as u32) << offset_bits);
                self.propagate_line_to_next_level(WritebackSource::L1D, evict_base, &evicted_data)?;
            }
        }
        self.dcache.stats.bytes_loaded += line_data.len() as u64;
        self.dcache.sets[idx].install(way, tag, line_data, replacement);
        Ok(())
    }
}
