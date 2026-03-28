use crate::falcon::cache::CacheConfig;
use super::CpiConfig;

// ── Cache tab state ─────────────────────────────────────────────────────────

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum CacheSubtab {
    Stats,
    Config,
    View,
}

/// Editable field in the Config subtab.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum ConfigField {
    Size,
    LineSize,
    Associativity,
    Replacement,
    WritePolicy,
    WriteAlloc,
    HitLatency,
    MissPenalty,
    AssocPenalty,
    TransferWidth,
    Inclusion,
}

impl ConfigField {
    pub(crate) fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Size
                | Self::LineSize
                | Self::Associativity
                | Self::HitLatency
                | Self::MissPenalty
                | Self::AssocPenalty
                | Self::TransferWidth
        )
    }
    pub(crate) fn all_editable() -> &'static [ConfigField] {
        &[
            Self::Size,
            Self::LineSize,
            Self::Associativity,
            Self::Replacement,
            Self::WritePolicy,
            Self::WriteAlloc,
            Self::HitLatency,
            Self::MissPenalty,
            Self::AssocPenalty,
            Self::TransferWidth,
            Self::Inclusion,
        ]
    }
    /// Row index in the rendered fields list (3 = Sets which is read-only, skip it)
    pub(crate) fn list_row(self) -> usize {
        match self {
            Self::Size => 0,
            Self::LineSize => 1,
            Self::Associativity => 2,
            Self::Replacement => 4,
            Self::WritePolicy => 5,
            Self::WriteAlloc => 6,
            Self::HitLatency => 7,
            Self::MissPenalty => 8,
            Self::AssocPenalty => 9,
            Self::TransferWidth => 10,
            Self::Inclusion => 11,
        }
    }
    pub(crate) fn from_list_row(row: usize) -> Option<Self> {
        match row {
            0 => Some(Self::Size),
            1 => Some(Self::LineSize),
            2 => Some(Self::Associativity),
            3 => None, // Sets is read-only
            4 => Some(Self::Replacement),
            5 => Some(Self::WritePolicy),
            6 => Some(Self::WriteAlloc),
            7 => Some(Self::HitLatency),
            8 => Some(Self::MissPenalty),
            9 => Some(Self::AssocPenalty),
            10 => Some(Self::TransferWidth),
            11 => Some(Self::Inclusion),
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

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum CacheScope {
    ICache,
    DCache,
    Both,
}

#[derive(PartialEq, Eq, Copy, Clone, Default)]
pub(crate) enum CacheDataFmt {
    #[default]
    Hex,
    DecU,
    DecS,
    Float,
}
impl CacheDataFmt {
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::Hex => Self::DecU,
            Self::DecU => Self::DecS,
            Self::DecS => Self::Float,
            Self::Float => Self::Hex,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::DecU => "DEC-U",
            Self::DecS => "DEC-S",
            Self::Float => "FLOAT",
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Default)]
pub(crate) enum CacheDataGroup {
    #[default]
    B1,
    B2,
    B4,
}
impl CacheDataGroup {
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::B1 => Self::B2,
            Self::B2 => Self::B4,
            Self::B4 => Self::B1,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::B1 => "1B",
            Self::B2 => "2B",
            Self::B4 => "4B",
        }
    }
    pub(crate) fn bytes(self) -> usize {
        match self {
            Self::B1 => 1,
            Self::B2 => 2,
            Self::B4 => 4,
        }
    }
}

pub(crate) struct CacheState {
    pub(crate) subtab: CacheSubtab,
    pub(crate) scope: CacheScope,
    pub(crate) stats_scroll: usize,
    // Level selector
    pub(crate) selected_level: usize,  // 0 = L1, 1 = L2, …
    pub(crate) hover_level: Vec<bool>, // one per level (L1 + extra)
    pub(crate) hover_add_level: bool,
    pub(crate) hover_remove_level: bool,
    // Hover flags
    pub(crate) hover_subtab_stats: bool,
    pub(crate) hover_subtab_config: bool,
    pub(crate) hover_subtab_view: bool,
    pub(crate) view_scroll: usize,
    pub(crate) view_h_scroll: usize, // I-cache (or unified/L2+) horizontal scroll
    pub(crate) view_h_scroll_d: usize, // D-cache horizontal scroll (separate from I-cache)
    pub(crate) data_fmt: CacheDataFmt,
    pub(crate) data_group: CacheDataGroup,
    pub(crate) show_tag: bool,
    // View legend button positions (set by render each frame, read by mouse)
    pub(crate) view_fmt_btn: std::cell::Cell<(u16, u16, u16)>, // (y, x_start, x_end)
    pub(crate) view_group_btn: std::cell::Cell<(u16, u16, u16)>, // (y, x_start, x_end)
    pub(crate) view_tag_btn: std::cell::Cell<(u16, u16, u16)>, // (y, x_start, x_end)
    pub(crate) hover_view_fmt: bool,
    pub(crate) hover_view_group: bool,
    pub(crate) hover_view_tag: bool,
    pub(crate) hover_scope_i: bool,
    pub(crate) hover_scope_d: bool,
    pub(crate) hover_scope_both: bool,
    pub(crate) hover_apply: bool,
    pub(crate) hover_apply_keep: bool,
    pub(crate) hover_preset_i: Option<usize>,
    pub(crate) hover_preset_d: Option<usize>,
    pub(crate) hover_config_field: Option<(bool, ConfigField)>,
    // Config form (pending values before Apply)
    pub(crate) pending_icache: CacheConfig,
    pub(crate) pending_dcache: CacheConfig,
    pub(crate) extra_pending: Vec<CacheConfig>, // L2, L3, … pending configs
    // Validation errors and status messages
    pub(crate) config_error: Option<String>,
    pub(crate) config_status: Option<String>,
    // Inline field editing: (is_icache, field) + text buffer for numeric fields
    // For L2+: is_icache is ignored (unified), treated as false
    pub(crate) edit_field: Option<(bool, ConfigField)>,
    pub(crate) edit_buf: String,
    // CPI config editing
    pub(crate) cpi_selected: usize, // 0-8 field index
    pub(crate) cpi_editing: bool,
    pub(crate) cpi_edit_buf: String,
    pub(crate) hover_cpi_field: Option<usize>,
    // Export / Import buttons
    pub(crate) hover_export_results: bool,
    pub(crate) hover_export_cfg: bool,
    pub(crate) hover_import_cfg: bool,
    // Session run history (captured with `s` key)
    pub(crate) session_history: Vec<CacheResultsSnapshot>,
    pub(crate) history_scroll: usize,
    pub(crate) viewing_snapshot: Option<usize>, // index into session_history, Some = popup open
    pub(crate) window_start_instr: u64,         // start of current capture window, reset on restart
    // Horizontal scrollbar (View subtab) — geometry set by render, read by mouse
    pub(crate) hover_hscrollbar: bool,
    pub(crate) hscroll_hover_track_x: u16, // track_x of hovered scrollbar
    pub(crate) hscroll_hover_track_w: u16, // track_w of hovered scrollbar
    pub(crate) hscroll_drag: bool,
    pub(crate) hscroll_drag_start_x: u16,
    pub(crate) hscroll_start: usize,
    pub(crate) hscroll_drag_max: usize,
    pub(crate) hscroll_drag_track_w: u16,
    pub(crate) hscroll_drag_is_dcache: bool, // true = dragging D-cache bar, false = I-cache/unified
    // Set each frame by render (via Cell so render takes &App).
    // tracks[0] = I-cache or primary/unified, tracks[1] = D-cache (0,0 if absent).
    // Each entry: (track_x, track_w).
    pub(crate) hscroll_row: std::cell::Cell<u16>,
    pub(crate) hscroll_tracks: std::cell::Cell<[(u16, u16); 2]>,
    pub(crate) hscroll_max: std::cell::Cell<usize>,
}

// ── Simulation results snapshot ──────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct LevelSnapshot {
    pub name: String,
    pub size: usize,
    pub line_size: usize,
    pub associativity: usize,
    pub replacement: String,
    pub write_policy: String,
    pub hit_latency: u64,
    pub miss_penalty: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub writebacks: u64,
    pub bytes_loaded: u64,
    pub bytes_stored: u64,
    pub total_cycles: u64,
    pub ram_write_bytes: u64,
    pub amat: f64,
}

#[derive(Clone)]
pub(crate) struct CacheResultsSnapshot {
    pub label: String,
    pub instr_start: u64,
    pub instr_end: u64,
    pub instruction_count: u64,
    pub total_cycles: u64,
    pub base_cycles: u64,
    pub cpi: f64,
    pub ipc: f64,
    pub icache: LevelSnapshot,
    pub dcache: LevelSnapshot,
    pub extra_levels: Vec<LevelSnapshot>,
    pub cpi_config: CpiConfig,
    pub miss_hotspots: Vec<(u32, u64)>,
    pub hit_rate_history_i: Vec<(f64, f64)>,
    pub hit_rate_history_d: Vec<(f64, f64)>,
}
