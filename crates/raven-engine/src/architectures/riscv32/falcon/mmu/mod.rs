// falcon/mmu/ â€” Sv32 virtual memory + unified TLB
//
// `Mmu::translate` is the single seam through which every cache-aware access
// (fetch, load, store) reaches RAM. When VM is disabled, or satp=Bare, or the
// hart is in M-mode, it short-circuits to identity. Otherwise it probes the
// TLB; on a miss it walks the Sv32 page table, installs the resulting entry,
// and surfaces page faults to the caller.

mod capability;
pub mod satp;
pub mod tlb;
pub mod walker;

pub use satp::{PrivMode, Satp, SatpMode};
pub use tlb::{PtePerms, Tlb, TlbConfig, TlbEntry, TlbStats};

use crate::falcon::memory::{Bus, Ram};

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

/// How virtual memory behaves, as chosen by the user.
///
/// The engine only knows two booleans (`enabled`, `force_translate`) plus the
/// active [`PagingScheme`]; this enum is the human-facing selector:
///   - `Off`    â†’ `enabled=false` (pure identity, TLB untouched)
///   - `Sv32`   â†’ didactic: auto-installed standard Sv32 (10+10+12) map,
///                `force_translate=true` so M-mode also translates and any
///                program shows TLB activity with no setup code.
///   - `Custom` â†’ didactic, but the auto-installed map uses a user-configured
///                paging scheme (number of levels / per-level index widths /
///                page-offset width). "Anything is possible" mode.
///   - `Manual` â†’ real RISC-V Sv32: M-mode bypasses; the program drives `satp`
///                + its own page tables (required for demand paging).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VmMode {
    #[default]
    Off,
    Sv32,
    Custom,
    Manual,
}

impl VmMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VmMode::Off => "OFF",
            VmMode::Sv32 => "SV32",
            VmMode::Custom => "CUSTOM",
            VmMode::Manual => "MANUAL",
        }
    }

    /// Cycle for the Settings selector: Off â†’ Sv32 â†’ Custom â†’ Manual â†’ Off.
    pub fn cycle(self) -> Self {
        match self {
            VmMode::Off => VmMode::Sv32,
            VmMode::Sv32 => VmMode::Custom,
            VmMode::Custom => VmMode::Manual,
            VmMode::Manual => VmMode::Off,
        }
    }

    /// True for the didactic auto-installed modes (Sv32 / Custom), where the
    /// simulator owns the page tables and `force_translate` is on.
    pub fn is_auto(self) -> bool {
        matches!(self, VmMode::Sv32 | VmMode::Custom)
    }

    /// Reconstruct an approximate mode from the legacy run-state booleans
    /// (`enabled`, `manual`). Old "auto" maps to `Sv32`; `Custom` is not
    /// representable this way (callers that need it store [`VmMode`] directly).
    pub fn from_user(enabled: bool, manual: bool) -> Self {
        match (enabled, manual) {
            (false, _) => VmMode::Off,
            (true, false) => VmMode::Sv32,
            (true, true) => VmMode::Manual,
        }
    }

    /// `(enabled, force_translate)` engine flags for this mode.
    pub fn flags(self) -> (bool, bool) {
        match self {
            VmMode::Off => (false, false),
            VmMode::Sv32 => (true, true),
            VmMode::Custom => (true, true),
            VmMode::Manual => (true, false),
        }
    }
}

/// A parametric multi-level paging scheme over a 32-bit virtual address.
///
/// The VA splits into per-level table indices (from the top level down to the
/// leaf) plus a final page offset, where
/// `offset_bits + Î£ level_bits == 32`. Sv32 is the `offset_bits = 12,
/// level_bits = [10, 10]` preset. A leaf PTE may occur at any level, producing
/// a superpage covering `offset_bits + Î£ (index bits below it)` bits.
///
/// Page tables themselves are always 4 KiB-frame-granular (PTE PPN = `paddr >>
/// 12`, like RISC-V); only the *virtual* page size scales with `offset_bits`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagingScheme {
    pub offset_bits: u8,
    /// Index width per level, ordered from the top (walked first) to the leaf.
    pub level_bits: Vec<u8>,
}

impl Default for PagingScheme {
    fn default() -> Self {
        Self::sv32()
    }
}

impl PagingScheme {
    /// Standard Sv32: 10 + 10 + 12.
    pub fn sv32() -> Self {
        Self {
            offset_bits: 12,
            level_bits: vec![10, 10],
        }
    }

    pub fn num_levels(&self) -> usize {
        self.level_bits.len()
    }

    /// `offset_bits + Î£ level_bits` â€” must equal 32 for a valid scheme.
    pub fn total_bits(&self) -> u32 {
        self.offset_bits as u32 + self.level_bits.iter().map(|b| *b as u32).sum::<u32>()
    }

    /// A scheme is valid when it tiles exactly 32 bits, every index width is
    /// 1..=12 (table â‰¤ 16 KiB), the page offset is 12..=30 (page â‰¥ 4 KiB), and
    /// it has 1..=4 levels.
    pub fn is_valid(&self) -> bool {
        !self.level_bits.is_empty()
            && self.level_bits.len() <= 4
            && (12..=30).contains(&self.offset_bits)
            && self.level_bits.iter().all(|&b| (1..=12).contains(&b))
            && self.total_bits() == 32
    }

    /// Bit position of `level`'s index (its low bit) = page_bits of a leaf
    /// found at that level = `offset_bits + Î£ level_bits below it`.
    pub fn shift_at(&self, level: usize) -> u32 {
        let below: u32 = self.level_bits[level + 1..].iter().map(|b| *b as u32).sum();
        self.offset_bits as u32 + below
    }

    /// Page size (bytes) of a leaf at `level`.
    pub fn page_bytes_at(&self, level: usize) -> u64 {
        1u64 << self.shift_at(level)
    }

    /// Number of entries in a `level`'s table.
    pub fn entries_at(&self, level: usize) -> u32 {
        1u32 << self.level_bits[level]
    }

    /// Distinct page-size classes (`mask_bits = page_bits - offset_bits`) this
    /// scheme can install â€” one per level a leaf may sit at â€” always sorted and
    /// including 0 (the leaf level). Drives the TLB probe. Sv32 â†’ `[0, 10]`.
    pub fn leaf_masks(&self) -> Vec<u8> {
        let mut v: Vec<u8> = (0..self.num_levels())
            .map(|l| (self.shift_at(l) - self.offset_bits as u32) as u8)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Physical base of the root table for a given RAM size: placed just below
    /// the top of memory, 4 KiB-aligned, leaving room for the top table.
    pub fn root_pa(&self, mem_size: u32) -> u32 {
        let top_bytes = self.entries_at(0) * 4;
        let aligned = (top_bytes + 0xFFF) & !0xFFF;
        (mem_size.saturating_sub(aligned)) & !0xFFF
    }
}

/// Translation shape for the auto-installed page map (Tree-tab config form).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapKind {
    /// `PA = VA`.
    Identity,
    /// `PA = VA + delta`, where the payload is a signed offset in **MiB**.
    Offset(i32),
}

impl MapKind {
    /// Byte delta added to each virtual address (wraps into `u32`).
    pub fn delta_bytes(self) -> u32 {
        match self {
            MapKind::Identity => 0,
            MapKind::Offset(mib) => (mib as i64 * 0x10_0000) as u32,
        }
    }
}

/// Page granularity for the auto-installed map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapGran {
    /// 4 MiB superpages (single-level: 1024 L1 leaves).
    Mega4M,
    /// 4 KiB pages (two-level walk) over the program window.
    Kilo4K,
}

/// Content of the page map installed by [`Mmu::install_map`] /
/// [`Mmu::install_map_scheme`]: how each page maps and with which permissions.
/// The *structure* (levels / page size) comes from the [`PagingScheme`]; the
/// legacy `gran` field is ignored by the scheme-driven installer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageMapSpec {
    pub kind: MapKind,
    /// Legacy granularity selector (kept for the old `install_map` entry point).
    pub gran: MapGran,
    pub perms: PtePerms,
    /// Sv32 Global (G) bit on every leaf.
    pub global: bool,
    /// ASID encoded into satp when this map is installed.
    pub asid: u16,
}

impl Default for PageMapSpec {
    fn default() -> Self {
        Self {
            kind: MapKind::Identity,
            gran: MapGran::Mega4M,
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

/// Sv32 R/W/X/U permission bits for a PTE (no V).
fn perm_bits(p: PtePerms) -> u32 {
    let mut b = 0;
    if p.r {
        b |= 0x2;
    }
    if p.w {
        b |= 0x4;
    }
    if p.x {
        b |= 0x8;
    }
    if p.u {
        b |= 0x10;
    }
    b
}

#[derive(Clone)]
pub struct Mmu {
    pub tlb: Tlb,
    pub satp: Satp,
    pub priv_mode: PrivMode,
    /// Mirror of `vm_enabled`. When false, `translate()` is pure identity and
    /// no TLB state is touched (zero overhead path).
    pub enabled: bool,
    /// When true, translation is applied even in M-mode. Used by the
    /// didactic standard mode so any program sees TLB activity without
    /// needing explicit page-table setup code.
    pub force_translate: bool,
    /// When false, the TLB cache is bypassed: every translation walks the page
    /// table (counts as a miss, costs `miss_penalty`) and no entry is installed
    /// or probed. Page faults still work. Lets the user see the cost of a TLB
    /// that never hits.
    pub tlb_enabled: bool,
    /// Active paging scheme (Sv32 preset by default; user-configurable in the
    /// Custom VM mode). Drives `translate`, the walker and `build_paddr`.
    pub scheme: PagingScheme,
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
            scheme: PagingScheme::sv32(),
        }
    }

    /// Adopt a paging scheme and propagate its page-offset width and page-size
    /// classes into the TLB (so probes/flushes index consistently).
    pub fn set_scheme(&mut self, scheme: PagingScheme) {
        self.tlb.set_scheme(scheme.offset_bits, scheme.leaf_masks());
        self.scheme = scheme;
    }

    /// Build an Sv32 satp value (mode=1) for `root_pa` and `asid`. Page tables
    /// are 4 KiB-frame-granular, so the PPN is always `root_pa >> 12`.
    pub fn make_satp(root_pa: u32, asid: u16) -> u32 {
        (1u32 << 31) | ((asid as u32 & 0x1FF) << 22) | (root_pa >> 12)
    }

    /// Write a full-coverage Sv32 megapage identity map at `root_pa` (VA == PA,
    /// RWX+U). Thin wrapper over [`Mmu::install_map`] kept for the CLI / back-compat.
    pub fn install_identity_megapages(ram: &mut Ram, root_pa: u32) {
        Self::install_map(ram, root_pa, PageMapSpec::default(), (0, 0));
    }

    /// Legacy Sv32 entry point (granularity from `spec.gran`). `Mega4M` fills the
    /// whole space with 4 MiB superpage leaves; `Kilo4K` refines the `window`
    /// region into real 4 KiB pages. Delegates to [`Mmu::install_map_scheme`] with
    /// the Sv32 preset.
    pub fn install_map(ram: &mut Ram, root_pa: u32, spec: PageMapSpec, window: (u32, u32)) {
        let w = if matches!(spec.gran, MapGran::Kilo4K) {
            window
        } else {
            (0, 0)
        };
        Self::install_map_scheme(ram, root_pa, &PagingScheme::sv32(), spec, w);
    }

    /// Build a page table at `root_pa` from `scheme` + `spec`.
    ///
    /// Every entry of every table is written: an entry whose virtual range
    /// intersects `window` (a `[start, end)` VA range â€” typically the loaded
    /// program) becomes a pointer to a freshly-allocated child table and is
    /// refined recursively down to the leaf level; every other entry becomes a
    /// **superpage leaf** at its own level (so the rest of memory â€” stack,
    /// etc. â€” still translates). With an empty window the top table is all
    /// superpage leaves (the megapage map). `PA = VA` (Identity) or `PA = VA +
    /// delta` (Offset); the Global bit and permissions come from `spec`. Child
    /// tables are placed 4 KiB-aligned, growing downward from `root_pa`.
    pub fn install_map_scheme(
        ram: &mut Ram,
        root_pa: u32,
        scheme: &PagingScheme,
        spec: PageMapSpec,
        window: (u32, u32),
    ) {
        let g = if spec.global { 0x20 } else { 0 };
        let leaf_flags = perm_bits(spec.perms) | g | 0x1; // perms | [G] | V
        let delta = spec.kind.delta_bytes();
        let (va_lo, va_hi) = if window.1 > window.0 {
            (window.0, window.1)
        } else {
            (0, 0) // empty window â†’ all superpage leaves
        };
        // Child tables grow downward from just below the top table.
        let mut next_free = root_pa;
        fill_table(
            ram,
            root_pa,
            0,
            scheme,
            va_lo,
            va_hi,
            delta,
            leaf_flags,
            &mut next_free,
        );
    }

    /// Translate a virtual address.
    ///
    /// Returns `(paddr, extra_stall_cycles)`. The stall is `hit_latency` on a
    /// TLB hit and `miss_penalty` on a walk. RAM is mutable because the walker
    /// auto-sets A/D on the leaf PTE.
    pub fn translate(
        &mut self,
        vaddr: u32,
        access: AccessType,
        ram: &mut Ram,
    ) -> Result<(u32, u8), PageFault> {
        // Short-circuit: VM off or satp=Bare â†’ identity (no TLB touch).
        if !self.enabled || self.satp.mode() == SatpMode::Bare {
            return Ok((vaddr, 0));
        }
        // M-mode bypasses the MMU on real hardware. In the didactic standard
        // mode (force_translate), we skip this bypass so TLB activity is
        // visible for any program without privilege-level boilerplate.
        if self.priv_mode == PrivMode::M && !self.force_translate {
            return Ok((vaddr, 0));
        }

        let offset_bits = self.scheme.offset_bits as u32;
        let vpn = vaddr >> offset_bits;
        let asid = self.satp.asid();

        // TLB probe. A Store on a non-dirty entry is forced through the walker
        // so the PTE gets its D bit set in RAM â€” mirrors real hardware.
        // Skipped entirely when the TLB is disabled: every access then walks.
        if self.tlb_enabled {
            if let Some(entry) = self.tlb.probe(vpn, asid) {
                let needs_d_writeback = matches!(access, AccessType::Store) && !entry.dirty;
                if !needs_d_writeback && self.check_perms(&entry, access).is_ok() {
                    self.tlb.stats.hits += 1;
                    let paddr = build_paddr(&entry, vaddr, offset_bits);
                    return Ok((paddr, self.tlb.config.hit_latency));
                }
                // Otherwise fall through and re-walk â€” either to fault on perms
                // or to set the D bit. The walker will reinstall the entry.
                if self.check_perms(&entry, access).is_err() && !needs_d_writeback {
                    self.tlb.stats.page_faults += 1;
                    return Err(PageFault {
                        cause: access.page_fault_cause(),
                        vaddr,
                    });
                }
            }
        }

        self.tlb.stats.misses += 1;
        let res = match walker::walk(
            &self.scheme,
            self.satp.ppn(),
            vaddr,
            ram,
            access,
            self.priv_mode,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.tlb.stats.page_faults += 1;
                return Err(e);
            }
        };

        let mask_bits = (res.page_bits as u32).saturating_sub(offset_bits) as u8;
        let entry = TlbEntry {
            valid: true,
            vpn: vpn & !((1u32 << mask_bits) - 1),
            ppn: res.ppn,
            asid,
            perms: res.perms,
            global: res.global,
            accessed: true,
            dirty: matches!(access, AccessType::Store),
            mask_bits,
            age: 0,
            ref_bit: false,
        };
        // With the TLB disabled we never cache the translation â€” the next
        // access to the same page walks again (miss, no hit).
        if self.tlb_enabled {
            self.tlb.install(entry);
        }

        let paddr = build_paddr(&entry, vaddr, offset_bits);
        Ok((paddr, self.tlb.config.miss_penalty))
    }

    fn check_perms(&self, entry: &TlbEntry, access: AccessType) -> Result<(), ()> {
        match access {
            AccessType::Fetch if !entry.perms.x => return Err(()),
            AccessType::Load if !entry.perms.r => return Err(()),
            AccessType::Store if !entry.perms.w => return Err(()),
            _ => {}
        }
        match self.priv_mode {
            PrivMode::U if !entry.perms.u => Err(()),
            PrivMode::S if entry.perms.u => Err(()), // ignore mstatus.SUM in Phase 2
            _ => Ok(()),
        }
    }

    pub fn flush(&mut self) {
        self.tlb.flush();
    }
}

fn build_paddr(entry: &TlbEntry, vaddr: u32, offset_bits: u32) -> u32 {
    // Page size = offset + the masked (superpage) bits. The frame base
    // (`ppn << 12`) is page-aligned; take the low `page_bits` from the vaddr.
    let page_bits = offset_bits + entry.mask_bits as u32;
    let page_mask: u32 = if page_bits >= 32 {
        u32::MAX
    } else {
        (1u32 << page_bits) - 1
    };
    ((entry.ppn << 12) & !page_mask) | (vaddr & page_mask)
}

/// Bump-allocate a 4 KiB-aligned page-table of `bytes`, growing downward.
fn alloc_table(next_free: &mut u32, bytes: u32) -> u32 {
    let sz = (bytes + 0xFFF) & !0xFFF;
    *next_free = next_free.saturating_sub(sz);
    *next_free
}

/// Recursively fill the table at `table_pa` for `level` (see
/// [`Mmu::install_map_scheme`]).
#[allow(clippy::too_many_arguments)]
fn fill_table(
    ram: &mut Ram,
    table_pa: u32,
    level: usize,
    scheme: &PagingScheme,
    va_lo: u32,
    va_hi: u32,
    delta: u32,
    leaf_flags: u32,
    next_free: &mut u32,
) {
    let n = scheme.num_levels();
    let shift = scheme.shift_at(level); // page_bits of a leaf at this level
    let entries = scheme.entries_at(level);
    let is_last = level + 1 == n;
    let entry_span: u64 = 1u64 << shift;
    let has_window = va_hi > va_lo;

    for idx in 0u32..entries {
        let va_base: u64 = (idx as u64) << shift;
        let in_window = has_window && va_base + entry_span > va_lo as u64 && va_base < va_hi as u64;
        let pte_addr = table_pa.wrapping_add(idx * 4);

        if in_window && !is_last {
            // Pointer PTE â†’ allocate and recurse into a child table.
            let child_bytes = scheme.entries_at(level + 1) * 4;
            let child_pa = alloc_table(next_free, child_bytes);
            for j in 0..scheme.entries_at(level + 1) {
                let _ = ram.store32(child_pa.wrapping_add(j * 4), 0);
            }
            let child_ppn = child_pa >> 12;
            let _ = ram.store32(pte_addr, (child_ppn << 10) | 0x1);
            fill_table(
                ram,
                child_pa,
                level + 1,
                scheme,
                va_lo,
                va_hi,
                delta,
                leaf_flags,
                next_free,
            );
        } else {
            // Superpage (or base-page) leaf covering this entry's range.
            let page_mask = (entry_span as u32).wrapping_sub(1);
            let pa_base = (va_base as u32).wrapping_add(delta) & !page_mask;
            let ppn = pa_base >> 12;
            let _ = ram.store32(pte_addr, (ppn << 10) | leaf_flags);
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/falcon_mmu.rs"]
mod tests;
