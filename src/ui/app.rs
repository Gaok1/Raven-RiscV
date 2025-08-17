use super::{editor::Editor, input::handle_key, view::ui};
use crate::falcon::{self, Cpu, Ram};
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(PartialEq, Eq, Copy, Clone)]
pub(super) enum FileDialogMode {
    Import,
    Export,
}

pub(super) struct FileDialog {
    pub mode: FileDialogMode,
    pub filename: String,
    pub error: Option<String>,
}

impl FileDialog {
    pub fn new(mode: FileDialogMode) -> Self {
        Self {
            mode,
            filename: String::new(),
            error: None,
        }
    }
}

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
    pub(super) mem_region: MemRegion,
    pub(super) show_registers: bool,
    pub(super) show_hex: bool,
    pub(super) regs_scroll: usize,
    pub(super) is_running: bool,
    pub(super) last_step_time: Instant,
    pub(super) step_interval: Duration,
    pub(super) faulted: bool,

    // Docs state
    pub(super) docs_scroll: usize,

    // Import/export dialog
    pub(super) file_dialog: Option<FileDialog>,
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
            mem_region: MemRegion::Data,
            show_registers: true,
            show_hex: true,
            regs_scroll: 0,
            is_running: false,
            last_step_time: Instant::now(),
            step_interval: Duration::from_millis(80),
            faulted: false,
            docs_scroll: 0,
            file_dialog: None,
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
                load_words(&mut self.mem, self.base_pc, &prog.text);
                load_bytes(&mut self.mem, prog.data_base, &prog.data);

                self.data_base = prog.data_base;
                self.mem_view_addr = prog.data_base;
                self.mem_region = MemRegion::Data;

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
                let (line, msg) = extract_line_info(&e);
                self.diag_line = line;
                self.diag_msg = Some(msg.clone());
                self.diag_line_text = line.and_then(|l| self.editor.lines.get(l).cloned());
                self.last_compile_ok = Some(false);
                self.last_assemble_msg = Some(format!(
                    "Assemble error at line {}: {}",
                    line.map(|n| n + 1).unwrap_or(0),
                    msg
                ));
            }
        }
    }

    // Lightweight background syntax check (does not reset CPU/mem)
    fn check_assemble(&mut self) {
        use falcon::asm::assemble;
        match assemble(&self.editor.text(), self.base_pc) {
            Ok(prog) => {
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
                let (line, msg) = extract_line_info(&e);
                self.diag_line = line;
                self.diag_msg = Some(msg.clone());
                self.diag_line_text = line.and_then(|l| self.editor.lines.get(l).cloned());
                self.last_compile_ok = Some(false);
            }
        }
        self.editor_dirty = false;
    }

    fn tick(&mut self) {
        if self.is_running && self.last_step_time.elapsed() >= self.step_interval {
            self.single_step();
            self.last_step_time = Instant::now();
        }
        // auto syntax check while in editor, with debounce
        if matches!(self.tab, Tab::Editor) && self.editor_dirty {
            if let Some(t) = self.last_edit_at {
                if t.elapsed() >= self.auto_check_delay {
                    self.check_assemble();
                }
            }
        }
    }

    pub(super) fn single_step(&mut self) {
        self.prev_x = self.cpu.x; // snapshot before step
        self.prev_pc = self.cpu.pc;
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            falcon::exec::step(&mut self.cpu, &mut self.mem)
        }));
        let alive = match res {
            Ok(v) => v,
            Err(_) => {
                self.faulted = true;
                false
            }
        };
        if !alive {
            self.is_running = false;
        }
    }
}

pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    let mut last_draw = Instant::now();
    loop {
        // Input
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(&mut app, key)? {
                    break;
                }
            }
        }
        // Tick/run
        app.tick();
        // Draw ~60 FPS cap
        if last_draw.elapsed() >= Duration::from_millis(16) {
            terminal.draw(|f| ui(f, &app))?;
            last_draw = Instant::now();
        }
    }
    Ok(())
}
fn extract_line_info(err: &str) -> (Option<usize>, String) {
    // very lightweight: find first integer in the message and treat as 1-based line
    let mut num: Option<usize> = None;
    let mut cur = String::new();
    for ch in err.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else {
            if !cur.is_empty() {
                if let Ok(n) = cur.parse::<usize>() {
                    num = Some(n.saturating_sub(1));
                    break;
                }
                cur.clear();
            }
        }
    }
    if num.is_none() && !cur.is_empty() {
        if let Ok(n) = cur.parse::<usize>() {
            num = Some(n.saturating_sub(1));
        }
    }
    (num, err.to_string())
}
