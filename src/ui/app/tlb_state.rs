// ui/app/tlb_state.rs — UI state for the top-level TLB / Virtual Memory tab.
//
// The TLB tab is split into four subviews: Stats (counters + hit-rate chart),
// Config (entries, associativity, replacement, latencies), Entries (table of
// installed translations) and Status (live satp / privilege / activation
// banner — explains why the TLB looks idle when satp=Bare or priv=M).

use crate::falcon::mmu::TlbConfig;

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum TlbSubtab {
    Stats,
    Config,
    Entries,
    Status,
    /// Live tree of the Sv32 page table rooted at `satp.ppn`, read from RAM.
    PageTree,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum TlbConfigField {
    EntryCount,
    Associativity,
    Replacement,
    HitLatency,
    MissPenalty,
}

impl TlbConfigField {
    pub(crate) fn hitbox_index(self) -> usize {
        match self {
            Self::EntryCount => 0,
            Self::Associativity => 1,
            Self::Replacement => 2,
            Self::HitLatency => 3,
            Self::MissPenalty => 4,
        }
    }
    pub(crate) fn is_numeric(self) -> bool {
        !matches!(self, Self::Replacement)
    }
    pub(crate) fn all_editable() -> &'static [TlbConfigField] {
        &[
            Self::EntryCount,
            Self::Associativity,
            Self::Replacement,
            Self::HitLatency,
            Self::MissPenalty,
        ]
    }
    pub(crate) fn list_row(self) -> usize {
        match self {
            Self::EntryCount => 0,
            Self::Associativity => 1,
            Self::Replacement => 3, // skip 2 (Sets readout)
            Self::HitLatency => 4,
            Self::MissPenalty => 5,
        }
    }
    pub(crate) fn from_list_row(row: usize) -> Option<Self> {
        match row {
            0 => Some(Self::EntryCount),
            1 => Some(Self::Associativity),
            2 => None, // Sets readout
            3 => Some(Self::Replacement),
            4 => Some(Self::HitLatency),
            5 => Some(Self::MissPenalty),
            _ => None,
        }
    }
    pub(crate) fn next(self) -> Self {
        let a = Self::all_editable();
        a[(a.iter().position(|&f| f == self).unwrap_or(0) + 1) % a.len()]
    }
    pub(crate) fn prev(self) -> Self {
        let a = Self::all_editable();
        let i = a.iter().position(|&f| f == self).unwrap_or(0);
        a[i.checked_sub(1).unwrap_or(a.len() - 1)]
    }
}

#[derive(PartialEq, Eq, Clone)]
pub(crate) enum TlbHoverTarget {
    SubtabStats,
    SubtabConfig,
    SubtabEntries,
    SubtabStatus,
    SubtabPageTree,
    ConfigField(TlbConfigField),
    Preset(usize),
    Apply,
    Flush,
}

pub(crate) struct TlbState {
    pub(crate) subtab: TlbSubtab,
    pub(crate) hover: Option<TlbHoverTarget>,
    pub(crate) subtab_stats_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) subtab_config_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) subtab_entries_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) subtab_status_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) subtab_page_tree_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) pending: TlbConfig,
    pub(crate) config_hitboxes: std::cell::Cell<[(u16, u16, u16); 5]>,
    pub(crate) preset_btns: std::cell::Cell<[(u16, u16, u16); 3]>,
    pub(crate) apply_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) flush_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) edit_field: Option<TlbConfigField>,
    pub(crate) edit_buf: String,
    pub(crate) config_error: Option<String>,
    pub(crate) config_status: Option<String>,
    pub(crate) entries_scroll: usize,
    pub(crate) page_tree_scroll: usize,
}

impl Default for TlbState {
    fn default() -> Self {
        Self {
            subtab: TlbSubtab::Stats,
            hover: None,
            subtab_stats_btn: std::cell::Cell::new((0, 0, 0)),
            subtab_config_btn: std::cell::Cell::new((0, 0, 0)),
            subtab_entries_btn: std::cell::Cell::new((0, 0, 0)),
            subtab_status_btn: std::cell::Cell::new((0, 0, 0)),
            subtab_page_tree_btn: std::cell::Cell::new((0, 0, 0)),
            pending: TlbConfig::default(),
            config_hitboxes: std::cell::Cell::new([(0, 0, 0); 5]),
            preset_btns: std::cell::Cell::new([(0, 0, 0); 3]),
            apply_btn: std::cell::Cell::new((0, 0, 0)),
            flush_btn: std::cell::Cell::new((0, 0, 0)),
            edit_field: None,
            edit_buf: String::new(),
            config_error: None,
            config_status: None,
            entries_scroll: 0,
            page_tree_scroll: 0,
        }
    }
}
