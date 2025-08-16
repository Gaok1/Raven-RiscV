use super::{
    app::{App, EditorMode, FileDialog, FileDialogMode, MemRegion, Tab},
    editor::Editor,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{io, time::Instant};

pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }
    // If a file dialog is open, handle it first
    if app.file_dialog.is_some() {
        return handle_file_dialog_key(app, key);
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

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                app.file_dialog = Some(FileDialog::new(FileDialogMode::Import));
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                app.file_dialog = Some(FileDialog::new(FileDialogMode::Export));
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
                    KeyCode::End => {
                        app.editor.cursor_col = Editor::char_count(app.editor.current_line())
                    }
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

            // Global assemble
            if ctrl && matches!(key.code, KeyCode::Char('r')) {
                app.assemble_and_load();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                app.file_dialog = Some(FileDialog::new(FileDialogMode::Import));
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                app.file_dialog = Some(FileDialog::new(FileDialogMode::Export));
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
                (KeyCode::Char('s'), Tab::Run) => {
                    app.single_step();
                }
                (KeyCode::Char('r'), Tab::Run) => {
                    app.is_running = true;
                }
                (KeyCode::Char('p'), Tab::Run) => {
                    app.is_running = false;
                }
                (KeyCode::Char('t'), Tab::Run) => {
                    app.show_registers = !app.show_registers;
                }
                (KeyCode::Char('f'), Tab::Run) => {
                    app.show_hex = !app.show_hex;
                }
                (KeyCode::Up, Tab::Run) if !app.show_registers => {
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(4);
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::Down, Tab::Run) if !app.show_registers => {
                    let max = app.mem_size.saturating_sub(4) as u32;
                    if app.mem_view_addr < max {
                        app.mem_view_addr = app.mem_view_addr.saturating_add(4).min(max);
                    }
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageUp, Tab::Run) if !app.show_registers => {
                    let delta: u32 = 4 * 16;
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(delta);
                    app.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageDown, Tab::Run) if !app.show_registers => {
                    let delta: u32 = 4 * 16;
                    let max = app.mem_size.saturating_sub(4) as u32;
                    let new = app.mem_view_addr.saturating_add(delta);
                    app.mem_view_addr = new.min(max);
                    app.mem_region = MemRegion::Custom;
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

fn handle_file_dialog_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if let Some(fd) = &mut app.file_dialog {
        match key.code {
            KeyCode::Esc => {
                app.file_dialog = None;
            }
            KeyCode::Enter => {
                let mut filename = fd.filename.clone();
                if !filename.ends_with(".fas") {
                    filename.push_str(".fas");
                }
                match fd.mode {
                    FileDialogMode::Import => match std::fs::read_to_string(&filename) {
                        Ok(content) => {
                            app.editor.lines = content.lines().map(|s| s.to_string()).collect();
                            app.editor.cursor_row = 0;
                            app.editor.cursor_col = 0;
                            app.file_dialog = None;
                        }
                        Err(e) => {
                            fd.error = Some(e.to_string());
                        }
                    },
                    FileDialogMode::Export => match std::fs::write(&filename, app.editor.text()) {
                        Ok(_) => {
                            app.file_dialog = None;
                        }
                        Err(e) => {
                            fd.error = Some(e.to_string());
                        }
                    },
                }
            }
            KeyCode::Backspace => {
                fd.filename.pop();
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    fd.filename.push(c);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}
