// ── Settings tab state ───────────────────────────────────────────────────────

/// Snap `v` to the nearest power of two within `[lo, hi]`.
pub(crate) fn nearest_pow2_clamp(v: usize, lo: usize, hi: usize) -> usize {
    if v == 0 {
        return lo;
    }
    let ceil = v.next_power_of_two(); // smallest pow2 >= v
    let floor = if ceil == v { v } else { ceil >> 1 }; // largest pow2 <= v
    let best = if ceil == v {
        v
    } else {
        let d_floor = v - floor;
        let d_ceil = ceil - v;
        if d_ceil <= d_floor { ceil } else { floor }
    };
    best.max(lo).min(hi)
}

/// Row index of the cache_enabled toggle in the settings list (0-indexed).
pub(crate) const SETTINGS_ROW_CACHE_ENABLED: usize = 0;
/// Row index of the max_cores selector.
pub(crate) const SETTINGS_ROW_MAX_CORES: usize = 1;
/// Row index of the memory size selector.
pub(crate) const SETTINGS_ROW_MEM_SIZE: usize = 2;
/// Row index of the run scope selector.
pub(crate) const SETTINGS_ROW_RUN_SCOPE: usize = 3;
/// Row index of the pipeline_enabled toggle.
pub(crate) const SETTINGS_ROW_PIPELINE_ENABLED: usize = 4;
/// Row index of the syscall tracing toggle.
pub(crate) const SETTINGS_ROW_TRACE_SYSCALLS: usize = 5;
/// First CPI row index in the settings list (6 rows + 1 blank separator).
pub(crate) const SETTINGS_ROW_CPI_START: usize = 7;
/// Total number of settings rows (6 rows + 1 blank + 11 CPI fields).
pub(crate) const SETTINGS_ROWS: usize = SETTINGS_ROW_CPI_START + 11;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum RunScope {
    AllHarts,
    FocusedHart,
}

impl RunScope {
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::AllHarts => Self::FocusedHart,
            Self::FocusedHart => Self::AllHarts,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AllHarts => "ALL",
            Self::FocusedHart => "FOCUS",
        }
    }
}

pub(crate) struct SettingsState {
    /// Index of currently highlighted row.
    pub(crate) selected: usize,
    /// Mouse hover over a whole settings row.
    pub(crate) hover_row: Option<usize>,
    /// true when a CPI field is being edited
    pub(crate) cpi_editing: bool,
    /// Text buffer while editing a CPI field
    pub(crate) cpi_edit_buf: String,
    /// Mouse hover: which CPI field row (0-based within CPI section)
    pub(crate) hover_cpi_field: Option<usize>,
    /// Mouse hover over the cache_enabled bool button
    pub(crate) hover_cache_enabled: bool,
    /// Mouse hover over the pipeline_enabled bool button
    pub(crate) hover_pipeline_enabled: bool,
    /// Mouse hover over the syscall tracing bool button
    pub(crate) hover_trace_syscalls: bool,
    /// Mouse hover over the run_scope selector
    pub(crate) hover_run_scope: bool,
    /// Mouse hover over the config import button
    pub(crate) hover_import_rcfg: bool,
    /// Mouse hover over the config export button
    pub(crate) hover_export_rcfg: bool,
    /// Geometry of the cache bool button (y, x_start, x_end) — written by render, read by mouse
    pub(crate) bool_btn_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the pipeline bool button (y, x_start, x_end)
    pub(crate) bool_btn_pipeline_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the syscall tracing bool button (y, x_start, x_end)
    pub(crate) bool_btn_trace_syscalls_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the run scope button (y, x_start, x_end)
    pub(crate) run_scope_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the import rcfg button (y, x_start, x_end)
    pub(crate) import_rcfg_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the export rcfg button (y, x_start, x_end)
    pub(crate) export_rcfg_rect: std::cell::Cell<(u16, u16, u16)>,
    /// Geometry of the visible settings list area (x, y, width, height)
    pub(crate) list_rect: std::cell::Cell<(u16, u16, u16, u16)>,
    /// Geometry of each CPI row (y) — written by render, read by mouse
    pub(crate) cpi_rows_y: std::cell::Cell<[u16; 11]>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected: 0,
            hover_row: None,
            cpi_editing: false,
            cpi_edit_buf: String::new(),
            hover_cpi_field: None,
            hover_cache_enabled: false,
            hover_pipeline_enabled: false,
            hover_trace_syscalls: false,
            hover_run_scope: false,
            hover_import_rcfg: false,
            hover_export_rcfg: false,
            bool_btn_rect: std::cell::Cell::new((0, 0, 0)),
            bool_btn_pipeline_rect: std::cell::Cell::new((0, 0, 0)),
            bool_btn_trace_syscalls_rect: std::cell::Cell::new((0, 0, 0)),
            run_scope_rect: std::cell::Cell::new((0, 0, 0)),
            import_rcfg_rect: std::cell::Cell::new((0, 0, 0)),
            export_rcfg_rect: std::cell::Cell::new((0, 0, 0)),
            list_rect: std::cell::Cell::new((0, 0, 0, 0)),
            cpi_rows_y: std::cell::Cell::new([0u16; 11]),
        }
    }
}
