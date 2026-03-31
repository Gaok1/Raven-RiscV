use crate::ui::app::{App, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    if !matches!(app.tab, Tab::Editor) || !(app.editor.find_open || app.editor.goto_open) {
        return false;
    }

    match key.code {
        KeyCode::Esc => {
            app.editor.find_open = false;
            app.editor.goto_open = false;
            app.editor.replace_open = false;
            app.editor.find_in_replace = false;
        }
        KeyCode::Enter => {
            if app.editor.goto_open {
                if let Ok(n) = app.editor.goto_query.trim().parse::<usize>() {
                    let row = n
                        .saturating_sub(1)
                        .min(app.editor.buf.lines.len().saturating_sub(1));
                    app.editor.buf.cursor_row = row;
                    app.editor.buf.cursor_col = 0;
                }
                app.editor.goto_open = false;
                app.editor.goto_query.clear();
            } else if app.editor.find_open && app.editor.find_in_replace {
                if let Some(&(row, col)) = app.editor.find_matches.get(app.editor.find_current) {
                    let q_chars = app.editor.find_query.chars().count();
                    let end_col = col + q_chars;
                    app.editor.buf.snapshot();
                    let sb = crate::ui::editor::Editor::byte_at(&app.editor.buf.lines[row], col);
                    let eb =
                        crate::ui::editor::Editor::byte_at(&app.editor.buf.lines[row], end_col);
                    let rep = app.editor.replace_query.clone();
                    app.editor.buf.lines[row].replace_range(sb..eb, &rep);
                    app.editor.find_matches = crate::ui::app::compute_find_matches(
                        &app.editor.find_query,
                        &app.editor.buf.lines,
                    );
                    app.editor.find_current = app
                        .editor
                        .find_current
                        .min(app.editor.find_matches.len().saturating_sub(1));
                    app.editor.dirty = true;
                    app.editor.last_edit_at = Some(Instant::now());
                }
            } else if !app.editor.find_matches.is_empty() {
                app.editor.find_current =
                    (app.editor.find_current + 1) % app.editor.find_matches.len();
                let (row, col) = app.editor.find_matches[app.editor.find_current];
                app.editor.buf.cursor_row = row;
                app.editor.buf.cursor_col = col;
            }
        }
        KeyCode::BackTab | KeyCode::Tab if app.editor.find_open => {
            app.editor.find_in_replace = !app.editor.find_in_replace;
            if app.editor.find_in_replace {
                app.editor.replace_open = true;
            }
        }
        KeyCode::Backspace => {
            if app.editor.goto_open {
                app.editor.goto_query.pop();
            } else if app.editor.find_in_replace {
                app.editor.replace_query.pop();
            } else {
                app.editor.find_query.pop();
                app.editor.find_matches = crate::ui::app::compute_find_matches(
                    &app.editor.find_query,
                    &app.editor.buf.lines,
                );
                app.editor.find_current = 0;
            }
        }
        KeyCode::Char(c) => {
            let ctrl_pressed = key.modifiers.contains(KeyModifiers::CONTROL);
            if ctrl_pressed && c == 'f' {
                if !app.editor.find_matches.is_empty() {
                    app.editor.find_current =
                        (app.editor.find_current + 1) % app.editor.find_matches.len();
                    let (row, col) = app.editor.find_matches[app.editor.find_current];
                    app.editor.buf.cursor_row = row;
                    app.editor.buf.cursor_col = col;
                }
            } else if !ctrl_pressed {
                if app.editor.goto_open {
                    if c.is_ascii_digit() {
                        app.editor.goto_query.push(c);
                    }
                } else if app.editor.find_in_replace {
                    app.editor.replace_query.push(c);
                } else {
                    app.editor.find_query.push(c);
                    app.editor.find_matches = crate::ui::app::compute_find_matches(
                        &app.editor.find_query,
                        &app.editor.buf.lines,
                    );
                    if !app.editor.find_matches.is_empty() {
                        let cursor_pos = (app.editor.buf.cursor_row, app.editor.buf.cursor_col);
                        let idx = app
                            .editor
                            .find_matches
                            .iter()
                            .position(|&(r, c_)| (r, c_) >= cursor_pos)
                            .unwrap_or(0);
                        app.editor.find_current = idx;
                        let (row, col) = app.editor.find_matches[idx];
                        app.editor.buf.cursor_row = row;
                        app.editor.buf.cursor_col = col;
                    } else {
                        app.editor.find_current = 0;
                    }
                }
            }
        }
        _ => {}
    }

    true
}
