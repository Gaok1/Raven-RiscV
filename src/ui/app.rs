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

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum Lang {
    EN,
    PT,
}

pub struct App {
    pub(super) tab: Tab,
    pub(super) mode: EditorMode,
    // Editor state
    pub(super) editor: Editor,
    pub(super) editor_dirty: bool,
    pub(super) last_edit_at: Option<Instant>,
    pub(super) auto_check_delay: Duration,
    pub(super) last_assemble_msg: Option<String>,
    pub(super) last_compile_ok: Option<bool>,

    // Keep last successfully assembled program for restart/loading without re-assembling
    pub(super) last_ok_text: Option<Vec<u32>>,   // instructions
    pub(super) last_ok_data: Option<Vec<u8>>,    // data bytes
    pub(super) last_ok_data_base: Option<u32>,   // data base address

    // Compile diagnostics
    pub(super) diag_line: Option<usize>, // 0-based line index
    pub(super) diag_msg: Option<String>,
    pub(super) diag_line_text: Option<String>,

    // Execution state
    pub(super) cpu: Cpu,
    pub(super) prev_x: [u32; 32],
    pub(super) prev_pc: u32,
    pub(super) mem: Ram,
    pub(super) mem_size: usize,
    pub(super) base_pc: u32,
    pub(super) data_base: u32,
    pub(super) mem_view_addr: u32,
    pub(super) mem_view_bytes: u32,
    pub(super) mem_region: MemRegion,
    pub(super) show_registers: bool,
    pub(super) fmt_mode: FormatMode,
    pub(super) show_signed: bool,
    pub(super) imem_width: u16,
    pub(super) hover_imem_bar: bool,
    pub(super) imem_drag: bool,
    pub(super) imem_drag_start_x: u16,
    pub(super) imem_width_start: u16,
    pub(super) imem_scroll: usize,
    pub(super) hover_imem_addr: Option<u32>,
    pub(super) console_height: u16,
    pub(super) hover_console_bar: bool,
    pub(super) hover_console_clear: bool,
    pub(super) console_drag: bool,
    pub(super) console_drag_start_y: u16,
    pub(super) console_height_start: u16,
    pub(super) regs_scroll: usize,
    pub(super) is_running: bool,
    pub(super) last_step_time: Instant,
    pub(super) step_interval: Duration,
    pub(super) faulted: bool,
    pub(super) show_exit_popup: bool,
    pub(super) should_quit: bool,

    // Docs state
    pub(super) docs_scroll: usize,

    // Language
    pub(super) lang: Lang,

    // Mouse tracking
    pub(super) mouse_x: u16,
    pub(super) mouse_y: u16,
    pub(super) hover_tab: Option<Tab>,
    pub(super) hover_run_button: Option<RunButton>,

    // Console for program I/O
    pub(super) console: Console,
}

impl App {
    pub fn new() -> Self {
        let mut cpu = Cpu::default();
        let base_pc = 0x0000_0000;
        cpu.pc = base_pc;
        let mem_size = 128 * 1024;
        cpu.write(2, mem_size as u32 - 4); // initialize stack pointer to a valid word
        let data_base = base_pc + 0x1000;
        Self {
            tab: Tab::Editor,
            mode: EditorMode::Insert,
            editor: Editor::with_sample(),
            editor_dirty: true,
            last_edit_at: Some(Instant::now()),
            auto_check_delay: Duration::from_millis(400),
            last_assemble_msg: None,
            last_compile_ok: None,
            last_ok_text: None,
            last_ok_data: None,
            last_ok_data_base: None,
            diag_line: None,
            diag_msg: None,
            diag_line_text: None,
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
            show_exit_popup: false,
            should_quit: false,
            docs_scroll: 0,
            lang: Lang::EN,
            mouse_x: 0,
            mouse_y: 0,
            hover_tab: None,
            hover_run_button: None,
            console: Console::default(),
        }
    }

    pub(super) fn assemble_and_load(&mut self) {
        use falcon::asm::assemble;
        use falcon::program::{load_bytes, load_words};

        self.prev_x = self.cpu.x; // keep snapshot before reset
        self.mem_size = 128 * 1024;
        self.cpu = Cpu::default();
        self.cpu.pc = self.base_pc;
        self.prev_pc = self.cpu.pc;
        self.cpu.write(2, self.mem_size as u32 - 4); // reset stack pointer
        self.mem = Ram::new(self.mem_size);
        self.faulted = false;

        match assemble(&self.editor.text(), self.base_pc) {
            Ok(prog) => {
                if let Err(e) = load_words(&mut self.mem, self.base_pc, &prog.text) {
                    self.console.push_error(e.to_string());
                    self.faulted = true;
                    return;
                }
                if let Err(e) = load_bytes(&mut self.mem, prog.data_base, &prog.data) {
                    self.console.push_error(e.to_string());
                    self.faulted = true;
                    return;
                }

                self.data_base = prog.data_base;
                self.mem_view_addr = prog.data_base;
                self.mem_region = MemRegion::Data;

                // Save last good program for restart
                self.last_ok_text = Some(prog.text.clone());
                self.last_ok_data = Some(prog.data.clone());
                self.last_ok_data_base = Some(prog.data_base);
                self.imem_scroll = 0;
                self.hover_imem_addr = None;

                self.last_assemble_msg = Some(format!(
                    "Assembled {} instructions, {} data bytes.",
                    prog.text.len(),
                    prog.data.len()
                ));
                self.last_compile_ok = Some(true);
                self.diag_line = None;
                self.diag_msg = None;
                self.diag_line_text = None;
            }
            Err(e) => {
                self.diag_line = Some(e.line);
                self.diag_msg = Some(e.msg.clone());
                self.diag_line_text = self.editor.lines.get(e.line).cloned();
                self.last_compile_ok = Some(false);
                self.last_assemble_msg =
                    Some(format!("Assemble error at line {}: {}", e.line + 1, e.msg));
            }
        }
    }

    // Lightweight background syntax check (does not reset CPU/mem)
    fn check_assemble(&mut self) {
        use falcon::asm::assemble;
        match assemble(&self.editor.text(), self.base_pc) {
            Ok(prog) => {
                // Save last good program snapshot
                self.last_ok_text = Some(prog.text.clone());
                self.last_ok_data = Some(prog.data.clone());
                self.last_ok_data_base = Some(prog.data_base);
                self.last_assemble_msg = Some(format!(
                    "OK: {} instructions, {} data bytes",
                    prog.text.len(),
                    prog.data.len()
                ));
                self.last_compile_ok = Some(true);
                self.diag_line = None;
                self.diag_msg = None;
                self.diag_line_text = None;
            }
            Err(e) => {
                self.diag_line = Some(e.line);
                self.diag_msg = Some(e.msg.clone());
                self.diag_line_text = self.editor.lines.get(e.line).cloned();
                self.last_compile_ok = Some(false);
            }
        }
        self.editor_dirty = false;
    }

    // Load the last successfully assembled program without re-parsing source
    fn load_last_ok_program(&mut self) {
        use falcon::program::{load_bytes, load_words};
        if let (Some(ref text), Some(ref data), Some(data_base)) = (
            self.last_ok_text.as_ref(),
            self.last_ok_data.as_ref(),
            self.last_ok_data_base,
        ) {
            self.prev_x = self.cpu.x; // keep snapshot before reset
            self.mem_size = 128 * 1024;
            self.cpu = Cpu::default();
            self.cpu.pc = self.base_pc;
            self.prev_pc = self.cpu.pc;
            self.cpu.write(2, self.mem_size as u32 - 4); // reset stack pointer
            self.mem = Ram::new(self.mem_size);
            self.faulted = false;

            if let Err(e) = load_words(&mut self.mem, self.base_pc, text) {
                self.console.push_error(e.to_string());
                self.faulted = true;
                return;
            }
            if let Err(e) = load_bytes(&mut self.mem, data_base, data) {
                self.console.push_error(e.to_string());
                self.faulted = true;
                return;
            }

            self.data_base = data_base;
            self.mem_view_addr = data_base;
            self.mem_region = MemRegion::Data;

            // Mirror the manual assemble message for consistency
            self.last_assemble_msg = Some(format!(
                "Assembled {} instructions, {} data bytes.",
                text.len(),
                data.len()
            ));
            self.last_compile_ok = Some(true);
            self.imem_scroll = 0;
            self.hover_imem_addr = None;
        }
    }

    // Public: restart simulation to the initial state of the last good program
    pub(super) fn restart_simulation(&mut self) {
        self.is_running = false;
        self.faulted = false;
        self.load_last_ok_program();
    }

    // Load a raw binary into memory at base_pc and update editor/disasm state
    pub(super) fn load_binary(&mut self, bytes: &[u8]) {
        use falcon::program::load_bytes;
        self.prev_x = self.cpu.x;
        self.mem_size = 128 * 1024;
        self.cpu = Cpu::default();
        self.cpu.pc = self.base_pc;
        self.prev_pc = self.cpu.pc;
        self.cpu.write(2, self.mem_size as u32 - 4);
        self.mem = Ram::new(self.mem_size);
        self.faulted = false;

        if let Err(e) = load_bytes(&mut self.mem, self.base_pc, bytes) {
            self.console.push_error(e.to_string());
            self.faulted = true;
            return;
        }

        // Convert bytes to words for instruction display and export
        let mut words = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut b = [0u8; 4];
            for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
            words.push(u32::from_le_bytes(b));
        }
        self.last_ok_text = Some(words);
        self.last_ok_data = Some(Vec::new());
        self.last_ok_data_base = Some(self.data_base);
        self.last_assemble_msg = Some(format!(
            "Loaded binary: {} bytes ({} words)",
            bytes.len(),
            self.last_ok_text.as_ref().map(|v| v.len()).unwrap_or(0)
        ));
        self.last_compile_ok = Some(true);
        self.diag_line = None;
        self.diag_msg = None;
        self.diag_line_text = None;
        self.imem_scroll = 0;
        self.hover_imem_addr = None;
    }

    fn tick(&mut self) {
        if self.is_running && self.last_step_time.elapsed() >= self.step_interval {
            self.single_step();
            self.last_step_time = Instant::now();
        }
        // While running, keep instruction memory view following the PC
        if self.is_running {
            if self.cpu.pc >= self.base_pc {
                let pc_idx = ((self.cpu.pc - self.base_pc) / 4) as usize;
                self.imem_scroll = pc_idx.saturating_sub(2);
            } else {
                self.imem_scroll = 0;
            }
        }
        // Auto syntax check while in editor, with debounce
        if matches!(self.tab, Tab::Editor) && self.editor_dirty {
            if let Some(t) = self.last_edit_at {
                if t.elapsed() >= self.auto_check_delay {
                    self.check_assemble();
                    // If source has no errors, auto-build (assemble and load) using last good program
                    if self.last_compile_ok == Some(true) {
                        self.load_last_ok_program();
                    }
                }
            }
        }
    }

    pub(super) fn single_step(&mut self) {
        self.prev_x = self.cpu.x; // snapshot before step
        self.prev_pc = self.cpu.pc;
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            falcon::exec::step(&mut self.cpu, &mut self.mem, &mut self.console)
        }));
        let alive = match res {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                self.console.push_error(e.to_string());
                self.faulted = true;
                false
            }
            Err(_) => {
                self.faulted = true;
                false
            }
        };
        if !alive {
            self.is_running = false;
            if !self.console.reading {
                self.faulted = true;
            }
        }

    }
}

pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    execute!(terminal.backend_mut(), EnableMouseCapture)?;
    let last_draw = Instant::now();
    loop {
        // Input
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
        // Tick/run
        app.tick();
        // Draw ~60 FPS cap
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, &app))?;
            //last_draw = Instant::now();
        }
    }
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    Ok(())
}
