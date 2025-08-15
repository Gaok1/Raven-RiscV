// === Falcon ASM: Ratatui UI (Three tabs: Editor, Run, Docs) ===
// Stable targets (as of Aug 2025):
// - ratatui = "0.28"
// - crossterm = "0.29"
//
// Cargo.toml:
// [dependencies]
// ratatui   = { version = "0.28", features = ["all-widgets"] }
// crossterm = "0.29"
//
// Drop this as src/main.rs or integrate.

mod falcon;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::{
    cmp::min,
    io,
    time::{Duration, Instant},
};

use falcon::{Cpu, Ram};

use crate::falcon::Bus;

// ---------- Simple text editor with lightweight syntax highlighting ----------
#[derive(Default)]
struct Editor {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll: usize,
}

impl Editor {
    fn with_sample() -> Self {
        let sample = vec![
            "addi x1, x0, 5".to_string(),
            "addi x2, x0, 7".to_string(),
            "loop:".to_string(),
            "  add  x3, x1, x2".to_string(),
            "  beq  x3, x0, loop".to_string(),
            "  ecall".to_string(),
        ];
        Self {
            lines: sample,
            cursor_row: 0,
            cursor_col: 0,
            scroll: 0,
        }
    }

    #[inline]
    fn ensure_line(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
    }
    #[inline]
    fn current_line(&self) -> &str {
        self.lines
            .get(self.cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
    #[inline]
    fn current_line_mut(&mut self) -> &mut String {
        self.ensure_line();
        &mut self.lines[self.cursor_row]
    }

    // ---- helpers: trabalhar em índice de caractere, converter p/ byte quando necessário
    #[inline]
    fn char_count(s: &str) -> usize {
        s.chars().count()
    }
    #[inline]
    fn byte_at(s: &str, char_pos: usize) -> usize {
        // retorna len() se for além do fim
        s.char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or_else(|| s.len())
    }

    fn insert_char(&mut self, ch: char) {
        self.ensure_line();
        let line = self.current_line();
        let col = self.cursor_col.min(Self::char_count(line));
        let byte_idx = Self::byte_at(line, col);
        self.current_line_mut().insert(byte_idx, ch);
        self.cursor_col = col + 1; // inserir avança o cursor
    }

    fn insert_spaces(&mut self, n: usize) {
        for _ in 0..n {
            self.insert_char(' ');
        }
    }

    fn backspace(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        if self.cursor_col > 0 {
            // remove o caractere anterior ao cursor
            let col = self.cursor_col - 1;
            let (start, end) = {
                let line = self.current_line();
                (Self::byte_at(line, col), Self::byte_at(line, col + 1))
            };
            self.current_line_mut().replace_range(start..end, "");
            self.cursor_col = col;
        } else if self.cursor_row > 0 {
            // juntar com a linha anterior
            let cur = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len_chars = Self::char_count(&self.lines[self.cursor_row]);
            self.lines[self.cursor_row].push_str(&cur);
            self.cursor_col = prev_len_chars;
        }
    }

    fn delete_char(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let len_chars = Self::char_count(self.current_line());
        let col = self.cursor_col.min(len_chars);
        if col < len_chars {
            // delete no próprio ponto
            let (start, end) = {
                let line = self.current_line();
                (Self::byte_at(line, col), Self::byte_at(line, col + 1))
            };
            self.current_line_mut().replace_range(start..end, "");
        } else if self.cursor_row + 1 < self.lines.len() {
            // fim da linha: mescla com a próxima
            let next = self.lines.remove(self.cursor_row + 1);
            self.current_line_mut().push_str(&next);
        }
    }

    fn enter(&mut self) {
        self.ensure_line();
        let (idx_bytes, rest) = {
            let line = self.current_line();
            let idx = Self::byte_at(line, self.cursor_col.min(Self::char_count(line)));
            (idx, line[idx..].to_string())
        };
        {
            let line_mut = self.current_line_mut();
            line_mut.truncate(idx_bytes);
        }
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, rest);
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = Self::char_count(&self.lines[self.cursor_row]);
        }
    }
    fn move_right(&mut self) {
        let len = Self::char_count(self.current_line());
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }
    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }
    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum Tab {
    Editor,
    Run,
    Docs,
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum EditorMode {
    Insert,
    Command,
}

struct App {
    tab: Tab,
    mode: EditorMode,
    // Editor state
    editor: Editor,
    editor_dirty: bool,
    last_edit_at: Option<Instant>,
    auto_check_delay: Duration,
    last_assemble_msg: Option<String>,
    last_compile_ok: Option<bool>,

    // Compile diagnostics
    diag_line: Option<usize>, // 0-based line index
    diag_msg: Option<String>,
    diag_line_text: Option<String>,

    // Execution state
    cpu: Cpu,
    prev_x: [u32; 32],
    mem: Ram,
    mem_size: usize,
    base_pc: u32,
    is_running: bool,
    last_step_time: Instant,
    step_interval: Duration,

    // Docs state
    docs_scroll: usize,
}

impl App {
    fn new() -> Self {
        let mut cpu = Cpu::default();
        let base_pc = 0x0000_0000;
        cpu.pc = base_pc;
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
            mem_size: 128 * 1024,
            mem: Ram::new(128 * 1024),
            base_pc,
            is_running: false,
            last_step_time: Instant::now(),
            step_interval: Duration::from_millis(80),
            docs_scroll: 0,
        }
    }

    fn assemble_and_load(&mut self) {
        use falcon::asm::assemble;
        use falcon::program::load_words;

        self.prev_x = self.cpu.x; // keep snapshot before reset
        self.cpu = Cpu::default();
        self.cpu.pc = self.base_pc;
        self.mem_size = 128 * 1024;
        self.mem = Ram::new(self.mem_size);

        match assemble(&self.editor.text(), self.base_pc) {
            Ok(words) => {
                load_words(&mut self.mem, self.base_pc, &words);
                self.last_assemble_msg = Some(format!("Assembled {} instructions.", words.len()));
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
            Ok(words) => {
                self.last_assemble_msg = Some(format!("OK: {} instructions", words.len()));
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

    fn single_step(&mut self) {
        self.prev_x = self.cpu.x; // snapshot before step
        let alive = falcon::exec::step(&mut self.cpu, &mut self.mem);
        if !alive {
            self.is_running = false;
        }
    }
}

fn main() -> io::Result<()> {
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run(&mut terminal, App::new());
    ratatui::restore();

    if let Err(e) = res {
        eprintln!("error: {e}");
    }
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
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

fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match app.mode {
        EditorMode::Insert => {
            // Special: Esc leaves insert -> command
            if key.code == KeyCode::Esc {
                app.mode = EditorMode::Command;
                return Ok(false);
            }

            // Assemble (Ctrl+R) também no modo Insert
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                app.assemble_and_load();
                return Ok(false);
            }

            match (key.code, app.tab) {
                // Insert mode: everything types into editor if on Editor tab
                (code, Tab::Editor) => match code {
                    KeyCode::Left => app.editor.move_left(),
                    KeyCode::Right => app.editor.move_right(),
                    KeyCode::Up => app.editor.move_up(),
                    KeyCode::Down => app.editor.move_down(),
                    KeyCode::Backspace => app.editor.backspace(),
                    KeyCode::Delete => app.editor.delete_char(),
                    KeyCode::Enter => app.editor.enter(),
                    KeyCode::Tab => app.editor.insert_spaces(4), // use spaces to avoid cursor width issues
                    KeyCode::Char(c) => app.editor.insert_char(c), // includes '1'/'2'
                    _ => {}
                },
                // In Insert mode, other tabs ignore typing
                _ => {}
            }
            app.editor_dirty = true;
            app.last_edit_at = Some(Instant::now());
            app.diag_line = None;
            app.diag_msg = None;
            app.diag_line_text = None;
            app.last_compile_ok = None;
            app.last_assemble_msg = None;
        }
        EditorMode::Command => {
            // Quit in command mode only
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(true);
            }

            // Mode toggle back to insert
            if matches!(key.code, KeyCode::Char('i') | KeyCode::Char('a')) {
                app.mode = EditorMode::Insert;
                return Ok(false);
            }

            // Global assemble
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                app.assemble_and_load();
                return Ok(false);
            }

            match (key.code, app.tab) {
                // Tab switching only in command mode
                (KeyCode::Char('1'), _) => app.tab = Tab::Editor,
                (KeyCode::Char('2'), _) => app.tab = Tab::Run,
                (KeyCode::Char('3'), _) => app.tab = Tab::Docs,

                // Run controls
                (KeyCode::Char('s'), Tab::Run) => {
                    app.single_step();
                }
                (KeyCode::Char('r'), Tab::Run) => {
                    app.is_running = true;
                }
                (KeyCode::Char('p'), Tab::Run) => {
                    app.is_running = false;
                }

                // Docs scroll
                (KeyCode::Up, Tab::Docs) => {
                    app.docs_scroll = app.docs_scroll.saturating_sub(1);
                }
                (KeyCode::Down, Tab::Docs) => {
                    app.docs_scroll += 1;
                }
                (KeyCode::PageUp, Tab::Docs) => {
                    app.docs_scroll = app.docs_scroll.saturating_sub(10);
                }
                (KeyCode::PageDown, Tab::Docs) => {
                    app.docs_scroll += 10;
                }

                // Editor navigation in command mode (optional)
                (KeyCode::Up, Tab::Editor) => app.editor.move_up(),
                (KeyCode::Down, Tab::Editor) => app.editor.move_down(),
                _ => {}
            }
        }
    }

    Ok(false)
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(5),    // content
            Constraint::Length(1), // status
        ])
        .split(size);

    // Tabs with strong highlight and divider
    let titles = ["Editor", "Run", "Docs"]
        .into_iter()
        .map(|t| Line::from(t))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Falcon ASM"))
        .style(Style::default())
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" │ ", Style::default().fg(Color::DarkGray)))
        .select(match app.tab {
            Tab::Editor => 0,
            Tab::Run => 1,
            Tab::Docs => 2,
        });
    f.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Editor => render_editor(f, chunks[1], app),
        Tab::Run => render_run(f, chunks[1], app),
        Tab::Docs => render_docs(f, chunks[1], app),
    }

    // Status line (mode + diagnostics)
    let mode = match app.mode {
        EditorMode::Insert => "INSERT",
        EditorMode::Command => "COMMAND",
    };
    let mut status = format!(
        "Mode: {}  |  Ctrl+R=Assemble  |  1/2/3 switch tabs (Command mode)",
        mode
    );
    if let Some(ref d) = app.diag_msg {
        let line = app.diag_line.map(|n| n + 1).unwrap_or(0);
        status.push_str(&format!("  |  Error line {}: {}", line, d));
    } else if let Some(ref ok) = app.last_assemble_msg {
        status.push_str(&format!("  |  {}", ok));
    }

    let status = Paragraph::new(status).block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[2]);
}

fn render_editor(f: &mut Frame, area: Rect, app: &App) {
    // Compute visible window and keep cursor visible
    let visible_h = area.height.saturating_sub(2) as usize; // minus borders
    let mut start = app.editor.scroll.min(app.editor.lines.len());
    if app.editor.cursor_row < start {
        start = app.editor.cursor_row;
    }
    if app.editor.cursor_row >= start + visible_h {
        start = app.editor.cursor_row + 1 - visible_h;
    }
    let end = min(app.editor.lines.len(), start + visible_h);

    // Persist scroll so it doesn't jump between frames
    // (This avoids cursor visual drift)
    //
    // Note: long lines are clipped instead of wrapped to keep cursor math correct.
    // Tabs insert 4 spaces to avoid width mismatch.
    let mut rows: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let mut line = Line::from(highlight_line(&app.editor.lines[i]));
        if Some(i) == app.diag_line {
            line = line.style(
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::UNDERLINED),
            );
        }
        rows.push(line);
    }

    let mut title = "Editor (RISC-V ASM) — Esc: Command, i: Insert, Ctrl+R: Assemble".to_string();
    if let Some(ok) = app.last_compile_ok {
        if ok {
            title.push_str(" [OK]");
        } else {
            title.push_str(" [ERROR]");
        }
    }
    let para = Paragraph::new(rows).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);

    // Draw cursor (single cell, no wrapping)
    let cur_row = app.editor.cursor_row as u16;
    let cur_col = app.editor.cursor_col as u16;
    let cursor_x = area.x + 1 + cur_col; // inside left border
    let cursor_y = area.y + 1 + (cur_row - start as u16);
    if cursor_y < area.y + area.height && cursor_x < area.x + area.width {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_run(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let (msg, style) = if app.last_compile_ok == Some(false) {
        let line = app.diag_line.map(|n| n + 1).unwrap_or(0);
        let text = app.diag_line_text.as_deref().unwrap_or("");
        let err = app.diag_msg.as_deref().unwrap_or("");
        (
            format!("Error line {}: {} ({})", line, text, err),
            Style::default().bg(Color::Red).fg(Color::Black),
        )
    } else if app.last_compile_ok == Some(true) {
        (
            app.last_assemble_msg.clone().unwrap_or_default(),
            Style::default().bg(Color::Green).fg(Color::Black),
        )
    } else {
        ("Not compiled".to_string(), Style::default())
    };
    let status = Paragraph::new(msg)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title("Build"));
    f.render_widget(status, chunks[0]);

    let area = chunks[1];
    // layout: left (registers), middle (disasm + format-aware bit view), right (memory)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(38),
            Constraint::Min(46),
            Constraint::Length(38),
        ])
        .split(area);

    // --- Left: registers (highlight changed) ---
    let mut rows = Vec::new();
    for i in 0..32u8 {
        let name = reg_name(i);
        let val = app.cpu.x[i as usize];
        let changed = val != app.prev_x[i as usize];
        let style = if changed {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        rows.push(Row::new(vec![
            Cell::from(format!("x{i:02} ({name})")).style(style),
            Cell::from(format!("0x{val:08x}")).style(style),
            Cell::from(format!("{val:>10}")).style(style),
        ]));
    }
    let reg_table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Min(8),
        ],
    )
    .block(Block::default().borders(Borders::ALL).title("Registers"));
    f.render_widget(reg_table, cols[0]);

    // --- Middle: disassembly + current instruction fields ---
    let mid_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Min(4),
        ])
        .split(cols[1]);

    let (cur_word, disasm_str) = if in_mem_range(app, app.cpu.pc) {
        let w = app.mem.load32(app.cpu.pc);
        let dis = disasm_word(w);
        (w, dis)
    } else {
        (0, "<PC out of RAM>".to_string())
    };

    let pc_line = Paragraph::new(vec![
        Line::from(format!("PC = 0x{:08x}", app.cpu.pc)),
        Line::from(format!("Word = 0x{:08x}", cur_word)),
        Line::from(format!("Instr = {}", disasm_str)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Current Instruction"),
    );
    f.render_widget(pc_line, mid_chunks[0]);

    let fmt = detect_format(cur_word);
    render_bit_fields(f, mid_chunks[1], cur_word, fmt);
    render_field_values(f, mid_chunks[2], cur_word, fmt);

    // --- Right: memory window (hexdump around PC) ---
    let right = cols[2];
    let mem_block = Block::default()
        .borders(Borders::ALL)
        .title("Memory (around PC) — s:step r:run p:pause");
    f.render_widget(mem_block.clone(), right);

    let inner = mem_block.inner(right);
    let mut items = Vec::new();
    let base = app.cpu.pc.saturating_sub(32);
    let lines = inner.height.saturating_sub(2) as u32;
    for off in (0..lines).map(|i| i * 4) {
        // 1 word per row
        let addr = base.wrapping_add(off);
        if in_mem_range(app, addr) {
            let w = app.mem.load32(addr);
            let marker = if addr == app.cpu.pc { "▶" } else { " " };
            items.push(ListItem::new(format!("{marker} 0x{addr:08x}: 0x{w:08x}")));
        }
    }
    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    let text = DOC_TEXT;
    // clip & scroll manually by lines
    let lines: Vec<&str> = text.lines().collect();
    let h = area.height.saturating_sub(2) as usize; // borders
    let start = app.docs_scroll.min(lines.len());
    let end = min(lines.len(), start + h);
    let body = lines[start..end].join(
        "
",
    );
    let para = Paragraph::new(body)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Docs — Supported Instructions (Up/Down/PageUp/PageDown)"),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn in_mem_range(app: &App, addr: u32) -> bool {
    (addr as usize) < app.mem_size.saturating_sub(3)
}

fn reg_name(i: u8) -> &'static str {
    match i {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0/fp",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "",
    }
}

// --- Syntax highlight (very lightweight tokenizer) ---
fn highlight_line(s: &str) -> Vec<Span<'_>> {
    use Color::*;
    if s.is_empty() {
        return vec![Span::raw("")];
    }

    let mut out = Vec::new();

    // preservar espaços à esquerda exatamente
    let mut lead_len = 0usize;
    for ch in s.chars() {
        if ch.is_whitespace() {
            lead_len += ch.len_utf8();
        } else {
            break;
        }
    }
    if lead_len > 0 {
        out.push(Span::raw(&s[..lead_len]));
    }
    let trimmed = &s[lead_len..];

    // achar fim do primeiro token (mnemonico/label) SEM perder o espaço seguinte
    let first_end = trimmed
        .char_indices()
        .find(|&(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(trimmed.len());

    let first = &trimmed[..first_end];
    let rest = &trimmed[first_end..]; // inclui os espaços imediatamente após o primeiro token

    if first.ends_with(':') {
        out.push(Span::styled(first, Style::default().fg(Yellow)));
        if !rest.is_empty() {
            out.push(Span::raw(rest));
        } // mantém tudo (inclui espaços)
        return out;
    }

    // mnemonico
    out.push(Span::styled(
        first,
        Style::default().fg(Cyan).add_modifier(Modifier::BOLD),
    ));

    // Operandos + pontuação, preservando espaços exatamente
    let mut token = String::new();
    for ch in rest.chars() {
        if ",()\t ".contains(ch) {
            if !token.is_empty() {
                out.push(color_operand(&token));
                token.clear();
            }
            out.push(Span::raw(ch.to_string())); // preserva separadores e espaços
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        out.push(color_operand(&token));
    }

    out
}

fn color_operand(tok: &str) -> Span<'static> {
    use Color::*;
    let is_xreg = tok.starts_with('x') && tok[1..].chars().all(|c| c.is_ascii_digit());
    let is_alias = matches!(
        tok,
        "zero"
            | "ra"
            | "sp"
            | "gp"
            | "tp"
            | "s0"
            | "fp"
            | "s1"
            | "s2"
            | "s3"
            | "s4"
            | "s5"
            | "s6"
            | "s7"
            | "s8"
            | "s9"
            | "s10"
            | "s11"
            | "t0"
            | "t1"
            | "t2"
            | "t3"
            | "t4"
            | "t5"
            | "t6"
            | "a0"
            | "a1"
            | "a2"
            | "a3"
            | "a4"
            | "a5"
            | "a6"
            | "a7"
    );
    let is_imm = tok.starts_with("0x") || tok.parse::<i32>().is_ok();
    let style = if is_xreg || is_alias {
        Style::default().fg(Green)
    } else if is_imm {
        Style::default().fg(Magenta)
    } else {
        Style::default()
    };
    // Owned content -> Span<'static>, so it doesn't borrow from `tok`
    Span::styled(tok.to_string(), style)
}

// ---------- Format-aware bit visualization ----------
#[derive(Clone, Copy)]
enum EncFormat {
    R,
    I,
    S,
    B,
    U,
    J,
}

fn detect_format(word: u32) -> EncFormat {
    let opc = word & 0x7f;
    match opc {
        0x33 => EncFormat::R,
        0x13 | 0x03 | 0x67 => EncFormat::I, // op-imm, loads, jalr
        0x23 => EncFormat::S,
        0x63 => EncFormat::B,
        0x37 | 0x17 => EncFormat::U, // lui, auipc
        0x6f => EncFormat::J,        // jal
        _ => EncFormat::R,           // default visualization
    }
}

fn render_bit_fields(f: &mut Frame, area: Rect, _w: u32, fmt: EncFormat) {
    use Color::*;
    let (segments, title) = match fmt {
        EncFormat::R => (
            vec![
                ("funct7", 7, Red),
                ("rs2", 5, LightRed),
                ("rs1", 5, LightMagenta),
                ("funct3", 3, Yellow),
                ("rd", 5, LightGreen),
                ("opcode", 7, Cyan),
            ],
            "Field map (R-type)",
        ),
        EncFormat::I => (
            vec![
                ("imm[11:0]", 12, Blue),
                ("rs1", 5, LightMagenta),
                ("funct3", 3, Yellow),
                ("rd", 5, LightGreen),
                ("opcode", 7, Cyan),
            ],
            "Field map (I-type)",
        ),
        EncFormat::S => (
            vec![
                ("imm[11:5]", 7, Blue),
                ("rs2", 5, LightRed),
                ("rs1", 5, LightMagenta),
                ("funct3", 3, Yellow),
                ("imm[4:0]", 5, Blue),
                ("opcode", 7, Cyan),
            ],
            "Field map (S-type)",
        ),
        EncFormat::B => (
            vec![
                ("imm[12]", 1, Blue),
                ("imm[10:5]", 6, Blue),
                ("rs2", 5, LightRed),
                ("rs1", 5, LightMagenta),
                ("funct3", 3, Yellow),
                ("imm[4:1]", 4, Blue),
                ("imm[11]", 1, Blue),
                ("opcode", 7, Cyan),
            ],
            "Field map (B-type)",
        ),
        EncFormat::U => (
            vec![
                ("imm[31:12]", 20, Blue),
                ("rd", 5, LightGreen),
                ("opcode", 7, Cyan),
            ],
            "Field map (U-type)",
        ),
        EncFormat::J => (
            vec![
                ("imm[20]", 1, Blue),
                ("imm[10:1]", 10, Blue),
                ("imm[11]", 1, Blue),
                ("imm[19:12]", 8, Blue),
                ("rd", 5, LightGreen),
                ("opcode", 7, Cyan),
            ],
            "Field map (J-type)",
        ),
    };

    // Visual: colored bars + labels in field order (MSB..LSB)
    let spans: Vec<Span> = segments
        .into_iter()
        .map(|(label, width, color)| {
            let bar = "▮".repeat(width.max(1));
            Span::styled(format!("{} {} ", bar, label), Style::default().fg(color))
        })
        .collect();

    let bits_line = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    f.render_widget(bits_line, area);
}

fn render_field_values(f: &mut Frame, area: Rect, w: u32, fmt: EncFormat) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Parsed fields");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut text = Vec::new();
    match fmt {
        EncFormat::R => {
            let funct7 = (w >> 25) & 0x7f;
            let rs2 = (w >> 20) & 0x1f;
            let rs1 = (w >> 15) & 0x1f;
            let funct3 = (w >> 12) & 0x7;
            let rd = (w >> 7) & 0x1f;
            let opcode = w & 0x7f;
            text.push(Line::from(format!(
                "funct7={:#04x}  rs2={}  rs1={}  funct3={:#03x}  rd={}  opcode={:#04x}",
                funct7, rs2, rs1, funct3, rd, opcode
            )));
        }
        EncFormat::I => {
            let imm = (((w >> 20) as i32) << 20) >> 20; // sign-extend 12
            let rs1 = (w >> 15) & 0x1f;
            let funct3 = (w >> 12) & 0x7;
            let rd = (w >> 7) & 0x1f;
            let opcode = w & 0x7f;
            text.push(Line::from(format!(
                "imm={}  rs1={}  funct3={:#03x}  rd={}  opcode={:#04x}",
                imm, rs1, funct3, rd, opcode
            )));
            if matches!(funct3, 0x1 | 0x5) {
                // SLLI/SRLI/SRAI
                let shamt = (w >> 20) & 0x1f;
                let f7 = (w >> 25) & 0x7f;
                text.push(Line::from(format!(
                    "(shift) funct7={:#04x} shamt={} rs1={} rd={}",
                    f7, shamt, rs1, rd
                )));
            }
        }
        EncFormat::S => {
            let imm_4_0 = (w >> 7) & 0x1f;
            let funct3 = (w >> 12) & 0x7;
            let rs1 = (w >> 15) & 0x1f;
            let rs2 = (w >> 20) & 0x1f;
            let imm_11_5 = (w >> 25) & 0x7f;
            let opcode = w & 0x7f;
            let imm = (((((imm_11_5 << 5) | imm_4_0) as i32) << 20) >> 20) as i32;
            text.push(Line::from(format!("imm[11:5]={:#04x} imm[4:0]={:#03x} => imm={}  rs2={} rs1={} funct3={:#03x} opcode={:#04x}", imm_11_5, imm_4_0, imm, rs2, rs1, funct3, opcode)));
        }
        EncFormat::B => {
            let b12 = (w >> 31) & 0x1;
            let b10_5 = (w >> 25) & 0x3f;
            let rs2 = (w >> 20) & 0x1f;
            let rs1 = (w >> 15) & 0x1f;
            let f3 = (w >> 12) & 0x7;
            let b4_1 = (w >> 8) & 0xf;
            let b11 = (w >> 7) & 0x1;
            let opc = w & 0x7f;
            let imm = (((((b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1)) as i32) << 19)
                >> 19) as i32;
            text.push(Line::from(format!("b12={} b11={} b10:5={:#04x} b4:1={:#03x} => imm={}  rs2={} rs1={} f3={:#03x} opc={:#04x}", b12, b11, b10_5, b4_1, imm, rs2, rs1, f3, opc)));
        }
        EncFormat::U => {
            let rd = (w >> 7) & 0x1f;
            let opc = w & 0x7f;
            let imm = (w & 0xfffff000) as i32;
            text.push(Line::from(format!(
                "imm[31:12]={:#07x} => imm={}  rd={} opc={:#04x}",
                imm >> 12,
                imm,
                rd,
                opc
            )));
        }
        EncFormat::J => {
            let b20 = (w >> 31) & 1;
            let b10_1 = (w >> 21) & 0x3ff;
            let b11 = (w >> 20) & 1;
            let b19_12 = (w >> 12) & 0xff;
            let rd = (w >> 7) & 0x1f;
            let opc = w & 0x7f;
            let imm = (((((b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1)) as i32) << 11)
                >> 11) as i32;
            text.push(Line::from(format!(
                "b20={} b19:12={:#04x} b11={} b10:1={:#05x} => imm={} rd={} opc={:#04x}",
                b20, b19_12, b11, b10_1, imm, rd, opc
            )));
        }
    }

    let para = Paragraph::new(text).wrap(Wrap { trim: true });
    f.render_widget(para, inner);
}

// ---------- Disassembly helper using your decoder ----------
fn disasm_word(w: u32) -> String {
    match falcon::decoder::decode(w) {
        Ok(ins) => pretty_instr(&ins),
        Err(e) => format!("<decode error: {e}>"),
    }
}

fn pretty_instr(i: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;
    match *i {
        // R-type
        Add { rd, rs1, rs2 } => format!("add  x{rd}, x{rs1}, x{rs2}"),
        Sub { rd, rs1, rs2 } => format!("sub  x{rd}, x{rs1}, x{rs2}"),
        And { rd, rs1, rs2 } => format!("and  x{rd}, x{rs1}, x{rs2}"),
        Or { rd, rs1, rs2 } => format!("or   x{rd}, x{rs1}, x{rs2}"),
        Xor { rd, rs1, rs2 } => format!("xor  x{rd}, x{rs1}, x{rs2}"),
        Sll { rd, rs1, rs2 } => format!("sll  x{rd}, x{rs1}, x{rs2}"),
        Srl { rd, rs1, rs2 } => format!("srl  x{rd}, x{rs1}, x{rs2}"),
        Sra { rd, rs1, rs2 } => format!("sra  x{rd}, x{rs1}, x{rs2}"),
        // I-type
        Addi { rd, rs1, imm } => format!("addi x{rd}, x{rs1}, {imm}"),
        Andi { rd, rs1, imm } => format!("andi x{rd}, x{rs1}, {imm}"),
        Ori { rd, rs1, imm } => format!("ori  x{rd}, x{rs1}, {imm}"),
        Xori { rd, rs1, imm } => format!("xori x{rd}, x{rs1}, {imm}"),
        Slli { rd, rs1, shamt } => format!("slli x{rd}, x{rs1}, {shamt}"),
        Srli { rd, rs1, shamt } => format!("srli x{rd}, x{rs1}, {shamt}"),
        Srai { rd, rs1, shamt } => format!("srai x{rd}, x{rs1}, {shamt}"),
        // Loads
        Lb { rd, rs1, imm } => format!("lb   x{rd}, {imm}(x{rs1})"),
        Lh { rd, rs1, imm } => format!("lh   x{rd}, {imm}(x{rs1})"),
        Lw { rd, rs1, imm } => format!("lw   x{rd}, {imm}(x{rs1})"),
        Lbu { rd, rs1, imm } => format!("lbu  x{rd}, {imm}(x{rs1})"),
        Lhu { rd, rs1, imm } => format!("lhu  x{rd}, {imm}(x{rs1})"),
        // Stores
        Sb { rs2, rs1, imm } => format!("sb   x{rs2}, {imm}(x{rs1})"),
        Sh { rs2, rs1, imm } => format!("sh   x{rs2}, {imm}(x{rs1})"),
        Sw { rs2, rs1, imm } => format!("sw   x{rs2}, {imm}(x{rs1})"),
        // Branches
        Beq { rs1, rs2, imm } => format!("beq  x{rs1}, x{rs2}, {imm}"),
        Bne { rs1, rs2, imm } => format!("bne  x{rs1}, x{rs2}, {imm}"),
        Blt { rs1, rs2, imm } => format!("blt  x{rs1}, x{rs2}, {imm}"),
        Bge { rs1, rs2, imm } => format!("bge  x{rs1}, x{rs2}, {imm}"),
        Bltu { rs1, rs2, imm } => format!("bltu x{rs1}, x{rs2}, {imm}"),
        Bgeu { rs1, rs2, imm } => format!("bgeu x{rs1}, x{rs2}, {imm}"),
        // U/J
        Lui { rd, imm } => format!("lui  x{rd}, {imm}"),
        Auipc { rd, imm } => format!("auipc x{rd}, {imm}"),
        Jal { rd, imm } => format!("jal  x{rd}, {imm}"),
        // JALR & system
        Jalr { rd, rs1, imm } => format!("jalr x{rd}, x{rs1}, {imm}"),
        Ecall => "ecall".to_string(),
        Ebreak => "ebreak".to_string(),
    }
}

// --- diagnostics: try to pull a line number out of assembler error messages ---
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

// --- Static docs text (short version synced with docs/format.md) ---
const DOC_TEXT: &str = r#"Falcon ASM — Supported Instructions (RV32I MVP)

R-type (opcode 0x33):
  ADD, SUB, AND, OR, XOR, SLL, SRL, SRA

I-type (opcode 0x13):
  ADDI, ANDI, ORI, XORI, SLLI, SRLI, SRAI

Loads (opcode 0x03):
  LB, LH, LW, LBU, LHU

Stores (opcode 0x23):
  SB, SH, SW

Branches (opcode 0x63):
  BEQ, BNE, BLT, BGE, BLTU, BGEU

Upper immediates:
  LUI (0x37), AUIPC (0x17)

Jumps:
  JAL (0x6F), JALR (0x67)

System:
  ECALL (0x00000073), EBREAK (0x00100073)

Notes:
• PC advances +4 each instruction. Branch/JAL immediates are byte offsets (must be even).
• Loads/Stores syntax: imm(rs1). Labels supported by 2-pass assembler.
• Pseudoinstructions: nop, mv, li(12-bit), j, jr, ret.
"#;
