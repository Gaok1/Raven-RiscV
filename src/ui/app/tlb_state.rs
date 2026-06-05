// ui/app/tlb_state.rs — UI state for the top-level Virtual Memory tab.
//
// The tab mirrors the Cache tab: a Virtual Memory world with four subtabs
// (`VmSubtab`) — Status (live satp / privilege / activation banner), Tree (the
// live page-table tree, read-only), Settings (the comprehensive VM control
// panel: mode, paging scheme, page map, root PT and TLB geometry) and Tlb. The
// Tlb subtab opens its own nested world with three sub-subtabs (`TlbSubtab`):
// Stats (counters + hit-rate chart), Entries (installed translations) and
// Settings (geometry + latencies + presets).

use crate::falcon::mmu::{PageMapSpec, PagingScheme, TlbConfig};

/// Top-level Virtual Memory subtab (first header row).
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum VmSubtab {
    Status,
    Tree,
    Settings,
    Tlb,
}

/// Nested TLB subtab (second header row, visible only inside `VmSubtab::Tlb`).
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum TlbSubtab {
    Stats,
    Entries,
    Settings,
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

/// An editable control in the comprehensive VM Settings panel.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum VmSettingsField {
    /// VM mode selector (Off / Sv32 / Custom / Manual).
    Mode,
    /// TLB cache enabled toggle.
    TlbEnabled,
    // ── Paging scheme (Custom mode only) ──
    /// Page-offset width, in bits.
    OffsetBits,
    /// Index width of level `n` (0 = top), in bits.
    LevelBits(usize),
    /// Append a paging level.
    AddLevel,
    /// Drop the last paging level.
    RemoveLevel,
    // ── Page map ──
    /// Identity ↔ Offset.
    Kind,
    /// Signed offset payload, in MiB (only when kind = Offset).
    Offset,
    PermR,
    PermW,
    PermX,
    PermU,
    /// Sv32 Global (G) bit.
    Global,
    /// ASID encoded into satp.
    Asid,
    // ── TLB geometry ──
    TlbEntries,
    TlbAssoc,
    TlbReplacement,
    TlbHitLat,
    TlbMissLat,
}

impl VmSettingsField {
    /// Fields edited by typing a number (vs. toggled / cycled by click).
    pub(crate) fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::OffsetBits
                | Self::LevelBits(_)
                | Self::Offset
                | Self::Asid
                | Self::TlbEntries
                | Self::TlbAssoc
                | Self::TlbHitLat
                | Self::TlbMissLat
        )
    }
}

#[derive(PartialEq, Eq, Clone)]
pub(crate) enum TlbHoverTarget {
    // VM-level header (row 1)
    VmStatus,
    VmTree,
    VmSettings,
    VmTlb,
    // TLB-level header (row 2)
    TlbStats,
    TlbEntries,
    TlbSettings,
    // TLB Settings form (nested TLB world)
    ConfigField(TlbConfigField),
    Preset(usize),
    Apply,
    Flush,
    // VM Settings panel
    VmField(VmSettingsField),
    VmApply,
    VmFlush,
}

pub(crate) struct TlbState {
    /// Top-level Virtual Memory subtab.
    pub(crate) vm_subtab: VmSubtab,
    /// Nested TLB subtab (only meaningful when `vm_subtab == Tlb`).
    pub(crate) subtab: TlbSubtab,
    pub(crate) hover: Option<TlbHoverTarget>,
    // Subtab-bar origins `(row, first_col)`: the `Toolbar` in `view::tlb` maps a
    // click column back to the subtab, so only the origin needs storing.
    pub(crate) vm_header_origin: std::cell::Cell<(u16, u16)>,
    pub(crate) tlb_subheader_origin: std::cell::Cell<(u16, u16)>,
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
    /// Max valid `page_tree_scroll`, recomputed each render so input handlers
    /// can clamp without re-walking the page table.
    pub(crate) page_tree_max_scroll: std::cell::Cell<usize>,
    // ── VM Settings panel ────────────────────────────────────────────────────
    /// Paging scheme being edited (applied to the MMU in Custom mode on apply).
    pub(crate) pending_scheme: PagingScheme,
    /// The map currently installed in RAM (applied).
    pub(crate) page_map: PageMapSpec,
    /// The map being edited in the panel (committed via `apply`).
    pub(crate) pending_map: PageMapSpec,
    /// Which VM-settings field is in keyboard-edit focus, if any.
    pub(crate) vm_edit_field: Option<VmSettingsField>,
    /// Text buffer while editing a numeric VM-settings field.
    pub(crate) vm_edit_buf: String,
    /// Status / error line for the VM Settings panel.
    pub(crate) map_status: Option<String>,
    /// Per-field hitboxes for the VM Settings panel: (field, y, x0, x1).
    pub(crate) vm_field_hitboxes: std::cell::RefCell<Vec<(VmSettingsField, u16, u16, u16)>>,
    /// Hitbox for the VM Settings `apply map` button.
    pub(crate) vm_apply_btn: std::cell::Cell<(u16, u16, u16)>,
    /// Hitbox for the VM Settings `flush tlb` button.
    pub(crate) vm_flush_btn: std::cell::Cell<(u16, u16, u16)>,
    /// Scroll offset for the (potentially tall) VM Settings panel.
    pub(crate) vm_settings_scroll: usize,
    pub(crate) vm_settings_max_scroll: std::cell::Cell<usize>,
}

impl Default for TlbState {
    fn default() -> Self {
        Self {
            vm_subtab: VmSubtab::Status,
            subtab: TlbSubtab::Stats,
            hover: None,
            vm_header_origin: std::cell::Cell::new((0, 0)),
            tlb_subheader_origin: std::cell::Cell::new((0, 0)),
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
            page_tree_max_scroll: std::cell::Cell::new(0),
            pending_scheme: PagingScheme::sv32(),
            page_map: PageMapSpec::default(),
            pending_map: PageMapSpec::default(),
            vm_edit_field: None,
            vm_edit_buf: String::new(),
            map_status: None,
            vm_field_hitboxes: std::cell::RefCell::new(Vec::new()),
            vm_apply_btn: std::cell::Cell::new((0, 0, 0)),
            vm_flush_btn: std::cell::Cell::new((0, 0, 0)),
            vm_settings_scroll: 0,
            vm_settings_max_scroll: std::cell::Cell::new(0),
        }
    }
}
