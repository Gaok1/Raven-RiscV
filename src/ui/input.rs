use crate::ui::app::RunHover;

use super::app::{App, EditorMode, MemRegion, Tab};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use rfd::FileDialog as OSFileDialog;
use std::{io, time::Instant};

pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match app.mode {
        EditorMode::Insert => {
            // Special: Esc leaves insert -> command
            if key.code == KeyCode::Esc {
                app.mode = EditorMode::Command;
                return Ok(false);
            }

            // Assemble (Ctrl+R) also works in Insert mode
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                app.assemble_and_load();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .pick_file()
                {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.editor.lines = content.lines().map(|s| s.to_string()).collect();
                        app.editor.cursor_row = 0;
                        app.editor.cursor_col = 0;
                    }
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .set_file_name("program.fas")
                    .save_file()
                {
                    let _ = std::fs::write(path, app.editor.text());
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('a')) && matches!(app.tab, Tab::Editor) {
                app.editor.select_all();
                return Ok(false);
            }

            match (key.code, app.tab) {
                // Insert mode: everything types into editor if on Editor tab
                (code, Tab::Editor) => match code {
                    KeyCode::Left => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_left();
                    }
                    KeyCode::Right => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_right();
                    }
                    KeyCode::Up => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_up();
                    }
                    KeyCode::Down => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_down();
                    }
                    KeyCode::Home => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_home();
                    }
                    KeyCode::End => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.move_end();
                    }
                    KeyCode::PageUp => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.page_up();
                    }
                    KeyCode::PageDown => {
                        if shift {
                            app.editor.start_selection();
                        } else {
                            app.editor.clear_selection();
                        }
                        app.editor.page_down();
                    }
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
            // Quit in command mode or close run menu
            if key.code == KeyCode::Esc {
                if app.tab == Tab::Run && app.show_menu {
                    app.show_menu = false;
                    return Ok(false);
                } else {
                    return Ok(true);
                }
            }
            if key.code == KeyCode::Char('q') {
                return Ok(true);
            }

            // Global assemble
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                app.assemble_and_load();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .pick_file()
                {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.editor.lines = content.lines().map(|s| s.to_string()).collect();
                        app.editor.cursor_row = 0;
                        app.editor.cursor_col = 0;
                    }
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .set_file_name("program.fas")
                    .save_file()
                {
                    let _ = std::fs::write(path, app.editor.text());
                }
                return Ok(false);
            }

            match (key.code, app.tab) {
                (KeyCode::Char('i') | KeyCode::Enter, Tab::Editor) => {
                    app.mode = EditorMode::Insert;
                    return Ok(false);
                }
                // Tab switching only in command mode
                (KeyCode::Char('1'), _) => app.tab = Tab::Editor,
                (KeyCode::Char('2'), _) => app.tab = Tab::Run,
                (KeyCode::Char('3'), _) => app.tab = Tab::Docs,

                // Run controls
                (KeyCode::Char('m'), Tab::Run) => {
                    app.show_menu = !app.show_menu;
                }
                (KeyCode::Char('s'), Tab::Run) => {
                    app.single_step();
                }
                (KeyCode::Char('r'), Tab::Run) => {
                    if !app.faulted {
                        app.is_running = true;
                    }
                }
                (KeyCode::Char('p'), Tab::Run) => {
                    app.is_running = false;
                }
                (KeyCode::Char('t'), Tab::Run) if app.show_menu => {
                    app.show_registers = !app.show_registers;
                }
                (KeyCode::Char('f'), Tab::Run) if app.show_menu => {
                    app.show_hex = !app.show_hex;
                }
                (KeyCode::Up, Tab::Run) if app.show_registers => {
                    app.regs_scroll = app.regs_scroll.saturating_sub(1);
                }
                (KeyCode::Down, Tab::Run) if app.show_registers => {
                    app.regs_scroll = app.regs_scroll.saturating_add(1);
                }
                (KeyCode::PageUp, Tab::Run) if app.show_registers => {
                    app.regs_scroll = app.regs_scroll.saturating_sub(10);
                }
                (KeyCode::PageDown, Tab::Run) if app.show_registers => {
                    app.regs_scroll = app.regs_scroll.saturating_add(10);
                }
                (KeyCode::Up, Tab::Run) if !app.show_registers => {
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(app.mem_view_bytes);
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::Down, Tab::Run) if !app.show_registers => {
                    let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                    if app.mem_view_addr < max {
                        app.mem_view_addr = app
                            .mem_view_addr
                            .saturating_add(app.mem_view_bytes)
                            .min(max);
                    }
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageUp, Tab::Run) if !app.show_registers => {
                    let delta: u32 = app.mem_view_bytes * 16;
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(delta);
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageDown, Tab::Run) if !app.show_registers => {
                    let delta: u32 = app.mem_view_bytes * 16;
                    let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                    let new = app.mem_view_addr.saturating_add(delta);
                    app.mem_view_addr = new.min(max);
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::Char('b'), Tab::Run) if app.show_menu && !app.show_registers => {
                    app.mem_view_bytes = match app.mem_view_bytes {
                        4 => 2,
                        2 => 1,
                        _ => 4,
                    };
                }
                (KeyCode::Char('d'), Tab::Run) => {
                    app.mem_view_addr = app.data_base;
                    app.mem_region = MemRegion::Data;
                    app.show_registers = false;
                }
                (KeyCode::Char('k'), Tab::Run) => {
                    app.mem_view_addr = app.cpu.x[2];
                    app.mem_region = MemRegion::Stack;
                    app.show_registers = false;
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


fn render_run_status(f: &mut Frame, area: Rect, app: &App) {
    use RunHover::*;
    let (view_text, view_color) = if app.show_registers { ("REGS", Color::Blue) } else { ("RAM", Color::Green) };
    let (fmt_text, fmt_color)   = if app.show_hex       { ("HEX", Color::Magenta) } else { ("DEC", Color::Cyan) };
    let (region_text, region_color) = match app.mem_region {
        MemRegion::Data => ("DATA", Color::Yellow),
        MemRegion::Stack => ("STACK", Color::LightBlue),
        MemRegion::Custom => ("ADDR", Color::Gray),
    };
    let run_text = if app.is_running { "RUN" } else { "PAUSE" };

    let mut spans: Vec<Span> = Vec::new();
    let emph = |txt: &str, base: Style, active: bool| {
        if active { Span::styled(txt.to_string(), base.bg(Color::DarkGray).add_modifier(Modifier::BOLD)) }
        else { Span::styled(txt.to_string(), base.add_modifier(Modifier::UNDERLINED)) }
    };

    spans.push(Span::raw("View: "));
    spans.push(emph(view_text, Style::default().fg(view_color), app.hover_run == View));
    spans.push(Span::raw("  Format: "));
    spans.push(emph(fmt_text, Style::default().fg(fmt_color), app.hover_run == Format));

    if !app.show_registers {
        let bytes_text = match app.mem_view_bytes { 4=>"4B", 2=>"2B", _=>"1B" };
        spans.push(Span::raw("  Bytes: "));
        spans.push(emph(bytes_text, Style::default().fg(Color::Yellow), app.hover_run == Bytes));
    }

    spans.push(Span::raw("  Region: "));
    spans.push(emph(region_text, Style::default().fg(region_color), app.hover_run == Region));

    spans.push(Span::raw("  State: "));
    spans.push(emph(run_text, Style::default().fg(if app.is_running { Color::Green } else { Color::Red }), app.hover_run == State));

    let line1 = Line::from(spans);
    let line2 = Line::from("Commands: s=step  r=run  p=pause  d=data  k=stack  Up/Down/PgUp/PgDn scroll  m=menu");

    let para = Paragraph::new(vec![line1, line2])
        .block(Block::default().borders(Borders::ALL).title("Run Controls"));
    f.render_widget(para, area);
}


fn render_run_menu(f: &mut Frame, area: Rect, app: &App) {
    use RunHover::*;
    let popup = centered_rect(area.width / 2, area.height / 2, area);
    f.render_widget(Clear, popup);

    let key_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let (view_text, view_color) = if app.show_registers { ("REGS", Color::Blue) } else { ("RAM", Color::Green) };
    let (fmt_text, fmt_color)   = if app.show_hex       { ("HEX", Color::Magenta) } else { ("DEC", Color::Cyan) };
    let (region_text, region_color) = match app.mem_region {
        MemRegion::Data => ("DATA", Color::Yellow),
        MemRegion::Stack => ("STACK", Color::LightBlue),
        MemRegion::Custom => ("ADDR", Color::Gray),
    };
    let bytes_text = match app.mem_view_bytes { 4 => "4B", 2 => "2B", _ => "1B" };
    let (run_text, run_color) = if app.is_running { ("RUN", Color::Green) } else { ("PAUSE", Color::Red) };

    // Cabeçalho de status
    let mut status = vec![
        Span::raw("View: "), Span::styled(view_text, Style::default().fg(view_color)),
        Span::raw("  Format: "), Span::styled(fmt_text, Style::default().fg(fmt_color)),
    ];
    if !app.show_registers {
        status.push(Span::raw("  Bytes: "));
        status.push(Span::styled(bytes_text, Style::default().fg(Color::Yellow)));
    }
    status.push(Span::raw("  Region: "));
    status.push(Span::styled(region_text, Style::default().fg(region_color)));
    status.push(Span::raw("  State: "));
    status.push(Span::styled(run_text, Style::default().fg(run_color)));

    // Helper de “botão” com hover
    let btn = |label: &str, key: &str, hover: bool| -> Vec<Span> {
        let mut v = Vec::new();
        let s_key = if hover { key_style.bg(Color::DarkGray) } else { key_style };
        let s_lbl = if hover { Style::default().bg(Color::DarkGray) } else { Style::default() };
        v.push(Span::styled(format!("[{}]", key), s_key));
        v.push(Span::raw(" "));
        v.push(Span::styled(label.to_string(), s_lbl));
        v
    };

    let lines = vec![
        Line::from(status),
        Line::raw(""),
        Line::from({
            let mut v = Vec::new();
            v.extend(btn("Step", "s", app.hover_run == MStep));
            v.push(Span::raw("  "));
            v.extend(btn("Run", "r", app.hover_run == MRun));
            v.push(Span::raw("  "));
            v.extend(btn("Pause", "p", app.hover_run == MPause));
            v
        }),
        Line::from({
            let mut v = Vec::new();
            v.extend(btn("View data", "d", app.hover_run == MViewData));
            v.push(Span::raw("  "));
            v.extend(btn("View stack", "k", app.hover_run == MViewStack));
            v
        }),
        Line::from(btn("Toggle view (REGS/RAM)", "t", app.hover_run == MToggleView)),
        Line::from(btn("Toggle format (HEX/DEC)", "f", app.hover_run == MToggleFormat)),
        Line::from(if app.show_registers {
            vec![Span::raw("(bytes disabled in REGS view)")]
        } else {
            btn("Cycle bytes (4B/2B/1B)", "b", app.hover_run == MCycleBytes)
        }),
        Line::from(btn("Close menu", "m / Esc", app.hover_run == MClose)),
    ];

    let menu = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(Span::styled("Menu", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
    );
    f.render_widget(menu, popup);
}


fn compute_run_status_hover(app: &App, me: MouseEvent, area: Rect) -> RunHover {
    
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ]).split(area);
    let run = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
        ]).split(root[1]);
    let status = run[1];
    if me.row != status.y + 1 { return None; }

    // Reconstrói os rótulos exatamente como em render_run_status
    let view = if app.show_registers { "REGS" } else { "RAM" };
    let fmt  = if app.show_hex { "HEX" } else { "DEC" };
    let bytes = match app.mem_view_bytes { 4=>"4B", 2=>"2B", _=>"1B" };
    let region = match app.mem_region { MemRegion::Data=>"DATA", MemRegion::Stack=>"STACK", MemRegion::Custom=>"ADDR" };
    let state = if app.is_running { "RUN" } else { "PAUSE" };

    let mut pos = status.x + 1;
    let col = me.column;

    let range = |start: &mut u16, label: &str| {
        let s = *start;
        *start += label.len() as u16;
        (s, *start)
    };
    let skip = |start: &mut u16, s: &str| { *start += s.len() as u16; };

    skip(&mut pos, "View: ");
    let (v0,v1) = range(&mut pos, view);

    skip(&mut pos, "  Format: ");
    let (f0,f1) = range(&mut pos, fmt);

    let (b0,b1) = if !app.show_registers {
        skip(&mut pos, "  Bytes: ");
        let (a,b) = range(&mut pos, bytes);
        (a,b)
    } else { (0,0) };

    skip(&mut pos, "  Region: ");
    let (r0,r1) = range(&mut pos, region);

    skip(&mut pos, "  State: ");
    let (s0,s1) = range(&mut pos, state);

    if col>=v0 && col<v1 { View }
    else if col>=f0 && col<f1 { Format }
    else if !app.show_registers && col>=b0 && col<b1 { Bytes }
    else if col>=r0 && col<r1 { Region }
    else if col>=s0 && col<s1 { State }
    else { None }
}

fn compute_run_menu_hover(app: &App, me: MouseEvent, area: Rect) -> RunHover {
    
    let popup = centered_rect(area.width / 2, area.height / 2, area);
    if me.column < popup.x + 1 || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1 || me.row >= popup.y + popup.height - 1 {
        return None;
    }
    let x = me.column - (popup.x + 1);
    let y = me.row - (popup.y + 1);

    // Mapear as mesmas linhas/labels usadas no render_run_menu
    match y {
        2 => {
            let step = "[s] Step";
            let run  = "[r] Run";
            let pause= "[p] Pause";
            let mut pos: u16 = 0;
            if x < step.len() as u16 { return MStep; }
            pos += step.len() as u16 + 2;
            if x >= pos && x < pos + run.len() as u16 { return MRun; }
            pos += run.len() as u16 + 2;
            if x >= pos && x < pos + pause.len() as u16 { return MPause; }
            None
        }
        3 => {
            let d = "[d] View data";
            let k = "[k] View stack";
            let mut pos: u16 = 0;
            if x < d.len() as u16 { return MViewData; }
            pos += d.len() as u16 + 2;
            if x >= pos && x < pos + k.len() as u16 { return MViewStack; }
            None
        }
        4 => MToggleView,
        5 => MToggleFormat,
        6 => if app.show_registers { None } else { MCycleBytes },
        7 => MClose,
        _ => None,
    }
}




pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

    // Atualiza hover dos tabs (como você já fazia)
    app.hover_tab = None;
    if me.row == area.y + 1 {
        let x = me.column.saturating_sub(area.x + 1);
        let titles = [("Editor", Tab::Editor), ("Run", Tab::Run), ("Docs", Tab::Docs)];
        let divider = " │ ".len() as u16;
        let mut pos: u16 = 0;
        for (i, (title, tab)) in titles.iter().enumerate() {
            let w = title.len() as u16;
            if x >= pos && x < pos + w {
                app.hover_tab = Some(*tab);
                if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                    app.tab = *tab;
                }
                break;
            }
            pos += w;
            if i + 1 < titles.len() { pos += divider; }
        }
    }

    // Scrolls padrão
    match me.kind {
        MouseEventKind::ScrollUp => match app.tab {
            Tab::Editor => app.editor.move_up(),
            Tab::Run => {
                if app.show_registers {
                    app.regs_scroll = app.regs_scroll.saturating_sub(1);
                } else {
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(app.mem_view_bytes);
                    app.mem_region = MemRegion::Custom;
                }
            }
            Tab::Docs => app.docs_scroll = app.docs_scroll.saturating_sub(1),
        },
        MouseEventKind::ScrollDown => match app.tab {
            Tab::Editor => app.editor.move_down(),
            Tab::Run => {
                if app.show_registers {
                    app.regs_scroll = app.regs_scroll.saturating_add(1);
                } else {
                    let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                    if app.mem_view_addr < max {
                        app.mem_view_addr = app.mem_view_addr
                            .saturating_add(app.mem_view_bytes)
                            .min(max);
                    }
                    app.mem_region = MemRegion::Custom;
                }
            }
            Tab::Docs => app.docs_scroll += 1,
        },
        _ => {}
    }

    // --- HOVER/Clique na aba RUN ---
    if let Tab::Run = app.tab {
        // Atualiza hover conforme mouse move
        if matches!(me.kind, MouseEventKind::Moved) {
            if app.show_menu {
                app.hover_run = compute_run_menu_hover(app, me, area);
            } else {
                app.hover_run = compute_run_status_hover(app, me, area);
            }
        }

        // Clique
        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            if app.show_menu {
                handle_run_menu_click(app, me, area);
            } else {
                handle_run_status_click(app, me, area);
            }
        }
    }
}


fn handle_run_status_click(app: &mut App, me: MouseEvent, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(root_chunks[1]);
    let status = run_chunks[1];
    if me.row != status.y + 1 {
        return;
    }

    let view_text = if app.show_registers { "REGS" } else { "RAM" };
    let fmt_text = if app.show_hex { "HEX" } else { "DEC" };
    let bytes_text = match app.mem_view_bytes {
        4 => "4B",
        2 => "2B",
        _ => "1B",
    };
    let region_text = match app.mem_region {
        MemRegion::Data => "DATA",
        MemRegion::Stack => "STACK",
        MemRegion::Custom => "ADDR",
    };
    let run_text = if app.is_running { "RUN" } else { "PAUSE" };

    let mut pos = status.x + 1;
    pos += "View: ".len() as u16;
    let view_start = pos;
    pos += view_text.len() as u16;
    let view_end = pos;

    pos += "  Format: ".len() as u16;
    let fmt_start = pos;
    pos += fmt_text.len() as u16;
    let fmt_end = pos;

    let (bytes_start, bytes_end) = if !app.show_registers {
        pos += "  Bytes: ".len() as u16;
        let start = pos;
        pos += bytes_text.len() as u16;
        let end = pos;
        (start, end)
    } else {
        (0u16, 0u16)
    };

    pos += "  Region: ".len() as u16;
    let region_start = pos;
    pos += region_text.len() as u16;
    let region_end = pos;

    pos += "  State: ".len() as u16;
    let state_start = pos;
    pos += run_text.len() as u16;
    let state_end = pos;

    let col = me.column;
    if col >= view_start && col < view_end {
        app.show_registers = !app.show_registers;
    } else if col >= fmt_start && col < fmt_end {
        app.show_hex = !app.show_hex;
    } else if !app.show_registers && col >= bytes_start && col < bytes_end {
        app.mem_view_bytes = match app.mem_view_bytes {
            4 => 2,
            2 => 1,
            _ => 4,
        };
    } else if !app.show_registers && col >= region_start && col < region_end {
        app.mem_region = match app.mem_region {
            MemRegion::Data => {
                app.mem_view_addr = app.cpu.x[2];
                MemRegion::Stack
            }
            MemRegion::Stack => MemRegion::Custom,
            MemRegion::Custom => {
                app.mem_view_addr = app.data_base;
                MemRegion::Data
            }
        };
    } else if col >= state_start && col < state_end {
        if app.is_running {
            app.is_running = false;
        } else if !app.faulted {
            app.is_running = true;
        }
    }
}

fn handle_run_menu_click(app: &mut App, me: MouseEvent, area: Rect) {
    let popup = centered_rect(area.width / 2, area.height / 2, area);
    if me.column < popup.x + 1
        || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1
        || me.row >= popup.y + popup.height - 1
    {
        app.show_menu = false;
        return;
    }
    let inner_x = me.column - (popup.x + 1);
    let inner_y = me.row - (popup.y + 1);
    match inner_y {
        2 => {
            const STEP: &str = "[s] Step";
            const RUN: &str = "[r] Run";
            const PAUSE: &str = "[p] Pause";
            let mut x = 0u16;
            if inner_x < STEP.len() as u16 {
                app.single_step();
            } else {
                x += STEP.len() as u16 + 2;
                if inner_x >= x && inner_x < x + RUN.len() as u16 {
                    if !app.faulted {
                        app.is_running = true;
                    }
                } else {
                    x += RUN.len() as u16 + 2;
                    if inner_x >= x && inner_x < x + PAUSE.len() as u16 {
                        app.is_running = false;
                    }
                }
            }
        }
        3 => {
            const DATA: &str = "[d] View data";
            const STACK: &str = "[k] View stack";
            let mut x = 0u16;
            if inner_x < DATA.len() as u16 {
                app.mem_view_addr = app.data_base;
                app.mem_region = MemRegion::Data;
                app.show_registers = false;
            } else {
                x += DATA.len() as u16 + 2;
                if inner_x >= x && inner_x < x + STACK.len() as u16 {
                    app.mem_view_addr = app.cpu.x[2];
                    app.mem_region = MemRegion::Stack;
                    app.show_registers = false;
                }
            }
        }
        4 => {
            app.show_registers = !app.show_registers;
        }
        5 => {
            app.show_hex = !app.show_hex;
        }
        6 => {
            if !app.show_registers {
                app.mem_view_bytes = match app.mem_view_bytes {
                    4 => 2,
                    2 => 1,
                    _ => 4,
                };
            }
        }
        7 => {
            app.show_menu = false;
        }
        _ => {}
    }
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Rect::new(
        r.x + (r.width.saturating_sub(width)) / 2,
        r.y + (r.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}
