// ui/app/tlb_state.rs — UI state for the top-level Virtual Memory tab.
//
// The tab mirrors the Cache tab: one flat header with five subtabs
// (`VmSubtab`) — Overview (live satp / privilege / activation banner + quick
// mode/TLB controls), Map (the live page-table tree, read-only), Tlb (the
// installed-translations table), Stats (counters + hit-rate chart + the shared
// session-snapshot history) and Settings (the single comprehensive VM control
// panel: mode, paging scheme, page map, TLB geometry + presets). Execution
// controls and a shared controls bar (results / import / export / flush)
// frame every subtab, exactly like the Cache tab.

use crate::falcon::mmu::{PageMapSpec, PagingScheme, TlbConfig};

/// Virtual Memory subtab (single flat header row).
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum VmSubtab {
    Overview,
    Map,
    Tlb,
    Stats,
    Settings,
}

impl VmSubtab {
    pub(crate) const ALL: [VmSubtab; 5] = [
        Self::Overview,
        Self::Map,
        Self::Tlb,
        Self::Stats,
        Self::Settings,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Map => "map",
            Self::Tlb => "tlb",
            Self::Stats => "stats",
            Self::Settings => "settings",
        }
    }
}

/// An editable control in the VM Settings panel.
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
    /// A header subtab button.
    Subtab(VmSubtab),
    // Overview quick controls
    QuickMode,
    QuickTlb,
    // VM Settings panel
    VmField(VmSettingsField),
    /// TLB geometry preset button (0=small, 1=med, 2=large).
    Preset(usize),
    VmApply,
    VmFlush,
    // Shared controls bar
    ExportResults,
    ImportCfg,
    ExportCfg,
    FlushTlb,
}

pub(crate) struct TlbState {
    /// Active Virtual Memory subtab.
    pub(crate) vm_subtab: VmSubtab,
    pub(crate) hover: Option<TlbHoverTarget>,
    /// Header subtab hitboxes (y, x0, x1), in `VmSubtab::ALL` order.
    pub(crate) subtab_btns: std::cell::Cell<[(u16, u16, u16); 5]>,
    // Execution controls (mirrors CacheState's exec_* cells).
    pub(crate) exec_speed_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) exec_state_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) exec_reset_btn: std::cell::Cell<(u16, u16, u16)>,
    // Shared controls bar.
    pub(crate) ctrl_results_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) ctrl_import_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) ctrl_export_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) ctrl_flush_btn: std::cell::Cell<(u16, u16, u16)>,
    // Overview quick controls.
    pub(crate) quick_mode_btn: std::cell::Cell<(u16, u16, u16)>,
    pub(crate) quick_tlb_btn: std::cell::Cell<(u16, u16, u16)>,
    /// TLB geometry being edited in the Settings panel (applied via `apply`).
    pub(crate) pending: TlbConfig,
    /// TLB preset button hitboxes in the Settings panel.
    pub(crate) preset_btns: std::cell::Cell<[(u16, u16, u16); 3]>,
    /// Footer status line (✓ / ✗ at the bottom of the screen).
    pub(crate) config_error: Option<String>,
    pub(crate) config_status: Option<String>,
    pub(crate) entries_scroll: usize,
    /// Vertical scrollbar of the Entries table — geometry set by render,
    /// hit-tested by mouse for click-to-jump + thumb drag.
    pub(crate) entries_sb: std::cell::Cell<Option<crate::ui::view::components::SbGeom>>,
    /// `Some(grab)` while the bar's thumb is being dragged.
    pub(crate) entries_sb_drag: Option<u16>,
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
    /// Hitbox for the VM Settings `apply` button.
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
            vm_subtab: VmSubtab::Overview,
            hover: None,
            subtab_btns: std::cell::Cell::new([(0, 0, 0); 5]),
            exec_speed_btn: std::cell::Cell::new((0, 0, 0)),
            exec_state_btn: std::cell::Cell::new((0, 0, 0)),
            exec_reset_btn: std::cell::Cell::new((0, 0, 0)),
            ctrl_results_btn: std::cell::Cell::new((0, 0, 0)),
            ctrl_import_btn: std::cell::Cell::new((0, 0, 0)),
            ctrl_export_btn: std::cell::Cell::new((0, 0, 0)),
            ctrl_flush_btn: std::cell::Cell::new((0, 0, 0)),
            quick_mode_btn: std::cell::Cell::new((0, 0, 0)),
            quick_tlb_btn: std::cell::Cell::new((0, 0, 0)),
            pending: TlbConfig::default(),
            preset_btns: std::cell::Cell::new([(0, 0, 0); 3]),
            config_error: None,
            config_status: None,
            entries_scroll: 0,
            entries_sb: std::cell::Cell::new(None),
            entries_sb_drag: None,
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
