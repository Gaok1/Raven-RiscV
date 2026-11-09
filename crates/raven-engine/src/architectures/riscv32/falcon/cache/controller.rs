// falcon/cache/controller.rs â€” Multi-level CacheController and Bus impl

use super::coherence::{CoherenceConfig, CoherenceProtocol, CoherenceTracker, line_state};
use super::{Cache, CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::capability::{
    CacheHierarchy, CacheHistory, CacheInclusionPolicy, CacheLevelConfig, CacheLevelStats,
    CacheLevelView, CacheLineView, CacheReplacementPolicy, CacheRole, CacheSetView,
    CacheWriteAllocation, CacheWritePolicy, CoherenceRequest, CoherenceState, CoherenceTransition,
    CoherenceView,
};
use crate::falcon::{
    errors::FalconError,
    memory::{AmoOp, Bus, HostRequest, Ram},
    mmu::{
        AccessType, Mmu, PageFault, PageMapSpec, PteMemory, Satp, Tlb, TlbConfig,
        TranslationRecord,
    },
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
    pub ram: Ram,
    pub icache: Cache,
    pub dcache: Cache,
    /// Extra unified cache levels: `extra_levels[0]`=L2, `extra_levels[1]`=L3, â€¦
    pub extra_levels: Vec<Cache>,
    pub instruction_count: u64,
    /// Base instruction-execution cycles (not cache): set via add_instruction_cycles().
    pub extra_cycles: u64,
    step_count: u64,
    /// When true, all cache lookups are skipped and RAM is accessed directly (no stats, no latency).
    pub bypass: bool,
    reservations: HashMap<u32, Reservation>,
    /// MMU / TLB. When `mmu.enabled == false` (default), translation is pure
    /// identity and zero-overhead. See `falcon::mmu`.
    pub mmu: Mmu,
    /// Physical address of the HTIF `tohost` word, when the loaded program
    /// has one. See [`CacheController::set_tohost`].
    tohost: Option<u32>,
    /// What the last store to `tohost` asked for, until the hart's next
    /// instruction boundary takes it ([`Bus::take_host_request`]).
    host_request: Option<HostRequest>,
    /// An unsupported request is reported once per program, not per store.
    host_request_reported: bool,
    /// Whether each core has its own L1, and how those L1s stay coherent.
    coherence: CoherenceConfig,
    /// The core whose private L1 is in `icache` and `dcache` right now. With
    /// a shared L1 it only names the core that ran last.
    active_core: usize,
    /// The other cores' private L1s, by core. `banks[active_core]` holds a
    /// spare: switching cores swaps caches in and out, never allocates. Empty
    /// with a shared L1.
    banks: Vec<L1Bank>,
    /// Bus counters per core and the recent requests.
    coh: CoherenceTracker,
    /// Every core has its own TLB, as every RISC-V hart does. The active
    /// core's sits in `mmu.tlb`; the others wait here by core, and
    /// `tlbs[active_core]` holds a spare — the same arrangement as `banks`.
    tlbs: Vec<Tlb>,
}

/// One core's private L1 while another core runs.
#[derive(Clone)]
struct L1Bank {
    icache: Cache,
    dcache: Cache,
}

/// What the other caches did with one bus request.
#[derive(Default)]
struct SnoopOutcome {
    /// Some other cache still holds the line after the request.
    others_hold: bool,
    /// The line, sent by another cache instead of the shared level.
    data: Option<Vec<u8>>,
    supplier: Option<usize>,
    /// A dirty copy was written back to the shared level first.
    flushed: bool,
    transitions: Vec<CoherenceTransition>,
}

#[derive(Clone, Copy)]
enum WritebackSource {
    L1D,
    Extra(usize),
}

/// A clone of every part of the cache subsystem **except the RAM** â€” the cache
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
    coherence: CoherenceConfig,
    active_core: usize,
    banks: Vec<L1Bank>,
    coh: CoherenceTracker,
    tlbs: Vec<Tlb>,
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
            tohost: None,
            host_request: None,
            host_request_reported: false,
            coherence: CoherenceConfig::default(),
            active_core: 0,
            banks: Vec::new(),
            coh: CoherenceTracker::default(),
            tlbs: Vec::new(),
        }
    }

    // ── Private L1s and coherence ───────────────────────────────────────────

    pub fn coherence_config(&self) -> &CoherenceConfig {
        &self.coherence
    }

    pub fn coherence_protocol(&self) -> CoherenceProtocol {
        self.coherence.protocol
    }

    /// Bus counters per core and the recent requests.
    pub fn coherence_tracker(&self) -> &CoherenceTracker {
        &self.coh
    }

    /// Switch protocol, or between one shared L1 and one per core.
    ///
    /// Dirty data survives the switch: the other cores' private L1s write
    /// their dirty lines to the shared level and go away, and the L1 in use
    /// becomes the shared one, or the active core's. Counters start over.
    pub fn set_coherence(&mut self, config: CoherenceConfig) {
        let was_private = self.coherence.protocol.private_l1();
        if was_private {
            for core in 0..self.banks.len() {
                if core != self.active_core {
                    let _ = self.flush_bank(core);
                }
            }
            self.banks.clear();
        }
        self.coherence = config;
        // The L1 that stays was the only copy of what it holds. A protocol
        // with no Exclusive state has no clean exclusive line: that is Shared.
        let exclusive_clean = self.coherence.protocol.has_exclusive();
        for set in &mut self.dcache.sets {
            for line in &mut set.lines {
                line.exclusive = line.valid && (line.dirty || exclusive_clean);
                line.invalidated = false;
            }
        }
        self.coh = CoherenceTracker::default();
    }

    /// Change what later bus requests cost, without touching any cache.
    pub fn set_coherence_latencies(&mut self, snoop: u64, cache_to_cache: u64) {
        self.coherence.snoop_latency = snoop;
        self.coherence.cache_to_cache_latency = cache_to_cache;
    }

    /// How many cores to keep private L1s for. Hosts call it once they know
    /// the core count, so a core that has not run yet still has caches to
    /// show. A core beyond it gets its L1 the first time it runs.
    pub fn set_core_count(&mut self, cores: usize) {
        if self.coherence.protocol.private_l1() {
            self.ensure_banks(cores.max(self.active_core + 1));
        }
        self.ensure_tlbs(cores.max(self.active_core + 1));
        self.coh.core(cores.saturating_sub(1));
    }

    /// The core whose accesses come next. With private L1s this puts that
    /// core's caches in front of the shared levels; it costs two swaps, so a
    /// host calls it before every turn a core gets.
    pub fn set_active_core(&mut self, core: usize) {
        if core == self.active_core {
            return;
        }
        // Each hart translates through its own TLB, whatever the caches do.
        self.ensure_tlbs(core.max(self.active_core) + 1);
        std::mem::swap(&mut self.mmu.tlb, &mut self.tlbs[core]);
        self.tlbs.swap(self.active_core, core);
        if self.coherence.protocol.private_l1() {
            self.ensure_banks(core.max(self.active_core) + 1);
            // `banks[active]` is the spare. Swap the new core's L1 in — the
            // old core's lands in the new core's slot — then trade the two
            // slots so each L1 sits under its own core again.
            std::mem::swap(&mut self.icache, &mut self.banks[core].icache);
            std::mem::swap(&mut self.dcache, &mut self.banks[core].dcache);
            self.banks.swap(self.active_core, core);
        }
        self.active_core = core;
    }

    pub fn active_core(&self) -> usize {
        self.active_core
    }

    fn ensure_tlbs(&mut self, cores: usize) {
        while self.tlbs.len() < cores {
            self.tlbs.push(Tlb::new(self.mmu.tlb.config.clone()));
        }
    }

    /// `core`'s TLB, wherever it is.
    pub fn tlb_of(&self, core: usize) -> &Tlb {
        if core == self.active_core {
            return &self.mmu.tlb;
        }
        self.tlbs.get(core).unwrap_or(&self.mmu.tlb)
    }

    /// How many cores keep a TLB — every core the host has announced or run.
    pub fn tlb_count(&self) -> usize {
        self.tlbs.len().max(self.active_core + 1)
    }

    /// Rebuild every core's TLB in a new shape; entries and counters start over.
    pub fn reconfigure_tlbs(&mut self, cfg: TlbConfig) {
        self.mmu.tlb.reconfigure(cfg.clone());
        for tlb in &mut self.tlbs {
            tlb.reconfigure(cfg.clone());
        }
    }

    /// Change what a hit and a walk cost on every core, keeping what the TLBs
    /// hold.
    pub fn set_tlb_latencies(&mut self, hit: u8, walk: u8) {
        for tlb in std::iter::once(&mut self.mmu.tlb).chain(self.tlbs.iter_mut()) {
            tlb.config.hit_latency = hit;
            tlb.config.miss_penalty = walk;
        }
    }

    /// Drop every core's cached translations. `sfence.vma` flushes only the
    /// hart that runs it; this is the host's flush.
    pub fn flush_all_tlbs(&mut self) {
        self.mmu.flush();
        for tlb in &mut self.tlbs {
            tlb.flush();
        }
    }

    fn ensure_banks(&mut self, cores: usize) {
        while self.banks.len() < cores {
            self.banks.push(L1Bank {
                icache: Cache::new(self.icache.config.clone()),
                dcache: Cache::new(self.dcache.config.clone()),
            });
        }
    }

    /// `core`'s L1 I-cache and D-cache, wherever they are. With a shared L1
    /// every core gets the same pair.
    pub fn l1_of(&self, core: usize) -> Option<(&Cache, &Cache)> {
        if core == self.active_core || !self.coherence.protocol.private_l1() {
            return Some((&self.icache, &self.dcache));
        }
        self.banks.get(core).map(|bank| (&bank.icache, &bank.dcache))
    }

    /// The other cores whose private D-caches a request has to reach.
    fn peers(&self) -> impl Iterator<Item = usize> + use<> {
        let active = self.active_core;
        (0..self.banks.len()).filter(move |&core| core != active)
    }

    /// Every private L1 that is not the active core's, for whole-machine sums.
    fn parked_banks(&self) -> impl Iterator<Item = &L1Bank> {
        self.banks
            .iter()
            .enumerate()
            .filter(|(core, _)| *core != self.active_core)
            .map(|(_, bank)| bank)
    }

    /// Write `core`'s dirty D-cache lines to the shared level. `core` is not
    /// the active one.
    fn flush_bank(&mut self, core: usize) -> Result<(), FalconError> {
        let cfg = &self.banks[core].dcache.config;
        if !cfg.is_valid_config() {
            return Ok(());
        }
        let (offset_bits, index_bits) = (cfg.offset_bits(), cfg.index_bits());
        let mut dirty = Vec::new();
        for (set_idx, set) in self.banks[core].dcache.sets.iter_mut().enumerate() {
            for line in &mut set.lines {
                if line.valid && line.dirty {
                    let base = (line.tag << (offset_bits + index_bits))
                        | ((set_idx as u32) << offset_bits);
                    dirty.push((base, line.data.clone()));
                    line.dirty = false;
                }
            }
        }
        for (base, data) in dirty {
            self.banks[core].dcache.stats.writebacks += 1;
            self.writeback_bytes_to_level(WritebackSource::L1D, 0, base, &data)?;
        }
        Ok(())
    }

    /// Put `request` for `addr`'s line on the bus and let every other core's
    /// D-cache answer it, as a snooping protocol does. Only called under a
    /// protocol that keeps the caches coherent. The active core waits
    /// `snoop_latency` for the answers, plus whatever write-back a dirty copy
    /// needed.
    fn snoop(&mut self, request: CoherenceRequest, addr: u32) -> Result<SnoopOutcome, FalconError> {
        use CoherenceRequest::*;
        use CoherenceState::*;
        let protocol = self.coherence.protocol;
        let line_base = self.dcache.config.line_base(addr);
        let mut outcome = SnoopOutcome::default();
        let me = self.active_core;
        match request {
            BusRd => self.coh.core(me).bus_reads += 1,
            BusRdX => self.coh.core(me).bus_read_exclusive += 1,
            BusUpgr => self.coh.core(me).bus_upgrades += 1,
            BusWr => self.coh.core(me).bus_writes += 1,
            IFetch | WriteBack => {}
        }
        let snoop = self.coherence.snoop_latency;
        self.coh.core(me).bus_cycles += snoop;
        if request == IFetch {
            self.icache.stats.total_cycles += snoop;
        } else {
            self.dcache.stats.total_cycles += snoop;
        }

        for peer in self.peers() {
            let Some((set, way)) = self.banks[peer].dcache.locate(line_base) else {
                continue;
            };
            let from = line_state(&self.banks[peer].dcache.sets[set].lines[way]);
            let dirty = matches!(from, Modified | Owned);
            // What the request leaves in the peer's cache, and whether it
            // sends the data across or writes it back on the way.
            let (to, supply, flush) = match request {
                BusRd if dirty && protocol.has_owned() => (Owned, true, false),
                BusRd if dirty => (Shared, false, true),
                BusRd => (Shared, false, false),
                IFetch if dirty => (Shared, false, true),
                IFetch => (from, false, false),
                BusRdX if dirty && protocol.has_owned() => (Invalid, true, false),
                BusRdX if dirty => (Invalid, false, true),
                // The requester holds the line too, Shared, with the same
                // bytes; it becomes the dirty owner, so the copy here can go.
                BusUpgr => (Invalid, false, false),
                // The write lands below this cache: a dirty copy has to get
                // there first, or its write-back later would undo the write.
                BusWr if dirty => (Invalid, false, true),
                BusRdX | BusWr => (Invalid, false, false),
                WriteBack => (from, false, false),
            };

            let line = &mut self.banks[peer].dcache.sets[set].lines[way];
            let data = (supply || flush).then(|| line.data.clone());
            match to {
                Invalid => {
                    line.valid = false;
                    line.dirty = false;
                    line.exclusive = false;
                    line.invalidated = true;
                }
                Owned => line.exclusive = false,
                Shared => {
                    if flush {
                        line.dirty = false;
                    }
                    line.exclusive = false;
                }
                _ => {}
            }
            if to == Invalid {
                self.coh.core(peer).invalidations_received += 1;
            } else {
                outcome.others_hold = true;
            }
            if let Some(data) = data {
                if supply {
                    self.coh.core(peer).cache_to_cache += 1;
                    outcome.supplier = Some(peer);
                    outcome.data = Some(data);
                } else {
                    self.coh.core(peer).flushes += 1;
                    self.banks[peer].dcache.stats.writebacks += 1;
                    let wait = self.dcache.config.miss_penalty
                        + self.dcache.config.line_transfer_cycles();
                    self.dcache.stats.total_cycles += wait;
                    self.coh.core(me).bus_cycles += wait;
                    self.writeback_bytes_to_level(WritebackSource::L1D, 0, line_base, &data)?;
                    outcome.flushed = true;
                }
            }
            if to != from {
                outcome.transitions.push(CoherenceTransition { core: peer, from, to });
            }
        }
        Ok(outcome)
    }

    /// The state of `addr`'s line in the active core's D-cache.
    fn active_state(&self, addr: u32) -> CoherenceState {
        self.dcache
            .locate(addr)
            .map_or(CoherenceState::Invalid, |(set, way)| {
                line_state(&self.dcache.sets[set].lines[way])
            })
    }

    /// Set the `exclusive` bit of `addr`'s line in the active D-cache.
    fn set_active_exclusive(&mut self, addr: u32, exclusive: bool) {
        if let Some((set, way)) = self.dcache.locate(addr) {
            self.dcache.sets[set].lines[way].exclusive = exclusive;
        }
    }

    /// Record `request` in the event log, with the active core's own
    /// transition first.
    fn log_request(
        &mut self,
        request: CoherenceRequest,
        addr: u32,
        from: CoherenceState,
        outcome: SnoopOutcome,
    ) {
        let line_base = self.dcache.config.line_base(addr);
        let to = self.active_state(addr);
        let mut transitions = Vec::with_capacity(outcome.transitions.len() + 1);
        transitions.push(CoherenceTransition { core: self.active_core, from, to });
        transitions.extend(outcome.transitions);
        self.coh.log(
            self.instruction_count,
            self.active_core,
            request,
            line_base,
            transitions,
            outcome.supplier,
            outcome.flushed,
        );
    }

    /// A clean line may be Exclusive only under a protocol with that state,
    /// and only with write-back: under write-through every store goes on the
    /// bus anyway, so the caches keep plain valid copies.
    fn clean_exclusive_allowed(&self) -> bool {
        self.coherence.protocol.has_exclusive()
            && self.dcache.config.write_policy == WritePolicy::WriteBack
    }

    /// Before a store that hits the active D-cache: under a coherent
    /// protocol, a line the core does not hold exclusively has to invalidate
    /// the other copies first (`BusUpgr`).
    fn upgrade_for_write(&mut self, addr: u32) -> Result<(), FalconError> {
        if !self.coherence.protocol.coherent()
            || self.dcache.config.write_policy != WritePolicy::WriteBack
        {
            return Ok(());
        }
        let from = self.active_state(addr);
        if matches!(from, CoherenceState::Modified | CoherenceState::Exclusive) {
            return Ok(());
        }
        let outcome = self.snoop(CoherenceRequest::BusUpgr, addr)?;
        self.set_active_exclusive(addr, true);
        // The store sets the dirty bit next; log the state it leaves.
        if let Some((set, way)) = self.dcache.locate(addr) {
            self.dcache.sets[set].lines[way].dirty = true;
        }
        self.log_request(CoherenceRequest::BusUpgr, addr, from, outcome);
        Ok(())
    }

    /// Give the bus the physical address of the program's HTIF `tohost` word
    /// (an ELF's `tohost` symbol), or `None` for a program without one.
    ///
    /// The word stays ordinary RAM. What changes is that a 32-bit store to it
    /// is also a request to the host — see [`HostRequest`]. Stores to the
    /// upper half, `tohost + 4`, are ignored: RV32 programs write the 64-bit
    /// word in two halves and only the lower one carries a request RAVEN
    /// answers. Every load must set this anew, so a program never inherits the
    /// previous one's address.
    pub fn set_tohost(&mut self, paddr: Option<u32>) {
        self.tohost = paddr;
        self.host_request = None;
        self.host_request_reported = false;
    }

    /// The `tohost` address the loaded program gave, if any.
    pub fn tohost(&self) -> Option<u32> {
        self.tohost
    }

    /// Run `edit` with `tohost` unwatched.
    ///
    /// For writes the program did not make — a debugger edit of a memory
    /// cell. Those are not the program talking to the host: one landing on
    /// `tohost` must neither end the run at whatever instruction comes next
    /// nor use up the one report of an unsupported request.
    pub fn with_tohost_unwatched<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        let tohost = self.tohost.take();
        let result = edit(self);
        self.tohost = tohost;
        result
    }

    /// Note what a word store of `val` at `paddr` asks of the host, if it
    /// landed on `tohost`. Zero is the program clearing the word.
    fn observe_tohost_store(&mut self, paddr: u32, val: u32) {
        if self.tohost != Some(paddr) || val == 0 {
            return;
        }
        if val & 1 == 1 {
            self.host_request = Some(HostRequest::Exit(val >> 1));
        } else if !self.host_request_reported {
            self.host_request_reported = true;
            self.host_request = Some(HostRequest::Unsupported(val));
        }
    }

    /// Translate a virtual address using the MMU and add the reported stall
    /// cycles to `extra_cycles`. A page fault comes back as
    /// `FalconError::Trap`, which the hart routes to its trap handler.
    pub fn translate_for_access(
        &mut self,
        vaddr: u32,
        access: AccessType,
    ) -> Result<u32, FalconError> {
        match self.run_mmu(vaddr, access) {
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

    /// Mutable access to the MMU â€” exposed for headless tests / harnesses
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
            coherence: self.coherence.clone(),
            active_core: self.active_core,
            banks: self.banks.clone(),
            coh: self.coh.clone(),
            tlbs: self.tlbs.clone(),
        }
    }

    /// Restore the cache subsystem from a [`CacheSnapshot`]. The RAM is left
    /// untouched â€” the caller rewinds it separately via its byte pre-images.
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
        self.coherence = snap.coherence;
        self.active_core = snap.active_core;
        self.banks = snap.banks;
        self.coh = snap.coh;
        self.tlbs = snap.tlbs;
        // A snapshot is taken between instructions, where no request is left
        // waiting: one still here was made after it, by a store this rewinds.
        self.host_request = None;
    }

    pub fn reset_stats(&mut self) {
        self.icache.reset_stats();
        self.dcache.reset_stats();
        for bank in &mut self.banks {
            bank.icache.reset_stats();
            bank.dcache.reset_stats();
        }
        self.coh.reset();
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
        // Every core's private L1 takes the new shape.
        let cores = self.banks.len();
        self.banks.clear();
        if self.coherence.protocol.private_l1() {
            self.ensure_banks(cores);
        }
        self.reset_stats();
    }

    pub fn total_program_cycles(&self) -> u64 {
        self.total_cache_cycles() + self.extra_cycles
    }

    pub fn total_cache_cycles(&self) -> u64 {
        let mut total = self.icache.stats.total_cycles + self.dcache.stats.total_cycles;
        for bank in self.parked_banks() {
            total += bank.icache.stats.total_cycles + bank.dcache.stats.total_cycles;
        }
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

        // Code another core just wrote may sit dirty in its D-cache. The
        // fetch is a bus read like any other, so that copy writes itself back
        // before the I-cache fills from the shared level.
        if self.coherence.protocol.coherent() {
            let outcome = self.snoop(CoherenceRequest::IFetch, line_base)?;
            if !outcome.transitions.is_empty() {
                self.coh.log(
                    self.instruction_count,
                    self.active_core,
                    CoherenceRequest::IFetch,
                    line_base,
                    outcome.transitions,
                    None,
                    outcome.flushed,
                );
            }
        }

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
        self.load_dcache_line_for(addr, false)
    }

    /// Read `addr`'s line through the active D-cache. `for_write` says the
    /// read is the first half of a read-modify-write (an AMO): a miss then
    /// asks for ownership (`BusRdX`) instead of a shared copy.
    fn load_dcache_line_for(&mut self, addr: u32, for_write: bool) -> Result<Vec<u8>, FalconError> {
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
        if self.coherence.protocol.coherent() {
            // Under write-through the store that follows goes on the bus
            // itself, so the read only needs a copy.
            let for_write = for_write && self.dcache.config.write_policy == WritePolicy::WriteBack;
            let request = if for_write {
                CoherenceRequest::BusRdX
            } else {
                CoherenceRequest::BusRd
            };
            return self.coherent_fill(addr, request);
        }
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

    /// Fill the active D-cache after a miss under a coherent protocol. The
    /// request goes on the bus first; the line then comes from the cache that
    /// owns it (MOESI) or from the shared level, and lands Exclusive when no
    /// other cache kept a copy and the protocol has that state, Modified-to-be
    /// for a write, and Shared otherwise. The miss itself is already counted;
    /// this charges the transfer.
    fn coherent_fill(
        &mut self,
        addr: u32,
        request: CoherenceRequest,
    ) -> Result<Vec<u8>, FalconError> {
        let cfg = &self.dcache.config;
        let tag_search = cfg.tag_search_cycles();
        let fetch_cost = cfg.miss_penalty + cfg.line_transfer_cycles();
        let line_base = cfg.line_base(addr);
        let line_size = cfg.line_size;
        let replacement = cfg.replacement;
        let offset_bits = cfg.offset_bits();
        let index_bits = cfg.index_bits();
        let tag = cfg.addr_tag(addr);
        let idx = cfg.addr_index(addr);

        if self.dcache.was_invalidated(addr) {
            self.coh.core(self.active_core).coherence_misses += 1;
        }
        let mut outcome = self.snoop(request, line_base)?;
        let line_data = match outcome.data.take() {
            Some(data) => {
                let c2c = self.coherence.cache_to_cache_latency;
                self.dcache.stats.total_cycles += tag_search + c2c;
                self.coh.core(self.active_core).bus_cycles += c2c;
                data
            }
            None => {
                self.dcache.stats.total_cycles += tag_search + fetch_cost;
                self.fetch_line(line_base, line_size, 0)?
            }
        };
        self.install_dcache_line(
            idx,
            tag,
            replacement,
            offset_bits,
            index_bits,
            line_data.clone(),
        )?;
        let exclusive = match request {
            CoherenceRequest::BusRdX => true,
            _ => !outcome.others_hold && self.clean_exclusive_allowed(),
        };
        self.set_active_exclusive(line_base, exclusive);
        if request == CoherenceRequest::BusRdX {
            // A write follows at once; the line is Modified when it ends.
            if let Some((set, way)) = self.dcache.locate(line_base) {
                self.dcache.sets[set].lines[way].dirty = true;
            }
        }
        self.log_request(request, line_base, CoherenceState::Invalid, outcome);
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

        let coherent = self.coherence.protocol.coherent();
        match self.dcache.config.write_policy {
            WritePolicy::WriteThrough => {
                // Every write-through store is on the bus already: it
                // invalidates the other cores' copies on its way to memory.
                let bus_write = if coherent {
                    let from = self.active_state(addr);
                    Some((from, self.snoop(CoherenceRequest::BusWr, addr)?))
                } else {
                    None
                };
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
                if let Some((from, outcome)) = bus_write {
                    self.set_active_exclusive(addr, false);
                    self.log_request(CoherenceRequest::BusWr, addr, from, outcome);
                }
            }
            WritePolicy::WriteBack => {
                self.dcache.stats.bytes_stored += bytes.len() as u64;
                if let Some(way) = hit_way {
                    self.dcache.stats.hits += 1;
                    self.dcache.stats.total_cycles += tag_search;
                    self.upgrade_for_write(addr)?;
                    for (i, &b) in bytes.iter().enumerate() {
                        self.dcache.sets[idx].lines[way].data[offset + i] = b;
                    }
                    self.dcache.sets[idx].lines[way].dirty = true;
                    self.dcache.sets[idx].touch(way, replacement);
                } else if coherent {
                    self.dcache.stats.misses += 1;
                    if let WriteAllocPolicy::WriteAllocate = self.dcache.config.write_alloc {
                        // Read for ownership: every other copy goes, and the
                        // line arrives here to be written.
                        self.coherent_fill(addr, CoherenceRequest::BusRdX)?;
                        if let Some((set, way)) = self.dcache.locate(addr) {
                            for (i, &b) in bytes.iter().enumerate() {
                                self.dcache.sets[set].lines[way].data[offset + i] = b;
                            }
                        }
                    } else {
                        // No allocation: the write goes to the shared level,
                        // where the other cores' next miss will find it.
                        self.dcache.stats.total_cycles += tag_search + miss_penalty + transfer_cyc;
                        let outcome = self.snoop(CoherenceRequest::BusWr, addr)?;
                        self.writeback_bytes_to_level(WritebackSource::L1D, 0, addr, bytes)?;
                        self.log_request(
                            CoherenceRequest::BusWr,
                            addr,
                            CoherenceState::Invalid,
                            outcome,
                        );
                    }
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
        // â€” e.g., when the evicted line and the newly-written line share the
        // same L2 cache line â€” causing silent data loss.

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
        for bank in &mut self.banks {
            bank.icache.invalidate();
            bank.dcache.invalidate();
        }
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
        // Private L1s: under a protocol at most one of them holds a line
        // dirty, so the order does not matter. With coherence off several may
        // hold different bytes for the same line; the active core's go last
        // and win, matching what `effective_read8` shows.
        let active = self.active_core;
        for (core, bank) in self.banks.iter_mut().enumerate() {
            if core != active {
                bank.dcache.writeback_dirty(&mut self.ram);
            }
        }
        self.dcache.writeback_dirty(&mut self.ram);
        self.drop_clean_exclusive_if_msi();
    }

    /// MSI has no clean exclusive state: a line written back is Shared.
    fn drop_clean_exclusive_if_msi(&mut self) {
        if self.coherence.protocol != CoherenceProtocol::Msi {
            return;
        }
        let caches = std::iter::once(&mut self.dcache)
            .chain(self.banks.iter_mut().map(|bank| &mut bank.dcache));
        for cache in caches {
            for line in cache.sets.iter_mut().flat_map(|set| set.lines.iter_mut()) {
                if !line.dirty {
                    line.exclusive = false;
                }
            }
        }
    }

    /// Write-back all dirty D-cache lines to RAM, then invalidate all caches.
    /// Use this before disabling the cache to keep RAM consistent.
    pub fn flush_all(&mut self) {
        // I-cache is read-only â€” just invalidate
        self.icache.invalidate();
        // Flush outer levels first (L3, L2, ...) then D-cache (L1).
        // D-cache is always authoritative: writing L2 after D-cache would
        // overwrite the correct D-cache data with stale L2 data in RAM.
        let ram_ptr = &mut self.ram as *mut Ram;
        for level in self.extra_levels.iter_mut().rev() {
            // SAFETY: `ram` and `extra_levels` are distinct fields, no aliasing.
            level.flush_to_ram(unsafe { &mut *ram_ptr });
        }
        let active = self.active_core;
        for (core, bank) in self.banks.iter_mut().enumerate() {
            bank.icache.invalidate();
            if core != active {
                bank.dcache.flush_to_ram(&mut self.ram);
            }
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
        self.l1_amat(&self.icache)
    }

    /// AMAT for D-cache hierarchy (hierarchical formula).
    pub fn dcache_amat(&self) -> f64 {
        self.l1_amat(&self.dcache)
    }

    /// AMAT of one L1, any core's.
    pub fn l1_amat(&self, cache: &Cache) -> f64 {
        let hit_lat = cache.config.hit_latency as f64;
        let total = cache.stats.total_accesses();
        let miss_rate = if total == 0 {
            0.0
        } else {
            cache.stats.misses as f64 / total as f64
        };
        let miss_cost = (cache.config.miss_penalty + cache.config.line_transfer_cycles()) as f64;
        if self.extra_levels.is_empty() {
            hit_lat + miss_rate * miss_cost
        } else {
            // miss_cost is the L1->L2 access overhead; add L2 AMAT on top
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

    /// Direct RAM read (no cache tracking) â€” used by UI rendering.
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

    /// Contabiliza o fetch de instruÃ§Ã£o no I-cache sem retornar o dado.
    /// Usado pelo JIT: a palavra jÃ¡ foi decodificada em compile-time; sÃ³
    /// precisamos atualizar hits/misses e instruction_count para manter
    /// fidelidade de mÃ©tricas. Evita o Vec::clone() do hit path de fetch32.
    pub fn account_fetch32(&mut self, addr: u32) {
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
        // interpretador, mas descarta o dado (nÃ£o precisamos da palavra).
        let _ = self.load_icache_line(addr);
    }

    /// Word-sized D-cache read at an already-translated physical address. Used
    /// internally by `lr_w` / `amo_w` to avoid translating twice.
    pub fn dcache_read_pa32(&mut self, paddr: u32) -> Result<u32, FalconError> {
        self.dcache_read_pa32_for(paddr, false)
    }

    /// [`Self::dcache_read_pa32`] for the read half of a read-modify-write:
    /// a miss asks for ownership, so the write that follows needs no second
    /// bus request.
    fn dcache_read_pa32_for(&mut self, paddr: u32, for_write: bool) -> Result<u32, FalconError> {
        if self.bypass {
            return self.ram.load32(paddr);
        }
        if !self.dcache.config.is_valid_config() {
            return self.ram.load32(paddr);
        }
        let line_size = self.dcache.config.line_size;
        let offset = self.dcache.config.addr_offset(paddr);
        let first_line = self.load_dcache_line_for(paddr, for_write)?;
        if offset + 3 < line_size {
            return Ok(u32::from_le_bytes([
                first_line[offset],
                first_line[offset + 1],
                first_line[offset + 2],
                first_line[offset + 3],
            ]));
        }
        let split = line_size - offset;
        let second_line = self.load_dcache_line_for(paddr.wrapping_add(split as u32), for_write)?;
        let mut bytes = [0u8; 4];
        bytes[..split].copy_from_slice(&first_line[offset..]);
        bytes[split..].copy_from_slice(&second_line[..4 - split]);
        Ok(u32::from_le_bytes(bytes))
    }

    /// Word-sized D-cache store at an already-translated physical address.
    /// Counterpart to `dcache_read_pa32` for `sc_w` / `amo_w`, and where
    /// `store32` ends up once it has translated: every word store passes
    /// here, which makes it the one place that watches `tohost`.
    pub fn store_pa32(&mut self, paddr: u32, val: u32) -> Result<(), FalconError> {
        self.dcache_store_bytes(paddr, &val.to_le_bytes())?;
        self.observe_tohost_store(paddr, val);
        Ok(())
    }

    /// Effective read: returns the most-recent dirty byte from the cache hierarchy,
    /// falling back to RAM. Checks L1 D-cache first, then extra_levels in order.
    /// No stats side-effects. Use for syscalls and the Run-tab memory view.
    pub fn effective_read8(&self, addr: u32) -> Result<u8, FalconError> {
        effective_byte(
            &self.ram,
            &self.dcache,
            self.parked_banks().map(|bank| &bank.dcache),
            &self.extra_levels,
            self.bypass,
            self.coherence.protocol,
            addr,
        )
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
        (0..bytes).any(|i| {
            let a = addr.wrapping_add(i);
            self.dcache.peek_dirty(a).is_some()
                || self.parked_banks().any(|bank| bank.dcache.peek_dirty(a).is_some())
        })
    }

    /// Returns `Some((level, dirty))` if `addr`'s cache line is present in the D-cache hierarchy.
    /// `level` is 1-based (1 = L1 D-cache, 2 = L2, â€¦). No side effects.
    pub fn data_cache_location(&self, addr: u32) -> Option<(u8, bool)> {
        if self.bypass {
            return None;
        }
        if let Some(dirty) = self.dcache.has_line(addr) {
            return Some((1, dirty));
        }
        if let Some(dirty) = self.parked_banks().find_map(|bank| bank.dcache.has_line(addr)) {
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
    /// `level` is 1-based (1 = L1 I-cache, 2 = L2, â€¦). No side effects.
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

impl CacheController {
    /// The cache `core` sees serving `role` at `level`.
    fn cache_for(&self, core: usize, level: usize, role: CacheRole) -> Option<&Cache> {
        match (level, role) {
            (0, CacheRole::Instruction) => self.l1_of(core).map(|(i, _)| i),
            (0, CacheRole::Data) => self.l1_of(core).map(|(_, d)| d),
            (0, CacheRole::Unified) => None,
            (_, CacheRole::Unified) => self.extra_levels.get(level - 1),
            _ => None,
        }
    }

    fn level_view<'a>(
        &self,
        cache: &'a Cache,
        level: usize,
        role: CacheRole,
    ) -> Option<CacheLevelView<'a>> {
        let config = &cache.config;
        let replacement = match config.replacement {
            ReplacementPolicy::Lru => CacheReplacementPolicy::Lru,
            ReplacementPolicy::Mru => CacheReplacementPolicy::Mru,
            ReplacementPolicy::Fifo => CacheReplacementPolicy::Fifo,
            ReplacementPolicy::Random => CacheReplacementPolicy::Random,
            ReplacementPolicy::Lfu => CacheReplacementPolicy::Lfu,
            ReplacementPolicy::Clock => CacheReplacementPolicy::Clock,
        };
        let write_policy = match config.write_policy {
            WritePolicy::WriteThrough => CacheWritePolicy::WriteThrough,
            WritePolicy::WriteBack => CacheWritePolicy::WriteBack,
        };
        let write_allocation = match config.write_alloc {
            WriteAllocPolicy::WriteAllocate => CacheWriteAllocation::WriteAllocate,
            WriteAllocPolicy::NoWriteAllocate => CacheWriteAllocation::NoWriteAllocate,
        };
        let inclusion = match config.inclusion {
            super::InclusionPolicy::NonInclusive => CacheInclusionPolicy::NonInclusive,
            super::InclusionPolicy::Inclusive => CacheInclusionPolicy::Inclusive,
            super::InclusionPolicy::Exclusive => CacheInclusionPolicy::Exclusive,
        };
        let (first, second) = cache.stats.history.as_slices();

        Some(CacheLevelView {
            name: match role {
                CacheRole::Instruction => "I-Cache".to_string(),
                CacheRole::Data => "D-Cache".to_string(),
                CacheRole::Unified => Self::extra_level_name(level - 1),
            },
            level,
            role,
            config: CacheLevelConfig {
                size: config.size,
                line_size: config.line_size,
                associativity: config.associativity,
                num_sets: cache.sets.len(),
                address_bits: 32,
                replacement,
                write_policy,
                write_allocation,
                inclusion,
                hit_latency: config.hit_latency,
                miss_penalty: config.miss_penalty,
                associativity_penalty: config.assoc_penalty,
                transfer_width: config.transfer_width as usize,
                enabled: config.is_valid_config(),
            },
            stats: CacheLevelStats {
                hits: cache.stats.hits,
                misses: cache.stats.misses,
                evictions: cache.stats.evictions,
                writebacks: cache.stats.writebacks,
                bytes_loaded: cache.stats.bytes_loaded,
                bytes_stored: cache.stats.bytes_stored,
                total_cycles: cache.stats.total_cycles,
                ram_write_bytes: cache.stats.ram_write_bytes,
                history: CacheHistory::new(first, second),
            },
        })
    }

    fn set_view<'a>(
        &self,
        cache: &'a Cache,
        level: usize,
        role: CacheRole,
        set: usize,
    ) -> Option<CacheSetView<'a>> {
        // Only the private D-caches carry a protocol state; the I-caches
        // hold read-only copies the protocol does not track.
        let tracked =
            level == 0 && role == CacheRole::Data && self.coherence.protocol.coherent();
        let set = cache.sets.get(set)?;
        Some(CacheSetView {
            lines: set
                .lines
                .iter()
                .map(|line| CacheLineView {
                    valid: line.valid,
                    dirty: line.dirty,
                    tag: u64::from(line.tag),
                    data: &line.data,
                    frequency: line.freq,
                    referenced: line.ref_bit,
                    state: tracked.then(|| line_state(line)),
                })
                .collect(),
            lru_order: set.lru_order.iter().copied().collect(),
            fifo_order: set.fifo_order.iter().copied().collect(),
            clock_hand: set.clock_hand,
        })
    }
}

impl CacheHierarchy for CacheController {
    fn level_count(&self) -> usize {
        1 + self.extra_levels.len()
    }

    /// Fully retunable: this is the hierarchy the Cache tab exists to explore.
    fn is_configurable(&self) -> bool {
        true
    }

    /// Hierarchical: an L1 miss costs the transfer plus the next level's AMAT,
    /// not a flat penalty.
    fn amat(&self, level: usize, role: CacheRole) -> f64 {
        self.core_amat(self.active_core, level, role)
    }

    fn cache(&self, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>> {
        self.core_cache(self.active_core, level, role)
    }

    fn set(&self, level: usize, role: CacheRole, set: usize) -> Option<CacheSetView<'_>> {
        self.core_set(self.active_core, level, role, set)
    }

    fn core_count(&self) -> usize {
        if self.coherence.protocol.private_l1() {
            self.banks.len().max(self.active_core + 1)
        } else {
            1
        }
    }

    fn core_cache(&self, core: usize, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>> {
        let cache = self.cache_for(core, level, role)?;
        self.level_view(cache, level, role)
    }

    fn core_set(
        &self,
        core: usize,
        level: usize,
        role: CacheRole,
        set: usize,
    ) -> Option<CacheSetView<'_>> {
        let cache = self.cache_for(core, level, role)?;
        self.set_view(cache, level, role, set)
    }

    fn core_amat(&self, core: usize, level: usize, role: CacheRole) -> f64 {
        match (level, role) {
            (0, CacheRole::Instruction | CacheRole::Data) => self
                .cache_for(core, 0, role)
                .map_or(0.0, |cache| self.l1_amat(cache)),
            (level, _) if level >= 1 => self.extra_level_amat(level - 1),
            _ => 0.0,
        }
    }

    /// The private L1s keep cycles of their own, so the whole machine's total
    /// is the controller's, not the active core's levels.
    fn total_cycles(&self) -> u64 {
        self.total_cache_cycles()
    }

    fn coherence(&self) -> Option<CoherenceView<'_>> {
        let protocol = self.coherence.protocol;
        if !protocol.private_l1() {
            return None;
        }
        Some(CoherenceView {
            protocol: protocol.label(),
            description: protocol.description(),
            coherent: protocol.coherent(),
            states: protocol.states(),
            snoop_latency: self.coherence.snoop_latency,
            cache_to_cache_latency: self.coherence.cache_to_cache_latency,
            cores: &self.coh.cores,
            events: &self.coh.events,
        })
    }
}

/// The value a load would see at `addr`: the active core's copy, then — under
/// a coherence protocol — a dirty copy another core owns, then the shared
/// levels, then RAM. No statistic moves.
fn effective_byte<'a>(
    ram: &Ram,
    dcache: &Cache,
    parked: impl Iterator<Item = &'a Cache>,
    extra_levels: &[Cache],
    bypass: bool,
    protocol: CoherenceProtocol,
    addr: u32,
) -> Result<u8, FalconError> {
    if bypass {
        return ram.load8(addr);
    }
    if protocol.private_l1() {
        // What the active core would read: its own copy, clean or not —
        // with coherence off that may be stale, and that is the point —
        // then, under a protocol, the one dirty copy another core owns.
        if let Some(v) = dcache.peek_valid(addr) {
            return Ok(v);
        }
        if protocol.coherent() {
            for cache in parked {
                if let Some(v) = cache.peek_dirty(addr) {
                    return Ok(v);
                }
            }
        }
    } else if let Some(v) = dcache.peek_dirty(addr) {
        return Ok(v);
    }
    for level in extra_levels {
        if let Some(v) = level.peek_dirty(addr) {
            return Ok(v);
        }
    }
    ram.load8(addr)
}

/// The memory hierarchy as the page walker sees it, borrowed apart from the
/// MMU that drives the walk.
///
/// A PTE is read as a load would see it, so one the program stored a moment
/// ago counts even while it still sits dirty in a write-back D-cache. The A/D
/// bits the walker sets go to RAM *and* to every cached copy, so a dirty line
/// written back later cannot erase them.
struct Hierarchy<'a> {
    ram: &'a mut Ram,
    icache: &'a mut Cache,
    dcache: &'a mut Cache,
    extra_levels: &'a mut [Cache],
    banks: &'a mut [L1Bank],
    active_core: usize,
    bypass: bool,
    protocol: CoherenceProtocol,
}

impl PteMemory for Hierarchy<'_> {
    fn read_pte(&mut self, pa: u32) -> Option<u32> {
        let active = self.active_core;
        let mut bytes = [0u8; 4];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let parked = self
                .banks
                .iter()
                .enumerate()
                .filter(|(core, _)| *core != active)
                .map(|(_, bank)| &bank.dcache);
            *byte = effective_byte(
                self.ram,
                self.dcache,
                parked,
                self.extra_levels,
                self.bypass,
                self.protocol,
                pa.checked_add(i as u32)?,
            )
            .ok()?;
        }
        Some(u32::from_le_bytes(bytes))
    }

    fn write_pte(&mut self, pa: u32, value: u32) -> bool {
        if self.ram.store32(pa, value).is_err() {
            return false;
        }
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            let at = pa.wrapping_add(i as u32);
            self.icache.patch_byte(at, byte);
            self.dcache.patch_byte(at, byte);
            for bank in self.banks.iter_mut() {
                bank.icache.patch_byte(at, byte);
                bank.dcache.patch_byte(at, byte);
            }
            for level in self.extra_levels.iter_mut() {
                level.patch_byte(at, byte);
            }
        }
        true
    }
}

/// Reads PTEs as a load would see them and writes nothing — for a viewer
/// asking where an address would go.
struct EffectiveView<'a>(&'a CacheController);

impl PteMemory for EffectiveView<'_> {
    fn read_pte(&mut self, pa: u32) -> Option<u32> {
        self.0.effective_read32(pa).ok()
    }

    fn write_pte(&mut self, _pa: u32, _value: u32) -> bool {
        false
    }
}

impl CacheController {
    /// The MMU and, borrowed apart from it, the memory its walker reads.
    fn split_mmu(&mut self) -> (&mut Mmu, Hierarchy<'_>) {
        let hierarchy = Hierarchy {
            ram: &mut self.ram,
            icache: &mut self.icache,
            dcache: &mut self.dcache,
            extra_levels: &mut self.extra_levels,
            banks: &mut self.banks,
            active_core: self.active_core,
            bypass: self.bypass,
            protocol: self.coherence.protocol,
        };
        (&mut self.mmu, hierarchy)
    }

    fn run_mmu(&mut self, vaddr: u32, access: AccessType) -> Result<(u32, u8), PageFault> {
        if !self.mmu.translation_applies() {
            return Ok((vaddr, 0));
        }
        let before = self.mmu.trace.last().map(|rec| rec.seq);
        let result = {
            let (mmu, mut hierarchy) = self.split_mmu();
            mmu.translate(vaddr, access, &mut hierarchy)
        };
        if self.mmu.trace.last().map(|rec| rec.seq) != before {
            self.charge_walk();
        }
        result
    }

    /// A walk's PTE reads are loads like any other: they go through the
    /// D-cache and the levels below, fill lines, count as hits or misses and
    /// cost what the hierarchy charges — on top of the walker's own cycles.
    /// The walker already read each value as a load would see it; this
    /// replays the reads for what they do to the caches and the clock.
    fn charge_walk(&mut self) {
        let Some(rec) = self.mmu.trace.last() else {
            return;
        };
        let mut steps = rec.steps;
        let count = usize::from(rec.step_count);
        let mut charged = 0u64;
        for step in steps.iter_mut().take(count) {
            step.served_by = self.walker_source(step.pte_addr);
            let before = self.total_cache_cycles();
            let _ = self.dcache_read_pa32_for(step.pte_addr, false);
            let cost = self.total_cache_cycles().saturating_sub(before);
            step.cycles = u16::try_from(cost).unwrap_or(u16::MAX);
            charged += cost;
        }
        if let Some(rec) = self.mmu.trace.last_mut() {
            rec.steps = steps;
            rec.cycles = rec.cycles.saturating_add(u32::try_from(charged).unwrap_or(u32::MAX));
        }
        let stats = &mut self.mmu.tlb.stats;
        stats.total_cycles += charged;
        stats.walk_cycles += charged;
    }

    /// The level that will answer a PTE read from the running core: its own
    /// L1, a shared level below, or `None` for memory. Another core's L1 does
    /// not count; reaching it is a trip down the hierarchy.
    fn walker_source(&self, pa: u32) -> Option<u8> {
        self.walker_source_for(self.active_core, pa)
    }

    /// [`Self::walker_source`] for `core`, running or parked.
    fn walker_source_for(&self, core: usize, pa: u32) -> Option<u8> {
        if self.bypass || !self.dcache.config.is_valid_config() {
            return None;
        }
        let l1 = match self.banks.get(core) {
            Some(bank) if core != self.active_core && self.coherence.protocol.private_l1() => {
                &bank.dcache
            }
            _ => &self.dcache,
        };
        if l1.has_line(pa).is_some() {
            return Some(1);
        }
        self.extra_levels
            .iter()
            .position(|level| level.has_line(pa).is_some())
            .map(|i| i as u8 + 2)
    }

    /// What translating `vaddr` would do, without doing it. `None` while
    /// nothing is translated.
    pub fn dry_run_translation(
        &self,
        vaddr: u32,
        access: AccessType,
    ) -> Option<TranslationRecord> {
        self.dry_run_translation_on(self.active_core, vaddr, access)
    }

    /// [`Self::dry_run_translation`] as `core` would see it: its TLB, and the
    /// level of its hierarchy each PTE read would reach. A dry run charges
    /// nothing, so the steps say where, not how long.
    pub fn dry_run_translation_on(
        &self,
        core: usize,
        vaddr: u32,
        access: AccessType,
    ) -> Option<TranslationRecord> {
        let mut rec = self
            .mmu
            .dry_run_on(self.tlb_of(core), vaddr, access, &mut EffectiveView(self))?;
        let count = usize::from(rec.step_count);
        for step in rec.steps.iter_mut().take(count) {
            step.served_by = self.walker_source_for(core, step.pte_addr);
        }
        Some(rec)
    }

    /// Read a PTE as a load would see it, without side effects.
    pub fn read_pte(&self, pa: u32) -> Option<u32> {
        EffectiveView(self).read_pte(pa)
    }

    /// Write the simulator's identity page map with its tables ending at
    /// `tables_top`, point the MMU at it and start the TLB and the trace over.
    /// Returns the `satp` value, which the caller mirrors into the CPU's CSR.
    pub fn install_auto_map(
        &mut self,
        tables_top: u32,
        spec: PageMapSpec,
        window: (u32, u32),
    ) -> Result<u32, String> {
        let layout = {
            let (_, mut hierarchy) = self.split_mmu();
            Mmu::install_auto_map(&mut hierarchy, tables_top, spec, window)?
        };
        let satp = Mmu::make_satp(layout.root, spec.asid);
        self.mmu.auto_map = Some(layout);
        self.mmu.satp = Satp::new(satp);
        self.flush_all_tlbs();
        self.mmu.trace.clear();
        Ok(satp)
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

    // store* = D-cache tracked writes (bypasses L2+ â€” write-through-to-RAM for evictions)
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
        self.store_pa32(addr, val)
    }

    // I-cache tracked fetch â€” hierarchical: L1 hit â†’ return; miss â†’ L2+/RAM fill
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

    // D-cache tracked reads â€” hierarchical: L1 hit â†’ return; miss â†’ L2+/RAM fill
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

    /// Make this hart's stores visible to its own instruction fetch.
    ///
    /// The I-cache fills from L2 or RAM and never looks at the D-cache, so
    /// code a store just wrote stays invisible to fetch while its line sits
    /// dirty in the D-cache. Write every dirty line to the next level — the
    /// path an eviction takes, counted as a writeback the same way — keep it
    /// valid and clean, and drop whatever the I-cache held.
    fn fence_i(&mut self) -> Result<(), FalconError> {
        if !self.bypass && self.dcache.config.is_valid_config() {
            let offset_bits = self.dcache.config.offset_bits();
            let index_bits = self.dcache.config.index_bits();
            let mut dirty = Vec::new();
            for (set_idx, set) in self.dcache.sets.iter_mut().enumerate() {
                for line in &mut set.lines {
                    if line.valid && line.dirty {
                        let base = (line.tag << (offset_bits + index_bits))
                            | ((set_idx as u32) << offset_bits);
                        dirty.push((base, line.data.clone(), line_state(line)));
                        line.dirty = false;
                    }
                }
            }
            self.drop_clean_exclusive_if_msi();
            for (base, data, from) in dirty {
                if self.coherence.protocol.coherent() {
                    let to = self.active_state(base);
                    self.coh.log(
                        self.instruction_count,
                        self.active_core,
                        CoherenceRequest::WriteBack,
                        base,
                        vec![CoherenceTransition {
                            core: self.active_core,
                            from,
                            to,
                        }],
                        None,
                        true,
                    );
                }
                self.propagate_line_to_next_level(WritebackSource::L1D, base, &data)?;
            }
        }
        self.icache.invalidate();
        Ok(())
    }

    fn translate(&mut self, vaddr: u32, access: AccessType) -> Result<(u32, u8), PageFault> {
        self.run_mmu(vaddr, access)
    }

    fn tlb_flush(&mut self) {
        self.mmu.flush();
    }

    fn tlb_flush_vaddr(&mut self, vaddr: u32) {
        self.mmu.tlb.flush_vaddr(vaddr);
    }

    fn set_satp(&mut self, val: u32) {
        let satp = Satp::new(val);
        // A program pointing satp elsewhere has left the simulator's map.
        if self
            .mmu
            .auto_map
            .is_some_and(|map| satp.ppn() << 12 != map.root)
        {
            self.mmu.auto_map = None;
        }
        self.mmu.satp = satp;
        self.mmu.flush();
    }

    fn set_priv_mode(&mut self, mode: crate::falcon::mmu::PrivMode) {
        self.mmu.priv_mode = mode;
    }

    fn note_instruction(&mut self, hart: u32, pc: u32) {
        self.mmu.note_instruction(hart, pc);
    }

    fn take_host_request(&mut self) -> Option<HostRequest> {
        self.host_request.take()
    }

    fn lr_w(&mut self, hart_id: u32, addr: u32) -> Result<u32, FalconError> {
        // Translate the load early â€” page faults must surface before the
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
        // Translate as a Store first â€” the W permission check happens before
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
        let old = self.dcache_read_pa32_for(aligned, true)?;
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
                if self.coherence.protocol.coherent() {
                    let from = line_state(&self.dcache.sets[idx].lines[way]);
                    self.coh.log(
                        self.instruction_count,
                        self.active_core,
                        CoherenceRequest::WriteBack,
                        evict_base,
                        vec![CoherenceTransition {
                            core: self.active_core,
                            from,
                            to: CoherenceState::Invalid,
                        }],
                        None,
                        true,
                    );
                }
                self.propagate_line_to_next_level(WritebackSource::L1D, evict_base, &evicted_data)?;
            }
        }
        self.dcache.stats.bytes_loaded += line_data.len() as u64;
        self.dcache.sets[idx].install(way, tag, line_data, replacement);
        Ok(())
    }
}
