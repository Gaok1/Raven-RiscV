use crate::ui::app::{App, EditorMode, MemRegion, Tab, Lang};
use crate::ui::view::docs::docs_total_rows;
use arboard::Clipboard;
use crossterm::{event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers}, terminal};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use rfd::FileDialog as OSFileDialog;
use std::{io, time::Instant};


use super::max_regs_scroll;

pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }

    // When waiting for console input, capture characters regardless of mode/tab
    if app.console.reading {
        match key.code {
            KeyCode::Char(c) => app.console.current.push(c),
            KeyCode::Backspace => {
                app.console.current.pop();
            }
            KeyCode::Enter => {
                let line = std::mem::take(&mut app.console.current);
                app.console.push_input(line);
                app.console.reading = false;
                // Resume CPU execution after providing input
                app.is_running = true;
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.show_exit_popup {
        if key.code == KeyCode::Esc {
            app.show_exit_popup = false;
        }
        return Ok(false);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match app.mode {
        EditorMode::Insert => {
            // Esc: leave insert -> command (stop typing)
            if key.code == KeyCode::Esc {
                app.mode = EditorMode::Command;
                return Ok(false);
            }

            // Ctrl+R: Restart simulation if on Run tab; otherwise assemble
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                if matches!(app.tab, Tab::Run) {
                    app.restart_simulation();
                } else {
                    app.assemble_and_load();
                }
                return Ok(false);
            }

            // Toggle language (Ctrl+L)
            if ctrl && matches!(key.code, KeyCode::Char('l')) {
                app.lang = match app.lang { Lang::EN => Lang::PT, Lang::PT => Lang::EN };
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas", "asm"])
                    .pick_file()
                {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.editor.lines = content.lines().map(|s| s.to_string()).collect();
                        app.editor.cursor_row = 0;
                        app.editor.cursor_col = 0;
                        // Assemble immediately after importing a new file
                        app.assemble_and_load();
                    }
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas", "asm"])
                    .set_file_name("program.fas")
                    .save_file()
                {
                    let _ = std::fs::write(path, app.editor.text());
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('c')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.selected_text() {
                    if let Ok(mut clip) = Clipboard::new() {
                        let _ = clip.set_text(text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('z')) && matches!(app.tab, Tab::Editor) {
                app.editor.undo();
                app.editor_dirty = true;
                app.last_edit_at = Some(Instant::now());
                app.diag_line = None;
                app.diag_msg = None;
                app.diag_line_text = None;
                app.last_compile_ok = None;
                app.last_assemble_msg = None;
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
                    KeyCode::BackTab => app.editor.shift_tab(),
                    KeyCode::Tab => app.editor.tab(),
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
            // Quit popup remains available
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') {
                app.show_exit_popup = true;
                return Ok(false);
            }

            // Ctrl+R: Restart simulation if on Run tab; otherwise assemble
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                if matches!(app.tab, Tab::Run) {
                    app.restart_simulation();
                } else {
                    app.assemble_and_load();
                }
                return Ok(false);
            }

            // Toggle language (Ctrl+L)
            if ctrl && matches!(key.code, KeyCode::Char('l')) {
                app.lang = match app.lang { Lang::EN => Lang::PT, Lang::PT => Lang::EN };
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('c')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.selected_text() {
                    if let Ok(mut clip) = Clipboard::new() {
                        let _ = clip.set_text(text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('z')) && matches!(app.tab, Tab::Editor) {
                app.editor.undo();
                app.editor_dirty = true;
                app.last_edit_at = Some(Instant::now());
                app.diag_line = None;
                app.diag_msg = None;
                app.diag_line_text = None;
                app.last_compile_ok = None;
                app.last_assemble_msg = None;
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
                        // Assemble immediately after importing a new file
                        app.assemble_and_load();
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
                // Remove keyboard-based mode switching and tab switching; tabs change via mouse only

                // Run controls
                (KeyCode::Char('s'), Tab::Run) => {
                    if !app.faulted {
                        app.single_step();
                    }
                }
                (KeyCode::Char('r'), Tab::Run) => {
                    if !app.faulted {
                        app.is_running = true;
                    }
                }
                (KeyCode::Char('p'), Tab::Run) => {
                    app.is_running = false;
                }
                (KeyCode::Up, Tab::Run) if ctrl => {
                    let visible = app.console_height.saturating_sub(3) as usize;
                    let max_scroll = app.console.lines.len().saturating_sub(visible);
                    if app.console.scroll > max_scroll {
                        app.console.scroll = max_scroll;
                    }
                    app.console.scroll = (app.console.scroll + 1).min(max_scroll);
                }
                (KeyCode::Down, Tab::Run) if ctrl => {
                    let visible = app.console_height.saturating_sub(3) as usize;
                    let max_scroll = app.console.lines.len().saturating_sub(visible);
                    if app.console.scroll > max_scroll {
                        app.console.scroll = max_scroll;
                    }
                    app.console.scroll = app.console.scroll.saturating_sub(1);
                }
                (KeyCode::Up, Tab::Run) if app.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.regs_scroll = app.regs_scroll.saturating_sub(1);
                    if app.regs_scroll > max_scroll {
                        app.regs_scroll = max_scroll;
                    }
                }
                (KeyCode::Down, Tab::Run) if app.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.regs_scroll > max_scroll {
                        app.regs_scroll = max_scroll;
                    }
                    app.regs_scroll = (app.regs_scroll + 1).min(max_scroll);
                }
                (KeyCode::PageUp, Tab::Run) if app.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.regs_scroll = app.regs_scroll.saturating_sub(10);
                    if app.regs_scroll > max_scroll {
                        app.regs_scroll = max_scroll;
                    }
                }
                (KeyCode::PageDown, Tab::Run) if app.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.regs_scroll > max_scroll {
                        app.regs_scroll = max_scroll;
                    }
                    app.regs_scroll = (app.regs_scroll + 10).min(max_scroll);
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

                // Docs scroll
                (KeyCode::Up, Tab::Docs) => { app.docs_scroll = app.docs_scroll.saturating_sub(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::Down, Tab::Docs) => { app.docs_scroll = app.docs_scroll.saturating_add(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageUp, Tab::Docs) => { app.docs_scroll = app.docs_scroll.saturating_sub(10); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageDown, Tab::Docs) => { app.docs_scroll = app.docs_scroll.saturating_add(10); clamp_docs_scroll_keyboard(app); }
                
                // Editor navigation in command mode (optional)
                (KeyCode::Up, Tab::Editor) => app.editor.move_up(),
                (KeyCode::Down, Tab::Editor) => app.editor.move_down(),
                _ => {}
            }
        }
    }

    Ok(false)
}

fn clamp_docs_scroll_keyboard(app: &mut App) {
    // Approximate visible rows using current terminal height and docs layout
    if let Ok((_, h)) = terminal::size() {
        // Root layout: 3 (tabs) + min(5) main + 1 (status)
        let docs_area_h = h.saturating_sub(4) as usize;
        // Docs split: 1 header + body
        let body_h = docs_area_h.saturating_sub(1);
        // Paragraph border (2) + ASCII table overhead (4)
        let visible_rows = body_h.saturating_sub(6);
        let total = docs_total_rows();
        let max_start = total.saturating_sub(visible_rows);
        if app.docs_scroll > max_start { app.docs_scroll = max_start; }
    }
}

