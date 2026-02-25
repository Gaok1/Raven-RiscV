use super::{
    console::Console,
    editor::Editor,
    input::{handle_key, handle_mouse},
    view::ui,
};
use crate::falcon::{self, Cpu, Ram};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
};
use ratatui::{DefaultTerminal, layout::Rect};
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum Tab {
    Editor,
    Run,
    Docs,
}

impl Tab {
    pub(super) fn all() -> &'static [Tab] {
        &[Tab::Editor, Tab::Run, Tab::Docs]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Editor => "Editor",
            Tab::Run => "Run",
            Tab::Docs => "Docs",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
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

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum RunButton {
    View,
    Format,
    Sign,
    Bytes,
    Region,
    State,
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
    pub(super) mem: Ram,
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

    // Instruction memory panel (resizable)
    pub(super) imem_width: u16,
    pub(super) hover_imem_bar: bool,
    pub(super) imem_drag: bool,
    pub(super) imem_drag_start_x: u16,
    pub(super) imem_width_start: u16,
    pub(super) imem_scroll: usize,
    pub(super) hover_imem_addr: Option<u32>,

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

    pub(super) show_exit_popup: bool,
    pub(super) should_quit: bool,

    // Mouse tracking (shared across tabs)
    pub(super) mouse_x: u16,
    pub(super) mouse_y: u16,
    pub(super) hover_tab: Option<Tab>,
    pub(super) hover_run_button: Option<RunButton>,

    // Program I/O console (shared across tabs)
    pub(super) console: Console,
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
                mem: Ram::new(mem_size),
                base_pc,
                data_base,
                mem_view_addr: data_base,
                mem_view_bytes: 4,
                mem_region: MemRegion::Data,
                show_registers: true,
                fmt_mode: FormatMode::Hex,
                show_signed: false,
                imem_width: 38,
                hover_imem_bar: false,
                imem_drag: false,
                imem_drag_start_x: 0,
                imem_width_start: 38,
                imem_scroll: 0,
                hover_imem_addr: None,
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
            },
            docs: DocsState { scroll: 0 },
            show_exit_popup: false,
            should_quit: false,
            mouse_x: 0,
            mouse_y: 0,
            hover_tab: None,
            hover_run_button: None,
            console: Console::default(),
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
        self.run.mem = Ram::new(self.run.mem_size);
        self.run.faulted = false;

        match assemble(&self.editor.buf.text(), self.run.base_pc) {
            Ok(prog) => {
                if let Err(e) = load_words(&mut self.run.mem, self.run.base_pc, &prog.text) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
                if let Err(e) = load_bytes(&mut self.run.mem, prog.data_base, &prog.data) {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
                let bss_base = prog.data_base.saturating_add(prog.data.len() as u32);
                if prog.bss_size > 0 {
                    if let Err(e) = zero_bytes(&mut self.run.mem, bss_base, prog.bss_size) {
                        self.console.push_error(e.to_string());
                        self.run.faulted = true;
                        return;
                    }
                }

                self.run.data_base = prog.data_base;
                self.run.mem_view_addr = prog.data_base;
                self.run.mem_region = MemRegion::Data;

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
            self.run.mem = Ram::new(self.run.mem_size);
            self.run.faulted = false;

            if let Err(e) = load_words(&mut self.run.mem, self.run.base_pc, text) {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                return;
            }
            if let Err(e) = load_bytes(&mut self.run.mem, data_base, data) {
                self.console.push_error(e.to_string());
                self.run.faulted = true;
                return;
            }
            if let Some(bss) = self.editor.last_ok_bss_size {
                if bss > 0 {
                    let bss_base = data_base.saturating_add(data.len() as u32);
                    if let Err(e) = zero_bytes(&mut self.run.mem, bss_base, bss) {
                        self.console.push_error(e.to_string());
                        self.run.faulted = true;
                        return;
                    }
                }
            }

            self.run.data_base = data_base;
            self.run.mem_view_addr = data_base;
            self.run.mem_region = MemRegion::Data;

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
        use falcon::program::load_bytes;
        self.run.prev_x = self.run.cpu.x;
        self.run.mem_size = 128 * 1024;
        self.run.cpu = Cpu::default();
        self.run.cpu.pc = self.run.base_pc;
        self.run.prev_pc = self.run.cpu.pc;
        self.run.cpu.write(2, self.run.mem_size as u32 - 4);
        self.run.mem = Ram::new(self.run.mem_size);
        self.run.faulted = false;

        if let Err(e) = load_bytes(&mut self.run.mem, self.run.base_pc, bytes) {
            self.console.push_error(e.to_string());
            self.run.faulted = true;
            return;
        }

        let mut words = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut b = [0u8; 4];
            for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
            words.push(u32::from_le_bytes(b));
        }
        self.editor.last_ok_text = Some(words);
        self.editor.last_ok_data = Some(Vec::new());
        self.editor.last_ok_data_base = Some(self.run.data_base);
        self.editor.last_assemble_msg = Some(format!(
            "Loaded binary: {} bytes ({} words)",
            bytes.len(),
            self.editor.last_ok_text.as_ref().map(|v| v.len()).unwrap_or(0)
        ));
        self.editor.last_compile_ok = Some(true);
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;
        self.run.imem_scroll = 0;
        self.run.hover_imem_addr = None;
    }

    fn tick(&mut self) {
        if self.run.is_running && self.run.last_step_time.elapsed() >= self.run.step_interval {
            self.single_step();
            self.run.last_step_time = Instant::now();
        }
        if self.run.is_running {
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
        if !alive {
            self.run.is_running = false;
            if !self.console.reading {
                self.run.faulted = self.run.cpu.exit_code.is_none();
            }
        }
    }
}

pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    execute!(terminal.backend_mut(), EnableMouseCapture)?;
    let last_draw = Instant::now();
    loop {
        if event::poll(Duration::from_millis(10))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key(&mut app, key)? {
                        break;
                    }
                }
                Event::Mouse(me) => {
                    let size = terminal.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    handle_mouse(&mut app, me, area);
                    if app.should_quit {
                        break;
                    }
                }
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
        app.tick();
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, &app))?;
        }
    }
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    Ok(())
}
