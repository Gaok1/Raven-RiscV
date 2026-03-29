mod cache_state;
mod cpi;
mod docs_state;
mod formatting;
mod hart;
mod run_loop;
mod run_state;
mod runtime;
mod settings_state;

use self::cpi::classify_cpi_cycles;
pub(crate) use self::cpi::{classify_cpi_for_display, cpi_class_label};
use self::formatting::{classify_mem_access, word_at};

// Re-export pub(crate) items from submodules so they are accessible as
// `crate::ui::app::X` from other modules in the crate.
pub(crate) use self::cache_state::{
    CacheDataFmt, CacheDataGroup, CacheResultsSnapshot, CacheScope, CacheState, CacheSubtab,
    ConfigField, LevelSnapshot,
};
pub(crate) use self::docs_state::{
    DocsLang, DocsPage, DocsState, PathInput, PathInputAction, TutorialState,
};
pub(crate) use self::hart::{HartCoreRuntime, HartLifecycle, step_hart_bg_inner};
pub(crate) use self::run_state::{
    BuildStats, EditorMode, EditorState, FormatMode, MemRegion, RunButton, RunSpeed, RunState,
};
pub(crate) use self::settings_state::{
    nearest_pow2_clamp, RunScope, SettingsState, SETTINGS_ROW_CACHE_ENABLED,
    SETTINGS_ROW_CPI_START, SETTINGS_ROW_MAX_CORES, SETTINGS_ROW_MEM_SIZE,
    SETTINGS_ROW_PIPELINE_ENABLED, SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROWS,
};

use super::{
    console::Console,
    editor::Editor,
    input::{handle_key, handle_mouse},
    view::ui,
};
use crate::falcon::cache::CacheConfig;
use crate::falcon::{self, CacheController, Cpu};
use arboard::Clipboard;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
};
use ratatui::{DefaultTerminal, layout::Rect};
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
use std::sync::{Arc, atomic::Ordering};
use std::{
    io,
    time::{Duration, Instant},
};

pub use run_loop::run;

// ── CPI (Cycles Per Instruction) configuration ───────────────────────────────

/// Base execution cycles per instruction class.
/// These are added on top of cache latency cycles.
#[derive(Clone, Debug)]
pub struct CpiConfig {
    pub alu: u64,              // Add, sub, logic, shifts, lui, auipc, immediate variants = 1
    pub mul: u64,              // mul, mulh, mulhsu, mulhu = 3
    pub div: u64,              // div, divu, rem, remu = 20
    pub load: u64,             // lb, lh, lw, lbu, lhu (extra over cache) = 0
    pub store: u64,            // sb, sh, sw (extra over cache) = 0
    pub branch_taken: u64,     // branch when taken = 3
    pub branch_not_taken: u64, // branch when not taken = 1
    pub jump: u64,             // jal, jalr = 2
    pub system: u64,           // ecall, ebreak, halt = 10
    pub fp: u64,               // RV32F instructions = 5
}

impl Default for CpiConfig {
    fn default() -> Self {
        // Values are EXTRA cycles beyond the base 1 cycle per instruction.
        // Effective cost = 1 + field_value.
        Self {
            alu: 0,              // effective 1
            mul: 2,              // effective 3
            div: 19,             // effective 20
            load: 0,             // effective 1 base; cache adds miss-penalty on top
            store: 0,            // effective 1 base
            branch_taken: 2,     // effective 3
            branch_not_taken: 0, // effective 1
            jump: 1,             // effective 2
            system: 9,           // effective 10
            fp: 4,               // effective 5
        }
    }
}

impl CpiConfig {
    pub(crate) fn field_names() -> &'static [&'static str] {
        &[
            "ALU",
            "MUL",
            "DIV",
            "Load+",
            "Store+",
            "Branch-T",
            "Branch-NT",
            "Jump",
            "System",
            "FP",
        ]
    }

    pub(crate) fn get(&self, idx: usize) -> u64 {
        match idx {
            0 => self.alu,
            1 => self.mul,
            2 => self.div,
            3 => self.load,
            4 => self.store,
            5 => self.branch_taken,
            6 => self.branch_not_taken,
            7 => self.jump,
            8 => self.system,
            9 => self.fp,
            _ => 0,
        }
    }

    pub(crate) fn set(&mut self, idx: usize, val: u64) {
        match idx {
            0 => self.alu = val,
            1 => self.mul = val,
            2 => self.div = val,
            3 => self.load = val,
            4 => self.store = val,
            5 => self.branch_taken = val,
            6 => self.branch_not_taken = val,
            7 => self.jump = val,
            8 => self.system = val,
            9 => self.fp = val,
            _ => {}
        }
    }

    pub(crate) fn descriptions() -> &'static [&'static str] {
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
pub(super) enum Tab {
    Editor,
    Run,
    Cache,
    Pipeline,
    Docs,
    Config,
}

impl Tab {
    pub(super) fn all() -> &'static [Tab] {
        &[
            Tab::Editor,
            Tab::Run,
            Tab::Cache,
            Tab::Pipeline,
            Tab::Docs,
            Tab::Config,
        ]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Editor => "Editor",
            Tab::Run => "Run",
            Tab::Cache => "Cache",
            Tab::Pipeline => "Pipeline",
            Tab::Docs => "Docs",
            Tab::Config => "Config",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
    }
}

// ── Top-level app ──────────────────────────────────────────────────────────────

pub struct App {
    pub(super) tab: Tab,
    pub(super) mode: EditorMode,

    pub(super) editor: EditorState,
    pub(super) run: RunState,
    pub(super) docs: DocsState,
    pub(super) cache: CacheState,
    pub(super) settings: SettingsState,
    pub(super) pipeline: crate::ui::pipeline::PipelineSimState,
    pub(crate) max_cores: usize,
    pub(crate) selected_core: usize,
    pub(crate) run_scope: RunScope,
    pub(crate) next_hart_id: u32,
    harts: Vec<HartCoreRuntime>,

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

    // TUI path input bar (fallback when OS file dialog returns None)
    pub(super) path_input: PathInput,

    // Interactive guided tutorial ([?] button)
    pub tutorial: TutorialState,
}

pub(super) fn compute_find_matches(query: &str, lines: &[String]) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return vec![];
    }
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
        let mem_size = ram_override.unwrap_or(16 * 1024 * 1024);
        cpu.write(2, mem_size as u32);
        let data_base = base_pc + 0x1000;
        cpu.heap_break = data_base;
        let mut app = Self {
            tab: Tab::Editor,
            mode: EditorMode::Insert,
            editor: EditorState {
                buf: Editor::with_sample(),
                dirty: true,
                last_edit_at: Some(Instant::now()),
                auto_check_delay: Duration::from_millis(400),
                last_assemble_msg: None,
                last_build_stats: None,
                last_compile_ok: None,
                last_ok_text: None,
                last_ok_data: None,
                last_ok_data_base: None,
                last_ok_bss_size: None,
                last_ok_elf_bytes: None,
                last_ok_comments: std::collections::HashMap::new(),
                last_ok_block_comments: std::collections::HashMap::new(),
                last_ok_labels: std::collections::HashMap::new(),
                last_ok_halt_pcs: std::collections::HashSet::new(),
                diag_line: None,
                diag_msg: None,
                diag_line_text: None,
                label_to_line: std::collections::HashMap::new(),
                line_to_addr: std::collections::HashMap::new(),
                show_addr_hints: false,
                elf_prompt_open: false,
                find_open: false,
                find_query: String::new(),
                replace_open: false,
                replace_query: String::new(),
                find_in_replace: false,
                find_matches: Vec::new(),
                find_current: 0,
                goto_open: false,
                goto_query: String::new(),
                show_encoding: false,
            },
            run: RunState {
                cpu,
                prev_x: [0; 32],
                prev_pc: base_pc,
                mem_size,
                mem: CacheController::new(
                    CacheConfig::default(),
                    CacheConfig::default(),
                    vec![],
                    mem_size,
                ),
                breakpoints: std::collections::HashSet::new(),
                base_pc,
                data_base,
                mem_view_addr: data_base,
                mem_view_bytes: 4,
                mem_region: MemRegion::Data,
                mem_search_open: false,
                mem_search_query: String::new(),
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
                imem_search_open: false,
                imem_search_query: String::new(),
                imem_vrow_cache: std::collections::HashMap::new(),
                labels_lower: std::collections::HashMap::new(),
                imem_search_matches: Vec::new(),
                imem_search_cursor: 0,
                imem_search_match_count: 0,
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
                halt_pcs: std::collections::HashSet::new(),
                elf_sections: Vec::new(),
                exec_counts: std::collections::HashMap::new(),
                exec_trace: std::collections::VecDeque::new(),
                reg_age: [255u8; 32],
                show_trace: false,
                pinned_regs: Vec::new(),
                reg_cursor: 0,
                block_comments: std::collections::HashMap::new(),
                reg_last_write_pc: [None; 32],
                show_dyn: false,
                dyn_mem_access: None,
                hover_reg_row: None,
                show_float_regs: false,
                prev_f: [0u32; 32],
                f_age: [255u8; 32],
                f_last_write_pc: [None; 32],
                cpi_config: CpiConfig::default(),
                show_exec_count: true,
                show_instr_type: true,
                mem_access_log: Vec::new(),
                cache_enabled: false,
            },
            docs: DocsState {
                page: DocsPage::InstrRef,
                lang: DocsLang::En,
                scroll: 0,
                search_open: false,
                search_query: String::new(),
                hover_page: None,
                type_filter: 0x0FFF,
                filter_cursor: 0,
                tab_bar_y: std::cell::Cell::new(0),
                tab_bar_xs: std::cell::Cell::new([(0, 0); 4]),
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
                show_tag: false,
                view_fmt_btn: std::cell::Cell::new((0, 0, 0)),
                view_group_btn: std::cell::Cell::new((0, 0, 0)),
                view_tag_btn: std::cell::Cell::new((0, 0, 0)),
                hover_view_fmt: false,
                hover_view_group: false,
                hover_view_tag: false,
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
                hover_export_results: false,
                hover_export_cfg: false,
                hover_import_cfg: false,
                session_history: Vec::new(),
                history_scroll: 0,
                viewing_snapshot: None,
                window_start_instr: 0,
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
            path_input: PathInput::new(),
            tutorial: TutorialState::default(),
            settings: SettingsState::default(),
            pipeline: crate::ui::pipeline::PipelineSimState::new(),
            max_cores: 4,
            selected_core: 0,
            run_scope: RunScope::AllHarts,
            next_hart_id: 1,
            harts: Vec::new(),
        };
        app.assemble_and_load();
        app.rebuild_harts();
        app
    }

    pub(super) fn assemble_and_load(&mut self) {
        use falcon::asm::assemble;
        use falcon::program::{load_bytes, load_words, zero_bytes};

        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = self.ram_override.unwrap_or(16 * 1024 * 1024);
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
        self.run.mem.bypass = !self.run.cache_enabled;
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
                let bss_end = prog
                    .data_base
                    .wrapping_add(prog.data.len() as u32 + prog.bss_size);
                self.run.cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

                self.run.comments = prog.comments;
                self.run.block_comments = prog.block_comments;
                self.run.labels = prog.labels;
                self.run.halt_pcs = prog.halt_pcs.clone();
                self.run.exec_counts.clear();
                self.run.exec_trace.clear();
                self.run.reg_age = [255u8; 32];
                self.run.reg_last_write_pc = [None; 32];
                self.editor.label_to_line = prog.label_to_line;
                self.editor.line_to_addr = prog.line_addrs;
                self.editor.last_ok_text = Some(prog.text.clone());
                self.rebuild_imem_vrow_cache();
                self.editor.last_ok_data = Some(prog.data.clone());
                self.editor.last_ok_data_base = Some(prog.data_base);
                self.editor.last_ok_bss_size = Some(prog.bss_size);
                self.editor.last_build_stats = Some(BuildStats {
                    instruction_count: prog.text.len(),
                    data_bytes: prog.data.len(),
                });
                self.run.imem_scroll = 0;
                self.run.hover_imem_addr = None;

                // Reset pipeline stages (shares cpu/mem with RunState)
                self.pipeline.reset_stages(self.run.cpu.pc);

                self.editor.last_assemble_msg = Some(format!(
                    "Assembled {} instructions, {} data bytes, {} bss bytes.",
                    prog.text.len(),
                    prog.data.len(),
                    prog.bss_size
                ));
                self.editor.last_compile_ok = Some(true);
                self.editor.last_ok_elf_bytes = None;
                self.editor.diag_line = None;
                self.editor.diag_msg = None;
                self.editor.diag_line_text = None;
                self.rebuild_harts();
            }
            Err(e) => {
                self.editor.diag_line = Some(e.line);
                self.editor.diag_msg = Some(e.msg.clone());
                self.editor.diag_line_text = self.editor.buf.lines.get(e.line).cloned();
                self.editor.last_build_stats = None;
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
                self.editor.last_build_stats = Some(BuildStats {
                    instruction_count: prog.text.len(),
                    data_bytes: prog.data.len(),
                });
                self.editor.last_ok_comments = prog.comments;
                self.editor.last_ok_block_comments = prog.block_comments;
                self.editor.last_ok_labels = prog.labels.clone();
                self.editor.last_ok_halt_pcs = prog.halt_pcs;
                self.editor.label_to_line = prog.label_to_line;
                self.editor.line_to_addr = prog.line_addrs;
                self.editor.last_assemble_msg = Some(format!(
                    "OK: {} instructions, {} data bytes, {} bss bytes",
                    prog.text.len(),
                    prog.data.len(),
                    prog.bss_size
                ));
                self.editor.last_compile_ok = Some(true);
                self.editor.last_ok_elf_bytes = None;
                self.editor.diag_line = None;
                self.editor.diag_msg = None;
                self.editor.diag_line_text = None;
            }
            Err(e) => {
                self.editor.diag_line = Some(e.line);
                self.editor.diag_msg = Some(e.msg.clone());
                self.editor.diag_line_text = self.editor.buf.lines.get(e.line).cloned();
                self.editor.last_build_stats = None;
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
            self.run.mem_size = self.ram_override.unwrap_or(16 * 1024 * 1024);
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
            self.run.mem.bypass = !self.run.cache_enabled;
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
            let bss_sz = self.editor.last_ok_bss_size.unwrap_or(0);
            let bss_end = data_base
                .wrapping_add(data.len() as u32)
                .wrapping_add(bss_sz);
            self.run.cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

            self.run.reg_age = [255u8; 32];
            self.run.f_age = [255u8; 32];
            self.run.f_last_write_pc = [None; 32];
            self.run.comments = self.editor.last_ok_comments.clone();
            self.run.block_comments = self.editor.last_ok_block_comments.clone();
            self.run.labels = self.editor.last_ok_labels.clone();
            self.run.halt_pcs = self.editor.last_ok_halt_pcs.clone();
            let text_len = text.len();
            let data_len = data.len();
            self.editor.last_build_stats = Some(BuildStats {
                instruction_count: text_len,
                data_bytes: data_len,
            });
            self.editor.last_assemble_msg = Some(format!(
                "Loaded last successful build: {} instructions, {} data bytes, {} bss bytes.",
                text_len,
                data_len,
                bss_sz
            ));
            self.rebuild_imem_vrow_cache();
            self.run.imem_scroll = 0;
            self.run.hover_imem_addr = None;
            // Reset pipeline stages so it picks up the reloaded program
            self.pipeline.reset_stages(self.run.cpu.pc);
            self.rebuild_harts();
        }
    }

    pub(super) fn restart_simulation(&mut self) {
        self.run.is_running = false;
        self.run.faulted = false;
        self.run.cpu.ebreak_hit = false;
        self.run.reg_last_write_pc = [None; 32];
        self.run.f_last_write_pc = [None; 32];
        self.run.reg_age = [255u8; 32];
        self.run.f_age = [255u8; 32];
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.run.mem_access_log.clear();
        self.cache.window_start_instr = 0;
        self.load_last_ok_program();
        // Reset pipeline AFTER loading program (cpu.pc is now set correctly)
        self.pipeline.reset_stages(self.run.cpu.pc);
        self.rebuild_harts();
    }

    pub(super) fn load_binary(&mut self, bytes: &[u8]) {
        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = self.ram_override.unwrap_or(16 * 1024 * 1024); // default 16 MB for ELF (heap support)
        self.run.cpu = Cpu::default();
        self.run.reg_age = [255u8; 32];
        self.run.f_age = [255u8; 32];
        self.run.reg_last_write_pc = [None; 32];
        self.run.f_last_write_pc = [None; 32];
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.run.mem_access_log.clear();
        self.run.cpu.write(2, self.run.mem_size as u32);
        self.run.mem = CacheController::new(
            self.cache.pending_icache.clone(),
            self.cache.pending_dcache.clone(),
            self.cache.extra_pending.clone(),
            self.run.mem_size,
        );
        self.run.mem.bypass = !self.run.cache_enabled;
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

            self.run.cpu.pc = info.entry;
            self.run.prev_pc = info.entry;
            self.run.base_pc = info.text_base;
            self.run.data_base = info.data_base;
            self.run.mem_view_addr = info.data_base;
            self.run.mem_region = crate::ui::app::MemRegion::Data;
            self.run.mem.invalidate_all();
            self.run.mem.reset_stats();
            let elf_data_bytes = info
                .sections
                .iter()
                .map(|section| section.bytes.len())
                .sum();

            // Populate labels and sections viewer from ELF symbol table
            self.run.labels = info.symbols;
            self.run.halt_pcs.clear();
            self.run.elf_sections = info.sections;
            self.run.cpu.heap_break = info.heap_start;

            let mut words = Vec::with_capacity(info.text_bytes.len() / 4);
            for chunk in info.text_bytes.chunks(4) {
                let mut b = [0u8; 4];
                for (i, &v) in chunk.iter().enumerate() {
                    b[i] = v;
                }
                words.push(u32::from_le_bytes(b));
            }
            let entry = info.entry;
            let data_base = info.data_base;
            self.editor.last_ok_text = Some(words);
            self.rebuild_imem_vrow_cache();
            self.editor.last_ok_data = Some(Vec::new());
            self.editor.last_ok_data_base = Some(data_base);
            self.editor.last_ok_bss_size = Some(0);
            self.editor.last_ok_elf_bytes = Some(bytes.to_vec());
            self.editor.last_build_stats = Some(BuildStats {
                instruction_count: self
                    .editor
                    .last_ok_text
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0),
                data_bytes: elf_data_bytes,
            });
            self.editor.last_assemble_msg = Some(format!(
                "Loaded ELF: {} bytes, entry 0x{entry:08X} ({} instructions)",
                info.total_bytes,
                self.editor
                    .last_ok_text
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0),
            ));
        } else {
            // ── FALC or flat binary ──────────────────────────────────────
            self.run.elf_sections = Vec::new();
            use falcon::program::{load_bytes, zero_bytes};
            let (text_bytes, data_bytes, bss_size): (Vec<u8>, Vec<u8>, u32) =
                if bytes.len() >= 16 && &bytes[0..4] == b"FALC" {
                    let text_sz = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
                    let data_sz = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
                    let bss_sz = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
                    let body = &bytes[16..];
                    if body.len() < text_sz + data_sz {
                        self.console.push_error("Binary truncated or corrupt");
                        self.run.faulted = true;
                        return;
                    }
                    (
                        body[..text_sz].to_vec(),
                        body[text_sz..text_sz + data_sz].to_vec(),
                        bss_sz,
                    )
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

            self.run.cpu.pc = self.run.base_pc;
            self.run.prev_pc = self.run.base_pc;
            self.run.mem.invalidate_all();
            self.run.mem.reset_stats();

            // Heap starts right after BSS, 16-byte aligned
            let bss_end = self
                .run
                .data_base
                .wrapping_add(data_bytes.len() as u32)
                .wrapping_add(bss_size);
            self.run.cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

            let mut words = Vec::with_capacity(text_bytes.len() / 4);
            for chunk in text_bytes.chunks(4) {
                let mut b = [0u8; 4];
                for (i, &v) in chunk.iter().enumerate() {
                    b[i] = v;
                }
                words.push(u32::from_le_bytes(b));
            }
            let total = text_bytes.len() + data_bytes.len();
            self.editor.last_ok_text = Some(words);
            self.editor.last_ok_data = Some(data_bytes);
            self.editor.last_ok_data_base = Some(self.run.data_base);
            self.editor.last_ok_bss_size = Some(bss_size);
            self.editor.last_ok_elf_bytes = None;
            self.editor.last_build_stats = Some(BuildStats {
                instruction_count: self
                    .editor
                    .last_ok_text
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0),
                data_bytes: self
                    .editor
                    .last_ok_data
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0),
            });
            self.editor.last_assemble_msg = Some(format!(
                "Loaded binary: {} bytes ({} instructions)",
                total,
                self.editor
                    .last_ok_text
                    .as_ref()
                    .map(|v| v.len())
                    .unwrap_or(0),
            ));
        }

        self.editor.last_compile_ok = Some(true);
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;
        self.run.imem_scroll = 0;
        self.run.hover_imem_addr = None;
        // Lock the editor when a binary is loaded; close any stale prompt.
        self.mode = EditorMode::Command;
        self.editor.elf_prompt_open = false;
        // Reset pipeline stages (shares cpu/mem with RunState)
        self.pipeline.reset_stages(self.run.cpu.pc);
        self.rebuild_harts();
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
                    ConfigField::Size => {
                        if let Ok(v) = s.parse::<usize>() {
                            // Size must yield a power-of-2 number of sets.
                            // Snap: compute the unit (line_size * assoc), then snap
                            // num_sets = v/unit to the nearest power of two.
                            let unit = cfg.line_size.max(1) * cfg.associativity.max(1);
                            let sets = (v / unit).max(1);
                            let snapped = nearest_pow2_clamp(sets, 1, 1 << 20);
                            cfg.size = snapped * unit;
                        }
                    }
                    ConfigField::LineSize => {
                        if let Ok(v) = s.parse::<usize>() {
                            cfg.line_size = nearest_pow2_clamp(v, 4, 4096);
                        }
                    }
                    ConfigField::Associativity => {
                        if let Ok(v) = s.parse::<usize>() {
                            cfg.associativity = v.max(1);
                        }
                    }
                    ConfigField::HitLatency => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.hit_latency = v.max(1);
                        }
                    }
                    ConfigField::MissPenalty => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.miss_penalty = v;
                        }
                    }
                    ConfigField::AssocPenalty => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.assoc_penalty = v;
                        }
                    }
                    ConfigField::TransferWidth => {
                        if let Ok(v) = s.parse::<u32>() {
                            cfg.transfer_width = v.max(1);
                        }
                    }
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
                        InclusionPolicy::Inclusive => InclusionPolicy::Exclusive,
                        InclusionPolicy::Exclusive => InclusionPolicy::NonInclusive,
                    }
                } else {
                    match cfg.inclusion {
                        InclusionPolicy::NonInclusive => InclusionPolicy::Exclusive,
                        InclusionPolicy::Inclusive => InclusionPolicy::NonInclusive,
                        InclusionPolicy::Exclusive => InclusionPolicy::Inclusive,
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
            if is_icache {
                &self.cache.pending_icache
            } else {
                &self.cache.pending_dcache
            }
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
            if is_icache {
                &mut self.cache.pending_icache
            } else {
                &mut self.cache.pending_dcache
            }
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
            if !self.imem_in_range(addr) {
                break;
            }
            if self.run.block_comments.contains_key(&addr) {
                count += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                count += names.len();
            }
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
            if !self.imem_in_range(addr) {
                return (base, 0);
            }
            let bc = if self.run.block_comments.contains_key(&addr) {
                1
            } else {
                0
            };
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
        if pc < self.run.base_pc {
            return None;
        }
        let mut vrow = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) {
                return None;
            }
            if self.run.block_comments.contains_key(&addr) {
                vrow += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                vrow += names.len();
            }
            if addr == pc {
                return Some(vrow);
            }
            vrow += 1;
            addr = addr.wrapping_add(4);
        }
    }

    /// Ensure PC is visible in the imem panel, updating imem_scroll if needed.
    pub(super) fn ensure_pc_visible_in_imem(&mut self) {
        let visible = self.run.imem_inner_height.get();
        if visible == 0 {
            return;
        }
        let Some(pc_vrow) = self.imem_visual_row_of_pc() else {
            return;
        };
        let scroll = self.run.imem_scroll;
        if pc_vrow < scroll {
            // PC above view
            self.run.imem_scroll = pc_vrow.saturating_sub(2);
        } else if pc_vrow + 1 >= scroll + visible {
            // PC at or below bottom edge
            self.run.imem_scroll = pc_vrow.saturating_sub(visible.saturating_sub(3));
        }
    }

    /// Visual row of an arbitrary address within the full instruction list.
    /// O(1) — reads the pre-computed cache built by `rebuild_imem_vrow_cache`.
    pub(super) fn imem_visual_row_of_addr(&self, target: u32) -> Option<usize> {
        self.run.imem_vrow_cache.get(&target).copied()
    }

    /// Rebuild the addr→visual-row cache and the pre-lowercased label index.
    /// Must be called whenever `run.labels` or `run.block_comments` change
    /// (i.e. after every program load).
    pub(super) fn rebuild_imem_vrow_cache(&mut self) {
        let mut cache = std::collections::HashMap::with_capacity(
            (self.run.mem_size / 4).min(1 << 20),
        );
        let mut vrow = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) {
                break;
            }
            if self.run.block_comments.contains_key(&addr) {
                vrow += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                vrow += names.len();
            }
            cache.insert(addr, vrow);
            vrow += 1;
            addr = addr.wrapping_add(4);
        }
        self.run.imem_vrow_cache = cache;
        self.run.labels_lower = self.run.labels.iter()
            .map(|(&a, names)| (a, names.iter().map(|n| n.to_lowercase()).collect()))
            .collect();
    }

    /// Scroll the instruction memory panel to bring `addr` near the top.
    pub(super) fn scroll_imem_to_addr(&mut self, addr: u32) {
        if let Some(vrow) = self.imem_visual_row_of_addr(addr) {
            self.run.imem_scroll = vrow.saturating_sub(2);
        }
    }

    fn tick(&mut self) {
        if self.run.is_running {
            // When pipeline is enabled and we're viewing the Pipeline tab,
            // use pipeline speed for rate-limiting (educational slow stepping).
            // Otherwise use run speed.
            use crate::ui::pipeline::PipelineSpeed;
            let use_pipeline_speed = self.pipeline.enabled && matches!(self.tab, Tab::Pipeline);

            if use_pipeline_speed {
                match self.pipeline.speed {
                    PipelineSpeed::Slow => {
                        if self.pipeline.last_tick.elapsed() >= Duration::from_millis(600) {
                            self.single_step();
                            self.pipeline.last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Normal => {
                        if self.pipeline.last_tick.elapsed() >= Duration::from_millis(300) {
                            self.single_step();
                            self.pipeline.last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Fast => {
                        if self.pipeline.last_tick.elapsed() >= Duration::from_millis(80) {
                            self.single_step();
                            self.pipeline.last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Instant => {
                        let budget = Duration::from_millis(8);
                        let start = Instant::now();
                        while self.run.is_running && start.elapsed() < budget {
                            self.single_step();
                        }
                    }
                }
            } else {
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
                        for _ in 0..4 {
                            if !self.run.is_running {
                                break;
                            }
                            self.single_step();
                        }
                    }
                    RunSpeed::X8 => {
                        for _ in 0..8 {
                            if !self.run.is_running {
                                break;
                            }
                            self.single_step();
                        }
                    }
                    RunSpeed::Instant => {
                        let budget = Duration::from_millis(14);
                        let start = Instant::now();
                        while self.run.is_running && start.elapsed() < budget {
                            self.single_step();
                        }
                    }
                }
            }
        }
        // Scroll instruction list to follow PC (skipped in Instant to avoid pointless churn)
        if self.run.is_running && !matches!(self.run.speed, RunSpeed::Instant) {
            self.ensure_pc_visible_in_imem();
        }
        // Auto-follow SP/HB in Stack and Heap views — runs every tick so it works
        // regardless of execution path (sequential or pipeline).
        match self.run.mem_region {
            MemRegion::Stack => {
                let sp = self.run.cpu.x[2];
                self.run.mem_view_addr = sp & !(self.run.mem_view_bytes - 1);
            }
            MemRegion::Heap => {
                let hb = self.run.cpu.heap_break;
                self.run.mem_view_addr = hb & !(self.run.mem_view_bytes - 1);
            }
            _ => {}
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

    fn finalize_selected_core_after_step(&mut self) {
        self.process_pending_hart_start_for_selected();
        let heap_break = self.run.cpu.heap_break;
        self.propagate_heap_break(heap_break);
        let program_exit = self.run.cpu.exit_code;

        let lifecycle = if self.run.cpu.local_exit {
            // FALCON_HART_EXIT: exit only this hart, leave others running.
            HartLifecycle::Exited
        } else if self.run.cpu.ebreak_hit {
            if self.run.halt_pcs.contains(&self.run.prev_pc) {
                HartLifecycle::Exited
            } else {
                HartLifecycle::Paused
            }
        } else if self.run.faulted || self.pipeline.faulted {
            HartLifecycle::Faulted
        } else if self.run.cpu.exit_code.is_some() || self.pipeline.halted {
            HartLifecycle::Exited
        } else {
            HartLifecycle::Running
        };

        if let Some(runtime) = self.selected_runtime_mut() {
            runtime.lifecycle = lifecycle;
            runtime.hart_id.get_or_insert(0);
            runtime.faulted = matches!(lifecycle, HartLifecycle::Faulted);
        }

        if let Some(code) = program_exit {
            // Global exit — kill all harts immediately.
            for hart in &mut self.harts {
                if hart.hart_id.is_some() {
                    hart.lifecycle = HartLifecycle::Exited;
                    hart.cpu.exit_code = Some(code);
                }
            }
            self.run.mem.flush_all();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Faulted) {
            // A fault in any hart stops the whole run.
            self.run.mem.flush_all();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Paused) {
            // In AllHarts scope: only stop the run when no other harts are still running.
            // The paused hart is skipped by step_all_cores_once; others keep going.
            // In FocusedHart scope or single-core: stop everything so the user can inspect.
            let stop_all = self.max_cores <= 1
                || matches!(self.run_scope, RunScope::FocusedHart)
                || !self.any_running_harts();
            if stop_all {
                self.run.is_running = false;
            }
        } else if !matches!(lifecycle, HartLifecycle::Running) && !self.any_running_harts() {
            // Last hart finished (halt/local-exit) — stop.
            self.run.mem.flush_all();
            self.run.is_running = false;
        }
    }

    fn any_running_harts(&self) -> bool {
        self.harts
            .iter()
            .any(|hart| matches!(hart.lifecycle, HartLifecycle::Running))
    }

    /// Finalise the lifecycle of a non-selected hart after it has been stepped
    /// by `step_hart_bg_inner`.  Mirrors `finalize_selected_core_after_step`
    /// but operates directly on `self.harts[core_idx]` instead of `self.run`.
    fn finalize_bg_hart(&mut self, core_idx: usize, breakpoint_hit: bool) {
        self.process_pending_hart_start_for_bg(core_idx);

        let heap_break = self.harts[core_idx].cpu.heap_break;
        self.propagate_heap_break(heap_break);

        let program_exit = self.harts[core_idx].cpu.exit_code;

        let pipe_halted = self.harts[core_idx]
            .pipeline
            .as_ref()
            .map_or(false, |p| p.halted);
        let pipe_faulted = self.harts[core_idx]
            .pipeline
            .as_ref()
            .map_or(false, |p| p.faulted);

        let lifecycle = if self.harts[core_idx].cpu.local_exit {
            HartLifecycle::Exited
        } else if breakpoint_hit {
            HartLifecycle::Paused
        } else if self.harts[core_idx].cpu.ebreak_hit {
            if self.run.halt_pcs.contains(&self.harts[core_idx].prev_pc) {
                HartLifecycle::Exited
            } else {
                HartLifecycle::Paused
            }
        } else if self.harts[core_idx].faulted || pipe_faulted {
            HartLifecycle::Faulted
        } else if program_exit.is_some() || pipe_halted {
            HartLifecycle::Exited
        } else {
            HartLifecycle::Running
        };

        if let Some(code) = program_exit {
            // Global exit — mark every hart (including the one currently in
            // self.run) as exited and stop the run loop.
            for h in &mut self.harts {
                if h.hart_id.is_some() {
                    h.lifecycle = HartLifecycle::Exited;
                    h.cpu.exit_code = Some(code);
                }
            }
            self.run.cpu.exit_code = Some(code);
            self.run.mem.flush_all();
            self.run.is_running = false;
            return;
        }

        self.harts[core_idx].lifecycle = lifecycle;
        self.harts[core_idx].faulted = matches!(lifecycle, HartLifecycle::Faulted);

        if matches!(lifecycle, HartLifecycle::Faulted) {
            self.run.mem.flush_all();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Paused) {
            // step_all_cores_once is only called in AllHarts scope; keep running
            // as long as at least one hart is still active.
            if !self.any_running_harts() {
                self.run.is_running = false;
            }
        } else if !matches!(lifecycle, HartLifecycle::Running) && !self.any_running_harts() {
            self.run.mem.flush_all();
            self.run.is_running = false;
        }

        // If this hart blocked on keyboard input, pause the entire run loop.
        // The keyboard handler resumes is_running when Enter is pressed.
        if self.console.reading {
            self.run.is_running = false;
        }
    }

    pub(crate) fn can_start_run(&self) -> bool {
        if self.max_cores <= 1 {
            let status = self.core_status(self.selected_core);
            return status == HartLifecycle::Paused
                || (!self.run.faulted && status == HartLifecycle::Running);
        }

        if matches!(self.run_scope, RunScope::FocusedHart) && !matches!(self.tab, Tab::Pipeline) {
            matches!(
                self.core_status(self.selected_core),
                HartLifecycle::Running | HartLifecycle::Paused
            )
        } else {
            self.any_running_harts()
                || self.core_status(self.selected_core) == HartLifecycle::Paused
        }
    }

    pub(crate) fn resume_selected_hart(&mut self) {
        if self.core_status(self.selected_core) != HartLifecycle::Paused {
            return;
        }
        self.run.cpu.ebreak_hit = false;
        self.run.faulted = false;
        self.pipeline.halted = false;
        self.pipeline.faulted = false;
        if let Some(runtime) = self.selected_runtime_mut() {
            if runtime.hart_id.is_some() {
                runtime.lifecycle = HartLifecycle::Running;
            }
        }
    }

    /// Execute one pipeline tick using shared cpu/mem state.
    /// Execute one pipeline cycle. Returns true if an instruction was committed.
    fn pipeline_step(&mut self) -> bool {
        if self.pipeline.halted || self.pipeline.faulted {
            return false;
        }

        self.run.prev_x = self.run.cpu.x;
        self.run.prev_f = self.run.cpu.f;
        self.run.prev_pc = self.run.cpu.pc;

        // Clone CpiConfig to avoid borrow conflict (80 bytes, cheap)
        let cpi = self.run.cpi_config.clone();

        let commit = crate::ui::pipeline::sim::pipeline_tick(
            &mut self.pipeline,
            &mut self.run.cpu,
            &mut self.run.mem,
            &cpi,
            &mut self.console,
        );

        let committed = if let Some(info) = commit {
            *self.run.exec_counts.entry(info.pc).or_insert(0) += 1;
            let disasm = {
                let word = self.run.mem.peek32(info.pc).unwrap_or(0);
                match falcon::decoder::decode(word) {
                    Ok(instr) => format!("{instr:?}"),
                    Err(_) => format!("0x{word:08x}"),
                }
            };
            self.run.exec_trace.push_back((info.pc, disasm));
            if self.run.exec_trace.len() > 200 {
                self.run.exec_trace.pop_front();
            }

            for i in 0..32usize {
                if self.run.cpu.x[i] != self.run.prev_x[i] {
                    self.run.reg_age[i] = 0;
                    self.run.reg_last_write_pc[i] = Some(info.pc);
                } else {
                    self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
                }
            }
            for i in 0..32usize {
                if self.run.cpu.f[i] != self.run.prev_f[i] {
                    self.run.f_age[i] = 0;
                    self.run.f_last_write_pc[i] = Some(info.pc);
                } else {
                    self.run.f_age[i] = self.run.f_age[i].saturating_add(1).min(8);
                }
            }
            self.run.prev_x = self.run.cpu.x;
            self.run.prev_f = self.run.cpu.f;
            self.run.prev_pc = info.pc;

            let cpi_word = self.run.mem.peek32(info.pc).unwrap_or(0);
            let cpi_cycles = classify_cpi_cycles(cpi_word, &self.run.cpu, &self.run.cpi_config);
            self.run.mem.add_instruction_cycles(cpi_cycles);
            self.run.mem.instruction_count = self.run.mem.instruction_count.saturating_add(1);
            if self.run.mem.instruction_count % 32 == 0 {
                self.run.mem.snapshot_stats();
            }
            true
        } else {
            false
        };

        if self.pipeline.faulted {
            self.run.faulted = true;
        }
        if self.run.breakpoints.contains(&self.run.cpu.pc) {
            self.run.is_running = false;
        }
        self.finalize_selected_core_after_step();
        committed
    }

    fn step_all_cores_once(&mut self) -> bool {
        let original = self.selected_core;
        let mut selected_committed = false;

        // Pre-compute values needed by step_hart_bg_inner.  These are read
        // here — before any mutable borrow of self.harts — to satisfy the
        // borrow checker's disjoint-field rules.
        let imem_start = self.run.base_pc;
        let imem_end = if let Some(text) = &self.editor.last_ok_text {
            self.run
                .base_pc
                .saturating_add((text.len() as u32).saturating_mul(4))
        } else {
            self.run.mem_size.saturating_sub(3) as u32
        };
        let mem_size = self.run.mem_size;
        let pipeline_enabled = self.pipeline.enabled;
        // CpiConfig is ~80 bytes; cheap to clone once per round.
        let cpi = self.run.cpi_config.clone();

        // In run mode is_running starts true; in single-step mode it starts false.
        // We only want to abort the round early when a hart *causes* a stop during
        // this round — not because is_running was already false before we began.
        let was_running = self.run.is_running;

        for core_idx in 0..self.max_cores {
            if core_idx == original {
                // ── Selected core: already live in self.run — no sync needed ─
                if self.core_status(core_idx) != HartLifecycle::Running {
                    continue;
                }
                if pipeline_enabled {
                    let committed = self.pipeline_step();
                    selected_committed = committed;
                } else {
                    self.single_step_selected_sequential();
                    selected_committed = true;
                }
            } else {
                // ── Non-selected core: step directly, zero HashMap/VecDeque clones ─
                if self.harts[core_idx].lifecycle != HartLifecycle::Running {
                    continue;
                }
                // Disjoint field borrows: self.harts vs self.run.mem vs self.console.
                let faulted = {
                    let hart = &mut self.harts[core_idx];
                    let mem = &mut self.run.mem;
                    let console = &mut self.console;
                    step_hart_bg_inner(
                        hart, mem, console, &cpi,
                        imem_start, imem_end, mem_size, pipeline_enabled,
                    )
                };
                let bp_hit = self.run.breakpoints.contains(&self.harts[core_idx].cpu.pc);
                let _ = faulted; // lifecycle determined inside finalize_bg_hart via hart.faulted
                self.finalize_bg_hart(core_idx, bp_hit);
            }
            // Only abort early if a hart *caused* a stop during this round.
            // When single-stepping, is_running is false from the start — that
            // must not be treated as a mid-round exit signal.
            if was_running && !self.run.is_running {
                break;
            }
        }

        // If multiple non-selected harts called sbrk in the same round, each
        // finalize_bg_hart propagated its own heap_break, overwriting the previous.
        // Propagate the maximum heap_break across all harts so none is lost.
        let max_break = self
            .harts
            .iter()
            .filter(|h| h.hart_id.is_some())
            .map(|h| h.cpu.heap_break)
            .chain(std::iter::once(self.run.cpu.heap_break))
            .max()
            .unwrap_or(self.run.cpu.heap_break);
        if max_break != self.run.cpu.heap_break {
            self.propagate_heap_break(max_break);
        }

        // Sync selected core's CPU snapshot to harts[original] (cheap — skips
        // exec_counts/exec_trace).  Keeps harts[selected].cpu current so that
        // UI code and tests that read it directly get a consistent view.
        if let Some(runtime) = self.harts.get_mut(original) {
            runtime.cpu = self.run.cpu.clone();
            runtime.prev_pc = self.run.prev_pc;
            runtime.prev_x = self.run.prev_x;
            runtime.prev_f = self.run.prev_f;
            runtime.faulted = self.run.faulted;
        }

        selected_committed
    }

    fn step_selected_core_once(&mut self) -> bool {
        let status = self.core_status(self.selected_core);
        if !matches!(status, HartLifecycle::Running | HartLifecycle::Paused) {
            return false;
        }
        if status == HartLifecycle::Paused {
            self.resume_selected_hart();
        }
        if self.pipeline.enabled {
            self.pipeline_step()
        } else {
            self.single_step_selected_sequential();
            true
        }
    }

    pub(super) fn single_step(&mut self) {
        if self.core_status(self.selected_core) == HartLifecycle::Paused {
            self.resume_selected_hart();
        }

        if self.max_cores > 1 {
            let all_scope = matches!(self.run_scope, RunScope::AllHarts);

            if self.pipeline.enabled && !matches!(self.tab, Tab::Pipeline) {
                for _ in 0..200 {
                    if self.core_status(self.selected_core) != HartLifecycle::Running {
                        break;
                    }
                    let committed = if all_scope {
                        self.step_all_cores_once()
                    } else {
                        self.step_selected_core_once()
                    };
                    if committed || self.core_status(self.selected_core) != HartLifecycle::Running {
                        break;
                    }
                }
                if !self.run.is_running {
                    self.ensure_pc_visible_in_imem();
                }
            } else if all_scope {
                self.step_all_cores_once();
            } else {
                self.step_selected_core_once();
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            return;
        }

        if self.pipeline.enabled {
            if matches!(self.tab, Tab::Pipeline) {
                // Pipeline tab: advance one cycle (educational single-cycle view)
                self.pipeline_step();
            } else {
                // Run/Cache/other tabs: advance until one instruction commits
                // Safety limit to prevent infinite loop on stall/halt/fault
                for _ in 0..200 {
                    let committed = self.pipeline_step();
                    if committed || self.pipeline.halted || self.pipeline.faulted {
                        break;
                    }
                }
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            return;
        }

        self.single_step_selected_sequential();
    }

    fn single_step_selected_sequential(&mut self) {
        let go_mode = matches!(self.run.speed, RunSpeed::Instant);
        // In GO mode skip the 256-byte register snapshot — reg_age not updated mid-run.
        if !go_mode {
            self.run.prev_x = self.run.cpu.x;
            self.run.prev_f = self.run.cpu.f;
        }
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

        // Fetch word once — reused for CPI, mem-access tracking, and disasm below.
        let word = self.run.mem.peek32(step_pc).unwrap_or(0);
        let cpi_cycles = classify_cpi_cycles(word, &self.run.cpu, &self.run.cpi_config);
        // In GO mode mem-access tracking is skipped (not visible while running).
        let mem_access = if go_mode { None } else { classify_mem_access(word, &self.run.cpu) };

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            falcon::exec::step(&mut self.run.cpu, &mut self.run.mem, &mut self.console)
        }));
        let alive = match res {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                use crate::falcon::errors::FalconError;
                let msg = if matches!(&e, FalconError::Bus(_)) {
                    let ram_kb = self.run.mem_size / 1024;
                    let suggest = if ram_kb < 1024 {
                        "16mb"
                    } else if ram_kb < 65536 {
                        "128mb"
                    } else {
                        "512mb"
                    };
                    format!("{e} (RAM is {ram_kb} KB — run with --mem {suggest} to increase)")
                } else {
                    e.to_string()
                };
                self.console.push_error(msg);
                self.run.faulted = true;
                false
            }
            Err(_) => {
                self.run.faulted = true;
                false
            }
        };
        self.run.mem.add_instruction_cycles(cpi_cycles);
        if self.run.mem.instruction_count % 32 == 0 {
            self.run.mem.snapshot_stats();
        }

        // Track execution counts (heatmap) — always kept.
        *self.run.exec_counts.entry(step_pc).or_insert(0) += 1;

        // In GO mode skip all display-only instrumentation: exec_trace formatting,
        // mem_access_log, and reg/float age tracking. None of these are visible
        // while the simulation is running at full speed.
        if !go_mode {
            let disasm = match falcon::decoder::decode(word) {
                Ok(instr) => format!("{instr:?}"),
                Err(_) => format!("0x{word:08x}"),
            };
            self.run.exec_trace.push_back((step_pc, disasm));
            if self.run.exec_trace.len() > 200 {
                self.run.exec_trace.pop_front();
            }

            // Update memory access log (age existing entries, insert new if load/store)
            for entry in &mut self.run.mem_access_log {
                entry.2 = entry.2.saturating_add(1);
            }
            self.run.mem_access_log.retain(|e| e.2 < 3);
            if let Some((addr, size, _)) = mem_access {
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
        }
        // Auto-follow last memory R/W when Access region is active.
        // Stack and Heap follow is handled in tick() so it covers all execution paths.
        if self.run.mem_region == crate::ui::app::MemRegion::Access {
            if let Some((addr, _, _)) = mem_access {
                self.run.mem_view_addr = addr & !(self.run.mem_view_bytes - 1);
            }
        }
        // Dyn view: remember last access; update mem_view_addr for memory sub-view
        self.run.dyn_mem_access = mem_access;
        if self.run.show_dyn {
            if let Some((addr, _, is_store)) = mem_access {
                if is_store {
                    self.run.mem_view_addr = addr & !(self.run.mem_view_bytes - 1);
                }
            }
        }

        // Check breakpoints: stop if the new PC is a breakpoint
        if alive && self.run.breakpoints.contains(&self.run.cpu.pc) {
            self.run.is_running = false;
        }
        if !alive {
            // Don't stop is_running here — finalize_selected_core_after_step decides
            // whether to stop based on other running harts (multi-hart aware).
            if !self.console.reading {
                self.run.faulted = self.run.cpu.exit_code.is_none()
                    && !self.run.cpu.ebreak_hit
                    && !self.run.cpu.local_exit;
            } else {
                // Hart is blocking on keyboard input — pause the run loop.
                // The keyboard handler resumes is_running when Enter is pressed.
                self.run.is_running = false;
            }
        }
        // Keep PC visible (single-step case — running case handled in tick())
        if !self.run.is_running {
            self.ensure_pc_visible_in_imem();
        }
        self.finalize_selected_core_after_step();
    }

    /// Jump editor cursor to the definition of the label under the cursor.
    pub(super) fn goto_label_definition(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() {
            return;
        }
        let line = &self.editor.buf.lines[row];
        let word = word_at(line, col);
        if word.is_empty() {
            return;
        }
        if let Some(&target_line) = self.editor.label_to_line.get(&word) {
            self.editor.buf.cursor_row = target_line;
            self.editor.buf.cursor_col = 0;
        }
    }

    /// Select next occurrence of the word currently under the cursor.
    pub(super) fn select_next_occurrence(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() {
            return;
        }
        let word = word_at(&self.editor.buf.lines[row], col);
        if word.is_empty() {
            return;
        }
        let lines = &self.editor.buf.lines;
        let total = lines.len();
        // Search from after current cursor position
        for offset in 1..=(total * lines[0].len().max(80) + 1) {
            let _ = offset; // silence lint
            break; // use proper search below
        }
        // Find next occurrence after (row, col+word.len())
        let start_col = col + 1;
        let positions: Vec<(usize, usize)> = lines
            .iter()
            .enumerate()
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
        if positions.is_empty() {
            return;
        }
        // Find the next position after current cursor
        let next = positions
            .iter()
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

#[cfg(test)]
#[path = "../../../tests/support/ui_app_internal.rs"]
mod tests;
