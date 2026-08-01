//! Virtual-to-physical address translation, described rather than assumed.
//!
//! The Virtual Memory tab was the last pane wired straight to one ISA's MMU:
//! it read Sv32 page-table entries, Sv32 permission bits and a 32-bit VPN. None
//! of that is inherent to paging — a scheme with different level widths, a
//! different page size, or no ASID at all is still a TLB in front of a page
//! walk, and a host can draw one if the backend says what shape it is.
//!
//! Widths are reported, never assumed: [`TranslationScheme::virtual_bits`] and
//! the per-level [`TranslationLevel::index_bits`] tell a host how many hex
//! digits a VPN needs and how deep the page tree goes.

/// One level of the page-table walk, outermost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationLevel {
    /// What this level is called: `"VPN[1]"`, `"PGD"`, `"L2"`.
    pub name: String,
    /// How many virtual-address bits index this level.
    pub index_bits: u8,
    /// Bytes covered by one entry at this level — the page size a leaf here
    /// would map. A host shows superpages with this.
    pub coverage: u64,
}

/// The shape of the active paging scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationScheme {
    /// Display name of the mode: `"Sv32"`, `"bare"`, `"Sv39"`.
    pub name: String,
    /// Virtual address width in bits.
    pub virtual_bits: u8,
    /// Physical address width in bits.
    pub physical_bits: u8,
    /// Page-offset width, so page size is `1 << offset_bits`.
    pub offset_bits: u8,
    /// Walk levels, outermost first. Empty when translation is off.
    pub levels: Vec<TranslationLevel>,
}

impl TranslationScheme {
    pub fn page_size(&self) -> u64 {
        1u64 << self.offset_bits
    }

    /// Hex digits needed to print a virtual address in this scheme.
    pub fn virtual_hex_width(&self) -> usize {
        usize::from(self.virtual_bits).div_ceil(4)
    }
}

/// What a mapping permits. Backends that do not distinguish a bit leave it
/// false rather than inventing one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TranslationPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    /// Reachable from unprivileged code.
    pub user: bool,
}

/// One resident translation, as a host draws it in a TLB table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlbEntryView {
    pub valid: bool,
    /// Virtual page number — the address with the page offset shifted out.
    pub virtual_page: u64,
    pub physical_page: u64,
    /// Address-space id, when the ISA tags entries with one.
    pub address_space: Option<u32>,
    pub permissions: TranslationPermissions,
    /// Visible in every address space.
    pub global: bool,
    pub accessed: bool,
    pub dirty: bool,
    /// Low virtual-page bits ignored on lookup: 0 is a base page, higher values
    /// a superpage covering `1 << superpage_bits` base pages.
    pub superpage_bits: u8,
    /// Which set this entry lives in, for a set-associative display.
    pub set: usize,
}

impl TlbEntryView {
    /// Bytes this entry maps, given the scheme's page size.
    pub fn coverage(&self, page_size: u64) -> u64 {
        page_size << self.superpage_bits
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlbStatistics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub faults: u64,
    pub total_cycles: u64,
}

impl TlbStatistics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64 * 100.0
    }
}

/// How a virtual address resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationOutcome {
    /// Translation is off; the address is already physical.
    Identity,
    /// Served from the TLB.
    TlbHit,
    /// Resolved by walking the page tables.
    Walked,
    /// No valid mapping.
    Fault,
}

/// The result of translating one address, for an inspector that must not
/// disturb the machine — resolving here never installs a TLB entry or moves a
/// statistic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranslationResult {
    pub outcome: TranslationOutcome,
    /// `None` on a fault.
    pub physical: Option<u64>,
    pub permissions: TranslationPermissions,
    /// How many levels the walk visited, for a page-tree view.
    pub levels_walked: usize,
}

/// A backend's address translation, if it has any.
///
/// Reached through [`Machine::translation`](crate::Machine::translation).
/// Everything here is read-only and side-effect free: a host calls these while
/// drawing a frame.
pub trait AddressTranslation {
    /// The active scheme. A backend with translation currently disabled still
    /// answers, with a `bare` scheme and no levels, so a host can say "off"
    /// rather than hide the pane.
    fn scheme(&self) -> TranslationScheme;

    /// True when translation is actually being applied.
    fn enabled(&self) -> bool;

    /// Physical address of the root page table, when translation is on.
    fn root_table(&self) -> Option<u64>;

    fn tlb_stats(&self) -> TlbStatistics;

    /// Number of TLB slots, resident or not — a host draws empty ways too.
    fn tlb_len(&self) -> usize;

    fn tlb_entry(&self, index: usize) -> Option<TlbEntryView>;

    /// How the TLB is organized. `1` means fully associative is reported as one
    /// set; a direct-mapped TLB reports `tlb_len()` sets.
    fn tlb_sets(&self) -> usize {
        1
    }

    /// Resolve `address` without touching the machine — no fill, no statistic.
    fn probe(&self, address: u64) -> TranslationResult;
}
