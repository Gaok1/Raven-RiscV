use super::{
    console::Console,
    editor::Editor,
    input::{handle_key, handle_mouse},
    view::ui,
};

/// Extract the identifier-like word at the given character column in a line.
fn word_at(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() { return String::new(); }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    if !is_word(chars[col]) { return String::new(); }
    let start = (0..=col).rev().take_while(|&i| i < chars.len() && is_word(chars[i])).last().unwrap_or(col);
    let end = (col..chars.len()).take_while(|&i| is_word(chars[i])).last().map(|i| i + 1).unwrap_or(col + 1);
    chars[start..end].iter().collect()
}
use crate::falcon::{self, Cpu, CacheController};
use crate::falcon::cache::CacheConfig;
use crossterm::{
    event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event},
    execute,
};
use arboard::Clipboard;
use ratatui::{DefaultTerminal, layout::Rect};
use std::{
    io,
    time::{Duration, Instant},
};
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
use std::sync::{Arc, atomic::Ordering};

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum Tab {
    Editor,
    Run,
    Cache,
    Docs,
}

impl Tab {
    pub(super) fn all() -> &'static [Tab] {
        &[Tab::Editor, Tab::Run, Tab::Cache, Tab::Docs]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Editor => "Editor",
            Tab::Run => "Run",
            Tab::Cache => "Cache",
            Tab::Docs => "Docs",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
    }
}

// ── Cache tab state ─────────────────────────────────────────────────────────

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum CacheSubtab {
    Stats,
    Config,
    View,
}

/// Editable field in the Config subtab.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum ConfigField {
    Size, LineSize, Associativity,
    Replacement, WritePolicy, WriteAlloc,
    HitLatency, MissPenalty, AssocPenalty, TransferWidth,
    Inclusion,
}

impl ConfigField {
    pub(super) fn is_numeric(self) -> bool {
        matches!(self, Self::Size | Self::LineSize | Self::Associativity | Self::HitLatency | Self::MissPenalty | Self::AssocPenalty | Self::TransferWidth)
    }
    pub(super) fn all_editable() -> &'static [ConfigField] {
        &[Self::Size, Self::LineSize, Self::Associativity, Self::Replacement,
          Self::WritePolicy, Self::WriteAlloc, Self::HitLatency, Self::MissPenalty,
          Self::AssocPenalty, Self::TransferWidth, Self::Inclusion]
    }
    /// Row index in the rendered fields list (3 = Sets which is read-only, skip it)
    pub(super) fn list_row(self) -> usize {
        match self {
            Self::Size => 0, Self::LineSize => 1, Self::Associativity => 2,
            Self::Replacement => 4, Self::WritePolicy => 5, Self::WriteAlloc => 6,
            Self::HitLatency => 7, Self::MissPenalty => 8,
            Self::AssocPenalty => 9, Self::TransferWidth => 10,
            Self::Inclusion => 11,
        }
    }
    pub(super) fn from_list_row(row: usize) -> Option<Self> {
        match row {
            0 => Some(Self::Size), 1 => Some(Self::LineSize), 2 => Some(Self::Associativity),
            3 => None, // Sets is read-only
            4 => Some(Self::Replacement), 5 => Some(Self::WritePolicy), 6 => Some(Self::WriteAlloc),
            7 => Some(Self::HitLatency), 8 => Some(Self::MissPenalty),
            9 => Some(Self::AssocPenalty), 10 => Some(Self::TransferWidth),
            11 => Some(Self::Inclusion),
            _ => None,
        }
    }
    pub(super) fn next(self) -> Self {
        let a = Self::all_editable();
        a[(a.iter().position(|&f| f == self).unwrap_or(0) + 1) % a.len()]
    }
    pub(super) fn prev(self) -> Self {
        let a = Self::all_editable();
        let i = a.iter().position(|&f| f == self).unwrap_or(0);
        a[i.checked_sub(1).unwrap_or(a.len() - 1)]
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum CacheScope {
    ICache,
    DCache,
    Both,
}

#[derive(PartialEq, Eq, Copy, Clone, Default)]
pub(super) enum CacheDataFmt {
    #[default] Hex,
    DecU,
    DecS,
    Float,
}
impl CacheDataFmt {
    pub(super) fn cycle(self) -> Self {
        match self {
            Self::Hex   => Self::DecU,
            Self::DecU  => Self::DecS,
            Self::DecS  => Self::Float,
            Self::Float => Self::Hex,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Hex   => "HEX",
            Self::DecU  => "DEC-U",
            Self::DecS  => "DEC-S",
            Self::Float => "FLOAT",
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Default)]
pub(super) enum CacheDataGroup {
    #[default] B1,
    B2,
    B4,
}
impl CacheDataGroup {
    pub(super) fn cycle(self) -> Self {
        match self { Self::B1 => Self::B2, Self::B2 => Self::B4, Self::B4 => Self::B1 }
    }
    pub(super) fn label(self) -> &'static str {
        match self { Self::B1 => "1B", Self::B2 => "2B", Self::B4 => "4B" }
    }
    pub(super) fn bytes(self) -> usize {
        match self { Self::B1 => 1, Self::B2 => 2, Self::B4 => 4 }
    }
}

pub(super) struct CacheState {
    pub(super) subtab: CacheSubtab,
    pub(super) scope: CacheScope,
    pub(super) stats_scroll: usize,
    // Level selector
    pub(super) selected_level: usize,        // 0 = L1, 1 = L2, …
    pub(super) hover_level: Vec<bool>,        // one per level (L1 + extra)
    pub(super) hover_add_level: bool,
    pub(super) hover_remove_level: bool,
    // Hover flags
    pub(super) hover_subtab_stats: bool,
    pub(super) hover_subtab_config: bool,
    pub(super) hover_subtab_view: bool,
    pub(super) view_scroll: usize,
    pub(super) view_h_scroll: usize,   // I-cache (or unified/L2+) horizontal scroll
    pub(super) view_h_scroll_d: usize, // D-cache horizontal scroll (separate from I-cache)
    pub(super) data_fmt: CacheDataFmt,
    pub(super) data_group: CacheDataGroup,
    // View legend button positions (set by render each frame, read by mouse)
    pub(super) view_fmt_btn: std::cell::Cell<(u16, u16, u16)>,   // (y, x_start, x_end)
    pub(super) view_group_btn: std::cell::Cell<(u16, u16, u16)>, // (y, x_start, x_end)
    pub(super) hover_view_fmt: bool,
    pub(super) hover_view_group: bool,
    pub(super) hover_scope_i: bool,
    pub(super) hover_scope_d: bool,
    pub(super) hover_scope_both: bool,
    pub(super) hover_apply: bool,
    pub(super) hover_apply_keep: bool,
    pub(super) hover_preset_i: Option<usize>,
    pub(super) hover_preset_d: Option<usize>,
    pub(super) hover_config_field: Option<(bool, ConfigField)>,
    // Config form (pending values before Apply)
    pub(super) pending_icache: CacheConfig,
    pub(super) pending_dcache: CacheConfig,
    pub(super) extra_pending: Vec<CacheConfig>,  // L2, L3, … pending configs
    // Validation errors and status messages
    pub(super) config_error: Option<String>,
    pub(super) config_status: Option<String>,
    // Inline field editing: (is_icache, field) + text buffer for numeric fields
    // For L2+: is_icache is ignored (unified), treated as false
    pub(super) edit_field: Option<(bool, ConfigField)>,
    pub(super) edit_buf: String,
    // CPI config editing
    pub(super) cpi_selected: usize, // 0-8 field index
    pub(super) cpi_editing: bool,
    pub(super) cpi_edit_buf: String,
    pub(super) hover_cpi_field: Option<usize>,
    // Export/Compare
    pub(super) loaded_snapshot: Option<Box<CacheResultsSnapshot>>,
    pub(super) hover_export_results: bool,
    pub(super) hover_compare: bool,
    // Horizontal scrollbar (View subtab) — geometry set by render, read by mouse
    pub(super) hover_hscrollbar: bool,
    pub(super) hscroll_hover_track_x: u16,  // track_x of hovered scrollbar
    pub(super) hscroll_hover_track_w: u16,  // track_w of hovered scrollbar
    pub(super) hscroll_drag: bool,
    pub(super) hscroll_drag_start_x: u16,
    pub(super) hscroll_start: usize,
    pub(super) hscroll_drag_max: usize,
    pub(super) hscroll_drag_track_w: u16,
    pub(super) hscroll_drag_is_dcache: bool, // true = dragging D-cache bar, false = I-cache/unified
    // Set each frame by render (via Cell so render takes &App).
    // tracks[0] = I-cache or primary/unified, tracks[1] = D-cache (0,0 if absent).
    // Each entry: (track_x, track_w).
    pub(super) hscroll_row: std::cell::Cell<u16>,
    pub(super) hscroll_tracks: std::cell::Cell<[(u16, u16); 2]>,
    pub(super) hscroll_max: std::cell::Cell<usize>,
}

// ── Simulation results snapshot ──────────────────────────────────────────────

pub(super) struct LevelSnapshot {
    pub name: String,
    pub size: usize, pub line_size: usize, pub associativity: usize,
    pub replacement: String, pub write_policy: String,
    pub hit_latency: u64, pub miss_penalty: u64,
    pub hits: u64, pub misses: u64, pub evictions: u64, pub writebacks: u64,
    pub bytes_loaded: u64, pub bytes_stored: u64,
    pub total_cycles: u64, pub ram_write_bytes: u64, pub amat: f64,
}

pub(super) struct CacheResultsSnapshot {
    pub label: String,
    pub instruction_count: u64, pub total_cycles: u64, pub base_cycles: u64,
    pub cpi: f64, pub ipc: f64,
    pub icache: LevelSnapshot, pub dcache: LevelSnapshot,
    pub extra_levels: Vec<LevelSnapshot>,
    pub cpi_config: CpiConfig,
    pub miss_hotspots: Vec<(u32, u64)>,
    pub hit_rate_history_i: Vec<(f64, f64)>,
    pub hit_rate_history_d: Vec<(f64, f64)>,
}

// ── CPI (Cycles Per Instruction) configuration ───────────────────────────────

/// Base execution cycles per instruction class.
/// These are added on top of cache latency cycles.
#[derive(Clone, Debug)]
pub(super) struct CpiConfig {
    pub alu:              u64,   // Add, sub, logic, shifts, lui, auipc, immediate variants = 1
    pub mul:              u64,   // mul, mulh, mulhsu, mulhu = 3
    pub div:              u64,   // div, divu, rem, remu = 20
    pub load:             u64,   // lb, lh, lw, lbu, lhu (extra over cache) = 0
    pub store:            u64,   // sb, sh, sw (extra over cache) = 0
    pub branch_taken:     u64,   // branch when taken = 3
    pub branch_not_taken: u64,   // branch when not taken = 1
    pub jump:             u64,   // jal, jalr = 2
    pub system:           u64,   // ecall, ebreak, halt = 10
    pub fp:               u64,   // RV32F instructions = 5
}

impl Default for CpiConfig {
    fn default() -> Self {
        Self {
            alu: 1, mul: 3, div: 20,
            load: 0, store: 0,
            branch_taken: 3, branch_not_taken: 1,
            jump: 2, system: 10, fp: 5,
        }
    }
}

impl CpiConfig {
    pub(super) fn field_names() -> &'static [&'static str] {
        &["ALU", "MUL", "DIV", "Load+", "Store+", "Branch-T", "Branch-NT", "Jump", "System", "FP"]
    }

    pub(super) fn get(&self, idx: usize) -> u64 {
        match idx {
            0 => self.alu, 1 => self.mul, 2 => self.div,
            3 => self.load, 4 => self.store,
            5 => self.branch_taken, 6 => self.branch_not_taken,
            7 => self.jump, 8 => self.system, 9 => self.fp,
            _ => 0,
        }
    }

    pub(super) fn set(&mut self, idx: usize, val: u64) {
        match idx {
            0 => self.alu = val, 1 => self.mul = val, 2 => self.div = val,
            3 => self.load = val, 4 => self.store = val,
            5 => self.branch_taken = val, 6 => self.branch_not_taken = val,
            7 => self.jump = val, 8 => self.system = val, 9 => self.fp = val,
            _ => {}
        }
    }

    pub(super) fn descriptions() -> &'static [&'static str] {
        &[
            "add/sub/logic/shift/lui/auipc/imm",
            "mul/mulh/mulhsu/mulhu",
            "div/divu/rem/remu",
            "load (extra over cache miss)",
            "store (extra over cache)",
            "branch when taken (pipeline flush)",
            "branch when not taken",
            "jal / jalr",
            "ecall / ebreak / halt",
            "RV32F float instructions",
        ]
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum EditorMode {
    Insert,
    Command,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum MemRegion {
    Data,
    Stack,
    Custom,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum FormatMode {
    Hex,
    Dec,
    Str,
}

/// Execution speed setting.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum RunSpeed {
    /// ~12 steps/sec — slow, instruction-by-instruction
    X1,
    /// ~50 steps/sec — faster but still watchable
    X2,
    /// ~400 steps/sec — fast, visual blur
    X4,
    /// ~800 steps/sec — very fast
    X8,
    /// Time-budgeted bulk (8 ms/frame) — effectively instant
    Instant,
}

impl RunSpeed {
    /// Cycle to the next speed level (wraps around).
    pub(super) fn cycle(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X4,
            Self::X4 => Self::X8,
            Self::X8 => Self::Instant,
            Self::Instant => Self::X1,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
            Self::Instant => "GO",
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum RunButton {
    View,
    Format,
    Sign,
    Bytes,
    Region,
    State,
    Speed,
    ExecCount,
    InstrType,
    Reset,
}

// ── State per tab ──────────────────────────────────────────────────────────────

pub(super) struct EditorState {
    pub(super) buf: Editor,
    pub(super) dirty: bool,
    pub(super) last_edit_at: Option<Instant>,
    pub(super) auto_check_delay: Duration,
    pub(super) last_assemble_msg: Option<String>,
    pub(super) last_compile_ok: Option<bool>,

    // Last successfully assembled program (for restart without re-parsing)
    pub(super) last_ok_text: Option<Vec<u32>>,
    pub(super) last_ok_data: Option<Vec<u8>>,
    pub(super) last_ok_data_base: Option<u32>,
    pub(super) last_ok_bss_size: Option<u32>,
    /// Raw ELF bytes stored for re-loading on reset (None when loaded from source/FALC/flat).
    pub(super) last_ok_elf_bytes: Option<Vec<u8>>,
    pub(super) last_ok_comments: std::collections::HashMap<u32, String>,
    pub(super) last_ok_block_comments: std::collections::HashMap<u32, String>,
    pub(super) last_ok_labels: std::collections::HashMap<u32, Vec<String>>,

    // Compile diagnostics
    pub(super) diag_line: Option<usize>,
    pub(super) diag_msg: Option<String>,
    pub(super) diag_line_text: Option<String>,

    // Source-level metadata from last successful assembly
    pub(super) label_to_line: std::collections::HashMap<String, usize>,
    pub(super) line_to_addr: std::collections::HashMap<usize, u32>,
    pub(super) show_addr_hints: bool,

    // Find bar
    pub(super) find_open: bool,
    pub(super) find_query: String,
    pub(super) replace_open: bool,
    pub(super) replace_query: String,
    pub(super) find_in_replace: bool,
    pub(super) find_matches: Vec<(usize, usize)>,
    pub(super) find_current: usize,
    // Goto bar
    pub(super) goto_open: bool,
    pub(super) goto_query: String,
}

pub(super) struct RunState {
    pub(super) cpu: Cpu,
    pub(super) prev_x: [u32; 32],
    pub(super) prev_pc: u32,
    pub(super) mem: CacheController,
    pub(super) breakpoints: std::collections::HashSet<u32>,
    pub(super) mem_size: usize,
    pub(super) base_pc: u32,
    pub(super) data_base: u32,

    // Memory view
    pub(super) mem_view_addr: u32,
    pub(super) mem_view_bytes: u32,
    pub(super) mem_region: MemRegion,

    // Display options
    pub(super) show_registers: bool,
    pub(super) fmt_mode: FormatMode,
    pub(super) show_signed: bool,

    // Sidebar panel (resizable + collapsible)
    pub(super) sidebar_width: u16,
    pub(super) hover_sidebar_bar: bool,
    pub(super) sidebar_drag: bool,
    pub(super) sidebar_drag_start_x: u16,
    pub(super) sidebar_width_start: u16,
    pub(super) sidebar_collapsed: bool,

    // Instruction memory panel (resizable + collapsible)
    pub(super) imem_width: u16,
    pub(super) hover_imem_bar: bool,
    pub(super) imem_drag: bool,
    pub(super) imem_drag_start_x: u16,
    pub(super) imem_width_start: u16,
    // imem_scroll is now in VISUAL ROWS (not instruction count)
    pub(super) imem_scroll: usize,
    pub(super) hover_imem_addr: Option<u32>,
    // Set each frame by render so scroll handlers use the correct height
    pub(super) imem_inner_height: std::cell::Cell<usize>,
    pub(super) imem_collapsed: bool,

    // Details panel (collapsible)
    pub(super) details_collapsed: bool,

    // Console panel (resizable)
    pub(super) console_height: u16,
    pub(super) hover_console_bar: bool,
    pub(super) hover_console_clear: bool,
    pub(super) console_drag: bool,
    pub(super) console_drag_start_y: u16,
    pub(super) console_height_start: u16,

    // Execution
    pub(super) regs_scroll: usize,
    pub(super) is_running: bool,
    pub(super) last_step_time: Instant,
    pub(super) step_interval: Duration,
    pub(super) faulted: bool,
    pub(super) speed: RunSpeed,

    // Visible comments from source (#! text), keyed by instruction address
    pub(super) comments: std::collections::HashMap<u32, String>,

    // Source label metadata
    pub(super) labels: std::collections::HashMap<u32, Vec<String>>,

    // ELF sections for the sections viewer (empty when loaded from ASM)
    pub(super) elf_sections: Vec<falcon::program::ElfSection>,

    // Execution statistics
    pub(super) exec_counts: std::collections::HashMap<u32, u64>,
    pub(super) exec_trace: std::collections::VecDeque<(u32, String)>,

    // Register highlight age: 0 = just changed, 255 = unchanged for long
    pub(super) reg_age: [u8; 32],

    // UI flags
    pub(super) show_trace: bool,
    pub(super) pinned_regs: Vec<u8>,
    pub(super) reg_cursor: usize, // 0 = PC, 1-32 = x0-x31

    // Feature: block comments from source (Feature 4)
    pub(super) block_comments: std::collections::HashMap<u32, String>,

    // Feature: register write trace (Feature 8)
    pub(super) reg_last_write_pc: [Option<u32>; 32],

    // Feature: breakpoint list view (Feature 10)
    pub(super) show_bp_list: bool,

    // Mouse hover row in register sidebar (visual row index, 0-based within inner area)
    pub(super) hover_reg_row: Option<usize>,

    // CPI configuration
    pub(super) cpi_config: CpiConfig,

    // Instruction list display toggles
    pub(super) show_exec_count: bool,
    pub(super) show_instr_type: bool,

    // RV32F: float register sidebar
    pub(super) show_float_regs: bool,         // toggle between int / float register view
    pub(super) prev_f: [u32; 32],             // previous float register values (for highlighting)
    pub(super) f_age: [u8; 32],               // highlight age for float registers (0=just changed)
    pub(super) f_last_write_pc: [Option<u32>; 32], // last instruction that wrote each f-reg

    // Memory access highlight: (base_addr, size_bytes, age); age 0=just accessed, disappears at 3
    pub(super) mem_access_log: Vec<(u32, u32, u8)>,
}

/// Pages in the Docs tab. Tab key cycles through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsPage {
    InstrRef,
    Syscalls,
    MemoryMap,
}

impl DocsPage {
    pub(super) fn next(self) -> Self {
        match self {
            Self::InstrRef   => Self::Syscalls,
            Self::Syscalls   => Self::MemoryMap,
            Self::MemoryMap  => Self::InstrRef,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::InstrRef  => "Instr Ref",
            Self::Syscalls  => "Syscalls",
            Self::MemoryMap => "Memory Map",
        }
    }
}

/// UI language for Syscalls and MemoryMap pages. L key toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsLang { En, PtBr }

impl DocsLang {
    pub(super) fn toggle(self) -> Self {
        match self { Self::En => Self::PtBr, Self::PtBr => Self::En }
    }
    pub(super) fn label(self) -> &'static str {
        match self { Self::En => "EN", Self::PtBr => "PT-BR" }
    }
}

pub(super) struct DocsState {
    pub(super) page: DocsPage,
    pub(super) lang: DocsLang,
    pub(super) scroll: usize,
    pub(super) search_open: bool,
    pub(super) search_query: String,
    /// Bitmask of visible type categories (see docs::ALL_MASK / TY_* constants).
    pub(super) type_filter: u16,
    /// Cursor position in the filter bar: 0 = "All", 1–12 = individual types.
    pub(super) filter_cursor: usize,
    // ── Render-side position tracking (set by render, read by mouse handler) ──
    /// Y row of the page tab bar (relative to terminal origin).
    pub(super) tab_bar_y: std::cell::Cell<u16>,
    /// (x_start, x_end) for each of the 3 page tabs, relative to terminal origin.
    pub(super) tab_bar_xs: std::cell::Cell<[(u16, u16); 3]>,
    /// Y row of the filter bar (InstrRef page only).
    pub(super) filter_bar_y: std::cell::Cell<u16>,
}

// ── Top-level app ──────────────────────────────────────────────────────────────

pub struct App {
    pub(super) tab: Tab,
    pub(super) mode: EditorMode,

    pub(super) editor: EditorState,
    pub(super) run: RunState,
    pub(super) docs: DocsState,
    pub(super) cache: CacheState,

    pub(super) show_exit_popup: bool,
    pub(super) should_quit: bool,

    // Help popup
    pub(super) help_open: bool,
    pub(super) help_page: usize,
    pub(super) hover_help: bool,

    // Mouse tracking (shared across tabs)
    pub(super) mouse_x: u16,
    pub(super) mouse_y: u16,
    pub(super) hover_tab: Option<Tab>,
    pub(super) hover_run_button: Option<RunButton>,

    // Program I/O console (shared across tabs)
    pub(super) console: Console,

    // Persistent clipboard — must stay alive on Linux/X11 to retain ownership
    pub(super) clipboard: Option<Clipboard>,

    // Timestamp of the last bracketed-paste event (Event::Paste). Used to
    // suppress the arboard Ctrl+V handler if a bracketed-paste already fired
    // within the same keypress cycle, preventing double-paste in terminals
    // that emit both Event::Paste and a Ctrl+V key event simultaneously.
    pub(super) last_bracketed_paste: Option<Instant>,

    // Splash screen — set to Some(start_instant) on launch, cleared after 4s
    pub(super) splash_start: Option<Instant>,

    // RAM size override from --mem CLI flag. None = use per-mode defaults.
    pub(super) ram_override: Option<usize>,
}

pub(super) fn compute_find_matches(query: &str, lines: &[String]) -> Vec<(usize, usize)> {
    if query.is_empty() { return vec![]; }
    let q = query.to_lowercase();
    let q_len = q.len();
    let mut matches = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        let mut byte_from = 0;
        while byte_from < line_lower.len() {
            if let Some(rel) = line_lower[byte_from..].find(&q) {
                let byte_pos = byte_from + rel;
                let col = line[..byte_pos].chars().count();
                matches.push((row, col));
                byte_from = byte_pos + q_len.max(1);
            } else {
                break;
            }
        }
    }
    matches
}

impl App {
    pub fn new(ram_override: Option<usize>) -> Self {
        let mut cpu = Cpu::default();
        let base_pc = 0x0000_0000;
        cpu.pc = base_pc;
        let mem_size = ram_override.unwrap_or(128 * 1024);
        cpu.write(2, mem_size as u32);
        let data_base = base_pc + 0x1000;
        Self {
            tab: Tab::Editor,
            mode: EditorMode::Insert,
            editor: EditorState {
                buf: Editor::with_sample(),
                dirty: true,
                last_edit_at: Some(Instant::now()),
                auto_check_delay: Duration::from_millis(400),
                last_assemble_msg: None,
                last_compile_ok: None,
                last_ok_text: None,
                last_ok_data: None,
                last_ok_data_base: None,
                last_ok_bss_size: None,
                last_ok_elf_bytes: None,
                last_ok_comments: std::collections::HashMap::new(),
                last_ok_block_comments: std::collections::HashMap::new(),
                last_ok_labels: std::collections::HashMap::new(),
                diag_line: None,
                diag_msg: None,
                diag_line_text: None,
                label_to_line: std::collections::HashMap::new(),
                line_to_addr: std::collections::HashMap::new(),
                show_addr_hints: false,
                find_open: false,
                find_query: String::new(),
                replace_open: false,
                replace_query: String::new(),
                find_in_replace: false,
                find_matches: Vec::new(),
                find_current: 0,
                goto_open: false,
                goto_query: String::new(),
            },
            run: RunState {
                cpu,
                prev_x: [0; 32],
                prev_pc: base_pc,
                mem_size,
                mem: CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], mem_size),
                breakpoints: std::collections::HashSet::new(),
                base_pc,
                data_base,
                mem_view_addr: data_base,
                mem_view_bytes: 4,
                mem_region: MemRegion::Data,
                show_registers: true,
                fmt_mode: FormatMode::Hex,
                show_signed: false,
                sidebar_width: 38,
                hover_sidebar_bar: false,
                sidebar_drag: false,
                sidebar_drag_start_x: 0,
                sidebar_width_start: 38,
                sidebar_collapsed: false,
                imem_width: 38,
                hover_imem_bar: false,
                imem_drag: false,
                imem_drag_start_x: 0,
                imem_width_start: 38,
                imem_scroll: 0,
                hover_imem_addr: None,
                imem_inner_height: std::cell::Cell::new(16),
                imem_collapsed: false,
                details_collapsed: false,
                console_height: 5,
                hover_console_bar: false,
                hover_console_clear: false,
                console_drag: false,
                console_drag_start_y: 0,
                console_height_start: 5,
                regs_scroll: 0,
                is_running: false,
                last_step_time: Instant::now(),
                step_interval: Duration::from_millis(80),
                faulted: false,
                speed: RunSpeed::X1,
                comments: std::collections::HashMap::new(),
                labels: std::collections::HashMap::new(),
                elf_sections: Vec::new(),
                exec_counts: std::collections::HashMap::new(),
                exec_trace: std::collections::VecDeque::new(),
                reg_age: [255u8; 32],
                show_trace: false,
                pinned_regs: Vec::new(),
                reg_cursor: 0,
                block_comments: std::collections::HashMap::new(),
                reg_last_write_pc: [None; 32],
                show_bp_list: false,
                hover_reg_row: None,
                show_float_regs: false,
                prev_f: [0u32; 32],
                f_age: [255u8; 32],
                f_last_write_pc: [None; 32],
                cpi_config: CpiConfig::default(),
                show_exec_count: true,
                show_instr_type: true,
                mem_access_log: Vec::new(),
            },
            docs: DocsState {
                page: DocsPage::InstrRef, lang: DocsLang::En, scroll: 0,
                search_open: false, search_query: String::new(),
                type_filter: 0x0FFF, filter_cursor: 0,
                tab_bar_y: std::cell::Cell::new(0),
                tab_bar_xs: std::cell::Cell::new([(0, 0); 3]),
                filter_bar_y: std::cell::Cell::new(0),
            },
            cache: CacheState {
                subtab: CacheSubtab::Stats,
                scope: CacheScope::Both,
                stats_scroll: 0,
                selected_level: 0,
                hover_level: vec![false],
                hover_add_level: false,
                hover_remove_level: false,
                hover_subtab_stats: false,
                hover_subtab_config: false,
                hover_subtab_view: false,
                view_scroll: 0,
                view_h_scroll: 0,
                view_h_scroll_d: 0,
                data_fmt: CacheDataFmt::Hex,
                data_group: CacheDataGroup::B1,
                view_fmt_btn: std::cell::Cell::new((0, 0, 0)),
                view_group_btn: std::cell::Cell::new((0, 0, 0)),
                hover_view_fmt: false,
                hover_view_group: false,
                hover_scope_i: false,
                hover_scope_d: false,
                hover_scope_both: false,
                hover_apply: false,
                hover_apply_keep: false,
                hover_preset_i: None,
                hover_preset_d: None,
                hover_config_field: None,
                pending_icache: CacheConfig::default(),
                pending_dcache: CacheConfig::default(),
                extra_pending: vec![],
                config_error: None,
                config_status: None,
                edit_field: None,
                edit_buf: String::new(),
                cpi_selected: 0,
                cpi_editing: false,
                cpi_edit_buf: String::new(),
                hover_cpi_field: None,
                loaded_snapshot: None,
                hover_export_results: false,
                hover_compare: false,
                hover_hscrollbar: false,
                hscroll_hover_track_x: 0,
                hscroll_hover_track_w: 0,
                hscroll_drag: false,
                hscroll_drag_start_x: 0,
                hscroll_start: 0,
                hscroll_drag_max: 0,
                hscroll_drag_track_w: 1,
                hscroll_drag_is_dcache: false,
                hscroll_row: std::cell::Cell::new(0),
                hscroll_tracks: std::cell::Cell::new([(0, 0); 2]),
                hscroll_max: std::cell::Cell::new(0),
            },
            show_exit_popup: false,
            should_quit: false,
            help_open: false,
            help_page: 0,
            hover_help: false,
            mouse_x: 0,
            mouse_y: 0,
            hover_tab: None,
            hover_run_button: None,
            console: Console::default(),
            clipboard: Clipboard::new().ok(),
            last_bracketed_paste: None,
            ram_override,
            splash_start: Some(Instant::now()),
        }
    }

    pub(super) fn assemble_and_load(&mut self) {
        use falcon::asm::assemble;
        use falcon::program::{load_bytes, load_words, zero_bytes};

        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = self.ram_override.unwrap_or(128 * 1024);
        self.run.cpu = Cpu::default();
        self.run.cpu.pc = self.run.base_pc;
        self.run.prev_pc = self.run.cpu.pc;
        self.run.cpu.write(2, self.run.mem_size as u32);
        self.run.mem = CacheController::new(
            self.cache.pending_icache.clone(),
            self.cache.pending_dcache.clone(),
            self.cache.extra_pending.clone(),
            self.run.mem_size,
        );
        self.run.faulted = false;

        match assemble(&self.editor.buf.text(), self.run.base_pc) {
            Ok(prog) => {
                // Write directly to RAM (bypass cache) so invalidate() won't discard data
                if let Err(e) = load_words(&mut self.run.mem.ram, self.run.base_pc, &prog.text) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
                if let Err(e) = load_bytes(&mut self.run.mem.ram, prog.data_base, &prog.data) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
                let bss_base = prog.data_base.saturating_add(prog.data.len() as u32);
                if prog.bss_size > 0 {
                    if let Err(e) = zero_bytes(&mut self.run.mem.ram, bss_base, prog.bss_size) {
                        self.console.push_error(e.to_string());
                        self.run.faulted = true;
                        return;
                    }
                }
                self.run.data_base = prog.data_base;
                self.run.mem_view_addr = prog.data_base;
                self.run.mem_region = MemRegion::Data;
                // Invalidate & reset stats so execution starts from cold cache
                self.run.mem.invalidate_all();
                self.run.mem.reset_stats();

                self.run.comments = prog.comments;
                self.run.block_comments = prog.block_comments;
                self.run.labels = prog.labels;
                self.run.exec_counts.clear();
                self.run.exec_trace.clear();
                self.run.reg_age = [255u8; 32];
                self.run.reg_last_write_pc = [None; 32];
                self.editor.label_to_line = prog.label_to_line;
                self.editor.line_to_addr = prog.line_addrs;
                self.editor.last_ok_text = Some(prog.text.clone());
                self.editor.last_ok_data = Some(prog.data.clone());
                self.editor.last_ok_data_base = Some(prog.data_base);
                self.editor.last_ok_bss_size = Some(prog.bss_size);
                self.run.imem_scroll = 0;
                self.run.hover_imem_addr = None;

                self.editor.last_assemble_msg = Some(format!(
                    "Assembled {} instructions, {} data bytes, {} bss bytes.",
                    prog.text.len(),
                    prog.data.len(),
                    prog.bss_size
                ));
                self.editor.last_compile_ok = Some(true);
                self.editor.diag_line = None;
                self.editor.diag_msg = None;
                self.editor.diag_line_text = None;
            }
            Err(e) => {
                self.editor.diag_line = Some(e.line);
                self.editor.diag_msg = Some(e.msg.clone());
                self.editor.diag_line_text = self.editor.buf.lines.get(e.line).cloned();
                self.editor.last_compile_ok = Some(false);
                self.editor.last_assemble_msg =
                    Some(format!("Assemble error at line {}: {}", e.line + 1, e.msg));
            }
        }
    }

    fn check_assemble(&mut self) {
        use falcon::asm::assemble;
        match assemble(&self.editor.buf.text(), self.run.base_pc) {
            Ok(prog) => {
                self.editor.last_ok_text = Some(prog.text.clone());
                self.editor.last_ok_data = Some(prog.data.clone());
                self.editor.last_ok_data_base = Some(prog.data_base);
                self.editor.last_ok_bss_size = Some(prog.bss_size);
                self.editor.last_ok_comments = prog.comments;
                self.editor.last_ok_block_comments = prog.block_comments;
                self.editor.last_ok_labels = prog.labels.clone();
                self.editor.label_to_line = prog.label_to_line;
                self.editor.line_to_addr = prog.line_addrs;
                self.editor.last_assemble_msg = Some(format!(
                    "OK: {} instructions, {} data bytes, {} bss bytes",
                    prog.text.len(),
                    prog.data.len(),
                    prog.bss_size
                ));
                self.editor.last_compile_ok = Some(true);
                self.editor.diag_line = None;
                self.editor.diag_msg = None;
                self.editor.diag_line_text = None;
            }
            Err(e) => {
                self.editor.diag_line = Some(e.line);
                self.editor.diag_msg = Some(e.msg.clone());
                self.editor.diag_line_text = self.editor.buf.lines.get(e.line).cloned();
                self.editor.last_compile_ok = Some(false);
                let line = e.line + 1;
                let text = self.editor.diag_line_text.as_deref().unwrap_or("");
                let err = self.editor.diag_msg.as_deref().unwrap_or("");
                self.editor.last_assemble_msg =
                    Some(format!("Error line {}: {} ({})", line, text, err));
            }
        }
        self.editor.dirty = false;
    }

    fn load_last_ok_program(&mut self) {
        // ELF path: re-parse the original bytes so all segments are restored correctly.
        if let Some(elf_bytes) = self.editor.last_ok_elf_bytes.clone() {
            self.load_binary(&elf_bytes);
            return;
        }

        use falcon::program::{load_bytes, load_words, zero_bytes};
        if let (Some(ref text), Some(ref data), Some(data_base)) = (
            self.editor.last_ok_text.as_ref(),
            self.editor.last_ok_data.as_ref(),
            self.editor.last_ok_data_base,
        ) {
            self.run.prev_x = self.run.cpu.x;
            self.run.mem_size = self.ram_override.unwrap_or(128 * 1024);
            self.run.cpu = Cpu::default();
            self.run.cpu.pc = self.run.base_pc;
            self.run.prev_pc = self.run.cpu.pc;
            self.run.cpu.write(2, self.run.mem_size as u32);
            self.run.mem = CacheController::new(
                self.cache.pending_icache.clone(),
                self.cache.pending_dcache.clone(),
                self.cache.extra_pending.clone(),
                self.run.mem_size,
            );
            self.run.faulted = false;

            // Write directly to RAM (bypass cache) so invalidate() won't discard data
            if let Err(e) = load_words(&mut self.run.mem.ram, self.run.base_pc, text) {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                return;
            }
            if let Err(e) = load_bytes(&mut self.run.mem.ram, data_base, data) {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                return;
            }
            if let Some(bss) = self.editor.last_ok_bss_size {
                if bss > 0 {
                    let bss_base = data_base.saturating_add(data.len() as u32);
                    if let Err(e) = zero_bytes(&mut self.run.mem.ram, bss_base, bss) {
                        self.console.push_error(e.to_string());
                        self.run.faulted = true;
                        return;
                    }
                }
            }

            self.run.data_base = data_base;
            self.run.mem_view_addr = data_base;
            self.run.mem_region = MemRegion::Data;
            self.run.mem.invalidate_all();
            self.run.mem.reset_stats();

            self.run.comments = self.editor.last_ok_comments.clone();
            self.run.block_comments = self.editor.last_ok_block_comments.clone();
            self.run.labels = self.editor.last_ok_labels.clone();

            let bss_sz = self.editor.last_ok_bss_size.unwrap_or(0);
            self.editor.last_assemble_msg = Some(format!(
                "Loaded last successful build: {} instructions, {} data bytes, {} bss bytes.",
                text.len(),
                data.len(),
                bss_sz
            ));
            self.run.imem_scroll = 0;
            self.run.hover_imem_addr = None;
        }
    }

    pub(super) fn restart_simulation(&mut self) {
        self.run.is_running = false;
        self.run.faulted = false;
        self.run.cpu.ebreak_hit = false;
        self.run.reg_last_write_pc = [None; 32];
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.run.mem_access_log.clear();
        self.load_last_ok_program();
    }

    pub(super) fn load_binary(&mut self, bytes: &[u8]) {
        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = self.ram_override.unwrap_or(16 * 1024 * 1024); // default 16 MB for ELF (heap support)
        self.run.cpu = Cpu::default();
        self.run.cpu.write(2, self.run.mem_size as u32);
        self.run.mem = CacheController::new(
            self.cache.pending_icache.clone(),
            self.cache.pending_dcache.clone(),
            self.cache.extra_pending.clone(),
            self.run.mem_size,
        );
        self.run.faulted = false;

        // ── Detect format and load ───────────────────────────────────────
        if bytes.len() >= 4 && &bytes[0..4] == b"\x7fELF" {
            // ── ELF32 LE RISC-V ─────────────────────────────────────────
            let info = match falcon::program::load_elf(bytes, &mut self.run.mem.ram) {
                Ok(i) => i,
                Err(e) => {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
            };

            self.run.cpu.pc        = info.entry;
            self.run.prev_pc       = info.entry;
            self.run.base_pc       = info.text_base;
            self.run.data_base     = info.data_base;
            self.run.mem_view_addr = info.data_base;
            self.run.mem_region    = crate::ui::app::MemRegion::Data;
            self.run.mem.invalidate_all();
            self.run.mem.reset_stats();

            // Populate labels and sections viewer from ELF symbol table
            self.run.labels = info.symbols;
            self.run.elf_sections = info.sections;
            self.run.cpu.heap_break = info.heap_start;

            let mut words = Vec::with_capacity(info.text_bytes.len() / 4);
            for chunk in info.text_bytes.chunks(4) {
                let mut b = [0u8; 4];
                for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
                words.push(u32::from_le_bytes(b));
            }
            let entry     = info.entry;
            let data_base = info.data_base;
            self.editor.last_ok_text       = Some(words);
            self.editor.last_ok_data       = Some(Vec::new());
            self.editor.last_ok_data_base  = Some(data_base);
            self.editor.last_ok_bss_size   = Some(0);
            self.editor.last_ok_elf_bytes  = Some(bytes.to_vec());
            self.editor.last_assemble_msg  = Some(format!(
                "Loaded ELF: {} bytes, entry 0x{entry:08X} ({} instructions)",
                info.total_bytes,
                self.editor.last_ok_text.as_ref().map(|v| v.len()).unwrap_or(0),
            ));
        } else {
            // ── FALC or flat binary ──────────────────────────────────────
            self.run.elf_sections = Vec::new();
            use falcon::program::{load_bytes, zero_bytes};
            let (text_bytes, data_bytes, bss_size): (Vec<u8>, Vec<u8>, u32) =
                if bytes.len() >= 16 && &bytes[0..4] == b"FALC" {
                    let text_sz = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
                    let data_sz = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
                    let bss_sz  = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
                    let body    = &bytes[16..];
                    if body.len() < text_sz + data_sz {
                        self.console.push_error("Binary truncated or corrupt");
                        self.run.faulted = true;
                        return;
                    }
                    (body[..text_sz].to_vec(), body[text_sz..text_sz + data_sz].to_vec(), bss_sz)
                } else {
                    (bytes.to_vec(), Vec::new(), 0)
                };

            if let Err(e) = load_bytes(&mut self.run.mem.ram, self.run.base_pc, &text_bytes) {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                return;
            }
            if !data_bytes.is_empty() {
                if let Err(e) = load_bytes(&mut self.run.mem.ram, self.run.data_base, &data_bytes) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
            }
            if bss_size > 0 {
                let bss_base = self.run.data_base + data_bytes.len() as u32;
                if let Err(e) = zero_bytes(&mut self.run.mem.ram, bss_base, bss_size) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
            }

            self.run.cpu.pc  = self.run.base_pc;
            self.run.prev_pc = self.run.base_pc;
            self.run.mem.invalidate_all();
            self.run.mem.reset_stats();

            // Heap starts right after BSS, 16-byte aligned
            let bss_end = self.run.data_base
                .wrapping_add(data_bytes.len() as u32)
                .wrapping_add(bss_size);
            self.run.cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

            let mut words = Vec::with_capacity(text_bytes.len() / 4);
            for chunk in text_bytes.chunks(4) {
                let mut b = [0u8; 4];
                for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
                words.push(u32::from_le_bytes(b));
            }
            let total = text_bytes.len() + data_bytes.len();
            self.editor.last_ok_text       = Some(words);
            self.editor.last_ok_data       = Some(data_bytes);
            self.editor.last_ok_data_base  = Some(self.run.data_base);
            self.editor.last_ok_bss_size   = Some(bss_size);
            self.editor.last_ok_elf_bytes  = None;
            self.editor.last_assemble_msg  = Some(format!(
                "Loaded binary: {} bytes ({} instructions)",
                total,
                self.editor.last_ok_text.as_ref().map(|v| v.len()).unwrap_or(0),
            ));
        }

        self.editor.last_compile_ok    = Some(true);
        self.editor.diag_line          = None;
        self.editor.diag_msg           = None;
        self.editor.diag_line_text     = None;
        self.run.imem_scroll           = 0;
        self.run.hover_imem_addr       = None;
    }

    /// Commit the current numeric edit_buf into pending config for the selected level.
    pub(super) fn commit_cache_edit(&mut self) {
        if let Some((is_icache, field)) = self.cache.edit_field {
            self.cache.config_error = None;
            self.cache.config_status = None;
            if field.is_numeric() {
                let s = self.cache.edit_buf.trim().to_string();
                let cfg = self.selected_level_pending_cfg_mut(is_icache);
                match field {
                    ConfigField::Size => { if let Ok(v) = s.parse::<usize>() { cfg.size = v; } }
                    ConfigField::LineSize => { if let Ok(v) = s.parse::<usize>() { cfg.line_size = v; } }
                    ConfigField::Associativity => { if let Ok(v) = s.parse::<usize>() { cfg.associativity = v.max(1); } }
                    ConfigField::HitLatency => { if let Ok(v) = s.parse::<u64>() { cfg.hit_latency = v.max(1); } }
                    ConfigField::MissPenalty => { if let Ok(v) = s.parse::<u64>() { cfg.miss_penalty = v; } }
                    ConfigField::AssocPenalty => { if let Ok(v) = s.parse::<u64>() { cfg.assoc_penalty = v; } }
                    ConfigField::TransferWidth => { if let Ok(v) = s.parse::<u32>() { cfg.transfer_width = v.max(1); } }
                    _ => {}
                }
            }
        }
    }

    /// Cycle an enum-typed config field (forward=true → next option).
    pub(super) fn cycle_cache_field(&mut self, is_icache: bool, field: ConfigField, forward: bool) {
        use crate::falcon::cache::{ReplacementPolicy, WriteAllocPolicy, WritePolicy};
        self.cache.config_error = None;
        self.cache.config_status = None;
        let cfg = self.selected_level_pending_cfg_mut(is_icache);
        match field {
            ConfigField::Replacement => {
                cfg.replacement = if forward {
                    match cfg.replacement {
                        ReplacementPolicy::Lru => ReplacementPolicy::Mru,
                        ReplacementPolicy::Mru => ReplacementPolicy::Fifo,
                        ReplacementPolicy::Fifo => ReplacementPolicy::Random,
                        ReplacementPolicy::Random => ReplacementPolicy::Lfu,
                        ReplacementPolicy::Lfu => ReplacementPolicy::Clock,
                        ReplacementPolicy::Clock => ReplacementPolicy::Lru,
                    }
                } else {
                    match cfg.replacement {
                        ReplacementPolicy::Lru => ReplacementPolicy::Clock,
                        ReplacementPolicy::Mru => ReplacementPolicy::Lru,
                        ReplacementPolicy::Fifo => ReplacementPolicy::Mru,
                        ReplacementPolicy::Random => ReplacementPolicy::Fifo,
                        ReplacementPolicy::Lfu => ReplacementPolicy::Random,
                        ReplacementPolicy::Clock => ReplacementPolicy::Lfu,
                    }
                };
            }
            ConfigField::WritePolicy => {
                cfg.write_policy = match cfg.write_policy {
                    WritePolicy::WriteThrough => WritePolicy::WriteBack,
                    WritePolicy::WriteBack => WritePolicy::WriteThrough,
                };
            }
            ConfigField::WriteAlloc => {
                cfg.write_alloc = match cfg.write_alloc {
                    WriteAllocPolicy::WriteAllocate => WriteAllocPolicy::NoWriteAllocate,
                    WriteAllocPolicy::NoWriteAllocate => WriteAllocPolicy::WriteAllocate,
                };
            }
            ConfigField::Inclusion => {
                use crate::falcon::cache::InclusionPolicy;
                cfg.inclusion = if forward {
                    match cfg.inclusion {
                        InclusionPolicy::NonInclusive => InclusionPolicy::Inclusive,
                        InclusionPolicy::Inclusive    => InclusionPolicy::Exclusive,
                        InclusionPolicy::Exclusive    => InclusionPolicy::NonInclusive,
                    }
                } else {
                    match cfg.inclusion {
                        InclusionPolicy::NonInclusive => InclusionPolicy::Exclusive,
                        InclusionPolicy::Inclusive    => InclusionPolicy::NonInclusive,
                        InclusionPolicy::Exclusive    => InclusionPolicy::Inclusive,
                    }
                };
            }
            _ => {}
        }
    }

    /// Current pending config field value as string (for populating edit_buf).
    pub(super) fn cache_field_value_str(&self, is_icache: bool, field: ConfigField) -> String {
        let cfg = self.selected_level_pending_cfg(is_icache);
        match field {
            ConfigField::Size => cfg.size.to_string(),
            ConfigField::LineSize => cfg.line_size.to_string(),
            ConfigField::Associativity => cfg.associativity.to_string(),
            ConfigField::HitLatency => cfg.hit_latency.to_string(),
            ConfigField::MissPenalty => cfg.miss_penalty.to_string(),
            ConfigField::AssocPenalty => cfg.assoc_penalty.to_string(),
            ConfigField::TransferWidth => cfg.transfer_width.to_string(),
            _ => String::new(),
        }
    }

    /// Get the pending config for the selected cache level (immutable).
    /// Level 0 = L1 (is_icache selects I or D); Level 1+ = L2+ (is_icache ignored).
    pub(super) fn selected_level_pending_cfg(&self, is_icache: bool) -> &CacheConfig {
        let level = self.cache.selected_level;
        if level == 0 {
            if is_icache { &self.cache.pending_icache } else { &self.cache.pending_dcache }
        } else if level - 1 < self.cache.extra_pending.len() {
            &self.cache.extra_pending[level - 1]
        } else {
            &self.cache.pending_dcache // fallback
        }
    }

    /// Get the pending config for the selected cache level (mutable).
    pub(super) fn selected_level_pending_cfg_mut(&mut self, is_icache: bool) -> &mut CacheConfig {
        let level = self.cache.selected_level;
        if level == 0 {
            if is_icache { &mut self.cache.pending_icache } else { &mut self.cache.pending_dcache }
        } else if level - 1 < self.cache.extra_pending.len() {
            &mut self.cache.extra_pending[level - 1]
        } else {
            &mut self.cache.pending_dcache // fallback
        }
    }

    /// Add a new extra cache level (L2, L3, …).
    pub(super) fn add_cache_level(&mut self) {
        use crate::falcon::cache::extra_level_presets;
        let cfg = extra_level_presets()[0].clone(); // Small L2 default
        self.cache.extra_pending.push(cfg.clone());
        self.run.mem.add_extra_level(cfg);
        // Select the newly added level
        self.cache.selected_level = self.cache.extra_pending.len(); // 1-based (L1=0)
        // Grow hover_level vec
        self.cache.hover_level.push(false);
    }

    /// Remove the last extra cache level.
    pub(super) fn remove_last_cache_level(&mut self) {
        if !self.cache.extra_pending.is_empty() {
            self.cache.extra_pending.pop();
            self.run.mem.remove_extra_level();
            let max_level = self.cache.extra_pending.len();
            if self.cache.selected_level > max_level {
                self.cache.selected_level = max_level;
            }
            if !self.cache.hover_level.is_empty() {
                self.cache.hover_level.pop();
            }
        }
    }

    // ── Instruction-memory scroll helpers (visual-row units) ─────────────────

    fn imem_in_range(&self, addr: u32) -> bool {
        if let Some(text) = &self.editor.last_ok_text {
            let start = self.run.base_pc;
            let end = start.saturating_add((text.len() as u32).saturating_mul(4));
            addr >= start && addr < end
        } else {
            (addr as usize) < self.run.mem_size.saturating_sub(3)
        }
    }

    /// Total visual rows in the instruction list (block_comment + labels + instruction per addr).
    pub(super) fn imem_total_visual_rows(&self) -> usize {
        let mut count = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) { break; }
            if self.run.block_comments.contains_key(&addr) { count += 1; }
            if let Some(names) = self.run.labels.get(&addr) { count += names.len(); }
            count += 1;
            addr = addr.wrapping_add(4);
        }
        count
    }

    /// Returns (start_addr, header_skip) for the current imem_scroll (visual row offset).
    /// header_skip = how many block_comment/label rows to skip at the top of start_addr's block.
    pub(super) fn imem_addr_skip_for_scroll(&self) -> (u32, usize) {
        let scroll = self.run.imem_scroll;
        let base = self.run.base_pc;
        let mut vrow = 0usize;
        let mut addr = base;
        loop {
            if !self.imem_in_range(addr) { return (base, 0); }
            let bc = if self.run.block_comments.contains_key(&addr) { 1 } else { 0 };
            let lbls = self.run.labels.get(&addr).map_or(0, |v| v.len());
            let block = bc + lbls + 1;
            if vrow + block > scroll {
                return (addr, scroll - vrow);
            }
            vrow += block;
            addr = addr.wrapping_add(4);
        }
    }

    /// Visual row of the current PC within the full instruction list.
    pub(super) fn imem_visual_row_of_pc(&self) -> Option<usize> {
        let pc = self.run.cpu.pc;
        if pc < self.run.base_pc { return None; }
        let mut vrow = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) { return None; }
            if self.run.block_comments.contains_key(&addr) { vrow += 1; }
            if let Some(names) = self.run.labels.get(&addr) { vrow += names.len(); }
            if addr == pc { return Some(vrow); }
            vrow += 1;
            addr = addr.wrapping_add(4);
        }
    }

    /// Ensure PC is visible in the imem panel, updating imem_scroll if needed.
    pub(super) fn ensure_pc_visible_in_imem(&mut self) {
        let visible = self.run.imem_inner_height.get();
        if visible == 0 { return; }
        let Some(pc_vrow) = self.imem_visual_row_of_pc() else { return; };
        let scroll = self.run.imem_scroll;
        if pc_vrow < scroll {
            // PC above view
            self.run.imem_scroll = pc_vrow.saturating_sub(2);
        } else if pc_vrow + 1 >= scroll + visible {
            // PC at or below bottom edge
            self.run.imem_scroll = pc_vrow.saturating_sub(visible.saturating_sub(3));
        }
    }

    fn tick(&mut self) {
        if let Some(t) = self.splash_start {
            if t.elapsed().as_secs_f64() >= 4.0 {
                self.splash_start = None;
            }
        }

        if self.run.is_running {
            match self.run.speed {
                RunSpeed::X1 => {
                    if self.run.last_step_time.elapsed() >= self.run.step_interval {
                        self.single_step();
                        self.run.last_step_time = Instant::now();
                    }
                }
                RunSpeed::X2 => {
                    if self.run.last_step_time.elapsed() >= Duration::from_millis(20) {
                        self.single_step();
                        self.run.last_step_time = Instant::now();
                    }
                }
                RunSpeed::X4 => {
                    // Multiple steps per tick for high throughput (~400 steps/sec at 100 ticks/sec)
                    for _ in 0..4 {
                        if !self.run.is_running { break; }
                        self.single_step();
                    }
                }
                RunSpeed::X8 => {
                    for _ in 0..8 {
                        if !self.run.is_running { break; }
                        self.single_step();
                    }
                }
                RunSpeed::Instant => {
                    // Spend up to 8 ms executing per tick — leaves UI responsive at 60 fps
                    let budget = Duration::from_millis(8);
                    let start = Instant::now();
                    while self.run.is_running && start.elapsed() < budget {
                        self.single_step();
                    }
                }
            }
        }
        // Scroll instruction list to follow PC (skipped in Instant to avoid pointless churn)
        if self.run.is_running && !matches!(self.run.speed, RunSpeed::Instant) {
            self.ensure_pc_visible_in_imem();
        }
        if matches!(self.tab, Tab::Editor) && self.editor.dirty {
            if let Some(t) = self.editor.last_edit_at {
                if t.elapsed() >= self.editor.auto_check_delay {
                    self.check_assemble();
                    if self.editor.last_compile_ok == Some(true) {
                        self.load_last_ok_program();
                    }
                }
            }
        }
    }

    pub(super) fn single_step(&mut self) {
        self.run.prev_x = self.run.cpu.x;
        self.run.prev_f = self.run.cpu.f;
        self.run.prev_pc = self.run.cpu.pc;
        let step_pc = self.run.cpu.pc;

        // Halt gracefully when PC leaves the loaded text segment.
        if !self.imem_in_range(step_pc) {
            self.console.push_error(format!(
                "Execution reached 0x{step_pc:08X}, outside the loaded program. \
                 Add `li a7, 93; ecall` to terminate cleanly."
            ));
            self.run.faulted = true;
            return;
        }

        // Classify instruction BEFORE stepping (registers still hold pre-step values)
        let cpi_cycles = classify_cpi_cycles(step_pc, &self.run.cpu, &self.run.mem, &self.run.cpi_config);
        let mem_access = self.run.mem.peek32(step_pc)
            .ok()
            .and_then(|w| classify_mem_access(w, &self.run.cpu));

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            falcon::exec::step(&mut self.run.cpu, &mut self.run.mem, &mut self.console)
        }));
        let alive = match res {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                false
            }
            Err(_) => {
                self.run.faulted = true;
                false
            }
        };
        self.run.mem.add_instruction_cycles(cpi_cycles);
        self.run.mem.snapshot_stats();

        // Track execution statistics
        *self.run.exec_counts.entry(step_pc).or_insert(0) += 1;
        let disasm = {
            let word = self.run.mem.peek32(step_pc).unwrap_or(0);
            match falcon::decoder::decode(word) {
                Ok(instr) => format!("{instr:?}"),
                Err(_) => format!("0x{word:08x}"),
            }
        };
        self.run.exec_trace.push_back((step_pc, disasm));
        if self.run.exec_trace.len() > 200 {
            self.run.exec_trace.pop_front();
        }

        // Update memory access log (age existing entries, insert new if load/store)
        for entry in &mut self.run.mem_access_log { entry.2 = entry.2.saturating_add(1); }
        self.run.mem_access_log.retain(|e| e.2 < 3);
        if let Some((addr, size)) = mem_access {
            self.run.mem_access_log.push((addr, size, 0));
        }

        // Update register age (fading highlight) and track last write PC
        for i in 0..32usize {
            if self.run.cpu.x[i] != self.run.prev_x[i] {
                self.run.reg_age[i] = 0;
                self.run.reg_last_write_pc[i] = Some(step_pc);
            } else {
                self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
            }
        }
        // Update float register age
        for i in 0..32usize {
            if self.run.cpu.f[i] != self.run.prev_f[i] {
                self.run.f_age[i] = 0;
                self.run.f_last_write_pc[i] = Some(step_pc);
            } else {
                self.run.f_age[i] = self.run.f_age[i].saturating_add(1).min(8);
            }
        }
        // Auto-follow SP when Stack region is active in the memory view
        if self.run.mem_region == crate::ui::app::MemRegion::Stack {
            let sp = self.run.cpu.x[2];
            self.run.mem_view_addr = sp & !(self.run.mem_view_bytes - 1);
        }

        // Check breakpoints: stop if the new PC is a breakpoint
        if alive && self.run.breakpoints.contains(&self.run.cpu.pc) {
            self.run.is_running = false;
        }
        if !alive {
            self.run.is_running = false;
            if !self.console.reading {
                self.run.faulted = self.run.cpu.exit_code.is_none() && !self.run.cpu.ebreak_hit;
            }
        }
        // Keep PC visible (single-step case — running case handled in tick())
        if !self.run.is_running {
            self.ensure_pc_visible_in_imem();
        }
    }

    /// Jump editor cursor to the definition of the label under the cursor.
    pub(super) fn goto_label_definition(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() { return; }
        let line = &self.editor.buf.lines[row];
        let word = word_at(line, col);
        if word.is_empty() { return; }
        if let Some(&target_line) = self.editor.label_to_line.get(&word) {
            self.editor.buf.cursor_row = target_line;
            self.editor.buf.cursor_col = 0;
        }
    }

    /// Select next occurrence of the word currently under the cursor.
    pub(super) fn select_next_occurrence(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() { return; }
        let word = word_at(&self.editor.buf.lines[row], col);
        if word.is_empty() { return; }
        let lines = &self.editor.buf.lines;
        let total = lines.len();
        // Search from after current cursor position
        for offset in 1..=(total * lines[0].len().max(80) + 1) {
            let _ = offset; // silence lint
            break; // use proper search below
        }
        // Find next occurrence after (row, col+word.len())
        let start_col = col + 1;
        let positions: Vec<(usize, usize)> = lines.iter().enumerate()
            .flat_map(|(r, l)| {
                let mut found = Vec::new();
                let mut search = l.as_str();
                let mut byte_off = 0;
                while let Some(idx) = search.find(&word) {
                    let char_col = Editor::char_count(&l[..byte_off + idx]);
                    found.push((r, char_col));
                    byte_off += idx + word.len();
                    search = &l[byte_off..];
                }
                found
            })
            .collect();
        if positions.is_empty() { return; }
        // Find the next position after current cursor
        let next = positions.iter()
            .find(|&&(r, c)| r > row || (r == row && c >= start_col))
            .or_else(|| positions.first());
        if let Some(&(r, c)) = next {
            self.editor.buf.cursor_row = r;
            self.editor.buf.cursor_col = c;
            // Select the word via the selection_anchor API
            self.editor.buf.selection_anchor = Some((r, c));
            self.editor.buf.cursor_col = c + Editor::char_count(&word);
        }
    }
}

/// Decode a raw instruction word and return the memory address + byte size that
/// the instruction accesses (load or store), or `None` for non-memory instructions.
/// Uses pre-step register values from `cpu`.
fn classify_mem_access(word: u32, cpu: &crate::falcon::Cpu) -> Option<(u32, u32)> {
    let opcode = word & 0x7F;
    let funct3 = (word >> 12) & 0x7;
    let rs1 = ((word >> 15) & 0x1F) as usize;

    match opcode {
        // LOAD (lb lh lw lbu lhu) and LOAD-FP (flw)
        0x03 | 0x07 => {
            let imm = ((word as i32) >> 20) as u32; // sign-extend bits[31:20]
            let addr = cpu.x[rs1].wrapping_add(imm);
            let size: u32 = match funct3 {
                0 | 4 => 1,  // lb / lbu
                1 | 5 => 2,  // lh / lhu
                2     => 4,  // lw / flw
                _     => return None,
            };
            Some((addr, size))
        }
        // STORE (sb sh sw) and STORE-FP (fsw)
        0x23 | 0x27 => {
            let imm_lo = (word >> 7) & 0x1F;
            let imm_hi = (word >> 25) & 0x7F;
            let imm = (((imm_hi << 5) | imm_lo) as i32).wrapping_shl(20).wrapping_shr(20) as u32;
            let addr = cpu.x[rs1].wrapping_add(imm);
            let size: u32 = match funct3 {
                0 => 1, // sb / fsb (unlikely but valid)
                1 => 2, // sh
                2 => 4, // sw / fsw
                _ => return None,
            };
            Some((addr, size))
        }
        _ => None,
    }
}

/// Classify the instruction at `pc` and return its base CPI cycles.
/// Branch taken/not-taken is determined from pre-step register values.
fn classify_cpi_cycles(pc: u32, cpu: &crate::falcon::Cpu, mem: &crate::falcon::CacheController, cpi: &CpiConfig) -> u64 {
    use crate::falcon::instruction::Instruction::*;
    let word = match mem.peek32(pc) {
        Ok(w) => w,
        Err(_) => return 1,
    };
    match crate::falcon::decoder::decode(word) {
        Ok(Add  { .. } | Sub   { .. } | And   { .. } | Or  { .. } | Xor  { .. } |
           Sll  { .. } | Srl   { .. } | Sra   { .. } | Slt { .. } | Sltu { .. } |
           Addi { .. } | Andi  { .. } | Ori   { .. } | Xori{ .. } | Slti { .. } |
           Sltiu{ .. } | Slli  { .. } | Srli  { .. } | Srai{ .. } |
           Lui  { .. } | Auipc { .. }) => cpi.alu,
        Ok(Mul  { .. } | Mulh  { .. } | Mulhsu{ .. } | Mulhu{ .. }) => cpi.mul,
        Ok(Div  { .. } | Divu  { .. } | Rem   { .. } | Remu { .. }) => cpi.div,
        Ok(Lb   { .. } | Lh    { .. } | Lw    { .. } | Lbu  { .. } | Lhu  { .. }) => cpi.load,
        Ok(Sb   { .. } | Sh    { .. } | Sw    { .. }) => cpi.store,
        Ok(Jal  { .. } | Jalr  { .. }) => cpi.jump,
        Ok(Ecall | Ebreak | Halt) => cpi.system,
        Ok(Beq  { rs1, rs2, .. }) => if cpu.x[rs1 as usize] == cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bne  { rs1, rs2, .. }) => if cpu.x[rs1 as usize] != cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Blt  { rs1, rs2, .. }) => if (cpu.x[rs1 as usize] as i32) <  (cpu.x[rs2 as usize] as i32) { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bge  { rs1, rs2, .. }) => if (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32) { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bltu { rs1, rs2, .. }) => if cpu.x[rs1 as usize] <  cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bgeu { rs1, rs2, .. }) => if cpu.x[rs1 as usize] >= cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Flw  { .. } | Fsw    { .. } |
           FaddS{ .. } | FsubS  { .. } | FmulS  { .. } | FdivS   { .. } | FsqrtS { .. } |
           FminS{ .. } | FmaxS  { .. } | FsgnjS { .. } | FsgnjnS { .. } | FsgnjxS{ .. } |
           FeqS { .. } | FltS   { .. } | FleS   { .. } |
           FcvtWS{..}  | FcvtWuS{ .. } | FcvtSW { .. } | FcvtSWu { .. } |
           FmvXW{ .. } | FmvWX  { .. } | FclassS{ .. } |
           FmaddS{..}  | FmsubS { .. } | FnmsubS{ .. } | FnmaddS { .. }) => cpi.fp,
        _ => 1,
    }
}

/// Classify instruction for display (doesn't need mutable mem, uses word directly).
pub(super) fn classify_cpi_for_display(word: u32, _addr: u32, cpu: &crate::falcon::Cpu, cpi: &CpiConfig) -> u64 {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(Add  { .. } | Sub   { .. } | And   { .. } | Or  { .. } | Xor  { .. } |
           Sll  { .. } | Srl   { .. } | Sra   { .. } | Slt { .. } | Sltu { .. } |
           Addi { .. } | Andi  { .. } | Ori   { .. } | Xori{ .. } | Slti { .. } |
           Sltiu{ .. } | Slli  { .. } | Srli  { .. } | Srai{ .. } |
           Lui  { .. } | Auipc { .. }) => cpi.alu,
        Ok(Mul  { .. } | Mulh  { .. } | Mulhsu{ .. } | Mulhu{ .. }) => cpi.mul,
        Ok(Div  { .. } | Divu  { .. } | Rem   { .. } | Remu { .. }) => cpi.div,
        Ok(Lb   { .. } | Lh    { .. } | Lw    { .. } | Lbu  { .. } | Lhu  { .. }) => cpi.load,
        Ok(Sb   { .. } | Sh    { .. } | Sw    { .. }) => cpi.store,
        Ok(Jal  { .. } | Jalr  { .. }) => cpi.jump,
        Ok(Ecall | Ebreak | Halt) => cpi.system,
        Ok(Beq  { rs1, rs2, .. }) => if cpu.x[rs1 as usize] == cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bne  { rs1, rs2, .. }) => if cpu.x[rs1 as usize] != cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Blt  { rs1, rs2, .. }) => if (cpu.x[rs1 as usize] as i32) <  (cpu.x[rs2 as usize] as i32) { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bge  { rs1, rs2, .. }) => if (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32) { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bltu { rs1, rs2, .. }) => if cpu.x[rs1 as usize] <  cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Bgeu { rs1, rs2, .. }) => if cpu.x[rs1 as usize] >= cpu.x[rs2 as usize] { cpi.branch_taken } else { cpi.branch_not_taken },
        Ok(Flw  { .. } | Fsw    { .. } |
           FaddS{ .. } | FsubS  { .. } | FmulS  { .. } | FdivS   { .. } | FsqrtS { .. } |
           FminS{ .. } | FmaxS  { .. } | FsgnjS { .. } | FsgnjnS { .. } | FsgnjxS{ .. } |
           FeqS { .. } | FltS   { .. } | FleS   { .. } |
           FcvtWS{..}  | FcvtWuS{ .. } | FcvtSW { .. } | FcvtSWu { .. } |
           FmvXW{ .. } | FmvWX  { .. } | FclassS{ .. } |
           FmaddS{..}  | FmsubS { .. } | FnmsubS{ .. } | FnmaddS { .. }) => cpi.fp,
        _ => 1,
    }
}

/// Return the CPI class label for an instruction word (for display purposes).
pub(super) fn cpi_class_label(word: u32) -> &'static str {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(Add  { .. } | Sub   { .. } | And   { .. } | Or  { .. } | Xor  { .. } |
           Sll  { .. } | Srl   { .. } | Sra   { .. } | Slt { .. } | Sltu { .. } |
           Addi { .. } | Andi  { .. } | Ori   { .. } | Xori{ .. } | Slti { .. } |
           Sltiu{ .. } | Slli  { .. } | Srli  { .. } | Srai{ .. } |
           Lui  { .. } | Auipc { .. }) => "ALU",
        Ok(Mul  { .. } | Mulh  { .. } | Mulhsu{ .. } | Mulhu{ .. }) => "MUL",
        Ok(Div  { .. } | Divu  { .. } | Rem   { .. } | Remu { .. }) => "DIV",
        Ok(Lb   { .. } | Lh    { .. } | Lw    { .. } | Lbu  { .. } | Lhu  { .. }) => "Load",
        Ok(Sb   { .. } | Sh    { .. } | Sw    { .. }) => "Store",
        Ok(Jal  { .. } | Jalr  { .. }) => "Jump",
        Ok(Ecall | Ebreak | Halt) => "System",
        Ok(Beq  { .. } | Bne   { .. } | Blt   { .. } |
           Bge  { .. } | Bltu  { .. } | Bgeu  { .. }) => "Branch",
        Ok(Flw  { .. } | Fsw    { .. } |
           FaddS{ .. } | FsubS  { .. } | FmulS  { .. } | FdivS   { .. } | FsqrtS { .. } |
           FminS{ .. } | FmaxS  { .. } | FsgnjS { .. } | FsgnjnS { .. } | FsgnjxS{ .. } |
           FeqS { .. } | FltS   { .. } | FleS   { .. } |
           FcvtWS{..}  | FcvtWuS{ .. } | FcvtSW { .. } | FcvtSWu { .. } |
           FmvXW{ .. } | FmvWX  { .. } | FclassS{ .. } |
           FmaddS{..}  | FmsubS { .. } | FnmsubS{ .. } | FnmaddS { .. }) => "FP",
        _ => "?",
    }
}

#[cfg(unix)]
pub fn run(terminal: &mut DefaultTerminal, mut app: App, quit_flag: Arc<AtomicBool>) -> io::Result<()> {
    run_inner(terminal, &mut app, Some(&quit_flag))
}

#[cfg(not(unix))]
pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    run_inner(terminal, &mut app, None::<&AtomicBool>)
}

fn run_inner(terminal: &mut DefaultTerminal, app: &mut App, #[allow(unused)] quit_flag: Option<&AtomicBool>) -> io::Result<()> {
    execute!(terminal.backend_mut(), EnableMouseCapture, EnableBracketedPaste)?;
    let mut last_draw = Instant::now();
    loop {
        #[cfg(unix)]
        if quit_flag.map_or(false, |f| f.load(Ordering::Relaxed)) {
            break;
        }

        match event::poll(Duration::from_millis(10)) {
            Ok(true) => {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if handle_key(app, key)? {
                            break;
                        }
                    }
                    Ok(Event::Mouse(me)) => {
                        let size = terminal.size()?;
                        let area = Rect::new(0, 0, size.width, size.height);
                        handle_mouse(app, me, area);
                        if app.should_quit {
                            break;
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if matches!(app.tab, Tab::Editor) {
                            use crate::ui::input::keyboard::paste_from_terminal;
                            app.last_bracketed_paste = Some(Instant::now());
                            paste_from_terminal(app, &text);
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(false) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }

        if app.should_quit {
            break;
        }
        app.tick();
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, app))?;
            last_draw = Instant::now();
        }
    }
    execute!(terminal.backend_mut(), DisableMouseCapture, DisableBracketedPaste)?;
    Ok(())
}
