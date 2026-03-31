use crate::ui::app::{App, Tab};
use crossterm::event::{KeyCode, KeyEvent};

use super::editor_shared::{
    handle_common_shortcuts, handle_insert_readonly_guard, mark_editor_edited,
};

pub(super) fn handle(app: &mut App, key: KeyEvent, ctrl: bool, shift: bool) -> bool {
    if key.code == KeyCode::Esc {
        app.mode = crate::ui::app::EditorMode::Command;
        return true;
    }

    if handle_insert_readonly_guard(app, key) {
        return true;
    }

    if handle_common_shortcuts(app, key, ctrl) {
        return true;
    }

    if ctrl && matches!(app.tab, Tab::Editor) {
        match key.code {
            KeyCode::Char('a') => {
                app.editor.buf.select_all();
                return true;
            }
            KeyCode::Char('w') => {
                app.editor.buf.select_word_at_cursor();
                return true;
            }
            KeyCode::Char('f') => {
                if app.editor.find_open {
                    if !app.editor.find_matches.is_empty() {
                        app.editor.find_current =
                            (app.editor.find_current + 1) % app.editor.find_matches.len();
                        let (row, col) = app.editor.find_matches[app.editor.find_current];
                        app.editor.buf.cursor_row = row;
                        app.editor.buf.cursor_col = col;
                    }
                } else {
                    app.editor.find_open = true;
                    app.editor.goto_open = false;
                    app.editor.find_query.clear();
                    app.editor.replace_query.clear();
                    app.editor.find_in_replace = false;
                    app.editor.find_matches.clear();
                    app.editor.find_current = 0;
                }
                return true;
            }
            KeyCode::Char('h') => {
                app.editor.find_open = true;
                app.editor.replace_open = true;
                app.editor.goto_open = false;
                app.editor.find_in_replace = false;
                app.editor.find_query.clear();
                app.editor.replace_query.clear();
                app.editor.find_matches.clear();
                app.editor.find_current = 0;
                return true;
            }
            KeyCode::Char('g') => {
                app.editor.goto_open = true;
                app.editor.find_open = false;
                app.editor.goto_query.clear();
                return true;
            }
            KeyCode::Char('/') => {
                app.editor.buf.toggle_comment();
                mark_editor_edited(app);
                return true;
            }
            KeyCode::Char('d') => {
                app.select_next_occurrence();
                return true;
            }
            _ => {}
        }
    }

    if key.code == KeyCode::F(12) && matches!(app.tab, Tab::Editor) {
        app.goto_label_definition();
        return true;
    }

    if key.code == KeyCode::F(2) && matches!(app.tab, Tab::Editor) {
        app.editor.show_addr_hints = !app.editor.show_addr_hints;
        return true;
    }

    let edited = match (key.code, app.tab) {
        (code, Tab::Editor) => match code {
            KeyCode::Left => {
                update_selection(app, shift);
                app.editor.buf.move_left();
                false
            }
            KeyCode::Right => {
                update_selection(app, shift);
                app.editor.buf.move_right();
                false
            }
            KeyCode::Up => {
                update_selection(app, shift);
                app.editor.buf.move_up();
                false
            }
            KeyCode::Down => {
                update_selection(app, shift);
                app.editor.buf.move_down();
                false
            }
            KeyCode::Home => {
                update_selection(app, shift);
                app.editor.buf.move_home();
                false
            }
            KeyCode::End => {
                update_selection(app, shift);
                app.editor.buf.move_end();
                false
            }
            KeyCode::PageUp => {
                update_selection(app, shift);
                app.editor.buf.page_up();
                false
            }
            KeyCode::PageDown => {
                update_selection(app, shift);
                app.editor.buf.page_down();
                false
            }
            KeyCode::Backspace => {
                app.editor.buf.backspace();
                true
            }
            KeyCode::Delete => {
                app.editor.buf.delete_char();
                true
            }
            KeyCode::Enter => {
                app.editor.buf.enter();
                true
            }
            KeyCode::BackTab => {
                app.editor.buf.shift_tab();
                true
            }
            KeyCode::Tab => {
                app.editor.buf.tab();
                true
            }
            KeyCode::Char(c) => {
                app.editor.buf.insert_char(c);
                true
            }
            _ => false,
        },
        _ => false,
    };

    if edited {
        mark_editor_edited(app);
    }

    edited
}

fn update_selection(app: &mut App, shift: bool) {
    if shift {
        app.editor.buf.start_selection();
    } else {
        app.editor.buf.clear_selection();
    }
}
