use crate::falcon::cache::{CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy, extra_level_presets, Cache};
use crate::ui::app::{App, CacheResultsSnapshot, CacheScope, CacheSubtab, CpiConfig, DocsPage, EditorMode, LevelSnapshot, MemRegion, Tab};
use crate::ui::view::docs::{docs_body_line_count, ALL_MASK, FILTER_ITEMS};
use crossterm::{event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers}, terminal};
use rfd::FileDialog as OSFileDialog;
use std::{collections::HashMap, io, time::Instant};


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
                app.run.is_running = true;
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

    // Help popup intercept — Esc closes, ←/→ navigate pages, any other key closes
    if app.help_open {
        // Count pages by matching tab
        let pages_count: usize = match app.tab {
            Tab::Run => 2,
            Tab::Editor => 1,
            Tab::Cache => 1,
            Tab::Docs => 1,
        };
        match key.code {
            KeyCode::Esc => { app.help_open = false; }
            KeyCode::Left | KeyCode::Char('h') => {
                app.help_page = app.help_page.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                app.help_page = (app.help_page + 1).min(pages_count.saturating_sub(1));
            }
            _ => { app.help_open = false; }
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
                        let row = n.saturating_sub(1).min(app.editor.buf.lines.len().saturating_sub(1));
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
                        let eb = crate::ui::editor::Editor::byte_at(&app.editor.buf.lines[row], end_col);
                        let rep = app.editor.replace_query.clone();
                        app.editor.buf.lines[row].replace_range(sb..eb, &rep);
                        app.editor.find_matches = crate::ui::app::compute_find_matches(
                            &app.editor.find_query, &app.editor.buf.lines);
                        app.editor.find_current = app.editor.find_current.min(
                            app.editor.find_matches.len().saturating_sub(1));
                        app.editor.dirty = true;
                        app.editor.last_edit_at = Some(Instant::now());
                    }
                } else {
                    if !app.editor.find_matches.is_empty() {
                        app.editor.find_current = (app.editor.find_current + 1) % app.editor.find_matches.len();
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
                        &app.editor.find_query, &app.editor.buf.lines);
                    app.editor.find_current = 0;
                }
            }
            KeyCode::Char(c) => {
                let mods = key.modifiers;
                let ctrl_pressed = mods.contains(crossterm::event::KeyModifiers::CONTROL);
                if ctrl_pressed && c == 'f' {
                    if !app.editor.find_matches.is_empty() {
                        app.editor.find_current = (app.editor.find_current + 1) % app.editor.find_matches.len();
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
                            &app.editor.find_query, &app.editor.buf.lines);
                        if !app.editor.find_matches.is_empty() {
                            let cursor_pos = (app.editor.buf.cursor_row, app.editor.buf.cursor_col);
                            let idx = app.editor.find_matches.iter()
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
            KeyCode::Backspace => { app.run.imem_search_query.pop(); }
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
            KeyCode::Backspace => { app.run.mem_search_query.pop(); }
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
            KeyCode::Esc => { app.docs.search_open = false; app.docs.search_query.clear(); }
            KeyCode::Backspace => { app.docs.search_query.pop(); }
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                app.docs.search_query.push(c);
            }
            _ => {}
        }
        return Ok(false);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    if matches!(app.tab, Tab::Run) && matches!(key.code, KeyCode::Char('R')) {
        app.restart_simulation();
        return Ok(false);
    }

    // '?' opens/closes help popup from any tab
    if key.code == KeyCode::Char('?') {
        app.help_open = !app.help_open;
        app.help_page = 0;
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

    if ctrl && matches!(key.code, KeyCode::Char('f')) && matches!(app.tab, Tab::Run)
        && !app.run.show_registers
    {
        app.run.mem_search_open = !app.run.mem_search_open;
        if !app.run.mem_search_open {
            app.run.mem_search_query.clear();
        }
        return Ok(false);
    }

    match app.mode {
        EditorMode::Insert => {
            if key.code == KeyCode::Esc {
                app.mode = EditorMode::Command;
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas", "asm"])
                    .pick_file()
                {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.editor.buf.lines = content.lines().map(|s| s.to_string()).collect();
                        app.editor.buf.cursor_row = 0;
                        app.editor.buf.cursor_col = 0;
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
                    let _ = std::fs::write(path, app.editor.buf.text());
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
                let recent_bracketed = app.last_bracketed_paste
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
                        app.editor.find_current = (app.editor.find_current + 1) % app.editor.find_matches.len();
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
                app.editor.last_assemble_msg = None;
                return Ok(false);
            }

            // Ctrl+D: select next occurrence of word under cursor
            if ctrl && matches!(key.code, KeyCode::Char('d')) && matches!(app.tab, Tab::Editor) {
                app.select_next_occurrence();
                return Ok(false);
            }

            match (key.code, app.tab) {
                (code, Tab::Editor) => match code {
                    KeyCode::Left => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_left();
                    }
                    KeyCode::Right => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_right();
                    }
                    KeyCode::Up => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_up();
                    }
                    KeyCode::Down => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_down();
                    }
                    KeyCode::Home => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_home();
                    }
                    KeyCode::End => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.move_end();
                    }
                    KeyCode::PageUp => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.page_up();
                    }
                    KeyCode::PageDown => {
                        if shift { app.editor.buf.start_selection(); } else { app.editor.buf.clear_selection(); }
                        app.editor.buf.page_down();
                    }
                    KeyCode::Backspace => app.editor.buf.backspace(),
                    KeyCode::Delete => app.editor.buf.delete_char(),
                    KeyCode::Enter => app.editor.buf.enter(),
                    KeyCode::BackTab => app.editor.buf.shift_tab(),
                    KeyCode::Tab => app.editor.buf.tab(),
                    KeyCode::Char(c) => app.editor.buf.insert_char(c),
                    _ => {}
                },
                _ => {}
            }
            app.editor.dirty = true;
            app.editor.last_edit_at = Some(Instant::now());
            app.editor.diag_line = None;
            app.editor.diag_msg = None;
            app.editor.diag_line_text = None;
            app.editor.last_compile_ok = None;
            app.editor.last_assemble_msg = None;
        }
        EditorMode::Command => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') {
                app.show_exit_popup = true;
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
                let recent_bracketed = app.last_bracketed_paste
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
                    app.editor.last_assemble_msg = None;
                }
                return Ok(false);
            }

            if ctrl && matches!(key.code, KeyCode::Char('o')) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Falcon ASM", &["fas"])
                    .pick_file()
                {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.editor.buf.lines = content.lines().map(|s| s.to_string()).collect();
                        app.editor.buf.cursor_row = 0;
                        app.editor.buf.cursor_col = 0;
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
                    let _ = std::fs::write(path, app.editor.buf.text());
                }
                return Ok(false);
            }

            // Cache config export/import (Ctrl+E / Ctrl+L) — available on Cache tab
            if ctrl && matches!(key.code, KeyCode::Char('e')) && matches!(app.tab, Tab::Cache) {
                let text = serialize_cache_configs(&app.cache.pending_icache, &app.cache.pending_dcache, &app.cache.extra_pending);
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Cache Config", &["fcache"])
                    .set_file_name("cache.fcache")
                    .save_file()
                {
                    match std::fs::write(&path, &text) {
                        Ok(()) => {
                            app.cache.config_error = None;
                            app.cache.config_status = Some(format!(
                                "Exported to {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        Err(e) => {
                            app.cache.config_status = None;
                            app.cache.config_error = Some(format!("Export failed: {e}"));
                        }
                    }
                }
                return Ok(false);
            }
            if ctrl && matches!(key.code, KeyCode::Char('l')) && matches!(app.tab, Tab::Cache) {
                if let Some(path) = OSFileDialog::new()
                    .add_filter("Cache Config", &["fcache"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => match parse_cache_configs(&text) {
                            Ok((icfg, dcfg, extra)) => {
                                app.cache.pending_icache = icfg;
                                app.cache.pending_dcache = dcfg;
                                // Sync extra_pending and live extra_levels
                                let n_extra = extra.len();
                                app.cache.extra_pending = extra;
                                // Rebuild live extra levels to match
                                app.run.mem.extra_levels.clear();
                                for cfg in &app.cache.extra_pending {
                                    app.run.mem.extra_levels.push(crate::falcon::cache::Cache::new(cfg.clone()));
                                }
                                // Resize hover_level vec
                                app.cache.hover_level = vec![false; n_extra + 1];
                                // Clamp selected_level
                                if app.cache.selected_level > n_extra {
                                    app.cache.selected_level = n_extra;
                                }
                                app.cache.config_error = None;
                                app.cache.config_status = Some(format!(
                                    "Imported from {}",
                                    path.file_name().unwrap_or_default().to_string_lossy()
                                ));
                            }
                            Err(msg) => {
                                app.cache.config_status = None;
                                app.cache.config_error = Some(format!("Import failed: {msg}"));
                            }
                        },
                        Err(e) => {
                            app.cache.config_status = None;
                            app.cache.config_error = Some(format!("Import failed: {e}"));
                        }
                    }
                }
                return Ok(false);
            }

            // Cache results export (Ctrl+R) — saves .fstats or .csv
            if ctrl && matches!(key.code, KeyCode::Char('r')) && matches!(app.tab, Tab::Cache) {
                do_export_results(app);
                return Ok(false);
            }
            // Cache results compare/load (Ctrl+M) — loads .fstats baseline
            if ctrl && matches!(key.code, KeyCode::Char('m')) && matches!(app.tab, Tab::Cache) {
                do_compare_load(app);
                return Ok(false);
            }

            match (key.code, app.tab) {
                (KeyCode::Char('s'), Tab::Run) => {
                    if !app.run.faulted {
                        app.single_step();
                    }
                }
                (KeyCode::Char('r'), Tab::Run) => {
                    if !app.run.faulted {
                        app.run.is_running = true;
                    }
                }
                // Pause/resume
                (KeyCode::Char('p'), Tab::Run) => {
                    if app.run.is_running {
                        app.run.is_running = false;
                    } else if !app.run.faulted {
                        app.run.is_running = true;
                    }
                }
                // v: cycle sidebar view — REGS → RAM → BP → REGS
                (KeyCode::Char('v'), Tab::Run) => {
                    if app.run.show_bp_list {
                        app.run.show_bp_list = false;
                        app.run.show_registers = true;
                    } else if app.run.show_registers {
                        app.run.show_registers = false;
                    } else {
                        app.run.show_bp_list = true;
                    }
                }
                // Tab (in register view): toggle between int and float registers
                (KeyCode::Tab, Tab::Run) if app.run.show_registers && !app.run.show_bp_list => {
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
                // k: toggle Stack region in the RAM memory view
                (KeyCode::Char('k'), Tab::Run) => {
                    if app.run.mem_region == MemRegion::Stack {
                        app.run.mem_region = MemRegion::Data;
                        app.run.mem_view_addr = app.run.data_base;
                    } else {
                        app.run.mem_region = MemRegion::Stack;
                        let sp = app.run.cpu.x[2];
                        app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
                    }
                    app.run.show_registers = false;
                    app.run.show_bp_list = false;
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
                    if app.run.regs_scroll > max_scroll { app.run.regs_scroll = max_scroll; }
                    app.run.reg_cursor = app.run.reg_cursor.saturating_sub(1);
                }
                (KeyCode::Down, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll { app.run.regs_scroll = max_scroll; }
                    app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
                    app.run.reg_cursor = (app.run.reg_cursor + 1).min(32);
                }
                (KeyCode::PageUp, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.run.regs_scroll = app.run.regs_scroll.saturating_sub(10);
                    if app.run.regs_scroll > max_scroll { app.run.regs_scroll = max_scroll; }
                    app.run.reg_cursor = app.run.reg_cursor.saturating_sub(10);
                }
                (KeyCode::PageDown, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll { app.run.regs_scroll = max_scroll; }
                    app.run.regs_scroll = (app.run.regs_scroll + 10).min(max_scroll);
                    app.run.reg_cursor = (app.run.reg_cursor + 10).min(32);
                }
                (KeyCode::Up, Tab::Run) if !app.run.show_registers => {
                    app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(app.run.mem_view_bytes);
                    app.run.mem_region = MemRegion::Custom;
                }
                (KeyCode::Down, Tab::Run) if !app.run.show_registers => {
                    let max = app.run.mem_size.saturating_sub(app.run.mem_view_bytes as usize) as u32;
                    if app.run.mem_view_addr < max {
                        app.run.mem_view_addr = app.run.mem_view_addr
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
                    let max = app.run.mem_size.saturating_sub(app.run.mem_view_bytes as usize) as u32;
                    let new = app.run.mem_view_addr.saturating_add(delta);
                    app.run.mem_view_addr = new.min(max);
                    app.run.mem_region = MemRegion::Custom;
                }

                // Docs page cycling (Tab) and language toggle (L)
                (KeyCode::Tab, Tab::Docs) => {
                    app.docs.page = app.docs.page.next();
                    app.docs.scroll = 0;
                }
                (KeyCode::Char('l'), Tab::Docs) if !app.docs.search_open => {
                    app.docs.lang = app.docs.lang.toggle();
                }
                // Docs scroll
                (KeyCode::Up, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_sub(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::Down, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_add(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageUp, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_sub(10); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageDown, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_add(10); clamp_docs_scroll_keyboard(app); }
                // Docs filter navigation (search not open)
                (KeyCode::Left, Tab::Docs) if !app.docs.search_open => {
                    let n = FILTER_ITEMS.len();
                    app.docs.filter_cursor = if app.docs.filter_cursor == 0 { n - 1 } else { app.docs.filter_cursor - 1 };
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

                // Cache tab — CPI panel editing (when editing a CPI field)
                (code, Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::Config) && app.cache.cpi_editing && app.cache.selected_level == 0 => {
                    let n = 9usize; // number of CPI fields
                    match code {
                        KeyCode::Esc => {
                            app.cache.cpi_editing = false;
                            app.cache.cpi_edit_buf.clear();
                        }
                        KeyCode::Enter => {
                            if let Ok(v) = app.cache.cpi_edit_buf.trim().parse::<u64>() {
                                app.run.cpi_config.set(app.cache.cpi_selected, v);
                            }
                            app.cache.cpi_editing = false;
                            app.cache.cpi_edit_buf.clear();
                        }
                        KeyCode::Up => {
                            if let Ok(v) = app.cache.cpi_edit_buf.trim().parse::<u64>() {
                                app.run.cpi_config.set(app.cache.cpi_selected, v);
                            }
                            app.cache.cpi_editing = false;
                            app.cache.cpi_edit_buf.clear();
                            app.cache.cpi_selected = app.cache.cpi_selected.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            if let Ok(v) = app.cache.cpi_edit_buf.trim().parse::<u64>() {
                                app.run.cpi_config.set(app.cache.cpi_selected, v);
                            }
                            app.cache.cpi_editing = false;
                            app.cache.cpi_edit_buf.clear();
                            app.cache.cpi_selected = (app.cache.cpi_selected + 1).min(n - 1);
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            app.cache.cpi_edit_buf.push(c);
                        }
                        KeyCode::Backspace => {
                            app.cache.cpi_edit_buf.pop();
                        }
                        _ => {}
                    }
                }
                // Cache tab — Config field editing takes priority
                (code, Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::Config) && app.cache.edit_field.is_some() => {
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
                            } else { String::new() };
                        }
                        KeyCode::Up => {
                            app.commit_cache_edit();
                            let prev = field.prev();
                            app.cache.edit_field = Some((is_icache, prev));
                            app.cache.edit_buf = if prev.is_numeric() {
                                app.cache_field_value_str(is_icache, prev)
                            } else { String::new() };
                        }
                        KeyCode::Down => {
                            app.commit_cache_edit();
                            let next = field.next();
                            app.cache.edit_field = Some((is_icache, next));
                            app.cache.edit_buf = if next.is_numeric() {
                                app.cache_field_value_str(is_icache, next)
                            } else { String::new() };
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
                        CacheSubtab::Stats  => CacheSubtab::View,
                        CacheSubtab::View   => CacheSubtab::Config,
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
                (KeyCode::Char('r'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.restart_simulation();
                }
                (KeyCode::Char('p'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    if app.run.is_running {
                        app.run.is_running = false;
                    } else if !app.run.faulted {
                        app.run.is_running = true;
                    }
                }
                // Scope shortcuts — work in Stats and View (not Config, where letters edit fields)
                (KeyCode::Char('i'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.cache.scope = CacheScope::ICache;
                }
                (KeyCode::Char('d'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.cache.scope = CacheScope::DCache;
                }
                (KeyCode::Char('b'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.cache.scope = CacheScope::Both;
                }
                // View subtab: cycle data format / byte grouping
                (KeyCode::Char('m'), Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::View) => {
                    app.cache.data_fmt = app.cache.data_fmt.cycle();
                }
                (KeyCode::Char('g'), Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::View) => {
                    use crate::ui::app::CacheDataFmt;
                    if app.cache.data_fmt != CacheDataFmt::Float {
                        app.cache.data_group = app.cache.data_group.cycle();
                    }
                }
                // Sidebar / region shortcuts (same behaviour as Run tab)
                (KeyCode::Char('v'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    if app.run.show_bp_list {
                        app.run.show_bp_list = false;
                        app.run.show_registers = true;
                    } else if app.run.show_registers {
                        app.run.show_registers = false;
                    } else {
                        app.run.show_bp_list = true;
                    }
                }
                (KeyCode::Char('k'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    if app.run.mem_region == MemRegion::Stack {
                        app.run.mem_region = MemRegion::Data;
                        app.run.mem_view_addr = app.run.data_base;
                    } else {
                        app.run.mem_region = MemRegion::Stack;
                        let sp = app.run.cpu.x[2];
                        app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
                    }
                    app.run.show_registers = false;
                    app.run.show_bp_list = false;
                }
                (KeyCode::Char('e'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.run.show_exec_count = !app.run.show_exec_count;
                }
                (KeyCode::Char('y'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    app.run.show_instr_type = !app.run.show_instr_type;
                }
                // Execution hotkeys — available in Stats/View (not Config, where letters edit fields)
                (KeyCode::Char('s'), Tab::Cache) if !matches!(app.cache.subtab, CacheSubtab::Config) => {
                    if !app.run.faulted {
                        app.single_step();
                    }
                }
                (KeyCode::Char('f'), Tab::Cache)
                    if !matches!(app.cache.subtab, CacheSubtab::Config) =>
                {
                    app.run.speed = app.run.speed.cycle();
                }
                // Clear loaded baseline (Stats subtab only)
                (KeyCode::Char('c'), Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::Stats) => {
                    app.cache.loaded_snapshot = None;
                }
                // CPI panel navigation (when Config subtab, L1, not in cache edit mode)
                (KeyCode::Enter, Tab::Cache) if matches!(app.cache.subtab, CacheSubtab::Config) && app.cache.selected_level == 0 && app.cache.edit_field.is_none() => {
                    app.cache.cpi_edit_buf = app.run.cpi_config.get(app.cache.cpi_selected).to_string();
                    app.cache.cpi_editing = true;
                }
                (KeyCode::Up, Tab::Cache) => match app.cache.subtab {
                    CacheSubtab::Stats => {
                        app.cache.stats_scroll = app.cache.stats_scroll.saturating_sub(1);
                    }
                    CacheSubtab::View => {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                    }
                    CacheSubtab::Config if app.cache.selected_level == 0 && app.cache.edit_field.is_none() => {
                        app.cache.cpi_selected = app.cache.cpi_selected.saturating_sub(1);
                    }
                    _ => {}
                },
                (KeyCode::Down, Tab::Cache) => match app.cache.subtab {
                    CacheSubtab::Stats => {
                        app.cache.stats_scroll = app.cache.stats_scroll.saturating_add(1);
                    }
                    CacheSubtab::View => {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_add(1);
                    }
                    CacheSubtab::Config if app.cache.selected_level == 0 && app.cache.edit_field.is_none() => {
                        app.cache.cpi_selected = (app.cache.cpi_selected + 1).min(8);
                    }
                    _ => {}
                },
                (KeyCode::Left, Tab::Cache) => {
                    if matches!(app.cache.subtab, CacheSubtab::View) {
                        app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
                    }
                }
                (KeyCode::Right, Tab::Cache) => {
                    if matches!(app.cache.subtab, CacheSubtab::View) {
                        app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
                    }
                }

                // Editor navigation in command mode
                (KeyCode::Up, Tab::Editor) => app.editor.buf.move_up(),
                (KeyCode::Down, Tab::Editor) => app.editor.buf.move_down(),
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
        if viewport_h == 0 { app.docs.scroll = 0; return; }
        let total = match app.docs.page {
            DocsPage::InstrRef => {
                // InstrRef: tab_bar(1) + legend(2) + filter(1) + col_hdr(1) + sep(1) = 6 extra rows
                let vp = viewport_h.saturating_sub(6);
                let q = app.docs.search_query.clone();
                docs_body_line_count(80, &q, app.docs.type_filter).saturating_sub(vp)
            }
            p => free_page_line_count(p, app.docs.lang)
                    .saturating_sub(viewport_h.saturating_sub(2)),
        };
        if app.docs.scroll > total { app.docs.scroll = total; }
    }
}

// ── Cache config serialization ────────────────────────────────────────────────

fn serialize_one_config(s: &mut String, prefix: &str, cfg: &CacheConfig) {
    s.push_str(&format!("{prefix}.size={}\n", cfg.size));
    s.push_str(&format!("{prefix}.line_size={}\n", cfg.line_size));
    s.push_str(&format!("{prefix}.associativity={}\n", cfg.associativity));
    s.push_str(&format!("{prefix}.replacement={:?}\n", cfg.replacement));
    s.push_str(&format!("{prefix}.write_policy={:?}\n", cfg.write_policy));
    s.push_str(&format!("{prefix}.write_alloc={:?}\n", cfg.write_alloc));
    s.push_str(&format!("{prefix}.hit_latency={}\n", cfg.hit_latency));
    s.push_str(&format!("{prefix}.miss_penalty={}\n", cfg.miss_penalty));
    s.push_str(&format!("{prefix}.assoc_penalty={}\n", cfg.assoc_penalty));
    s.push_str(&format!("{prefix}.transfer_width={}\n", cfg.transfer_width));
}

fn serialize_cache_configs(icfg: &CacheConfig, dcfg: &CacheConfig, extra: &[CacheConfig]) -> String {
    let mut s = String::from("# FALCON-ASM Cache Config v2\n");
    s.push_str(&format!("levels={}\n", extra.len()));
    serialize_one_config(&mut s, "icache", icfg);
    serialize_one_config(&mut s, "dcache", dcfg);
    for (i, cfg) in extra.iter().enumerate() {
        let prefix = level_prefix(i);
        serialize_one_config(&mut s, &prefix, cfg);
    }
    s
}

/// Returns prefix like "l2", "l3", etc. for extra_level index i (0-based → L2, L3, …)
fn level_prefix(i: usize) -> String {
    format!("l{}", i + 2)
}

fn parse_cache_configs(text: &str) -> Result<(CacheConfig, CacheConfig, Vec<CacheConfig>), String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let icfg = parse_single_config(&map, "icache")?;
    let dcfg = parse_single_config(&map, "dcache")?;

    // Read number of extra levels (v2 format); default 0 for v1
    let n_extra: usize = map.get("levels")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut extra = Vec::with_capacity(n_extra);
    let presets = extra_level_presets();
    for i in 0..n_extra {
        let prefix = level_prefix(i);
        // If prefix keys are present, parse them; otherwise use a default preset
        if map.contains_key(&format!("{prefix}.size")) {
            extra.push(parse_single_config(&map, &prefix)?);
        } else {
            extra.push(presets[1].clone()); // medium preset as fallback
        }
    }

    Ok((icfg, dcfg, extra))
}

fn parse_single_config(map: &HashMap<String, String>, prefix: &str) -> Result<CacheConfig, String> {
    let get = |key: &str| -> Result<&str, String> {
        map.get(&format!("{prefix}.{key}"))
            .map(|s| s.as_str())
            .ok_or_else(|| format!("Missing {prefix}.{key}"))
    };
    let get_usize = |key: &str| -> Result<usize, String> {
        get(key)?.parse::<usize>().map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };
    let get_u64 = |key: &str| -> Result<u64, String> {
        get(key)?.parse::<u64>().map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };

    let replacement = match get("replacement")? {
        "Lru" => ReplacementPolicy::Lru,
        "Mru" => ReplacementPolicy::Mru,
        "Fifo" => ReplacementPolicy::Fifo,
        "Random" => ReplacementPolicy::Random,
        "Lfu" => ReplacementPolicy::Lfu,
        "Clock" => ReplacementPolicy::Clock,
        other => return Err(format!("Unknown replacement policy: {other}")),
    };
    let write_policy = match get("write_policy")? {
        "WriteThrough" => WritePolicy::WriteThrough,
        "WriteBack" => WritePolicy::WriteBack,
        other => return Err(format!("Unknown write_policy: {other}")),
    };
    let write_alloc = match get("write_alloc")? {
        "WriteAllocate" => WriteAllocPolicy::WriteAllocate,
        "NoWriteAllocate" => WriteAllocPolicy::NoWriteAllocate,
        other => return Err(format!("Unknown write_alloc: {other}")),
    };

    let assoc_penalty = map.get(&format!("{prefix}.assoc_penalty"))
        .and_then(|v| v.parse::<u64>().ok()).unwrap_or(1);
    let transfer_width = map.get(&format!("{prefix}.transfer_width"))
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(8).max(1);

    use crate::falcon::cache::InclusionPolicy;
    let inclusion = match map.get(&format!("{prefix}.inclusion")).map(String::as_str).unwrap_or("NonInclusive") {
        "Inclusive"  => InclusionPolicy::Inclusive,
        "Exclusive"  => InclusionPolicy::Exclusive,
        _            => InclusionPolicy::NonInclusive,
    };

    Ok(CacheConfig {
        size: get_usize("size")?,
        line_size: get_usize("line_size")?,
        associativity: get_usize("associativity")?,
        replacement,
        write_policy,
        write_alloc,
        inclusion,
        hit_latency: get_u64("hit_latency")?,
        miss_penalty: get_u64("miss_penalty")?,
        assoc_penalty,
        transfer_width,
    })
}

// ── Simulation results export/import ─────────────────────────────────────────

fn make_level_snapshot(name: &str, cache: &Cache, _instructions: u64, amat: f64) -> LevelSnapshot {
    let cfg = &cache.config;
    LevelSnapshot {
        name: name.to_string(),
        size: cfg.size, line_size: cfg.line_size, associativity: cfg.associativity,
        replacement: format!("{:?}", cfg.replacement),
        write_policy: format!("{:?}", cfg.write_policy),
        hit_latency: cfg.hit_latency, miss_penalty: cfg.miss_penalty,
        hits: cache.stats.hits, misses: cache.stats.misses,
        evictions: cache.stats.evictions, writebacks: cache.stats.writebacks,
        bytes_loaded: cache.stats.bytes_loaded, bytes_stored: cache.stats.bytes_stored,
        total_cycles: cache.stats.total_cycles, ram_write_bytes: cache.stats.ram_write_bytes,
        amat,
    }
}

fn capture_snapshot(app: &App) -> CacheResultsSnapshot {
    let mem = &app.run.mem;
    let i_amat = mem.icache_amat();
    let d_amat = mem.dcache_amat();
    let icache_snap = make_level_snapshot("I-Cache L1", &mem.icache, mem.instruction_count, i_amat);
    let dcache_snap = make_level_snapshot("D-Cache L1", &mem.dcache, mem.instruction_count, d_amat);
    let extra_snaps: Vec<LevelSnapshot> = mem.extra_levels.iter().enumerate().map(|(i, lvl)| {
        let name = format!("{} Unified", crate::falcon::cache::CacheController::extra_level_name(i));
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 { lvl.config.hit_latency as f64 }
                   else { lvl.stats.total_cycles as f64 / total as f64 };
        make_level_snapshot(&name, lvl, mem.instruction_count, amat)
    }).collect();

    let mut hotspots: Vec<(u32, u64)> = mem.icache.stats.miss_pcs.iter().map(|(&k, &v)| (k, v)).collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(10);

    CacheResultsSnapshot {
        label: String::new(),
        instruction_count: mem.instruction_count,
        total_cycles: mem.total_program_cycles(),
        base_cycles: mem.extra_cycles,
        cpi: mem.overall_cpi(),
        ipc: mem.ipc(),
        icache: icache_snap,
        dcache: dcache_snap,
        extra_levels: extra_snaps,
        cpi_config: app.run.cpi_config.clone(),
        miss_hotspots: hotspots,
        hit_rate_history_i: mem.icache.stats.history.iter().cloned().collect(),
        hit_rate_history_d: mem.dcache.stats.history.iter().cloned().collect(),
    }
}

pub(super) fn do_export_results(app: &mut App) {
    let mut snap = capture_snapshot(app);
    if let Some(path) = OSFileDialog::new()
        .add_filter("FALCON Stats", &["fstats"])
        .add_filter("CSV Spreadsheet", &["csv"])
        .set_file_name("results.fstats")
        .save_file()
    {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("fstats");
        snap.label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let text = if ext == "csv" { serialize_results_csv(&snap) } else { serialize_results_fstats(&snap) };
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.cache.config_status = Some(format!(
                    "Results exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                app.cache.config_error = None;
            }
            Err(e) => {
                app.cache.config_error = Some(format!("Export failed: {e}"));
                app.cache.config_status = None;
            }
        }
    }
}

pub(super) fn do_compare_load(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("FALCON Stats", &["fstats"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_results_snapshot(&text) {
                Ok(mut snap) => {
                    snap.label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    app.cache.loaded_snapshot = Some(Box::new(snap));
                    app.cache.config_status = Some(format!(
                        "Baseline loaded from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    app.cache.config_error = None;
                }
                Err(msg) => {
                    app.cache.config_error = Some(format!("Load failed: {msg}"));
                    app.cache.config_status = None;
                }
            },
            Err(e) => {
                app.cache.config_error = Some(format!("Load failed: {e}"));
                app.cache.config_status = None;
            }
        }
    }
}

fn write_level_snap(s: &mut String, prefix: &str, l: &LevelSnapshot) {
    s.push_str(&format!("{prefix}.name={}\n", l.name));
    s.push_str(&format!("{prefix}.size={}\n", l.size));
    s.push_str(&format!("{prefix}.line_size={}\n", l.line_size));
    s.push_str(&format!("{prefix}.associativity={}\n", l.associativity));
    s.push_str(&format!("{prefix}.replacement={}\n", l.replacement));
    s.push_str(&format!("{prefix}.write_policy={}\n", l.write_policy));
    s.push_str(&format!("{prefix}.hit_latency={}\n", l.hit_latency));
    s.push_str(&format!("{prefix}.miss_penalty={}\n", l.miss_penalty));
    s.push_str(&format!("{prefix}.hits={}\n", l.hits));
    s.push_str(&format!("{prefix}.misses={}\n", l.misses));
    s.push_str(&format!("{prefix}.evictions={}\n", l.evictions));
    s.push_str(&format!("{prefix}.writebacks={}\n", l.writebacks));
    s.push_str(&format!("{prefix}.bytes_loaded={}\n", l.bytes_loaded));
    s.push_str(&format!("{prefix}.bytes_stored={}\n", l.bytes_stored));
    s.push_str(&format!("{prefix}.total_cycles={}\n", l.total_cycles));
    s.push_str(&format!("{prefix}.ram_write_bytes={}\n", l.ram_write_bytes));
    s.push_str(&format!("{prefix}.amat={:.4}\n", l.amat));
}

fn serialize_results_fstats(snap: &CacheResultsSnapshot) -> String {
    let mut s = String::from("# FALCON-ASM Simulation Results v1\n");
    s.push_str(&format!("label={}\n", snap.label));
    s.push_str(&format!("prog.instructions={}\n", snap.instruction_count));
    s.push_str(&format!("prog.total_cycles={}\n", snap.total_cycles));
    s.push_str(&format!("prog.base_cycles={}\n", snap.base_cycles));
    s.push_str(&format!("prog.cache_cycles={}\n", snap.total_cycles.saturating_sub(snap.base_cycles)));
    s.push_str(&format!("prog.cpi={:.4}\n", snap.cpi));
    s.push_str(&format!("prog.ipc={:.4}\n", snap.ipc));
    s.push_str(&format!("extra_levels={}\n", snap.extra_levels.len()));
    write_level_snap(&mut s, "icache", &snap.icache);
    write_level_snap(&mut s, "dcache", &snap.dcache);
    for (i, lvl) in snap.extra_levels.iter().enumerate() {
        write_level_snap(&mut s, &format!("l{}", i + 2), lvl);
    }
    let cpi = &snap.cpi_config;
    s.push_str(&format!("cpi.alu={}\ncpi.mul={}\ncpi.div={}\n", cpi.alu, cpi.mul, cpi.div));
    s.push_str(&format!("cpi.load={}\ncpi.store={}\n", cpi.load, cpi.store));
    s.push_str(&format!("cpi.branch_taken={}\ncpi.branch_not_taken={}\n", cpi.branch_taken, cpi.branch_not_taken));
    s.push_str(&format!("cpi.jump={}\ncpi.system={}\n", cpi.jump, cpi.system));
    s.push_str(&format!("miss_hotspot_count={}\n", snap.miss_hotspots.len()));
    for (i, (pc, count)) in snap.miss_hotspots.iter().enumerate() {
        s.push_str(&format!("miss_hotspot.{i}.pc=0x{pc:08x}\n"));
        s.push_str(&format!("miss_hotspot.{i}.count={count}\n"));
    }
    s.push_str(&format!("history_i_count={}\n", snap.hit_rate_history_i.len()));
    for (i, (x, y)) in snap.hit_rate_history_i.iter().enumerate() {
        s.push_str(&format!("history_i.{i}={x}:{y}\n"));
    }
    s.push_str(&format!("history_d_count={}\n", snap.hit_rate_history_d.len()));
    for (i, (x, y)) in snap.hit_rate_history_d.iter().enumerate() {
        s.push_str(&format!("history_d.{i}={x}:{y}\n"));
    }
    s
}

fn csv_level_row(s: &mut String, label: &str, l: &LevelSnapshot, instructions: u64) {
    let total = l.hits + l.misses;
    let hit_rate = if total == 0 { 0.0 } else { l.hits as f64 / total as f64 * 100.0 };
    let miss_rate = 100.0 - hit_rate;
    let mpki = if instructions == 0 { 0.0 } else { l.misses as f64 / instructions as f64 * 1000.0 };
    s.push_str(&format!(
        "{label},{},{},{},{:.1},{:.1},{:.2},{:.2},{},{},{},{},{}\n",
        l.hits, l.misses, total, hit_rate, miss_rate, mpki, l.amat,
        l.evictions, l.writebacks, l.bytes_loaded, l.ram_write_bytes, l.total_cycles
    ));
}

fn serialize_results_csv(snap: &CacheResultsSnapshot) -> String {
    let mut s = String::new();
    s.push_str("PROGRAM SUMMARY\n");
    s.push_str("Instructions,Total Cycles,Base Cycles,Cache Cycles,CPI,IPC\n");
    s.push_str(&format!(
        "{},{},{},{},{:.4},{:.4}\n",
        snap.instruction_count, snap.total_cycles, snap.base_cycles,
        snap.total_cycles.saturating_sub(snap.base_cycles),
        snap.cpi, snap.ipc
    ));
    s.push('\n');
    s.push_str("CACHE LEVELS\n");
    s.push_str("Level,Hits,Misses,Total Accesses,Hit Rate (%),Miss Rate (%),MPKI,AMAT (cycles),Evictions,Writebacks,RAM Reads (B),RAM Writes (B),Total Cycles\n");
    csv_level_row(&mut s, "I-Cache L1", &snap.icache, snap.instruction_count);
    csv_level_row(&mut s, "D-Cache L1", &snap.dcache, snap.instruction_count);
    for lvl in &snap.extra_levels {
        csv_level_row(&mut s, &lvl.name, lvl, snap.instruction_count);
    }
    s.push('\n');
    s.push_str("MISS HOTSPOTS (I-Cache)\n");
    s.push_str("PC,Miss Count\n");
    for (pc, count) in &snap.miss_hotspots {
        s.push_str(&format!("0x{pc:08x},{count}\n"));
    }
    s
}

fn parse_level_snap(map: &std::collections::HashMap<String, String>, prefix: &str) -> Result<LevelSnapshot, String> {
    let get_str = |key: &str| -> String {
        map.get(&format!("{prefix}.{key}")).cloned().unwrap_or_default()
    };
    let get_u64 = |key: &str| -> u64 {
        map.get(&format!("{prefix}.{key}")).and_then(|v| v.parse().ok()).unwrap_or(0)
    };
    let get_f64 = |key: &str| -> f64 {
        map.get(&format!("{prefix}.{key}")).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    };
    Ok(LevelSnapshot {
        name: get_str("name"),
        size: map.get(&format!("{prefix}.size")).and_then(|v| v.parse().ok()).unwrap_or(0),
        line_size: map.get(&format!("{prefix}.line_size")).and_then(|v| v.parse().ok()).unwrap_or(1),
        associativity: map.get(&format!("{prefix}.associativity")).and_then(|v| v.parse().ok()).unwrap_or(1),
        replacement: get_str("replacement"),
        write_policy: get_str("write_policy"),
        hit_latency: get_u64("hit_latency"),
        miss_penalty: get_u64("miss_penalty"),
        hits: get_u64("hits"), misses: get_u64("misses"),
        evictions: get_u64("evictions"), writebacks: get_u64("writebacks"),
        bytes_loaded: get_u64("bytes_loaded"), bytes_stored: get_u64("bytes_stored"),
        total_cycles: get_u64("total_cycles"), ram_write_bytes: get_u64("ram_write_bytes"),
        amat: get_f64("amat"),
    })
}

fn parse_results_snapshot(text: &str) -> Result<CacheResultsSnapshot, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let get_u64 = |key: &str| -> Result<u64, String> {
        map.get(key).and_then(|v| v.parse().ok())
            .ok_or_else(|| format!("Missing or invalid key: {key}"))
    };
    let get_f64 = |key: &str| -> f64 {
        map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    };

    let n_extra: usize = map.get("extra_levels").and_then(|v| v.parse().ok()).unwrap_or(0);
    let icache = parse_level_snap(&map, "icache")?;
    let dcache = parse_level_snap(&map, "dcache")?;
    let mut extra = Vec::new();
    for i in 0..n_extra {
        extra.push(parse_level_snap(&map, &format!("l{}", i + 2))?);
    }

    let cpi_config = CpiConfig {
        alu:              map.get("cpi.alu").and_then(|v| v.parse().ok()).unwrap_or(1),
        mul:              map.get("cpi.mul").and_then(|v| v.parse().ok()).unwrap_or(3),
        div:              map.get("cpi.div").and_then(|v| v.parse().ok()).unwrap_or(20),
        load:             map.get("cpi.load").and_then(|v| v.parse().ok()).unwrap_or(0),
        store:            map.get("cpi.store").and_then(|v| v.parse().ok()).unwrap_or(0),
        branch_taken:     map.get("cpi.branch_taken").and_then(|v| v.parse().ok()).unwrap_or(3),
        branch_not_taken: map.get("cpi.branch_not_taken").and_then(|v| v.parse().ok()).unwrap_or(1),
        jump:             map.get("cpi.jump").and_then(|v| v.parse().ok()).unwrap_or(2),
        system:           map.get("cpi.system").and_then(|v| v.parse().ok()).unwrap_or(10),
        fp:               map.get("cpi.fp").and_then(|v| v.parse().ok()).unwrap_or(5),
    };

    let n_hotspots: usize = map.get("miss_hotspot_count").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut hotspots = Vec::new();
    for i in 0..n_hotspots {
        let pc_str = map.get(&format!("miss_hotspot.{i}.pc")).cloned().unwrap_or_default();
        let count: u64 = map.get(&format!("miss_hotspot.{i}.count")).and_then(|v| v.parse().ok()).unwrap_or(0);
        let pc = u32::from_str_radix(pc_str.trim_start_matches("0x"), 16).unwrap_or(0);
        hotspots.push((pc, count));
    }

    let n_hist_i: usize = map.get("history_i_count").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut hist_i = Vec::new();
    for i in 0..n_hist_i {
        if let Some(val) = map.get(&format!("history_i.{i}")) {
            if let Some((xs, ys)) = val.split_once(':') {
                if let (Ok(x), Ok(y)) = (xs.parse::<f64>(), ys.parse::<f64>()) {
                    hist_i.push((x, y));
                }
            }
        }
    }
    let n_hist_d: usize = map.get("history_d_count").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut hist_d = Vec::new();
    for i in 0..n_hist_d {
        if let Some(val) = map.get(&format!("history_d.{i}")) {
            if let Some((xs, ys)) = val.split_once(':') {
                if let (Ok(x), Ok(y)) = (xs.parse::<f64>(), ys.parse::<f64>()) {
                    hist_d.push((x, y));
                }
            }
        }
    }

    Ok(CacheResultsSnapshot {
        label: map.get("label").cloned().unwrap_or_default(),
        instruction_count: get_u64("prog.instructions")?,
        total_cycles: get_u64("prog.total_cycles")?,
        base_cycles: map.get("prog.base_cycles").and_then(|v| v.parse().ok()).unwrap_or(0),
        cpi: get_f64("prog.cpi"),
        ipc: get_f64("prog.ipc"),
        icache, dcache,
        extra_levels: extra,
        cpi_config,
        miss_hotspots: hotspots,
        hit_rate_history_i: hist_i,
        hit_rate_history_d: hist_d,
    })
}

fn apply_imem_search(app: &mut App) {
    let q = app.run.imem_search_query.trim().to_lowercase();
    if q.is_empty() { return; }
    let mut matches: Vec<u32> = app.run.labels.iter()
        .filter(|(_, labels)| labels.iter().any(|l| l.to_lowercase().contains(&q)))
        .map(|(&addr, _)| addr)
        .collect();
    matches.sort();
    if let Some(&addr) = matches.first() {
        app.scroll_imem_to_addr(addr);
    }
}

fn apply_mem_search(app: &mut App) {
    let q = app.run.mem_search_query
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if let Ok(addr) = u32::from_str_radix(q, 16) {
        let aligned = addr & !(app.run.mem_view_bytes - 1);
        let max = app.run.mem_size.saturating_sub(app.run.mem_view_bytes as usize) as u32;
        app.run.mem_view_addr = aligned.min(max);
        app.run.mem_region = crate::ui::app::MemRegion::Custom;
    }
}
