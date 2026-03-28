mod serialization;
use self::serialization::*;
pub(crate) use self::serialization::{do_export_cfg, do_export_results, do_import_cfg, open_path_input};

use crate::ui::app::{
    App, CacheScope, CacheSubtab, DocsPage, EditorMode,
    MemRegion, PathInputAction, SETTINGS_ROW_CACHE_ENABLED,
    SETTINGS_ROW_CPI_START, SETTINGS_ROW_MAX_CORES, SETTINGS_ROW_MEM_SIZE,
    SETTINGS_ROW_PIPELINE_ENABLED, SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROWS, Tab,
};
use crate::ui::view::docs::{ALL_MASK, FILTER_ITEMS, docs_body_line_count};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal,
};
use rfd::FileDialog as OSFileDialog;
use std::{io, time::Instant};

use super::max_regs_scroll;

fn handle_run_execution_key(app: &mut App, code: KeyCode) -> bool {
    if !matches!(app.tab, Tab::Run) {
        return false;
    }

    match code {
        KeyCode::Char('s') => {
            if app.core_status(app.selected_core) == crate::ui::app::HartLifecycle::Paused
                || !app.run.faulted
            {
                app.single_step();
            }
            true
        }
        KeyCode::Char('r') => {
            if app.core_status(app.selected_core) == crate::ui::app::HartLifecycle::Paused
                || !app.run.faulted
            {
                app.resume_selected_hart();
                if app.can_start_run() {
                    app.run.is_running = true;
                }
            }
            true
        }
        KeyCode::Char('p') => {
            if app.run.is_running {
                app.run.is_running = false;
            }
            true
        }
        _ => false,
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }

    // Path input bar intercept
    if app.path_input.open {
        match key.code {
            KeyCode::Esc => {
                app.path_input.open = false;
                app.path_input.query.clear();
                app.path_input.completions.clear();
            }
            KeyCode::Enter => {
                let q = app.path_input.query.trim().to_string();
                let path = std::path::PathBuf::from(&q);
                // If the target is a directory, navigate into it instead of dispatching
                if q.ends_with('/') || path.is_dir() {
                    if !q.ends_with('/') {
                        app.path_input.query = format!("{q}/");
                    }
                    refresh_path_completions(&mut app.path_input);
                } else {
                    let action = app.path_input.action.clone();
                    app.path_input.open = false;
                    app.path_input.query.clear();
                    app.path_input.completions.clear();
                    dispatch_path_input(app, action, path);
                }
            }
            KeyCode::Tab => {
                if !app.path_input.completions.is_empty() {
                    app.path_input.query =
                        app.path_input.completions[app.path_input.completion_sel].clone();
                    if app.path_input.query.ends_with('/') {
                        // Navigated into a directory — refresh to show its contents
                        refresh_path_completions(&mut app.path_input);
                    } else {
                        // File completion — cycle through siblings
                        app.path_input.completion_sel =
                            (app.path_input.completion_sel + 1) % app.path_input.completions.len();
                    }
                }
            }
            KeyCode::Down => {
                if !app.path_input.completions.is_empty() {
                    app.path_input.completion_sel =
                        (app.path_input.completion_sel + 1) % app.path_input.completions.len();
                    app.path_input.query =
                        app.path_input.completions[app.path_input.completion_sel].clone();
                }
            }
            KeyCode::Up => {
                if !app.path_input.completions.is_empty() {
                    let n = app.path_input.completions.len();
                    app.path_input.completion_sel = if app.path_input.completion_sel == 0 {
                        n - 1
                    } else {
                        app.path_input.completion_sel - 1
                    };
                    app.path_input.query =
                        app.path_input.completions[app.path_input.completion_sel].clone();
                }
            }
            KeyCode::Backspace => {
                app.path_input.query.pop();
                refresh_path_completions(&mut app.path_input);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.path_input.query.push(c);
                refresh_path_completions(&mut app.path_input);
            }
            _ => {}
        }
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
                if app.can_start_run() {
                    app.run.is_running = true;
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.show_exit_popup {
        match key.code {
            KeyCode::Esc => app.show_exit_popup = false,
            KeyCode::Enter | KeyCode::Char('y') => return Ok(true),
            _ => {}
        }
        return Ok(false);
    }

    // Tutorial intercept — arrow keys navigate steps, Esc closes
    if app.tutorial.active {
        use crate::ui::tutorial::{advance_tutorial, retreat_tutorial};
        match key.code {
            KeyCode::Esc => {
                app.tutorial.active = false;
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => advance_tutorial(app),
            KeyCode::Left | KeyCode::Backspace => retreat_tutorial(app),
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.tutorial.lang = app.tutorial.lang.toggle();
            }
            _ => {}
        }
        return Ok(false);
    }

    // Help popup intercept — Esc closes, ←/→ navigate pages, any other key closes
    if app.help_open {
        // Count pages by matching tab
        let pages_count: usize = match app.tab {
            Tab::Run => 2,
            Tab::Editor => 1,
            Tab::Cache => 1,
            Tab::Pipeline => 1,
            Tab::Docs => 1,
            Tab::Config => 1,
        };
        match key.code {
            KeyCode::Esc => {
                app.help_open = false;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                app.help_page = app.help_page.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                app.help_page = (app.help_page + 1).min(pages_count.saturating_sub(1));
            }
            _ => {
                app.help_open = false;
            }
        }
        return Ok(false);
    }

    // Find bar / goto bar intercept (Editor tab)
    if matches!(app.tab, Tab::Editor) && (app.editor.find_open || app.editor.goto_open) {
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
                    if let Some(&(row, col)) = app.editor.find_matches.get(app.editor.find_current)
                    {
                        let q_chars = app.editor.find_query.chars().count();
                        let end_col = col + q_chars;
                        app.editor.buf.snapshot();
                        let sb =
                            crate::ui::editor::Editor::byte_at(&app.editor.buf.lines[row], col);
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
                } else {
                    if !app.editor.find_matches.is_empty() {
                        app.editor.find_current =
                            (app.editor.find_current + 1) % app.editor.find_matches.len();
                        let (row, col) = app.editor.find_matches[app.editor.find_current];
                        app.editor.buf.cursor_row = row;
                        app.editor.buf.cursor_col = col;
                    }
                }
            }
            KeyCode::BackTab if app.editor.find_open => {
                app.editor.find_in_replace = !app.editor.find_in_replace;
            }
            KeyCode::Tab if app.editor.find_open => {
                app.editor.find_in_replace = !app.editor.find_in_replace;
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
                let mods = key.modifiers;
                let ctrl_pressed = mods.contains(crossterm::event::KeyModifiers::CONTROL);
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
        return Ok(false);
    }

    // Imem label search bar intercept
    if matches!(app.tab, Tab::Run) && app.run.imem_search_open {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.run.imem_search_open = false;
                app.run.imem_search_query.clear();
            }
            KeyCode::Backspace => {
                app.run.imem_search_query.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.run.imem_search_query.push(c);
            }
            _ => {}
        }
        apply_imem_search(app);
        return Ok(false);
    }

    // RAM jump bar intercept
    if matches!(app.tab, Tab::Run) && !app.run.show_registers && app.run.mem_search_open {
        match key.code {
            KeyCode::Esc => {
                app.run.mem_search_open = false;
                app.run.mem_search_query.clear();
            }
            KeyCode::Enter => {
                app.run.mem_search_open = false;
                // address already applied live — just close
            }
            KeyCode::Backspace => {
                app.run.mem_search_query.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.run.mem_search_query.push(c);
            }
            _ => {}
        }
        apply_mem_search(app);
        return Ok(false);
    }

    // Docs search bar intercept
    if matches!(app.tab, Tab::Docs) && app.docs.search_open {
        match key.code {
            KeyCode::Esc => {
                app.docs.search_open = false;
                app.docs.search_query.clear();
            }
            KeyCode::Backspace => {
                app.docs.search_query.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                app.docs.search_query.push(c);
            }
            _ => {}
        }
        return Ok(false);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // ELF prompt: intercept all keys while the popup is open.
    if app.editor.elf_prompt_open && matches!(app.tab, Tab::Editor) {
        if key.code == KeyCode::Esc {
            app.editor.elf_prompt_open = false;
        }
        return Ok(false);
    }

    // Snapshot detail popup: Esc closes it, everything else is swallowed.
    if app.cache.viewing_snapshot.is_some() && matches!(app.tab, Tab::Cache) {
        if key.code == KeyCode::Esc {
            app.cache.viewing_snapshot = None;
        }
        return Ok(false);
    }

    if matches!(app.tab, Tab::Run) && matches!(key.code, KeyCode::Char('R')) {
        app.restart_simulation();
        return Ok(false);
    }

    if matches!(app.tab, Tab::Run | Tab::Pipeline) {
        match key.code {
            KeyCode::Char('[') => {
                app.cycle_selected_core(-1);
                return Ok(false);
            }
            KeyCode::Char(']') => {
                app.cycle_selected_core(1);
                return Ok(false);
            }
            _ => {}
        }
    }

    // '?' opens tutorial (non-Docs tabs) or help popup (Docs tab)
    if key.code == KeyCode::Char('?') {
        if !matches!(app.tab, Tab::Docs) && !crate::ui::tutorial::get_steps(app.tab).is_empty() {
            crate::ui::tutorial::start_tutorial(app);
        } else {
            app.help_open = !app.help_open;
            app.help_page = 0;
        }
        return Ok(false);
    }

    // F9: toggle breakpoint — works in any mode when on Run tab
    if key.code == KeyCode::F(9) && matches!(app.tab, Tab::Run) {
        let addr = app.run.hover_imem_addr.unwrap_or(app.run.cpu.pc);
        if app.run.breakpoints.contains(&addr) {
            app.run.breakpoints.remove(&addr);
        } else {
            app.run.breakpoints.insert(addr);
        }
        return Ok(false);
    }

    if ctrl && matches!(key.code, KeyCode::Char('g')) && matches!(app.tab, Tab::Run) {
        app.run.imem_search_open = !app.run.imem_search_open;
        if !app.run.imem_search_open {
            app.run.imem_search_query.clear();
        }
        return Ok(false);
    }

    if ctrl
        && matches!(key.code, KeyCode::Char('f'))
        && matches!(app.tab, Tab::Run)
        && !app.run.show_registers
    {
        app.run.mem_search_open = !app.run.mem_search_open;
        if !app.run.mem_search_open {
            app.run.mem_search_query.clear();
        }
        return Ok(false);
    }

    if handle_run_execution_key(app, key.code) {
        return Ok(false);
    }

    match app.mode {
        EditorMode::Insert => {
            if key.code == KeyCode::Esc {
                app.mode = EditorMode::Command;
                return Ok(false);
            }

            // ELF mode: block all editing — show prompt on any non-navigation key.
            if matches!(app.tab, Tab::Editor) && app.editor.last_ok_elf_bytes.is_some() {
                match key.code {
                    KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End => {}
                    _ => {
                        app.editor.elf_prompt_open = true;
                        return Ok(false);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Assembly / ELF", &["fas", "asm", "elf", "bin"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
                {
                    open_file_autodetect(app, &path);
                } else {
                    open_path_input(app, PathInputAction::OpenFas);
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas", "asm"])
                    .set_file_name("program.fas")
                    .save_file()
                {
                    let _ = std::fs::write(path, app.editor.buf.text());
                } else {
                    open_path_input(app, PathInputAction::SaveFas);
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('c')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.buf.selected_text() {
                    if let Some(clip) = app.clipboard.as_mut() {
                        let _ = clip.set_text(text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('v')) && matches!(app.tab, Tab::Editor) {
                // Skip arboard paste if a bracketed-paste event just fired (within 100ms),
                // to prevent double-paste in terminals that emit both Event::Paste and Ctrl+V.
                let recent_bracketed = app
                    .last_bracketed_paste
                    .map_or(false, |t| t.elapsed().as_millis() < 100);
                if !recent_bracketed {
                    let text = app.clipboard.as_mut().and_then(|clip| clip.get_text().ok());
                    if let Some(text) = text {
                        paste_editor(app, &text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('z')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.undo();
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('y')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.redo();
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('x')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.buf.selected_text() {
                    if let Some(clip) = app.clipboard.as_mut() {
                        let _ = clip.set_text(text);
                    }
                    app.editor.buf.delete_selection();
                    app.editor.dirty = true;
                    app.editor.last_edit_at = Some(Instant::now());
                    app.editor.diag_line = None;
                    app.editor.diag_msg = None;
                    app.editor.diag_line_text = None;
                    app.editor.last_compile_ok = None;
                    app.editor.last_build_stats = None;
                    app.editor.last_assemble_msg = None;
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('a')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.select_all();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('w')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.select_word_at_cursor();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('f')) && matches!(app.tab, Tab::Editor) {
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
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('h')) && matches!(app.tab, Tab::Editor) {
                app.editor.find_open = true;
                app.editor.replace_open = true;
                app.editor.goto_open = false;
                app.editor.find_in_replace = false;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('g')) && matches!(app.tab, Tab::Editor) {
                app.editor.goto_open = true;
                app.editor.find_open = false;
                app.editor.goto_query.clear();
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('f')) && matches!(app.tab, Tab::Docs) {
                app.docs.search_open = !app.docs.search_open;
                if !app.docs.search_open {
                    app.docs.search_query.clear();
                }
                return Ok(false);
            }

            // F12: go to label definition
            if key.code == KeyCode::F(12) && matches!(app.tab, Tab::Editor) {
                app.goto_label_definition();
                return Ok(false);
            }

            // F2: toggle address hints gutter
            if key.code == KeyCode::F(2) && matches!(app.tab, Tab::Editor) {
                app.editor.show_addr_hints = !app.editor.show_addr_hints;
                return Ok(false);
            }

            // Ctrl+/: toggle line comment
            if ctrl && matches!(key.code, KeyCode::Char('/')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.toggle_comment();
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            // Ctrl+D: select next occurrence of word under cursor
            if ctrl && matches!(key.code, KeyCode::Char('d')) && matches!(app.tab, Tab::Editor) {
                app.select_next_occurrence();
                return Ok(false);
            }

            // Ctrl+Enter: assemble and switch to Run tab (B1)
            if ctrl && key.code == KeyCode::Enter && matches!(app.tab, Tab::Editor) {
                app.assemble_and_load();
                if app.editor.last_compile_ok == Some(true) {
                    app.tab = Tab::Run;
                }
                return Ok(false);
            }

            // Ctrl+E: toggle encoding overlay (B3)
            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Editor) {
                app.editor.show_encoding = !app.editor.show_encoding;
                return Ok(false);
            }

            let edited = match (key.code, app.tab) {
                (code, Tab::Editor) => match code {
                    KeyCode::Left => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_left();
                        false
                    }
                    KeyCode::Right => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_right();
                        false
                    }
                    KeyCode::Up => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_up();
                        false
                    }
                    KeyCode::Down => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_down();
                        false
                    }
                    KeyCode::Home => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_home();
                        false
                    }
                    KeyCode::End => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.move_end();
                        false
                    }
                    KeyCode::PageUp => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
                        app.editor.buf.page_up();
                        false
                    }
                    KeyCode::PageDown => {
                        if shift {
                            app.editor.buf.start_selection();
                        } else {
                            app.editor.buf.clear_selection();
                        }
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
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
            }
        }
        EditorMode::Command => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') {
                app.show_exit_popup = true;
                return Ok(false);
            }

            // ELF mode: intercept editing Ctrl-keys and show prompt.
            if matches!(app.tab, Tab::Editor) && app.editor.last_ok_elf_bytes.is_some() && ctrl {
                match key.code {
                    KeyCode::Char('z')
                    | KeyCode::Char('y')
                    | KeyCode::Char('x')
                    | KeyCode::Char('v')
                    | KeyCode::Char('/') => {
                        app.editor.elf_prompt_open = true;
                        return Ok(false);
                    }
                    _ => {}
                }
            }

            if ctrl && matches!(key.code, KeyCode::Char('c')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.buf.selected_text() {
                    if let Some(clip) = app.clipboard.as_mut() {
                        let _ = clip.set_text(text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('v')) && matches!(app.tab, Tab::Editor) {
                // Skip arboard paste if a bracketed-paste event just fired (within 100ms),
                // to prevent double-paste in terminals that emit both Event::Paste and Ctrl+V.
                let recent_bracketed = app
                    .last_bracketed_paste
                    .map_or(false, |t| t.elapsed().as_millis() < 100);
                if !recent_bracketed {
                    let text = app.clipboard.as_mut().and_then(|clip| clip.get_text().ok());
                    if let Some(text) = text {
                        paste_editor(app, &text);
                    }
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('z')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.undo();
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('y')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.redo();
                app.editor.dirty = true;
                app.editor.last_edit_at = Some(Instant::now());
                app.editor.diag_line = None;
                app.editor.diag_msg = None;
                app.editor.diag_line_text = None;
                app.editor.last_compile_ok = None;
                app.editor.last_build_stats = None;
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('x')) && matches!(app.tab, Tab::Editor) {
                if let Some(text) = app.editor.buf.selected_text() {
                    if let Some(clip) = app.clipboard.as_mut() {
                        let _ = clip.set_text(text);
                    }
                    app.editor.buf.delete_selection();
                    app.editor.dirty = true;
                    app.editor.last_edit_at = Some(Instant::now());
                    app.editor.diag_line = None;
                    app.editor.diag_msg = None;
                    app.editor.diag_line_text = None;
                    app.editor.last_compile_ok = None;
                    app.editor.last_build_stats = None;
                    app.editor.last_assemble_msg = None;
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Assembly / ELF", &["fas", "asm", "elf", "bin"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
                {
                    open_file_autodetect(app, &path);
                } else {
                    open_path_input(app, PathInputAction::OpenFas);
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('s')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .set_file_name("program.fas")
                    .save_file()
                {
                    let _ = std::fs::write(path, app.editor.buf.text());
                } else {
                    open_path_input(app, PathInputAction::SaveFas);
                }
                return Ok(false);
            }

            // Cache config export/import (Ctrl+E / Ctrl+L) — available on Cache tab
            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Cache) {
                do_export_cfg(app);
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('l')) && matches!(app.tab, Tab::Cache) {
                do_import_cfg(app);
                return Ok(false);
            }

            // Sim settings export/import (Ctrl+E / Ctrl+L) — available on Config tab
            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Config) {
                do_export_rcfg(app);
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('l')) && matches!(app.tab, Tab::Config) {
                do_import_rcfg(app);
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Pipeline) {
                do_export_pcfg(app);
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('l')) && matches!(app.tab, Tab::Pipeline) {
                do_import_pcfg(app);
                return Ok(false);
            }

            // Cache results export (Ctrl+R) — saves .fstats or .csv
            if ctrl && matches!(key.code, KeyCode::Char('r')) && matches!(app.tab, Tab::Cache) {
                do_export_results(app);
                return Ok(false);
            }
            // Ctrl+Enter: assemble and switch to Run tab (B1)
            if ctrl && key.code == KeyCode::Enter && matches!(app.tab, Tab::Editor) {
                app.assemble_and_load();
                if app.editor.last_compile_ok == Some(true) {
                    app.tab = Tab::Run;
                }
                return Ok(false);
            }

            // Ctrl+E: toggle encoding overlay (B3)
            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Editor) {
                app.editor.show_encoding = !app.editor.show_encoding;
                return Ok(false);
            }

            match (key.code, app.tab) {
                // v: cycle sidebar view — RAM → REGS → DYN → RAM
                (KeyCode::Char('v'), Tab::Run) => {
                    if app.run.show_dyn {
                        app.run.show_dyn = false;
                    } else if app.run.show_registers {
                        app.run.show_registers = false;
                        app.run.show_dyn = true;
                    } else {
                        app.run.show_registers = true;
                    }
                }
                // Tab (in register view): toggle between int and float registers
                (KeyCode::Tab, Tab::Run) if app.run.show_registers && !app.run.show_dyn => {
                    app.run.show_float_regs = !app.run.show_float_regs;
                }
                // t: toggle execution trace panel
                (KeyCode::Char('t'), Tab::Run) => {
                    app.run.show_trace = !app.run.show_trace;
                }
                // e: toggle exec count display
                (KeyCode::Char('e'), Tab::Run) => {
                    app.run.show_exec_count = !app.run.show_exec_count;
                }
                // y: toggle instruction type badge
                (KeyCode::Char('y'), Tab::Run) => {
                    app.run.show_instr_type = !app.run.show_instr_type;
                }
                // k: cycle memory region DATA → STACK → R/W → HEAP → DATA (only in pure RAM mode)
                (KeyCode::Char('k'), Tab::Run) if !app.run.show_registers && !app.run.show_dyn => {
                    match app.run.mem_region {
                        MemRegion::Data | MemRegion::Custom => {
                            app.run.mem_region = MemRegion::Stack;
                            let sp = app.run.cpu.x[2];
                            app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
                        }
                        MemRegion::Stack => {
                            app.run.mem_region = MemRegion::Access;
                        }
                        MemRegion::Access => {
                            app.run.mem_region = MemRegion::Heap;
                            let hb = app.run.cpu.heap_break;
                            app.run.mem_view_addr = hb & !(app.run.mem_view_bytes - 1);
                        }
                        MemRegion::Heap => {
                            app.run.mem_region = MemRegion::Data;
                            app.run.mem_view_addr = app.run.data_base;
                        }
                    }
                    app.run.show_registers = false;
                    app.run.show_dyn = false;
                }
                // P (shift+p): pin/unpin the currently selected register
                (KeyCode::Char('P'), Tab::Run) if app.run.show_registers => {
                    let idx = app.run.reg_cursor;
                    if idx >= 1 {
                        let reg = (idx - 1) as u8;
                        if let Some(pos) = app.run.pinned_regs.iter().position(|&r| r == reg) {
                            app.run.pinned_regs.remove(pos);
                        } else {
                            app.run.pinned_regs.push(reg);
                        }
                    }
                }
                // Cycle speed: 1x → 2x → 4x → GO → 1x
                (KeyCode::Char('f'), Tab::Run) => {
                    app.run.speed = app.run.speed.cycle();
                }
                (KeyCode::Up, Tab::Run) if ctrl => {
                    let visible = app.run.console_height.saturating_sub(3) as usize;
                    let max_scroll = app.console.lines.len().saturating_sub(visible);
                    if app.console.scroll > max_scroll {
                        app.console.scroll = max_scroll;
                    }
                    app.console.scroll = (app.console.scroll + 1).min(max_scroll);
                }
                (KeyCode::Down, Tab::Run) if ctrl => {
                    let visible = app.run.console_height.saturating_sub(3) as usize;
                    let max_scroll = app.console.lines.len().saturating_sub(visible);
                    if app.console.scroll > max_scroll {
                        app.console.scroll = max_scroll;
                    }
                    app.console.scroll = app.console.scroll.saturating_sub(1);
                }
                (KeyCode::Up, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.run.regs_scroll = app.run.regs_scroll.saturating_sub(1);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.reg_cursor = app.run.reg_cursor.saturating_sub(1);
                }
                (KeyCode::Down, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
                    app.run.reg_cursor = (app.run.reg_cursor + 1).min(32);
                }
                (KeyCode::PageUp, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.run.regs_scroll = app.run.regs_scroll.saturating_sub(10);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.reg_cursor = app.run.reg_cursor.saturating_sub(10);
                }
                (KeyCode::PageDown, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.regs_scroll = (app.run.regs_scroll + 10).min(max_scroll);
                    app.run.reg_cursor = (app.run.reg_cursor + 10).min(32);
                }
                (KeyCode::Up, Tab::Run) if !app.run.show_registers => {
                    app.run.mem_view_addr =
                        app.run.mem_view_addr.saturating_sub(app.run.mem_view_bytes);
                    app.run.mem_region = MemRegion::Custom;
                }
                (KeyCode::Down, Tab::Run) if !app.run.show_registers => {
                    let max = app
                        .run
                        .mem_size
                        .saturating_sub(app.run.mem_view_bytes as usize)
                        as u32;
                    if app.run.mem_view_addr < max {
                        app.run.mem_view_addr = app
                            .run
                            .mem_view_addr
                            .saturating_add(app.run.mem_view_bytes)
                            .min(max);
                    }
                    app.run.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageUp, Tab::Run) if !app.run.show_registers => {
                    let delta: u32 = app.run.mem_view_bytes * 16;
                    app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(delta);
                    app.run.mem_region = MemRegion::Custom;
                }
                (KeyCode::PageDown, Tab::Run) if !app.run.show_registers => {
                    let delta: u32 = app.run.mem_view_bytes * 16;
                    let max = app
                        .run
                        .mem_size
                        .saturating_sub(app.run.mem_view_bytes as usize)
                        as u32;
                    let new = app.run.mem_view_addr.saturating_add(delta);
                    app.run.mem_view_addr = new.min(max);
                    app.run.mem_region = MemRegion::Custom;
                }

                // Docs page cycling (Tab) and language toggle (L)
                (KeyCode::Tab, Tab::Docs) => {
                    app.docs.page = app.docs.page.next();
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('1'), Tab::Docs) => {
                    app.docs.page = DocsPage::InstrRef;
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('2'), Tab::Docs) => {
                    app.docs.page = DocsPage::Syscalls;
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('3'), Tab::Docs) => {
                    app.docs.page = DocsPage::MemoryMap;
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('4'), Tab::Docs) => {
                    app.docs.page = DocsPage::FcacheRef;
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('l'), Tab::Docs) if !app.docs.search_open => {
                    app.docs.lang = app.docs.lang.toggle();
                }
                // Docs scroll
                (KeyCode::Up, Tab::Docs) => {
                    app.docs.scroll = app.docs.scroll.saturating_sub(1);
                    clamp_docs_scroll_keyboard(app);
                }
                (KeyCode::Down, Tab::Docs) => {
                    app.docs.scroll = app.docs.scroll.saturating_add(1);
                    clamp_docs_scroll_keyboard(app);
                }
                (KeyCode::PageUp, Tab::Docs) => {
                    app.docs.scroll = app.docs.scroll.saturating_sub(10);
                    clamp_docs_scroll_keyboard(app);
                }
                (KeyCode::PageDown, Tab::Docs) => {
                    app.docs.scroll = app.docs.scroll.saturating_add(10);
                    clamp_docs_scroll_keyboard(app);
                }
                // Docs filter navigation (search not open)
                (KeyCode::Left, Tab::Docs) if !app.docs.search_open => {
                    let n = FILTER_ITEMS.len();
                    app.docs.filter_cursor = if app.docs.filter_cursor == 0 {
                        n - 1
                    } else {
                        app.docs.filter_cursor - 1
                    };
                }
                (KeyCode::Right, Tab::Docs) if !app.docs.search_open => {
                    let n = FILTER_ITEMS.len();
                    app.docs.filter_cursor = (app.docs.filter_cursor + 1) % n;
                }
                (KeyCode::Char(' '), Tab::Docs) if !app.docs.search_open => {
                    if app.docs.filter_cursor == 0 {
                        // "All" toggle: restore full mask if any bit is off, otherwise do nothing useful
                        app.docs.type_filter = ALL_MASK;
                    } else {
                        let bit = FILTER_ITEMS[app.docs.filter_cursor].1;
                        app.docs.type_filter ^= bit;
                    }
                    app.docs.scroll = 0;
                }

                // Cache tab — Config field editing takes priority
                (code, Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::Config)
                        && app.cache.edit_field.is_some() =>
                {
                    let (is_icache, field) = app.cache.edit_field.unwrap();
                    match code {
                        KeyCode::Esc => {
                            app.cache.edit_field = None;
                            app.cache.edit_buf.clear();
                        }
                        KeyCode::Enter => {
                            app.commit_cache_edit();
                            app.cache.edit_field = None;
                            app.cache.edit_buf.clear();
                        }
                        KeyCode::Tab => {
                            app.commit_cache_edit();
                            let next = field.next();
                            app.cache.edit_field = Some((is_icache, next));
                            app.cache.edit_buf = if next.is_numeric() {
                                app.cache_field_value_str(is_icache, next)
                            } else {
                                String::new()
                            };
                        }
                        KeyCode::Up => {
                            app.commit_cache_edit();
                            let prev = field.prev();
                            app.cache.edit_field = Some((is_icache, prev));
                            app.cache.edit_buf = if prev.is_numeric() {
                                app.cache_field_value_str(is_icache, prev)
                            } else {
                                String::new()
                            };
                        }
                        KeyCode::Down => {
                            app.commit_cache_edit();
                            let next = field.next();
                            app.cache.edit_field = Some((is_icache, next));
                            app.cache.edit_buf = if next.is_numeric() {
                                app.cache_field_value_str(is_icache, next)
                            } else {
                                String::new()
                            };
                        }
                        KeyCode::Left if !field.is_numeric() => {
                            app.cycle_cache_field(is_icache, field, false);
                        }
                        KeyCode::Right if !field.is_numeric() => {
                            app.cycle_cache_field(is_icache, field, true);
                        }
                        KeyCode::Char(c) if field.is_numeric() && c.is_ascii_digit() => {
                            app.cache.edit_buf.push(c);
                            app.cache.config_error = None;
                            app.cache.config_status = None;
                        }
                        KeyCode::Backspace if field.is_numeric() => {
                            app.cache.edit_buf.pop();
                            app.cache.config_error = None;
                            app.cache.config_status = None;
                        }
                        _ => {}
                    }
                }
                // Cache tab — normal (no active edit)
                // Tab cycles: Stats → View → Config → Stats
                (KeyCode::Tab, Tab::Cache) => {
                    app.cache.subtab = match app.cache.subtab {
                        CacheSubtab::Stats => CacheSubtab::View,
                        CacheSubtab::View => CacheSubtab::Config,
                        CacheSubtab::Config => CacheSubtab::Stats,
                    };
                }
                // Cache level add/remove
                (KeyCode::Char('+'), Tab::Cache) | (KeyCode::Char('='), Tab::Cache) => {
                    app.add_cache_level();
                }
                (KeyCode::Char('-'), Tab::Cache) | (KeyCode::Char('_'), Tab::Cache) => {
                    app.remove_last_cache_level();
                }
                (KeyCode::Char('r'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.restart_simulation();
                }
                (KeyCode::Char('p'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    if app.run.is_running {
                        app.run.is_running = false;
                    } else if !app.run.faulted && app.can_start_run() {
                        app.run.is_running = true;
                    }
                }
                // Scope shortcuts — work in Stats and View (not Config, where letters edit fields)
                (KeyCode::Char('i'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.cache.scope = CacheScope::ICache;
                }
                (KeyCode::Char('d'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.cache.scope = CacheScope::DCache;
                }
                (KeyCode::Char('b'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.cache.scope = CacheScope::Both;
                }
                // View subtab: cycle data format / byte grouping
                (KeyCode::Char('m'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::View) =>
                {
                    app.cache.data_fmt = app.cache.data_fmt.cycle();
                }
                (KeyCode::Char('g'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::View) =>
                {
                    use crate::ui::app::CacheDataFmt;
                    if app.cache.data_fmt != CacheDataFmt::Float {
                        app.cache.data_group = app.cache.data_group.cycle();
                    }
                }
                (KeyCode::Char('t'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::View) =>
                {
                    app.cache.show_tag = !app.cache.show_tag;
                }
                // Sidebar / region shortcuts (same behaviour as Run tab)
                (KeyCode::Char('v'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    if app.run.show_dyn {
                        app.run.show_dyn = false;
                        app.run.show_registers = true;
                    } else if app.run.show_registers {
                        app.run.show_registers = false;
                    } else {
                        app.run.show_dyn = true;
                    }
                }
                (KeyCode::Char('k'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config)
                        && !app.run.show_registers
                        && !app.run.show_dyn =>
                {
                    match app.run.mem_region {
                        MemRegion::Data | MemRegion::Custom => {
                            app.run.mem_region = MemRegion::Stack;
                            let sp = app.run.cpu.x[2];
                            app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
                        }
                        MemRegion::Stack => {
                            app.run.mem_region = MemRegion::Access;
                        }
                        MemRegion::Access => {
                            app.run.mem_region = MemRegion::Heap;
                            let hb = app.run.cpu.heap_break;
                            app.run.mem_view_addr = hb & !(app.run.mem_view_bytes - 1);
                        }
                        MemRegion::Heap => {
                            app.run.mem_region = MemRegion::Data;
                            app.run.mem_view_addr = app.run.data_base;
                        }
                    }
                    app.run.show_registers = false;
                    app.run.show_dyn = false;
                }
                (KeyCode::Char('e'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.run.show_exec_count = !app.run.show_exec_count;
                }
                (KeyCode::Char('y'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.run.show_instr_type = !app.run.show_instr_type;
                }
                // `s` in Stats captures a snapshot; in View it single-steps
                (KeyCode::Char('s'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::Stats) =>
                {
                    let snap = capture_snapshot(app);
                    let label = snap.label.clone();
                    let instr_end = snap.instr_end;
                    app.cache.session_history.push(snap);
                    app.cache.history_scroll = app.cache.session_history.len().saturating_sub(1);
                    app.cache.window_start_instr = instr_end;
                    app.cache.config_status = Some(format!("Captured {label}"));
                }
                (KeyCode::Char('s'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::View) =>
                {
                    if !app.run.faulted {
                        app.single_step();
                    }
                }
                (KeyCode::Char('f'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.run.speed = app.run.speed.cycle();
                }
                // History: D = delete entry
                (KeyCode::Char('D'), Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::Stats)
                        && !app.cache.session_history.is_empty() =>
                {
                    let idx = app
                        .cache
                        .history_scroll
                        .min(app.cache.session_history.len() - 1);
                    app.cache.session_history.remove(idx);
                    if !app.cache.session_history.is_empty() {
                        app.cache.history_scroll = idx.min(app.cache.session_history.len() - 1);
                    } else {
                        app.cache.history_scroll = 0;
                    }
                    if let Some(v) = app.cache.viewing_snapshot {
                        if v == idx {
                            app.cache.viewing_snapshot = None;
                        } else if v > idx {
                            app.cache.viewing_snapshot = Some(v - 1);
                        }
                    }
                    if app.cache.session_history.is_empty() {
                        app.cache.viewing_snapshot = None;
                    }
                }
                // Stats: Enter = open snapshot detail popup
                (KeyCode::Enter, Tab::Cache)
                    if matches!(app.cache.subtab, CacheSubtab::Stats)
                        && !app.cache.session_history.is_empty()
                        && !app.run.is_running =>
                {
                    let idx = app
                        .cache
                        .history_scroll
                        .min(app.cache.session_history.len() - 1);
                    app.cache.viewing_snapshot = Some(idx);
                }
                (KeyCode::Up, Tab::Cache) => match app.cache.subtab {
                    CacheSubtab::Stats => {
                        app.cache.history_scroll = app.cache.history_scroll.saturating_sub(1);
                    }
                    CacheSubtab::View => {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                    }
                    _ => {}
                },
                (KeyCode::Down, Tab::Cache) => match app.cache.subtab {
                    CacheSubtab::Stats => {
                        if !app.cache.session_history.is_empty() {
                            app.cache.history_scroll = (app.cache.history_scroll + 1)
                                .min(app.cache.session_history.len() - 1);
                        }
                    }
                    CacheSubtab::View => {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_add(1);
                    }
                    _ => {}
                },
                (KeyCode::Left, Tab::Cache) => {
                    if matches!(app.cache.subtab, CacheSubtab::View) {
                        if app.cache.selected_level == 0 {
                            match app.cache.scope {
                                CacheScope::DCache => {
                                    app.cache.view_h_scroll_d =
                                        app.cache.view_h_scroll_d.saturating_sub(3);
                                }
                                CacheScope::Both => {
                                    app.cache.view_h_scroll =
                                        app.cache.view_h_scroll.saturating_sub(3);
                                    app.cache.view_h_scroll_d =
                                        app.cache.view_h_scroll_d.saturating_sub(3);
                                }
                                _ => {
                                    app.cache.view_h_scroll =
                                        app.cache.view_h_scroll.saturating_sub(3);
                                }
                            }
                        } else {
                            app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
                        }
                    }
                }
                (KeyCode::Right, Tab::Cache) => {
                    if matches!(app.cache.subtab, CacheSubtab::View) {
                        if app.cache.selected_level == 0 {
                            match app.cache.scope {
                                CacheScope::DCache => {
                                    app.cache.view_h_scroll_d =
                                        app.cache.view_h_scroll_d.saturating_add(3);
                                }
                                CacheScope::Both => {
                                    app.cache.view_h_scroll =
                                        app.cache.view_h_scroll.saturating_add(3);
                                    app.cache.view_h_scroll_d =
                                        app.cache.view_h_scroll_d.saturating_add(3);
                                }
                                _ => {
                                    app.cache.view_h_scroll =
                                        app.cache.view_h_scroll.saturating_add(3);
                                }
                            }
                        } else {
                            app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
                        }
                    }
                }

                // Editor navigation in command mode
                (KeyCode::Up, Tab::Editor) => app.editor.buf.move_up(),
                (KeyCode::Down, Tab::Editor) => app.editor.buf.move_down(),

                // ── Config tab — numeric field editing ──────────────────────
                (code, Tab::Config) if app.settings.cpi_editing => {
                    let commit_numeric_edit = |app: &mut App| {
                        if app.settings.selected == SETTINGS_ROW_MAX_CORES {
                            if let Ok(v) = app.settings.cpi_edit_buf.trim().parse::<usize>() {
                                if (1..=32).contains(&v) && v != app.max_cores {
                                    app.max_cores = v;
                                    app.restart_simulation();
                                }
                            }
                        } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
                            if let Ok(v) = app.settings.cpi_edit_buf.trim().parse::<usize>() {
                                let snapped =
                                    crate::ui::app::nearest_pow2_clamp(v.max(1), 1, 4096);
                                let new_bytes = snapped * 1024 * 1024;
                                if new_bytes != app.run.mem_size {
                                    app.ram_override = Some(new_bytes);
                                    app.restart_simulation();
                                }
                            }
                        } else {
                            let cpi_idx =
                                app.settings.selected.saturating_sub(SETTINGS_ROW_CPI_START);
                            if let Ok(v) = app.settings.cpi_edit_buf.trim().parse::<u64>() {
                                app.run.cpi_config.set(cpi_idx, v);
                            }
                        }
                    };
                    match code {
                        KeyCode::Esc => {
                            app.settings.cpi_editing = false;
                            app.settings.cpi_edit_buf.clear();
                        }
                        KeyCode::Enter => {
                            commit_numeric_edit(app);
                            app.settings.cpi_editing = false;
                            app.settings.cpi_edit_buf.clear();
                        }
                        KeyCode::Up => {
                            commit_numeric_edit(app);
                            app.settings.cpi_editing = false;
                            app.settings.cpi_edit_buf.clear();
                            if app.settings.selected > 0 {
                                app.settings.selected -= 1;
                                if app.settings.selected == 5 {
                                    app.settings.selected = SETTINGS_ROW_PIPELINE_ENABLED;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Tab => {
                            commit_numeric_edit(app);
                            app.settings.cpi_editing = false;
                            app.settings.cpi_edit_buf.clear();
                            if app.settings.selected + 1 < SETTINGS_ROWS {
                                app.settings.selected += 1;
                                if app.settings.selected == 5 {
                                    app.settings.selected = SETTINGS_ROW_CPI_START;
                                }
                            }
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            if app.settings.selected == SETTINGS_ROW_MAX_CORES
                                || app.settings.selected == SETTINGS_ROW_MEM_SIZE
                            {
                                app.settings.cpi_edit_buf.clear();
                                app.settings.cpi_edit_buf.push(c);
                            } else {
                                app.settings.cpi_edit_buf.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            app.settings.cpi_edit_buf.pop();
                        }
                        _ => {}
                    }
                }
                // Config tab — navigation and toggle
                (code, Tab::Config) => {
                    match code {
                        KeyCode::Up => {
                            if app.settings.selected > 0 {
                                app.settings.selected -= 1;
                                // Skip blank separator row
                                if app.settings.selected == 5 {
                                    app.settings.selected = SETTINGS_ROW_PIPELINE_ENABLED;
                                }
                            }
                        }
                        KeyCode::Down => {
                            if app.settings.selected + 1 < SETTINGS_ROWS {
                                app.settings.selected += 1;
                                // Skip blank separator row
                                if app.settings.selected == 5 {
                                    app.settings.selected = SETTINGS_ROW_CPI_START;
                                }
                            }
                        }
                        KeyCode::Left => {}
                        KeyCode::Right => {}
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            if app.settings.selected == SETTINGS_ROW_CACHE_ENABLED {
                                app.set_cache_enabled(!app.run.cache_enabled);
                            } else if app.settings.selected == SETTINGS_ROW_MAX_CORES {
                                app.settings.cpi_edit_buf = app.max_cores.to_string();
                                app.settings.cpi_editing = true;
                            } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
                                let mb = app.run.mem_size / (1024 * 1024);
                                app.settings.cpi_edit_buf = mb.to_string();
                                app.settings.cpi_editing = true;
                            } else if app.settings.selected == SETTINGS_ROW_RUN_SCOPE {
                                app.run_scope = app.run_scope.cycle();
                            } else if app.settings.selected == SETTINGS_ROW_PIPELINE_ENABLED {
                                app.set_pipeline_enabled(!app.pipeline.enabled);
                            } else if app.settings.selected >= SETTINGS_ROW_CPI_START {
                                let i = app.settings.selected - SETTINGS_ROW_CPI_START;
                                app.settings.cpi_edit_buf = app.run.cpi_config.get(i).to_string();
                                app.settings.cpi_editing = true;
                            }
                        }
                        KeyCode::Char(c)
                            if (app.settings.selected == SETTINGS_ROW_MAX_CORES
                                || app.settings.selected == SETTINGS_ROW_MEM_SIZE)
                                && c.is_ascii_digit() =>
                        {
                            app.settings.cpi_edit_buf.clear();
                            app.settings.cpi_edit_buf.push(c);
                            app.settings.cpi_editing = true;
                        }
                        _ => {}
                    }
                }

                // ── Pipeline tab ──────────────────────────────────────────
                (KeyCode::Tab, Tab::Pipeline) => {
                    use crate::ui::pipeline::PipelineSubtab;
                    app.pipeline.subtab = match app.pipeline.subtab {
                        PipelineSubtab::Main => PipelineSubtab::Config,
                        PipelineSubtab::Config => PipelineSubtab::Main,
                    };
                }
                (KeyCode::Char('e'), Tab::Pipeline) => {
                    app.set_pipeline_enabled(!app.pipeline.enabled);
                }
                (KeyCode::Char('r') | KeyCode::Char('R'), Tab::Pipeline) => {
                    app.restart_simulation();
                }
                (KeyCode::Char('f'), Tab::Pipeline) => {
                    app.pipeline.speed = app.pipeline.speed.next();
                    app.pipeline.last_tick = std::time::Instant::now();
                }
                (KeyCode::Char('b'), Tab::Pipeline) => {
                    use crate::ui::pipeline::BranchResolve;
                    app.pipeline.branch_resolve = match app.pipeline.branch_resolve {
                        BranchResolve::Id => BranchResolve::Ex,
                        BranchResolve::Ex => BranchResolve::Mem,
                        BranchResolve::Mem => BranchResolve::Id,
                    };
                }
                (KeyCode::Char('s'), Tab::Pipeline) => {
                    if app.pipeline.enabled && !app.pipeline.faulted {
                        app.single_step();
                    }
                }
                (KeyCode::Char('p') | KeyCode::Char(' '), Tab::Pipeline)
                    if matches!(
                        app.pipeline.subtab,
                        crate::ui::pipeline::PipelineSubtab::Main
                    ) =>
                {
                    if app.pipeline.enabled && !app.pipeline.faulted {
                        if app.pipeline.halted {
                            app.restart_simulation();
                            if app.can_start_run() {
                                app.run.is_running = true;
                            }
                        } else {
                            app.resume_selected_hart();
                            if app.run.is_running {
                                app.run.is_running = false;
                            } else if app.can_start_run() {
                                app.run.is_running = true;
                            }
                        }
                    }
                }
                (KeyCode::Enter, Tab::Pipeline)
                    if matches!(
                        app.pipeline.subtab,
                        crate::ui::pipeline::PipelineSubtab::Config
                    ) =>
                {
                    use crate::ui::pipeline::{BranchPredict, BranchResolve, PipelineMode};
                    match app.pipeline.config_cursor {
                        0 => app.pipeline.forwarding = !app.pipeline.forwarding,
                        1 => {
                            app.pipeline.mode = match app.pipeline.mode {
                                PipelineMode::SingleCycle => PipelineMode::FunctionalUnits,
                                PipelineMode::FunctionalUnits => PipelineMode::SingleCycle,
                            }
                        }
                        2 => {
                            app.pipeline.branch_resolve = match app.pipeline.branch_resolve {
                                BranchResolve::Id => BranchResolve::Ex,
                                BranchResolve::Ex => BranchResolve::Mem,
                                BranchResolve::Mem => BranchResolve::Id,
                            }
                        }
                        3 => {
                            app.pipeline.predict = match app.pipeline.predict {
                                BranchPredict::NotTaken => BranchPredict::Taken,
                                BranchPredict::Taken => BranchPredict::NotTaken,
                            }
                        }
                        _ => {}
                    }
                }
                (KeyCode::Up, Tab::Pipeline) => {
                    app.pipeline.config_cursor = app.pipeline.config_cursor.saturating_sub(1);
                }
                (KeyCode::Down, Tab::Pipeline) => {
                    app.pipeline.config_cursor = (app.pipeline.config_cursor + 1).min(3);
                }

                _ => {}
            }
        }
    }

    Ok(false)
}

pub fn paste_from_terminal(app: &mut App, text: &str) {
    app.editor.buf.paste_text(text);
    app.editor.dirty = true;
    app.editor.last_edit_at = Some(Instant::now());
    app.editor.diag_line = None;
    app.editor.diag_msg = None;
    app.editor.diag_line_text = None;
    app.editor.last_compile_ok = None;
    app.editor.last_build_stats = None;
    app.editor.last_assemble_msg = None;
}

fn paste_editor(app: &mut App, text: &str) {
    paste_from_terminal(app, text);
}

fn clamp_docs_scroll_keyboard(app: &mut App) {
    use crate::ui::view::docs::free_page_line_count;
    if let Ok((_, h)) = terminal::size() {
        // Tab bar(3) + status(1) = 4 overhead
        let viewport_h = h.saturating_sub(6) as usize;
        if viewport_h == 0 {
            app.docs.scroll = 0;
            return;
        }
        let total = match app.docs.page {
            DocsPage::InstrRef => {
                // InstrRef: tab_bar(1) + legend(2) + filter(1) + col_hdr(1) + sep(1) = 6 extra rows
                let vp = viewport_h.saturating_sub(6);
                let q = app.docs.search_query.clone();
                docs_body_line_count(80, &q, app.docs.type_filter).saturating_sub(vp)
            }
            p => {
                free_page_line_count(p, app.docs.lang).saturating_sub(viewport_h.saturating_sub(2))
            }
        };
        if app.docs.scroll > total {
            app.docs.scroll = total;
        }
    }
}


#[cfg(test)]
#[path = "../../../../tests/support/ui_input_keyboard.rs"]
mod tests;
