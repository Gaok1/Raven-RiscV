use crate::falcon::cache::{CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::ui::app::{App, CacheScope, CacheSubtab, EditorMode, MemRegion, Tab};
use crate::ui::view::docs::docs_body_line_count;
use arboard::Clipboard;
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

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    if matches!(app.tab, Tab::Run) && matches!(key.code, KeyCode::Char('R')) {
        app.restart_simulation();
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
                    if let Ok(mut clip) = Clipboard::new() {
                        let _ = clip.set_text(text);
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

            if ctrl && matches!(key.code, KeyCode::Char('a')) && matches!(app.tab, Tab::Editor) {
                app.editor.buf.select_all();
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
                    if let Ok(mut clip) = Clipboard::new() {
                        let _ = clip.set_text(text);
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
                let text = serialize_cache_configs(&app.cache.pending_icache, &app.cache.pending_dcache);
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
                            Ok((icfg, dcfg)) => {
                                app.cache.pending_icache = icfg;
                                app.cache.pending_dcache = dcfg;
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
                (KeyCode::Char('p'), Tab::Run) => {
                    if app.run.is_running {
                        app.run.is_running = false;
                    } else if !app.run.faulted {
                        app.run.is_running = true;
                    }
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
                }
                (KeyCode::Down, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
                }
                (KeyCode::PageUp, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    app.run.regs_scroll = app.run.regs_scroll.saturating_sub(10);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                }
                (KeyCode::PageDown, Tab::Run) if app.run.show_registers => {
                    let max_scroll = max_regs_scroll(app);
                    if app.run.regs_scroll > max_scroll {
                        app.run.regs_scroll = max_scroll;
                    }
                    app.run.regs_scroll = (app.run.regs_scroll + 10).min(max_scroll);
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

                // Docs scroll
                (KeyCode::Up, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_sub(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::Down, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_add(1); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageUp, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_sub(10); clamp_docs_scroll_keyboard(app); }
                (KeyCode::PageDown, Tab::Docs) => { app.docs.scroll = app.docs.scroll.saturating_add(10); clamp_docs_scroll_keyboard(app); }

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
                        }
                        KeyCode::Backspace if field.is_numeric() => {
                            app.cache.edit_buf.pop();
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
                (KeyCode::Char('r'), Tab::Cache) => {
                    app.run.mem.reset_stats();
                }
                (KeyCode::Char('p'), Tab::Cache) => {
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
                (KeyCode::Up, Tab::Cache) => match app.cache.subtab {
                    CacheSubtab::Stats => {
                        app.cache.stats_scroll = app.cache.stats_scroll.saturating_sub(1);
                    }
                    CacheSubtab::View => {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
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
                    _ => {}
                },

                // Editor navigation in command mode
                (KeyCode::Up, Tab::Editor) => app.editor.buf.move_up(),
                (KeyCode::Down, Tab::Editor) => app.editor.buf.move_down(),
                _ => {}
            }
        }
    }

    Ok(false)
}

fn clamp_docs_scroll_keyboard(app: &mut App) {
    if let Ok((w, h)) = terminal::size() {
        let docs_area_h = h.saturating_sub(4);
        let table_h = docs_area_h.saturating_sub(2);
        let viewport_h = table_h.saturating_sub(4) as usize;
        if viewport_h == 0 {
            app.docs.scroll = 0;
            return;
        }
        let total_body = docs_body_line_count(w);
        let max_start = total_body.saturating_sub(viewport_h);
        if app.docs.scroll > max_start {
            app.docs.scroll = max_start;
        }
    }
}

// ── Cache config serialization ────────────────────────────────────────────────

fn serialize_cache_configs(icfg: &CacheConfig, dcfg: &CacheConfig) -> String {
    let mut s = String::from("# FALCON-ASM Cache Config v1\n");
    for (prefix, cfg) in [("icache", icfg), ("dcache", dcfg)] {
        s.push_str(&format!("{prefix}.size={}\n", cfg.size));
        s.push_str(&format!("{prefix}.line_size={}\n", cfg.line_size));
        s.push_str(&format!("{prefix}.associativity={}\n", cfg.associativity));
        s.push_str(&format!("{prefix}.replacement={:?}\n", cfg.replacement));
        s.push_str(&format!("{prefix}.write_policy={:?}\n", cfg.write_policy));
        s.push_str(&format!("{prefix}.write_alloc={:?}\n", cfg.write_alloc));
        s.push_str(&format!("{prefix}.hit_latency={}\n", cfg.hit_latency));
        s.push_str(&format!("{prefix}.miss_penalty={}\n", cfg.miss_penalty));
    }
    s
}

fn parse_cache_configs(text: &str) -> Result<(CacheConfig, CacheConfig), String> {
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
    Ok((icfg, dcfg))
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

    Ok(CacheConfig {
        size: get_usize("size")?,
        line_size: get_usize("line_size")?,
        associativity: get_usize("associativity")?,
        replacement,
        write_policy,
        write_alloc,
        hit_latency: get_u64("hit_latency")?,
        miss_penalty: get_u64("miss_penalty")?,
    })
}
