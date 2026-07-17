// falcon/cache/controller.rs — Multi-level CacheController and Bus impl

use super::{Cache, CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::falcon::{
    errors::FalconError,
    memory::{AmoOp, Bus, Ram},
    mmu::{AccessType, Mmu, PageFault, TlbConfig},
};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Reservation {
    addr: u32,
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
    /// MMU / TLB. When `mmu.enabled == false` (default), translation is pure
    /// identity and zero-overhead. See `falcon::mmu`.
    pub(crate) mmu: Mmu,
}

#[derive(Clone, Copy)]
enum WritebackSource {
    L1D,
    Extra(usize),
}

/// A clone of every part of the cache subsystem **except the RAM** — the cache
/// levels, the MMU/TLB, and the scalar counters. Captured per journaled step so
/// a step-back can restore cache/TLB/stat state wholesale; the (large) RAM is
/// rewound separately via byte-level pre-images, so it never appears here.
///
/// For didactic configurations (small or disabled caches) this clone is cheap.
#[derive(Clone)]
pub struct CacheSnapshot {
    icache: Cache,
    dcache: Cache,
    extra_levels: Vec<Cache>,
    mmu: Mmu,
    instruction_count: u64,
    extra_cycles: u64,
    step_count: u64,
    bypass: bool,
    reservations: HashMap<u32, Reservation>,
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
            mmu: Mmu::new(TlbConfig::default()),
        }
    }

    /// Internal helper: translate a virtual address using the MMU and add the
    /// reported stall cycles to `extra_cycles`. Page faults are surfaced as
    /// `FalconError::Bus` for now; Phase 2 will route them through a real trap
    /// handler (mtvec / mepc / mcause).
    pub(crate) fn translate_for_access(
        &mut self,
        vaddr: u32,
        access: AccessType,
    ) -> Result<u32, FalconError> {
        match self.mmu.translate(vaddr, access, &mut self.ram) {
            Ok((paddr, stall)) => {
                if stall != 0 {
                    self.extra_cycles = self.extra_cycles.saturating_add(stall as u64);
                }
                Ok(paddr)
            }
            Err(PageFault { cause, vaddr }) => Err(FalconError::Trap {
                cause,
                tval: vaddr,
                vaddr,
            }),
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

        // TLB hit-rate history (only meaningful when VM is on).
        if self.mmu.enabled {
            let tlb_rate = self.mmu.tlb.stats.hit_rate();
            let hist = &mut self.mmu.tlb.stats.history;
            if hist.len() >= MAX_HISTORY {
                hist.pop_front();
            }
            hist.push_back((step, tlb_rate));
        }
    }

    /// Mutable access to the MMU — exposed for headless tests / harnesses
    /// that need to flip `enabled`, inspect TLB stats, or read `satp`. Runtime
    /// callers should prefer the `Bus::set_satp` / `Bus::tlb_flush` helpers.
    pub fn mmu(&self) -> &crate::falcon::mmu::Mmu {
        &self.mmu
    }
    pub fn mmu_mut(&mut self) -> &mut crate::falcon::mmu::Mmu {
        &mut self.mmu
    }

    /// Read-only access to the underlying RAM. The page-table tree view uses
    /// this to walk PTEs for display without mutating anything.
    pub fn ram(&self) -> &Ram {
        &self.ram
    }

    /// Mutable access to the underlying RAM. Tests use this to preload page
    /// tables before flipping `vm_enabled`.
    pub fn ram_mut(&mut self) -> &mut Ram {
        &mut self.ram
    }

    /// Clone the cache subsystem (everything but the RAM) into a
    /// [`CacheSnapshot`] for the step journal. See that type's docs.
    pub fn snapshot_state(&self) -> CacheSnapshot {
        CacheSnapshot {
            icache: self.icache.clone(),
            dcache: self.dcache.clone(),
            extra_levels: self.extra_levels.clone(),
            mmu: self.mmu.clone(),
            instruction_count: self.instruction_count,
            extra_cycles: self.extra_cycles,
            step_count: self.step_count,
            bypass: self.bypass,
            reservations: self.reservations.clone(),
        }
    }

    /// Restore the cache subsystem from a [`CacheSnapshot`]. The RAM is left
    /// untouched — the caller rewinds it separately via its byte pre-images.
    pub fn restore_state(&mut self, snap: CacheSnapshot) {
        self.icache = snap.icache;
        self.dcache = snap.dcache;
        self.extra_levels = snap.extra_levels;
        self.mmu = snap.mmu;
        self.instruction_count = snap.instruction_count;
        self.extra_cycles = snap.extra_cycles;
        self.step_count = snap.step_count;
        self.bypass = snap.bypass;
        self.reservations = snap.reservations;
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

    fn measure_access_latency<T, F>(&mut self, access: F) -> (Result<T, FalconError>, u64)
    where
        F: FnOnce(&mut Self) -> Result<T, FalconError>,
    {
        let before = self.total_program_cycles();
        let result = access(self);
        let after = self.total_program_cycles();
        (result, after.saturating_sub(before))
    }

    pub fn fetch32_timed(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::fetch32(mem, addr))
    }

    pub fn fetch32_timed_no_count(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        let before_instr = self.instruction_count;
        let (result, latency) =
            self.measure_access_latency(|mem| <Self as Bus>::fetch32(mem, addr));
        self.instruction_count = before_instr;
        (result, latency)
    }

    pub fn dcache_read8_timed(&mut self, addr: u32) -> (Result<u8, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::dcache_read8(mem, addr))
    }

    pub fn dcache_read16_timed(&mut self, addr: u32) -> (Result<u16, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::dcache_read16(mem, addr))
    }

    pub fn dcache_read32_timed(&mut self, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::dcache_read32(mem, addr))
    }

    pub fn store8_timed(&mut self, addr: u32, val: u8) -> (Result<(), FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::store8(mem, addr, val))
    }

    pub fn store16_timed(&mut self, addr: u32, val: u16) -> (Result<(), FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::store16(mem, addr, val))
    }

    pub fn store32_timed(&mut self, addr: u32, val: u32) -> (Result<(), FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::store32(mem, addr, val))
    }

    pub fn lr_w_timed(&mut self, hart_id: u32, addr: u32) -> (Result<u32, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::lr_w(mem, hart_id, addr))
    }

    pub fn sc_w_timed(
        &mut self,
        hart_id: u32,
        addr: u32,
        val: u32,
    ) -> (Result<bool, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::sc_w(mem, hart_id, addr, val))
    }

    pub fn amo_w_timed(
        &mut self,
        hart_id: u32,
        addr: u32,
        op: AmoOp,
        operand: u32,
    ) -> (Result<u32, FalconError>, u64) {
        self.measure_access_latency(|mem| <Self as Bus>::amo_w(mem, hart_id, addr, op, operand))
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

    /// Contabiliza o fetch de instrução no I-cache sem retornar o dado.
    /// Usado pelo JIT: a palavra já foi decodificada em compile-time; só
    /// precisamos atualizar hits/misses e instruction_count para manter
    /// fidelidade de métricas. Evita o Vec::clone() do hit path de fetch32.
    pub(crate) fn account_fetch32(&mut self, addr: u32) {
        if self.bypass || !self.icache.config.is_valid_config() {
            self.instruction_count += 1;
            return;
        }
        let tag = self.icache.config.addr_tag(addr);
        let idx = self.icache.config.addr_index(addr);
        let tag_search = self.icache.config.tag_search_cycles();
        let replacement = self.icache.config.replacement;
        self.instruction_count += 1;
        if let Some(way) = self.icache.sets[idx].lookup(tag) {
            self.icache.stats.hits += 1;
            self.icache.stats.total_cycles += tag_search;
            self.icache.sets[idx].touch(way, replacement);
            return;
        }
        // Miss: preenche a linha para manter o estado do cache coerente com o
        // interpretador, mas descarta o dado (não precisamos da palavra).
        let _ = self.load_icache_line(addr);
    }

    /// Word-sized D-cache read at an already-translated physical address. Used
    /// internally by `lr_w` / `amo_w` to avoid translating twice.
    pub(crate) fn dcache_read_pa32(&mut self, paddr: u32) -> Result<u32, FalconError> {
        if self.bypass {
            return self.ram.load32(paddr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load32(paddr);
        }
        let line_size = self.dcache.config.line_size;
        let offset = self.dcache.config.addr_offset(paddr);
        let first_line = self.load_dcache_line(paddr)?;
        if offset + 3 < line_size {
            return Ok(u32::from_le_bytes([
                first_line[offset],
                first_line[offset + 1],
                first_line[offset + 2],
                first_line[offset + 3],
            ]));
        }
        let split = line_size - offset;
        let second_line = self.load_dcache_line(paddr.wrapping_add(split as u32))?;
        let mut bytes = [0u8; 4];
        bytes[..split].copy_from_slice(&first_line[offset..]);
        bytes[split..].copy_from_slice(&second_line[..4 - split]);
        Ok(u32::from_le_bytes(bytes))
    }

    /// Word-sized D-cache store at an already-translated physical address.
    /// Counterpart to `dcache_read_pa32` for `sc_w` / `amo_w`.
    pub(crate) fn store_pa32(&mut self, paddr: u32, val: u32) -> Result<(), FalconError> {
        self.dcache_store_bytes(paddr, &val.to_le_bytes())
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
    fn mem_len(&self) -> u32 {
        self.ram.data_len().min(u32::MAX as usize) as u32
    }

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
        let addr = self.translate_for_access(addr, AccessType::Store)?;
        self.dcache_store_bytes(addr, &[val])
    }
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError> {
        let addr = self.translate_for_access(addr, AccessType::Store)?;
        self.dcache_store_bytes(addr, &val.to_le_bytes())
    }
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError> {
        let addr = self.translate_for_access(addr, AccessType::Store)?;
        self.dcache_store_bytes(addr, &val.to_le_bytes())
    }

    // I-cache tracked fetch — hierarchical: L1 hit → return; miss → L2+/RAM fill
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        let addr = self.translate_for_access(addr, AccessType::Fetch)?;
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
        let addr = self.translate_for_access(addr, AccessType::Load)?;
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
        let addr = self.translate_for_access(addr, AccessType::Load)?;
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
        let addr = self.translate_for_access(addr, AccessType::Load)?;
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

    fn translate(
        &mut self,
        vaddr: u32,
        access: AccessType,
    ) -> Result<(u32, u8), PageFault> {
        self.mmu.translate(vaddr, access, &mut self.ram)
    }

    fn tlb_flush(&mut self) {
        self.mmu.flush();
    }

    fn tlb_flush_vaddr(&mut self, vaddr: u32) {
        self.mmu.tlb.flush_vaddr(vaddr);
    }

    fn set_satp(&mut self, val: u32) {
        self.mmu.satp = crate::falcon::mmu::Satp::new(val);
        self.mmu.flush();
    }

    fn set_priv_mode(&mut self, mode: crate::falcon::mmu::PrivMode) {
        self.mmu.priv_mode = mode;
    }

    fn lr_w(&mut self, hart_id: u32, addr: u32) -> Result<u32, FalconError> {
        // Translate the load early — page faults must surface before the
        // cache is touched. Reservation is keyed on the *physical* aligned
        // address so a later satp/sfence.vma remap can't accidentally make
        // a later SC succeed against the wrong page.
        let paddr = self.translate_for_access(addr, AccessType::Load)?;
        let aligned = paddr & !0x3;
        let val = self.dcache_read_pa32(aligned)?;
        self.reservations
            .insert(hart_id, Reservation { addr: aligned });
        Ok(val)
    }

    fn sc_w(&mut self, hart_id: u32, addr: u32, val: u32) -> Result<bool, FalconError> {
        // Translate as a Store first — the W permission check happens before
        // we consult the reservation (matches real LR/SC semantics).
        let paddr = self.translate_for_access(addr, AccessType::Store)?;
        let aligned = paddr & !0x3;
        let success = self
            .reservations
            .get(&hart_id)
            .is_some_and(|res| res.addr == aligned);
        self.reservations.remove(&hart_id);
        if success {
            self.store_pa32(aligned, val)?;
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
        // Single Store-mode translation up front so the W check faults before
        // the cache read commits side-effects (hits/misses). Both halves of
        // the AMO then operate on the resolved paddr.
        let paddr = self.translate_for_access(addr, AccessType::Store)?;
        let aligned = paddr & !0x3;
        let old = self.dcache_read_pa32(aligned)?;
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
        self.store_pa32(aligned, new)?;
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
