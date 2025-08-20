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

pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

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
            Tab::Docs => {
                app.docs_scroll = app.docs_scroll.saturating_sub(1);
            }
        },
        MouseEventKind::ScrollDown => match app.tab {
            Tab::Editor => app.editor.move_down(),
            Tab::Run => {
                if app.show_registers {
                    app.regs_scroll = app.regs_scroll.saturating_add(1);
                } else {
                    let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                    if app.mem_view_addr < max {
                        app.mem_view_addr = app
                            .mem_view_addr
                            .saturating_add(app.mem_view_bytes)
                            .min(max);
                    }
                    app.mem_region = MemRegion::Custom;
                }
            }
            Tab::Docs => {
                app.docs_scroll += 1;
            }
        },
        MouseEventKind::Down(MouseButton::Left) => {
            if let Tab::Run = app.tab {
                if app.show_menu {
                    handle_run_menu_click(app, me, area);
                } else {
                    handle_run_status_click(app, me, area);
                }
            }
        }
        _ => {}
    }

    // Determine which tab (if any) is hovered by the cursor. Tabs are laid out
    // exactly as rendered: "Editor │ Run │ Docs". We compute the bounds based on
    // the title widths and divider rather than splitting the area evenly so that
    // the hover/click regions align with the actual tab positions.
    app.hover_tab = None;
    if me.row == area.y + 1 {
        let x = me.column.saturating_sub(area.x + 1); // inside border
        let titles = [
            ("Editor", Tab::Editor),
            ("Run", Tab::Run),
            ("Docs", Tab::Docs),
        ];
        let divider = " │ ".len() as u16;
        let mut pos: u16 = 0;
        for (i, (title, tab)) in titles.iter().enumerate() {
            let title_width = title.len() as u16; // ASCII titles
            if x >= pos && x < pos + title_width {
                app.hover_tab = Some(*tab);
                if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                    app.tab = *tab;
                }
                break;
            }
            pos += title_width;
            if i < titles.len() - 1 {
                pos += divider;
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
