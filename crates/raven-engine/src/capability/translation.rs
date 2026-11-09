//! Virtual-to-physical address translation, described rather than assumed.
//!
//! A scheme with different level widths, a different page size, or no ASID at
//! all is still a TLB in front of a page walk, and a host can draw one if the
//! backend says what shape it is. Widths are reported, never assumed:
//! [`TranslationScheme::virtual_bits`] and the per-level
//! [`TranslationLevel::index_bits`] tell a host how to split an address, and
//! [`TranslationScheme::entry_fields`] how to label the bits of a table entry.
//!
//! Beyond the resident state, a backend can say what its last translations
//! *did* — [`AddressTranslation::recent`] — which is what lets a host replay a
//! walk step by step instead of only showing where it ended.

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

/// A named run of bits in a page-table entry, for a host that shows one
/// decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryField {
    pub name: &'static str,
    pub high: u8,
    pub low: u8,
}

impl EntryField {
    pub fn width(&self) -> u8 {
        self.high - self.low + 1
    }

    /// This field's value in `entry`.
    pub fn extract(&self, entry: u64) -> u64 {
        (entry >> self.low) & ((1u64 << self.width()) - 1)
    }
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
    /// Width of one page-table entry, in bits.
    pub entry_bits: u8,
    /// The entry's fields, most significant first.
    pub entry_fields: Vec<EntryField>,
}

impl TranslationScheme {
    pub fn page_size(&self) -> u64 {
        1u64 << self.offset_bits
    }

    /// Hex digits needed to print a virtual address in this scheme.
    pub fn virtual_hex_width(&self) -> usize {
        usize::from(self.virtual_bits).div_ceil(4)
    }

    /// Hex digits needed to print a physical address.
    pub fn physical_hex_width(&self) -> usize {
        usize::from(self.physical_bits).div_ceil(4)
    }

    /// The index `address` selects at `level` (outermost is 0).
    pub fn index_at(&self, address: u64, level: usize) -> u64 {
        let below: u32 = self.levels[level + 1..]
            .iter()
            .map(|l| u32::from(l.index_bits))
            .sum();
        let shift = u32::from(self.offset_bits) + below;
        (address >> shift) & ((1u64 << self.levels[level].index_bits) - 1)
    }

    /// The bits of `address` below the page number.
    pub fn offset_of(&self, address: u64) -> u64 {
        address & (self.page_size() - 1)
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
    /// Its way within that set.
    pub way: usize,
    /// 1 for the most recently used valid entry of its set, 2 for the next…
    /// `None` when the slot is empty.
    pub recency: Option<usize>,
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
    /// Cycles all translations cost.
    pub total_cycles: u64,
    /// The part of `total_cycles` spent walking page tables.
    pub walk_cycles: u64,
    /// Walks that reached a page for the first time since it was mapped.
    pub first_touches: u64,
    /// Bytes of those pages — how much memory the program has touched.
    pub first_touch_bytes: u64,
    /// Walks run only to record a first write in the page's dirty bit.
    pub dirty_rewalks: u64,
    /// Walks that found no valid mapping.
    pub walk_faults: u64,
}

impl TlbStatistics {
    /// Misses for a page the TLB had held before and lost — to eviction, a
    /// flush, or a context switch. What a bigger or better-organized TLB saves.
    pub fn returning_misses(&self) -> u64 {
        self.misses
            .saturating_sub(self.first_touches + self.dirty_rewalks + self.walk_faults)
    }

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

/// Which kind of access asked for a translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationAccess {
    Fetch,
    Load,
    Store,
}

impl TranslationAccess {
    pub fn label(self) -> &'static str {
        match self {
            TranslationAccess::Fetch => "fetch",
            TranslationAccess::Load => "load",
            TranslationAccess::Store => "store",
        }
    }
}

/// What a page-table entry says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEntryKind {
    /// Not valid: nothing mapped.
    Invalid,
    /// Valid, but an encoding the ISA reserves.
    Reserved,
    /// Points at the next level's table.
    Pointer { table: u64 },
    /// Maps a page or superpage.
    Leaf {
        /// Physical address of the frame.
        frame: u64,
        permissions: TranslationPermissions,
        global: bool,
        accessed: bool,
        dirty: bool,
    },
}

/// One entry of a page table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableEntryView {
    /// Its index within the table.
    pub index: u64,
    /// Where the entry itself lives.
    pub address: u64,
    pub raw: u64,
    pub kind: TableEntryKind,
}

/// One entry a walk read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalkStepView {
    /// Index into [`TranslationScheme::levels`] — 0 is the root.
    pub level: usize,
    pub entry: TableEntryView,
    /// The cache level that answered the read (1 is L1), or `None` when
    /// memory did.
    pub served_by: Option<u8>,
    /// What the read cost through the caches. `None` for a probe, which
    /// charges nothing.
    pub cycles: Option<u32>,
}

/// Everything one translation did, for a host that replays it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationRecordView {
    /// Running count of translations; 0 for a probe that never ran.
    pub seq: u64,
    pub hart: u32,
    /// The instruction that asked.
    pub pc: u64,
    pub access: TranslationAccess,
    /// Privilege level, as the ISA spells it.
    pub privilege: &'static str,
    pub virtual_address: u64,
    pub address_space: Option<u32>,
    /// TLB sets the lookup probed, each with the page size it was probing for.
    /// Empty when the TLB is off.
    pub probed_sets: Vec<(usize, u64)>,
    /// The TLB slot that matched.
    pub hit_entry: Option<usize>,
    /// The entry matched but the walk ran anyway, to set the dirty bit.
    pub rewalk_for_dirty: bool,
    pub steps: Vec<WalkStepView>,
    pub outcome: TranslationOutcome,
    /// Why it faulted, in words.
    pub fault: Option<&'static str>,
    /// The exception code the fault raises.
    pub fault_code: Option<u32>,
    pub physical: Option<u64>,
    /// Size of the page that mapped it.
    pub page_size: u64,
    pub cycles: u32,
    /// The slot the walk's result went into.
    pub installed_entry: Option<usize>,
    /// What that slot held before.
    pub evicted: Option<TlbEntryView>,
    pub set_accessed: bool,
    pub set_dirty: bool,
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

    /// Why translation is not being applied, when it is not — in the ISA's own
    /// words, so a host can say it without knowing the ISA.
    fn idle_reason(&self) -> Option<String> {
        None
    }

    /// Physical address of the root page table, when translation is on.
    fn root_table(&self) -> Option<u64>;

    fn tlb_stats(&self) -> TlbStatistics;

    /// Number of TLB slots, resident or not — a host draws empty ways too.
    fn tlb_len(&self) -> usize;

    fn tlb_entry(&self, index: usize) -> Option<TlbEntryView>;

    /// How the TLB is organized. A fully associative TLB is one set; a
    /// direct-mapped one reports `tlb_len()` sets.
    fn tlb_sets(&self) -> usize {
        1
    }

    /// The replacement policy, for a caption.
    fn tlb_policy(&self) -> String {
        String::new()
    }

    /// Whether the TLB is in use. When it is not, every translation walks.
    fn tlb_enabled(&self) -> bool {
        true
    }

    /// Resolve a load from `address` without touching the machine — no fill,
    /// no statistic, no accessed bit. A TLB miss walks the tables read-only.
    fn probe(&self, address: u64) -> TranslationRecordView;

    /// The most recent translations, oldest first.
    fn recent(&self) -> Vec<TranslationRecordView> {
        Vec::new()
    }

    /// The entries of the table at `address`, which sits at walk `level`
    /// (0 is the root). Empty when there is nothing to read there.
    fn read_table(&self, _address: u64, _level: usize) -> Vec<TableEntryView> {
        Vec::new()
    }

    /// Rolling `(step, hit-rate percent)` samples for a history chart, oldest
    /// first. Empty when the backend keeps no history — a host then draws the
    /// current rate without a trend line.
    fn hit_rate_history(&self) -> Vec<(f64, f64)> {
        Vec::new()
    }

    /// How many cores keep a TLB of their own. The plain methods answer for
    /// the core running now; the `core_` ones for any of them.
    fn tlb_count(&self) -> usize {
        1
    }

    fn core_tlb_stats(&self, _core: usize) -> TlbStatistics {
        self.tlb_stats()
    }

    fn core_tlb_entry(&self, _core: usize, index: usize) -> Option<TlbEntryView> {
        self.tlb_entry(index)
    }

    fn core_hit_rate_history(&self, _core: usize) -> Vec<(f64, f64)> {
        self.hit_rate_history()
    }

    /// [`Self::probe`] through `core`'s TLB.
    fn core_probe(&self, _core: usize, address: u64) -> TranslationRecordView {
        self.probe(address)
    }
}
