use crate::ui::app::{App, PathInputAction, Tab};
use crossterm::event::{KeyCode, KeyEvent};
use rfd::FileDialog as OSFileDialog;
use std::time::Instant;

use super::paste::paste_editor;
use super::serialization::{open_file_autodetect, open_path_input};

pub(super) fn mark_editor_edited(app: &mut App) {
    app.editor.dirty = true;
    app.editor.last_edit_at = Some(Instant::now());
    app.editor.diag_line = None;
    app.editor.diag_msg = None;
    app.editor.diag_line_text = None;
    app.editor.last_compile_ok = None;
    app.editor.last_build_stats = None;
    app.editor.last_assemble_msg = None;
}

pub(super) fn handle_command_readonly_guard(app: &mut App, key: KeyEvent, ctrl: bool) -> bool {
    if matches!(app.tab, Tab::Editor) && app.editor.last_ok_elf_bytes.is_some() && ctrl {
        match key.code {
            KeyCode::Char('z')
            | KeyCode::Char('y')
            | KeyCode::Char('x')
            | KeyCode::Char('v')
            | KeyCode::Char('/') => {
                app.editor.elf_prompt_open = true;
                true
            }
            _ => false,
        }
    } else {
        false
    }
}

pub(super) fn handle_insert_readonly_guard(app: &mut App, key: KeyEvent) -> bool {
    if matches!(app.tab, Tab::Editor) && app.editor.last_ok_elf_bytes.is_some() {
        match key.code {
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => false,
            _ => {
                app.editor.elf_prompt_open = true;
                true
            }
        }
    } else {
        false
    }
}

pub(super) fn handle_common_shortcuts(app: &mut App, key: KeyEvent, ctrl: bool) -> bool {
    if ctrl && matches!(key.code, KeyCode::Char('o')) {
        open_file(app);
        return true;
    }

    if ctrl && matches!(key.code, KeyCode::Char('s')) {
        save_file(app);
        return true;
    }

    if ctrl && matches!(app.tab, Tab::Editor) {
        match key.code {
            KeyCode::Char('c') => {
                copy_selection(app);
                return true;
            }
            KeyCode::Char('v') => {
                paste_clipboard(app);
                return true;
            }
            KeyCode::Char('z') => {
                app.editor.buf.undo();
                mark_editor_edited(app);
                return true;
            }
            KeyCode::Char('y') => {
                app.editor.buf.redo();
                mark_editor_edited(app);
                return true;
            }
            KeyCode::Char('x') => {
                cut_selection(app);
                return true;
            }
            _ => {}
        }
    }

    if ctrl && key.code == KeyCode::Enter && matches!(app.tab, Tab::Editor) {
        app.assemble_and_load();
        if app.editor.last_compile_ok == Some(true) {
            app.tab = Tab::Run;
        }
        return true;
    }

    if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Editor) {
        app.editor.show_encoding = !app.editor.show_encoding;
        return true;
    }

    false
}

fn open_file(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("Assembly / ELF", &["fas", "asm", "elf", "bin"])
        .add_filter("All Files", &["*"])
        .pick_file()
    {
        open_file_autodetect(app, &path);
    } else {
        open_path_input(app, PathInputAction::OpenFas);
    }
}

fn save_file(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("Falcon ASM", &["fas", "asm"])
        .set_file_name("program.fas")
        .save_file()
    {
        let _ = std::fs::write(path, app.editor.buf.text());
    } else {
        open_path_input(app, PathInputAction::SaveFas);
    }
}

fn copy_selection(app: &mut App) {
    if let Some(text) = app.editor.buf.selected_text() {
        if let Some(clip) = app.clipboard.as_mut() {
            let _ = clip.set_text(text);
        }
    }
}

fn cut_selection(app: &mut App) {
    if let Some(text) = app.editor.buf.selected_text() {
        if let Some(clip) = app.clipboard.as_mut() {
            let _ = clip.set_text(text);
        }
        app.editor.buf.delete_selection();
        mark_editor_edited(app);
    }
}

fn paste_clipboard(app: &mut App) {
    let recent_bracketed = app
        .last_bracketed_paste
        .is_some_and(|t| t.elapsed().as_millis() < 100);
    if recent_bracketed {
        return;
    }

    let text = app.clipboard.as_mut().and_then(|clip| clip.get_text().ok());
    if let Some(text) = text {
        paste_editor(app, &text);
    }
}
