//! TUI-only pipeline presentation and interaction state.

pub use raven_engine::falcon::pipeline::*;
pub mod sim {
    pub use raven_engine::falcon::pipeline::sim::*;
}

use std::cell::Cell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(crate) fn gantt_window_bounds(rows: &[&GanttRow], history_cols: usize) -> (u64, u64) {
    let history_cols = history_cols.max(1) as u64;
    let mut min_start: Option<u64> = None;
    let mut max_end: Option<u64> = None;

    for row in rows {
        if row.cells.is_empty() {
            continue;
        }
        min_start = Some(min_start.map_or(row.first_cycle, |cur| cur.min(row.first_cycle)));
        let row_end = row.first_cycle + row.cells.len() as u64;
        max_end = Some(max_end.map_or(row_end, |cur| cur.max(row_end)));
    }

    let min_start = min_start.unwrap_or(0);
    let max_end = max_end.unwrap_or(min_start);
    let end = max_end.max(min_start + 1);
    let start = end.saturating_sub(history_cols).max(min_start);
    (start, end)
}

pub(crate) fn gantt_view_rows<'a>(
    rows: &'a VecDeque<GanttRow>,
    scroll: usize,
    visible_rows: usize,
) -> Vec<&'a GanttRow> {
    let visible = visible_rows.max(1);
    let scroll = scroll.min(gantt_max_scroll_for_len(rows.len(), visible));
    let end = rows.len().saturating_sub(scroll);
    let start = end.saturating_sub(visible);
    rows.iter().skip(start).take(end - start).collect()
}

pub(crate) fn gantt_visible_rows(gantt_area_height: u16) -> usize {
    let inner_h = gantt_area_height.saturating_sub(2) as usize;
    inner_h.saturating_sub(2).max(1)
}

pub(crate) fn gantt_max_scroll_for_len(len: usize, visible_rows: usize) -> usize {
    len.saturating_sub(visible_rows.max(1))
}

pub(crate) fn gantt_max_scroll(state: &PipelineSimState, gantt_area_height: u16) -> usize {
    gantt_max_scroll_for_len(state.gantt.len(), gantt_visible_rows(gantt_area_height))
}

// ── Subtabs ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PipelineSubtab {
    Main,
    Config,
}

// ── Speed control ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipelineSpeed {
    Slow,
    Normal,
    Fast,
    Instant,
}

impl PipelineSpeed {
    pub fn interval(self) -> Duration {
        match self {
            Self::Slow => Duration::from_millis(600),
            Self::Normal => Duration::from_millis(300),
            Self::Fast => Duration::from_millis(80),
            Self::Instant => Duration::ZERO,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Slow => "Slow",
            Self::Normal => "Normal",
            Self::Fast => "Fast",
            Self::Instant => "Instant",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Slow => Self::Normal,
            Self::Normal => Self::Fast,
            Self::Fast => Self::Instant,
            Self::Instant => Self::Slow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineConfig {
    pub enabled: bool,
    pub bypass: PipelineBypassConfig,
    pub branch_resolve: BranchResolve,
    pub mode: PipelineMode,
    pub fu_capacity: [u8; FuKind::COUNT],
    pub predict: BranchPredict,
    pub speed: PipelineSpeed,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bypass: PipelineBypassConfig::default(),
            branch_resolve: BranchResolve::Ex,
            mode: PipelineMode::SingleCycle,
            fu_capacity: [1; FuKind::COUNT],
            predict: BranchPredict::NotTaken,
            speed: PipelineSpeed::Normal,
        }
    }
}

impl PipelineConfig {
    pub fn from_state(state: &PipelineSimState) -> Self {
        Self::from_state_and_speed(state, PipelineSpeed::Normal)
    }

    pub fn from_state_and_speed(state: &PipelineSimState, speed: PipelineSpeed) -> Self {
        Self {
            enabled: state.enabled,
            bypass: state.bypass,
            branch_resolve: state.branch_resolve,
            mode: state.mode,
            fu_capacity: state.fu_capacity,
            predict: state.predict,
            speed,
        }
    }

    pub fn apply_to_state(self, state: &mut PipelineSimState) {
        state.enabled = self.enabled;
        state.bypass = self.bypass;
        state.branch_resolve = self.branch_resolve;
        state.mode = self.mode;
        state.fu_capacity = self.fu_capacity;
        state.set_predict(self.predict);
    }
}

pub fn serialize_pipeline_config(cfg: &PipelineConfig) -> String {
    let mut s = String::from("# Raven Pipeline Config v1\n");
    s.push_str(&format!("enabled={}\n", cfg.enabled));
    s.push_str(&format!("bypass.ex_to_ex={}\n", cfg.bypass.ex_to_ex));
    s.push_str(&format!("bypass.mem_to_ex={}\n", cfg.bypass.mem_to_ex));
    s.push_str(&format!("bypass.wb_to_id={}\n", cfg.bypass.wb_to_id));
    s.push_str(&format!(
        "bypass.store_to_load={}\n",
        cfg.bypass.store_to_load
    ));
    let mode = match cfg.mode {
        PipelineMode::SingleCycle => "Serialized",
        PipelineMode::FunctionalUnits => "ParallelUFs",
    };
    s.push_str(&format!("mode={mode}\n"));
    s.push_str(&format!(
        "fu.alu={}\n",
        cfg.fu_capacity[FuKind::Alu.index()]
    ));
    s.push_str(&format!(
        "fu.mul={}\n",
        cfg.fu_capacity[FuKind::Mul.index()]
    ));
    s.push_str(&format!(
        "fu.div={}\n",
        cfg.fu_capacity[FuKind::Div.index()]
    ));
    s.push_str(&format!(
        "fu.fpu={}\n",
        cfg.fu_capacity[FuKind::Fpu.index()]
    ));
    s.push_str(&format!(
        "fu.lsu={}\n",
        cfg.fu_capacity[FuKind::Lsu.index()]
    ));
    s.push_str(&format!(
        "fu.sys={}\n",
        cfg.fu_capacity[FuKind::Sys.index()]
    ));
    s.push_str(&format!("branch_resolve={:?}\n", cfg.branch_resolve));
    s.push_str(&format!("predict={:?}\n", cfg.predict));
    s.push_str(&format!("speed={:?}\n", cfg.speed));
    s
}

pub fn parse_pipeline_config(text: &str) -> Result<PipelineConfig, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
        }
    }

    let get_bool = |key: &str, default: bool| -> bool {
        map.get(key)
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
            .unwrap_or(default)
    };
    let get_u8 = |key: &str, default: u8| -> u8 {
        map.get(key)
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(default)
    };

    let mode = match map.get("mode").map(String::as_str).unwrap_or("serialized") {
        "singlecycle" | "single-cycle" | "serialized" => PipelineMode::SingleCycle,
        "functionalunits" | "functional-units" | "functional_units" | "parallelufs"
        | "parallel-ufs" | "parallel_ufs" => PipelineMode::FunctionalUnits,
        other => return Err(format!("Unknown pipeline mode: {other}")),
    };

    let branch_resolve = match map
        .get("branch_resolve")
        .map(String::as_str)
        .unwrap_or("ex")
    {
        "id" => BranchResolve::Id,
        "ex" => BranchResolve::Ex,
        "mem" => BranchResolve::Mem,
        other => return Err(format!("Unknown branch_resolve: {other}")),
    };

    let predict = match map.get("predict").map(String::as_str).unwrap_or("nottaken") {
        "nottaken" | "not-taken" | "not_taken" => BranchPredict::NotTaken,
        "taken" => BranchPredict::Taken,
        "btfnt" => BranchPredict::Btfnt,
        "twobit" | "two-bit" | "two_bit" | "2bit" | "2-bit" => BranchPredict::TwoBit,
        other => return Err(format!("Unknown predict mode: {other}")),
    };

    let speed = match map.get("speed").map(String::as_str).unwrap_or("normal") {
        "slow" => PipelineSpeed::Slow,
        "normal" => PipelineSpeed::Normal,
        "fast" => PipelineSpeed::Fast,
        "instant" => PipelineSpeed::Instant,
        other => return Err(format!("Unknown pipeline speed: {other}")),
    };

    let bypass = if map.contains_key("forwarding") {
        if get_bool("forwarding", true) {
            PipelineBypassConfig::legacy_enabled()
        } else {
            PipelineBypassConfig::disabled()
        }
    } else {
        PipelineBypassConfig {
            ex_to_ex: get_bool("bypass.ex_to_ex", true),
            mem_to_ex: get_bool("bypass.mem_to_ex", true),
            wb_to_id: get_bool("bypass.wb_to_id", true),
            store_to_load: get_bool("bypass.store_to_load", false),
        }
    };

    let fu_capacity = [
        get_u8("fu.alu", 1),
        get_u8("fu.mul", 1),
        get_u8("fu.div", 1),
        get_u8("fu.fpu", 1),
        get_u8("fu.lsu", 1),
        get_u8("fu.sys", 1),
    ];

    Ok(PipelineConfig {
        enabled: get_bool("enabled", true),
        bypass,
        branch_resolve,
        mode,
        fu_capacity,
        predict,
        speed,
    })
}

/// Non-reversible state owned exclusively by the TUI.
pub struct PipelineViewState {
    pub speed: PipelineSpeed,
    pub last_tick: Instant,
    pub subtab: PipelineSubtab,
    pub config_cursor: usize,
    pub gantt_scroll: usize,
    pub gantt_visible_rows_cache: Cell<usize>,
    pub gantt_max_scroll_cache: Cell<usize>,
    pub hover_subtab_main: bool,
    pub hover_subtab_config: bool,
    pub hover_core: bool,
    pub hover_reset: bool,
    pub hover_speed: bool,
    pub hover_state: bool,
    pub hover_export_results: bool,
    pub hover_import_cfg: bool,
    pub hover_export_cfg: bool,
    pub status_msg: Option<String>,
    pub status_error: Option<String>,
    pub hover_config_row: Option<usize>,
    pub config_row_rects: Cell<[(u16, u16, u16); PipelineBypassConfig::CONFIG_ROWS]>,
    pub btn_subtab_main_rect: Cell<(u16, u16, u16)>,
    pub btn_subtab_config_rect: Cell<(u16, u16, u16)>,
    pub btn_core_rect: Cell<(u16, u16, u16)>,
    pub btn_reset_rect: Cell<(u16, u16, u16)>,
    pub btn_speed_rect: Cell<(u16, u16, u16)>,
    pub btn_state_rect: Cell<(u16, u16, u16)>,
    pub btn_export_results_rect: Cell<(u16, u16, u16)>,
    pub btn_import_cfg_rect: Cell<(u16, u16, u16)>,
    pub btn_export_cfg_rect: Cell<(u16, u16, u16)>,
    pub gantt_area_rect: Cell<(u16, u16, u16, u16)>,
}

impl PipelineViewState {
    pub fn new() -> Self {
        Self {
            speed: PipelineSpeed::Normal,
            last_tick: Instant::now(),
            subtab: PipelineSubtab::Main,
            config_cursor: 0,
            gantt_scroll: 0,
            gantt_visible_rows_cache: Cell::new(0),
            gantt_max_scroll_cache: Cell::new(0),
            hover_subtab_main: false,
            hover_subtab_config: false,
            hover_core: false,
            hover_reset: false,
            hover_speed: false,
            hover_state: false,
            hover_export_results: false,
            hover_import_cfg: false,
            hover_export_cfg: false,
            status_msg: None,
            status_error: None,
            hover_config_row: None,
            config_row_rects: Cell::new([(0, 0, 0); PipelineBypassConfig::CONFIG_ROWS]),
            btn_subtab_main_rect: Cell::new((0, 0, 0)),
            btn_subtab_config_rect: Cell::new((0, 0, 0)),
            btn_core_rect: Cell::new((0, 0, 0)),
            btn_reset_rect: Cell::new((0, 0, 0)),
            btn_speed_rect: Cell::new((0, 0, 0)),
            btn_state_rect: Cell::new((0, 0, 0)),
            btn_export_results_rect: Cell::new((0, 0, 0)),
            btn_import_cfg_rect: Cell::new((0, 0, 0)),
            btn_export_cfg_rect: Cell::new((0, 0, 0)),
            gantt_area_rect: Cell::new((0, 0, 0, 0)),
        }
    }

    pub fn clear_hover_state(&mut self) {
        self.hover_subtab_main = false;
        self.hover_subtab_config = false;
        self.hover_core = false;
        self.hover_reset = false;
        self.hover_speed = false;
        self.hover_state = false;
        self.hover_export_results = false;
        self.hover_import_cfg = false;
        self.hover_export_cfg = false;
        self.hover_config_row = None;
    }

    pub fn reset_for_program(&mut self) {
        self.gantt_scroll = 0;
        self.gantt_visible_rows_cache.set(0);
        self.gantt_max_scroll_cache.set(0);
        self.last_tick = Instant::now();
        self.status_msg = None;
        self.status_error = None;
    }
}

impl Default for PipelineViewState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../tests/support/ui_pipeline_mod.rs"]
mod tests;

#[cfg(test)]
mod boundary_tests {
    use super::{PipelineSimState, PipelineSpeed, PipelineViewState};
    use raven_engine::falcon::machine::JournaledPipeline;

    #[test]
    fn restore_does_not_touch_visual_state() {
        let mut core = PipelineSimState::new();
        let mut view = PipelineViewState::new();
        let snapshot = core.exec_snapshot();
        core.cycle_count = 7;
        view.speed = PipelineSpeed::Fast;
        core.restore_exec(snapshot);
        assert_eq!(core.cycle_count, 0);
        assert_eq!(view.speed, PipelineSpeed::Fast);
    }
}
