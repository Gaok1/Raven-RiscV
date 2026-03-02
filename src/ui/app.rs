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
    pub(super) view_h_scroll: usize,
    pub(super) hover_reset: bool,
    pub(super) hover_pause: bool,
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
}

impl Default for CpiConfig {
    fn default() -> Self {
        Self {
            alu: 1, mul: 3, div: 20,
            load: 0, store: 0,
            branch_taken: 3, branch_not_taken: 1,
            jump: 2, system: 10,
        }
    }
}

impl CpiConfig {
    pub(super) fn field_names() -> &'static [&'static str] {
        &["ALU", "MUL", "DIV", "Load+", "Store+", "Branch-T", "Branch-NT", "Jump", "System"]
    }

    pub(super) fn get(&self, idx: usize) -> u64 {
        match idx {
            0 => self.alu, 1 => self.mul, 2 => self.div,
            3 => self.load, 4 => self.store,
            5 => self.branch_taken, 6 => self.branch_not_taken,
            7 => self.jump, 8 => self.system,
            _ => 0,
        }
    }

    pub(super) fn set(&mut self, idx: usize, val: u64) {
        match idx {
            0 => self.alu = val, 1 => self.mul = val, 2 => self.div = val,
            3 => self.load = val, 4 => self.store = val,
            5 => self.branch_taken = val, 6 => self.branch_not_taken = val,
            7 => self.jump = val, 8 => self.system = val,
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
    /// Time-budgeted bulk (8 ms/frame) — effectively instant
    Instant,
}

impl RunSpeed {
    /// Cycle to the next speed level (wraps around).
    pub(super) fn cycle(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X4,
            Self::X4 => Self::Instant,
            Self::Instant => Self::X1,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
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
    pub(super) imem_scroll: usize,
    pub(super) hover_imem_addr: Option<u32>,
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
}

#[derive(PartialEq, Eq, Copy, Clone, Default)]
pub(super) enum DocsPage {
    #[default]
    InstrRef,
    RunGuide,
}

pub(super) struct DocsState {
    pub(super) scroll: usize,
    pub(super) search_open: bool,
    pub(super) search_query: String,
    pub(super) page: DocsPage,
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
    pub fn new() -> Self {
        let mut cpu = Cpu::default();
        let base_pc = 0x0000_0000;
        cpu.pc = base_pc;
        let mem_size = 128 * 1024;
        cpu.write(2, mem_size as u32 - 4);
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
                cpi_config: CpiConfig::default(),
                show_exec_count: true,
                show_instr_type: true,
            },
            docs: DocsState { scroll: 0, search_open: false, search_query: String::new(), page: DocsPage::InstrRef },
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
                hover_reset: false,
                hover_pause: false,
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
        }
    }

    pub(super) fn assemble_and_load(&mut self) {
        use falcon::asm::assemble;
        use falcon::program::{load_bytes, load_words, zero_bytes};

        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = 128 * 1024;
        self.run.cpu = Cpu::default();
        self.run.cpu.pc = self.run.base_pc;
        self.run.prev_pc = self.run.cpu.pc;
        self.run.cpu.write(2, self.run.mem_size as u32 - 4);
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
        use falcon::program::{load_bytes, load_words, zero_bytes};
        if let (Some(ref text), Some(ref data), Some(data_base)) = (
            self.editor.last_ok_text.as_ref(),
            self.editor.last_ok_data.as_ref(),
            self.editor.last_ok_data_base,
        ) {
            self.run.prev_x = self.run.cpu.x;
            self.run.mem_size = 128 * 1024;
            self.run.cpu = Cpu::default();
            self.run.cpu.pc = self.run.base_pc;
            self.run.prev_pc = self.run.cpu.pc;
            self.run.cpu.write(2, self.run.mem_size as u32 - 4);
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
        self.run.reg_last_write_pc = [None; 32];
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.load_last_ok_program();
    }

    pub(super) fn load_binary(&mut self, bytes: &[u8]) {
        use falcon::program::{load_bytes, zero_bytes};
        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = 128 * 1024;
        self.run.cpu = Cpu::default();
        self.run.cpu.pc = self.run.base_pc;
        self.run.prev_pc = self.run.cpu.pc;
        self.run.cpu.write(2, self.run.mem_size as u32 - 4);
        self.run.mem = CacheController::new(
            self.cache.pending_icache.clone(),
            self.cache.pending_dcache.clone(),
            self.cache.extra_pending.clone(),
            self.run.mem_size,
        );
        self.run.faulted = false;

        // Parse FALC header if present; otherwise treat whole file as legacy flat text.
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
                // Legacy flat binary: everything is text, no data/bss.
                (bytes.to_vec(), Vec::new(), 0)
            };

        // Load sections into RAM directly (bypass cache).
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
        self.run.mem.invalidate_all();
        self.run.mem.reset_stats();

        // Build instruction word list from text section only (not data).
        let mut words = Vec::with_capacity(text_bytes.len() / 4);
        for chunk in text_bytes.chunks(4) {
            let mut b = [0u8; 4];
            for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
            words.push(u32::from_le_bytes(b));
        }
        let total = text_bytes.len() + data_bytes.len();
        self.editor.last_ok_text = Some(words);
        self.editor.last_ok_data = Some(data_bytes);
        self.editor.last_ok_data_base = Some(self.run.data_base);
        self.editor.last_ok_bss_size = Some(bss_size);
        self.editor.last_assemble_msg = Some(format!(
            "Loaded binary: {} bytes ({} instructions)",
            total,
            self.editor.last_ok_text.as_ref().map(|v| v.len()).unwrap_or(0)
        ));
        self.editor.last_compile_ok = Some(true);
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;
        self.run.imem_scroll = 0;
        self.run.hover_imem_addr = None;
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

    fn tick(&mut self) {
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
            if self.run.cpu.pc >= self.run.base_pc {
                let pc_idx = ((self.run.cpu.pc - self.run.base_pc) / 4) as usize;
                self.run.imem_scroll = pc_idx.saturating_sub(2);
            } else {
                self.run.imem_scroll = 0;
            }
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
        self.run.prev_pc = self.run.cpu.pc;
        let step_pc = self.run.cpu.pc;

        // Classify instruction BEFORE stepping (registers still hold pre-step values)
        let cpi_cycles = classify_cpi_cycles(step_pc, &self.run.cpu, &self.run.mem, &self.run.cpi_config);

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

        // Update register age (fading highlight) and track last write PC
        for i in 0..32usize {
            if self.run.cpu.x[i] != self.run.prev_x[i] {
                self.run.reg_age[i] = 0;
                self.run.reg_last_write_pc[i] = Some(step_pc);
            } else {
                self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
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
                self.run.faulted = self.run.cpu.exit_code.is_none();
            }
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
