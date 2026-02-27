use super::{
    console::Console,
    editor::Editor,
    input::{handle_key, handle_mouse},
    view::ui,
};
use crate::falcon::{self, Cpu, CacheController};
use crate::falcon::cache::CacheConfig;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
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
}

impl ConfigField {
    pub(super) fn is_numeric(self) -> bool {
        matches!(self, Self::Size | Self::LineSize | Self::Associativity | Self::HitLatency | Self::MissPenalty | Self::AssocPenalty | Self::TransferWidth)
    }
    pub(super) fn all_editable() -> &'static [ConfigField] {
        &[Self::Size, Self::LineSize, Self::Associativity, Self::Replacement,
          Self::WritePolicy, Self::WriteAlloc, Self::HitLatency, Self::MissPenalty,
          Self::AssocPenalty, Self::TransferWidth]
    }
    /// Row index in the rendered fields list (3 = Sets which is read-only, skip it)
    pub(super) fn list_row(self) -> usize {
        match self {
            Self::Size => 0, Self::LineSize => 1, Self::Associativity => 2,
            Self::Replacement => 4, Self::WritePolicy => 5, Self::WriteAlloc => 6,
            Self::HitLatency => 7, Self::MissPenalty => 8,
            Self::AssocPenalty => 9, Self::TransferWidth => 10,
        }
    }
    pub(super) fn from_list_row(row: usize) -> Option<Self> {
        match row {
            0 => Some(Self::Size), 1 => Some(Self::LineSize), 2 => Some(Self::Associativity),
            3 => None, // Sets is read-only
            4 => Some(Self::Replacement), 5 => Some(Self::WritePolicy), 6 => Some(Self::WriteAlloc),
            7 => Some(Self::HitLatency), 8 => Some(Self::MissPenalty),
            9 => Some(Self::AssocPenalty), 10 => Some(Self::TransferWidth),
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
    // Validation errors and status messages
    pub(super) config_error: Option<String>,
    pub(super) config_status: Option<String>,
    // Inline field editing: (is_icache, field) + text buffer for numeric fields
    pub(super) edit_field: Option<(bool, ConfigField)>,
    pub(super) edit_buf: String,
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

    // Compile diagnostics
    pub(super) diag_line: Option<usize>,
    pub(super) diag_msg: Option<String>,
    pub(super) diag_line_text: Option<String>,
}

pub(super) struct RunState {
    pub(super) cpu: Cpu,
    pub(super) prev_x: [u32; 32],
    pub(super) prev_pc: u32,
    pub(super) mem: CacheController,
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
}

pub(super) struct DocsState {
    pub(super) scroll: usize,
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
                diag_line: None,
                diag_msg: None,
                diag_line_text: None,
            },
            run: RunState {
                cpu,
                prev_x: [0; 32],
                prev_pc: base_pc,
                mem_size,
                mem: CacheController::new(CacheConfig::default(), CacheConfig::default(), mem_size),
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
            },
            docs: DocsState { scroll: 0 },
            cache: CacheState {
                subtab: CacheSubtab::Stats,
                scope: CacheScope::Both,
                stats_scroll: 0,
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
                config_error: None,
                config_status: None,
                edit_field: None,
                edit_buf: String::new(),
            },
            show_exit_popup: false,
            should_quit: false,
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
                self.run.mem.icache.invalidate();
                self.run.mem.dcache.invalidate();
                self.run.mem.reset_stats();

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
            self.run.mem.icache.invalidate();
            self.run.mem.dcache.invalidate();
            self.run.mem.reset_stats();

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
        self.run.mem.icache.invalidate();
        self.run.mem.dcache.invalidate();
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

    /// Commit the current numeric edit_buf into pending_icache/pending_dcache.
    pub(super) fn commit_cache_edit(&mut self) {
        if let Some((is_icache, field)) = self.cache.edit_field {
            self.cache.config_error = None;
            self.cache.config_status = None;
            if field.is_numeric() {
                let s = self.cache.edit_buf.trim().to_string();
                let cfg = if is_icache { &mut self.cache.pending_icache } else { &mut self.cache.pending_dcache };
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
        let cfg = if is_icache { &mut self.cache.pending_icache } else { &mut self.cache.pending_dcache };
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
            _ => {}
        }
    }

    /// Current pending config field value as string (for populating edit_buf).
    pub(super) fn cache_field_value_str(&self, is_icache: bool, field: ConfigField) -> String {
        let cfg = if is_icache { &self.cache.pending_icache } else { &self.cache.pending_dcache };
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
        self.run.mem.snapshot_stats();
        if !alive {
            self.run.is_running = false;
            if !self.console.reading {
                self.run.faulted = self.run.cpu.exit_code.is_none();
            }
        }
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
    execute!(terminal.backend_mut(), EnableMouseCapture)?;
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
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    Ok(())
}
