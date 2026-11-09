// falcon/mmu/ — Sv32 virtual memory + unified TLB
//
// `Mmu::translate` is the single seam through which every cache-aware access
// (fetch, load, store) reaches memory. When paging is off, satp is Bare, or the
// hart is in M-mode (unless forced), it is the identity. Otherwise it probes
// the TLB; on a miss it walks the Sv32 page table, installs the entry, and
// reports page faults. Every translation leaves a record in `trace`.

mod capability;
pub mod satp;
pub mod tlb;
pub mod trace;
pub mod walker;

pub use satp::{PrivMode, Satp, SatpMode};
pub use tlb::{Installed, PtePerms, Tlb, TlbConfig, TlbEntry, TlbStats};
pub use trace::{
    FaultReason, TraceOutcome, TranslationRecord, TranslationTrace, WalkStep, TRACE_CAPACITY,
};
pub use walker::{Pte, PteMemory, check_access};

use walker::{PTE_G, PTE_R, PTE_U, PTE_V, PTE_W, PTE_X};

/// Page-offset width: 4 KiB pages.
pub const PAGE_BITS: u32 = 12;
/// Index width of each level.
pub const VPN_BITS: u32 = 10;
/// Sv32 walks two levels.
pub const LEVELS: usize = 2;
/// Low VPN bits a 4 MiB megapage covers.
pub const MEGAPAGE_BITS: u8 = 10;
/// Bytes in one page table (1024 four-byte PTEs).
pub const TABLE_BYTES: u32 = 4096;

/// What kind of access is being translated. Determines which permission bit is
/// checked and which fault code is raised on failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessType {
    Fetch,
    Load,
    Store,
}

impl AccessType {
    /// RISC-V cause code for a page fault on this access type.
    pub fn page_fault_cause(self) -> u32 {
        match self {
            AccessType::Fetch => 12,
            AccessType::Load => 13,
            AccessType::Store => 15,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFault {
    pub cause: u32,
    pub vaddr: u32,
}

/// Who builds the page table.
///
/// - `Off`: no translation at all.
/// - `Auto`: the simulator writes an Sv32 table when a program loads and
///   translates even in M-mode, so any program shows the MMU at work with no
///   setup code of its own.
/// - `Program`: real RISC-V — the program writes `satp` and its own tables,
///   and M-mode bypasses translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VmMode {
    #[default]
    Off,
    Auto,
    Program,
}

impl VmMode {
    pub const ALL: [VmMode; 3] = [VmMode::Off, VmMode::Auto, VmMode::Program];

    /// The spelling used on screen and in config files.
    pub fn as_str(self) -> &'static str {
        match self {
            VmMode::Off => "off",
            VmMode::Auto => "auto",
            VmMode::Program => "program",
        }
    }

    /// Parse a config value. The names earlier versions wrote still load:
    /// `sv32` and `custom` were the simulator-built map, `manual` the
    /// program-built one.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(VmMode::Off),
            "auto" | "sv32" | "custom" => Some(VmMode::Auto),
            "program" | "manual" => Some(VmMode::Program),
            _ => None,
        }
    }

    /// The next mode, for a selector that cycles: off → auto → program → off.
    pub fn cycle(self) -> Self {
        match self {
            VmMode::Off => VmMode::Auto,
            VmMode::Auto => VmMode::Program,
            VmMode::Program => VmMode::Off,
        }
    }

    /// The simulator owns the page table.
    pub fn is_auto(self) -> bool {
        matches!(self, VmMode::Auto)
    }

    /// `(enabled, force_translate)` engine flags for this mode.
    pub fn flags(self) -> (bool, bool) {
        match self {
            VmMode::Off => (false, false),
            VmMode::Auto => (true, true),
            VmMode::Program => (true, false),
        }
    }
}

/// What the simulator's page map grants. Every page maps to itself (VA == PA):
/// the loader put the program at its virtual addresses, so any other map would
/// leave the CPU fetching from frames nothing was loaded into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageMapSpec {
    pub perms: PtePerms,
    /// Sv32 Global (G) bit on every leaf.
    pub global: bool,
    /// ASID encoded into satp when this map is installed.
    pub asid: u16,
}

impl Default for PageMapSpec {
    fn default() -> Self {
        Self {
            perms: PtePerms {
                r: true,
                w: true,
                x: true,
                u: true,
            },
            global: false,
            asid: 0,
        }
    }
}

/// Where the simulator put its page tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapLayout {
    /// The root table, which `satp` points at.
    pub root: u32,
    /// The tables span `[tables_lo, tables_hi)`.
    pub tables_lo: u32,
    pub tables_hi: u32,
    /// Leaf tables under the root, one per 4 MiB the program window touches.
    pub leaf_tables: u32,
}

impl MapLayout {
    pub fn bytes(&self) -> u32 {
        self.tables_hi - self.tables_lo
    }
}

fn perm_bits(p: PtePerms) -> u32 {
    let mut b = 0;
    if p.r {
        b |= PTE_R;
    }
    if p.w {
        b |= PTE_W;
    }
    if p.x {
        b |= PTE_X;
    }
    if p.u {
        b |= PTE_U;
    }
    b
}

/// Bytes a page of this size covers.
pub fn page_bytes(megapage: bool) -> u64 {
    if megapage {
        1 << (PAGE_BITS + u32::from(MEGAPAGE_BITS))
    } else {
        1 << PAGE_BITS
    }
}

/// The physical address `vaddr` reaches through a leaf with `ppn`: the frame
/// base, plus the low bits of the address the page passes through untouched.
pub fn physical(ppn: u32, megapage: bool, vaddr: u32) -> u32 {
    let mask = (page_bytes(megapage) - 1) as u32;
    ((ppn << PAGE_BITS) & !mask) | (vaddr & mask)
}

#[derive(Clone)]
pub struct Mmu {
    pub tlb: Tlb,
    pub satp: Satp,
    pub priv_mode: PrivMode,
    /// When false, `translate()` is pure identity and no TLB state is touched.
    pub enabled: bool,
    /// Translate in M-mode too (auto mode), so a program needs no privilege
    /// switch to be translated.
    pub force_translate: bool,
    /// When false, the TLB is bypassed: every translation walks the page table
    /// and nothing is cached. Page faults still work.
    pub tlb_enabled: bool,
    /// The tables the simulator wrote, while its auto map is installed.
    pub auto_map: Option<MapLayout>,
    pub trace: TranslationTrace,
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new(TlbConfig::default())
    }
}

impl Mmu {
    pub fn new(cfg: TlbConfig) -> Self {
        Self {
            tlb: Tlb::new(cfg),
            satp: Satp::default(),
            priv_mode: PrivMode::M,
            enabled: false,
            force_translate: false,
            tlb_enabled: true,
            auto_map: None,
            trace: TranslationTrace::default(),
        }
    }

    /// Build an Sv32 satp value (mode=1) for `root_pa` and `asid`.
    pub fn make_satp(root_pa: u32, asid: u16) -> u32 {
        (1u32 << 31) | ((asid as u32 & 0x1FF) << 22) | (root_pa >> PAGE_BITS)
    }

    /// Write the simulator's identity map, with its tables ending at
    /// `tables_top`.
    ///
    /// Every 4 MiB of the address space the program window `[start, end)` does
    /// not touch becomes one megapage leaf in the root; each 4 MiB it does
    /// touch gets a leaf table of 4 KiB pages, so the program's own accesses
    /// show a full two-level walk. The tables sit just below `tables_top` —
    /// callers pass the bottom of the stacks, so a push can never land on a
    /// PTE.
    pub fn install_auto_map<M: PteMemory + ?Sized>(
        mem: &mut M,
        tables_top: u32,
        spec: PageMapSpec,
        window: (u32, u32),
    ) -> Result<MapLayout, String> {
        let megapage_shift = PAGE_BITS + u32::from(MEGAPAGE_BITS);
        let refined: Vec<u32> = if window.1 > window.0 {
            ((window.0 >> megapage_shift)..=((window.1 - 1) >> megapage_shift)).collect()
        } else {
            Vec::new()
        };
        let leaf_tables = refined.len() as u32;
        let bytes = TABLE_BYTES * (1 + leaf_tables);
        let top = tables_top & !(TABLE_BYTES - 1);
        let Some(tables_lo) = top.checked_sub(bytes) else {
            return Err(format!(
                "no room for {} KiB of page tables below 0x{top:08x}",
                bytes / 1024
            ));
        };
        if window.1 > window.0 && tables_lo < window.1 {
            return Err(format!(
                "the page tables (0x{tables_lo:08x}–0x{top:08x}) would overlap the program, \
                 which ends at 0x{:08x}; give the machine more memory",
                window.1
            ));
        }

        let root = top - TABLE_BYTES;
        let leaf_flags = perm_bits(spec.perms) | if spec.global { PTE_G } else { 0 } | PTE_V;
        let write = |mem: &mut M, addr: u32, value: u32| {
            if mem.write_pte(addr, value) {
                Ok(())
            } else {
                Err(format!("page table at 0x{addr:08x} lies outside RAM"))
            }
        };

        for vpn1 in 0..(1u32 << VPN_BITS) {
            let pte_addr = root + vpn1 * 4;
            let base = vpn1 << megapage_shift;
            match refined.iter().position(|&v| v == vpn1) {
                Some(n) => {
                    let table = root - TABLE_BYTES * (n as u32 + 1);
                    write(mem, pte_addr, ((table >> PAGE_BITS) << 10) | PTE_V)?;
                    for vpn0 in 0..(1u32 << VPN_BITS) {
                        let page = base | (vpn0 << PAGE_BITS);
                        write(mem, table + vpn0 * 4, ((page >> PAGE_BITS) << 10) | leaf_flags)?;
                    }
                }
                None => write(mem, pte_addr, ((base >> PAGE_BITS) << 10) | leaf_flags)?,
            }
        }

        Ok(MapLayout {
            root,
            tables_lo,
            tables_hi: top,
            leaf_tables,
        })
    }

    /// Whether loads and stores are translated right now: paging on, satp in
    /// Sv32, and either not in M-mode or forced.
    pub fn translation_applies(&self) -> bool {
        self.enabled
            && self.satp.mode() == SatpMode::Sv32
            && (self.priv_mode != PrivMode::M || self.force_translate)
    }

    /// Why addresses are not being translated, when they are not.
    pub fn idle_reason(&self) -> Option<&'static str> {
        if !self.enabled {
            Some("virtual memory is off")
        } else if self.satp.mode() == SatpMode::Bare {
            Some("satp is Bare: no page table installed")
        } else if self.priv_mode == PrivMode::M && !self.force_translate {
            Some("the hart is in M-mode, which bypasses translation")
        } else {
            None
        }
    }

    /// Name the instruction whose accesses the next translations belong to.
    pub fn note_instruction(&mut self, hart: u32, pc: u32) {
        self.trace.note_instruction(hart, pc);
    }

    /// Translate a virtual address.
    ///
    /// Returns `(paddr, extra_stall_cycles)`: `hit_latency` on a TLB hit,
    /// `miss_penalty` on a walk. `mem` is mutable because the walker sets A/D
    /// in the leaf PTE.
    pub fn translate<M: PteMemory + ?Sized>(
        &mut self,
        vaddr: u32,
        access: AccessType,
        mem: &mut M,
    ) -> Result<(u32, u8), PageFault> {
        if !self.translation_applies() {
            return Ok((vaddr, 0));
        }
        let vpn = vaddr >> PAGE_BITS;
        let asid = self.satp.asid();
        let fault = PageFault {
            cause: access.page_fault_cause(),
            vaddr,
        };
        let mut rec = self.trace.start(access, self.priv_mode, vaddr, asid);

        if self.tlb_enabled {
            rec.tlb_used = true;
            let [small, mega] = self.tlb.probe_sets(vpn);
            rec.probed_sets = [small as u16, mega as u16];
            if let Some((slot, entry)) = self.tlb.probe(vpn, asid) {
                rec.hit_slot = Some(slot as u16);
                rec.megapage = entry.megapage;
                if let Err(reason) = check_access(entry.perms, access, self.priv_mode) {
                    self.tlb.stats.page_faults += 1;
                    rec.outcome = TraceOutcome::Fault(reason);
                    self.trace.push(rec);
                    return Err(fault);
                }
                // A store through a clean entry walks again, so the PTE gets
                // its D bit — what hardware that manages A/D itself does.
                if matches!(access, AccessType::Store) && !entry.dirty {
                    rec.dirty_rewalk = true;
                } else {
                    let cycles = self.tlb.config.hit_latency;
                    let paddr = physical(entry.ppn, entry.megapage, vaddr);
                    self.tlb.stats.hits += 1;
                    self.tlb.stats.total_cycles += u64::from(cycles);
                    rec.outcome = TraceOutcome::Hit;
                    rec.paddr = Some(paddr);
                    rec.cycles = u32::from(cycles);
                    self.trace.push(rec);
                    return Ok((paddr, cycles));
                }
            }
        }

        self.tlb.stats.misses += 1;
        let walk = walker::walk(self.satp.ppn(), vaddr, access, self.priv_mode, mem, true);
        rec.steps = walk.steps;
        rec.step_count = walk.step_count;
        let leaf = match walk.result {
            Ok(leaf) => leaf,
            Err(reason) => {
                self.tlb.stats.page_faults += 1;
                self.tlb.stats.walk_faults += 1;
                rec.outcome = TraceOutcome::Fault(reason);
                self.trace.push(rec);
                return Err(fault);
            }
        };

        let cycles = self.tlb.config.miss_penalty;
        let stats = &mut self.tlb.stats;
        stats.total_cycles += u64::from(cycles);
        stats.walk_cycles += u64::from(cycles);
        if rec.dirty_rewalk {
            stats.dirty_rewalks += 1;
        }
        if leaf.first_touch {
            stats.first_touches += 1;
            stats.first_touch_bytes += page_bytes(leaf.megapage);
        }
        let paddr = physical(leaf.ppn, leaf.megapage, vaddr);
        rec.outcome = TraceOutcome::Walked;
        rec.paddr = Some(paddr);
        rec.megapage = leaf.megapage;
        rec.cycles = u32::from(cycles);
        rec.set_accessed = leaf.set_accessed;
        rec.set_dirty = leaf.set_dirty;

        if self.tlb_enabled {
            let installed = self.tlb.install(TlbEntry {
                valid: true,
                vpn: if leaf.megapage {
                    vpn & !((1 << MEGAPAGE_BITS) - 1)
                } else {
                    vpn
                },
                ppn: leaf.ppn,
                asid,
                perms: leaf.perms,
                global: leaf.global,
                dirty: leaf.dirty,
                megapage: leaf.megapage,
                age: 0,
                ref_bit: false,
            });
            rec.installed_slot = Some(installed.slot as u16);
            rec.evicted = installed.evicted;
        }
        self.trace.push(rec);
        Ok((paddr, cycles))
    }

    /// What translating `vaddr` would do now, without doing it: no TLB entry
    /// moves, no statistic counts, no A/D bit is written. For a viewer.
    pub fn dry_run<M: PteMemory + ?Sized>(
        &self,
        vaddr: u32,
        access: AccessType,
        mem: &mut M,
    ) -> Option<TranslationRecord> {
        self.dry_run_on(&self.tlb, vaddr, access, mem)
    }

    /// [`Self::dry_run`] through `tlb` — another hart's, parked while this
    /// one runs.
    pub fn dry_run_on<M: PteMemory + ?Sized>(
        &self,
        tlb: &Tlb,
        vaddr: u32,
        access: AccessType,
        mem: &mut M,
    ) -> Option<TranslationRecord> {
        if !self.translation_applies() {
            return None;
        }
        let vpn = vaddr >> PAGE_BITS;
        let asid = self.satp.asid();
        let mut trace = TranslationTrace::default();
        let mut rec = trace.start(access, self.priv_mode, vaddr, asid);
        rec.seq = 0;

        if self.tlb_enabled {
            rec.tlb_used = true;
            let [small, mega] = tlb.probe_sets(vpn);
            rec.probed_sets = [small as u16, mega as u16];
            if let Some(slot) = tlb.find(vpn, asid) {
                let entry = tlb.entries[slot];
                rec.hit_slot = Some(slot as u16);
                rec.megapage = entry.megapage;
                rec.outcome = match check_access(entry.perms, access, self.priv_mode) {
                    Err(reason) => TraceOutcome::Fault(reason),
                    Ok(()) => {
                        rec.paddr = Some(physical(entry.ppn, entry.megapage, vaddr));
                        rec.cycles = u32::from(tlb.config.hit_latency);
                        TraceOutcome::Hit
                    }
                };
                return Some(rec);
            }
        }

        let walk = walker::walk(self.satp.ppn(), vaddr, access, self.priv_mode, mem, false);
        rec.steps = walk.steps;
        rec.step_count = walk.step_count;
        rec.outcome = match walk.result {
            Ok(leaf) => {
                rec.paddr = Some(physical(leaf.ppn, leaf.megapage, vaddr));
                rec.megapage = leaf.megapage;
                rec.cycles = u32::from(tlb.config.miss_penalty);
                TraceOutcome::Walked
            }
            Err(reason) => TraceOutcome::Fault(reason),
        };
        Some(rec)
    }

    pub fn flush(&mut self) {
        self.tlb.flush();
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/falcon_mmu.rs"]
mod tests;
