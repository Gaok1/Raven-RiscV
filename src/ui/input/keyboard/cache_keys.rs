use crate::ui::app::{App, CacheDataFmt, CacheScope, CacheSubtab, CacheViewFocus};
use crossterm::event::{KeyCode, KeyEvent};

use super::run_keys::cycle_memory_region;
use super::serialization::capture_snapshot;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    if matches!(app.cache.subtab, CacheSubtab::Config) && app.cache.edit_field.is_some() {
        return handle_config_field_edit(app, key.code);
    }

    match key.code {
        KeyCode::Tab => {
            app.cache.subtab = match app.cache.subtab {
                CacheSubtab::Stats => CacheSubtab::View,
                CacheSubtab::View => CacheSubtab::Config,
                CacheSubtab::Config => CacheSubtab::Stats,
            };
            true
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.add_cache_level();
            true
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            app.remove_last_cache_level();
            true
        }
        KeyCode::Char('r') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.restart_simulation();
            true
        }
        KeyCode::Char('p') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            if app.run.is_running {
                app.run.is_running = false;
            } else if app.core_status(app.selected_core) == crate::ui::app::HartLifecycle::Paused
                || !app.run.faulted
            {
                app.resume_selected_hart();
                if app.can_start_run() {
                    app.run.is_running = true;
                }
            }
            true
        }
        KeyCode::Char('i') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.cache.scope = CacheScope::ICache;
            app.cache.view_focus = CacheViewFocus::ICache;
            true
        }
        KeyCode::Char('d') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.cache.scope = CacheScope::DCache;
            app.cache.view_focus = CacheViewFocus::DCache;
            true
        }
        KeyCode::Char('b') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.cache.scope = CacheScope::Both;
            true
        }
        KeyCode::Char('m') if matches!(app.cache.subtab, CacheSubtab::View) => {
            app.cache.data_fmt = app.cache.data_fmt.cycle();
            true
        }
        KeyCode::Char('g') if matches!(app.cache.subtab, CacheSubtab::View) => {
            if app.cache.data_fmt != CacheDataFmt::Float {
                app.cache.data_group = app.cache.data_group.cycle();
            }
            true
        }
        KeyCode::Char('t') if matches!(app.cache.subtab, CacheSubtab::View) => {
            app.cache.addr_mode = app.cache.addr_mode.cycle();
            true
        }
        KeyCode::Char('v') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            if app.run.show_dyn {
                app.run.show_dyn = false;
            } else if app.run.show_registers {
                app.run.show_registers = false;
                app.run.show_dyn = true;
            } else {
                app.run.show_registers = true;
            }
            true
        }
        KeyCode::Char('k') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            cycle_memory_region(app);
            true
        }
        KeyCode::Char('e') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.run.show_exec_count = !app.run.show_exec_count;
            true
        }
        KeyCode::Char('y') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.run.show_instr_type = !app.run.show_instr_type;
            true
        }
        KeyCode::Char('s') if matches!(app.cache.subtab, CacheSubtab::Stats) => {
            let snap = capture_snapshot(app);
            let label = snap.label.clone();
            let instr_end = snap.instr_end;
            app.cache.session_history.push(snap);
            app.cache.history_scroll = app.cache.session_history.len().saturating_sub(1);
            app.cache.window_start_instr = instr_end;
            app.cache.config_status = Some(format!("Captured {label}"));
            true
        }
        KeyCode::Char('s') if matches!(app.cache.subtab, CacheSubtab::View) => {
            if !app.run.faulted {
                app.single_step();
            }
            true
        }
        KeyCode::Char('f') if !matches!(app.cache.subtab, CacheSubtab::Config) => {
            app.run.speed = app.run.speed.cycle();
            true
        }
        KeyCode::Char('D')
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
            true
        }
        KeyCode::Enter
            if matches!(app.cache.subtab, CacheSubtab::Stats)
                && !app.cache.session_history.is_empty()
                && !app.run.is_running =>
        {
            let idx = app
                .cache
                .history_scroll
                .min(app.cache.session_history.len() - 1);
            app.cache.viewing_snapshot = Some(idx);
            true
        }
        KeyCode::Up => {
            match app.cache.subtab {
                CacheSubtab::Stats => {
                    app.cache.history_scroll = app.cache.history_scroll.saturating_sub(1);
                }
                CacheSubtab::View => {
                    if app.cache.selected_level == 0 {
                        match app.cache.scope {
                            CacheScope::DCache => {
                                app.cache.view_scroll_d = app.cache.view_scroll_d.saturating_sub(1);
                            }
                            CacheScope::Both => {
                                if matches!(app.cache.view_focus, CacheViewFocus::DCache) {
                                    app.cache.view_scroll_d =
                                        app.cache.view_scroll_d.saturating_sub(1);
                                } else {
                                    app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                                }
                            }
                            CacheScope::ICache => {
                                app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                            }
                        }
                    } else {
                        app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                    }
                }
                CacheSubtab::Config => {}
            }
            true
        }
        KeyCode::Down => {
            match app.cache.subtab {
                CacheSubtab::Stats => {
                    if !app.cache.session_history.is_empty() {
                        app.cache.history_scroll =
                            (app.cache.history_scroll + 1).min(app.cache.session_history.len() - 1);
                    }
                }
                CacheSubtab::View => {
                    if app.cache.selected_level == 0 {
                        match app.cache.scope {
                            CacheScope::DCache => {
                                if app.cache.view_num_sets_d.get() == 0 {
                                    app.cache.view_scroll_d =
                                        app.cache.view_scroll_d.saturating_add(1);
                                } else {
                                    app.cache.view_scroll_d = (app.cache.view_scroll_d + 1)
                                        .min(app.cache.view_scroll_max_d.get());
                                }
                            }
                            CacheScope::Both => {
                                if matches!(app.cache.view_focus, CacheViewFocus::DCache) {
                                    if app.cache.view_num_sets_d.get() == 0 {
                                        app.cache.view_scroll_d =
                                            app.cache.view_scroll_d.saturating_add(1);
                                    } else {
                                        app.cache.view_scroll_d = (app.cache.view_scroll_d + 1)
                                            .min(app.cache.view_scroll_max_d.get());
                                    }
                                } else {
                                    if app.cache.view_num_sets.get() == 0 {
                                        app.cache.view_scroll =
                                            app.cache.view_scroll.saturating_add(1);
                                    } else {
                                        app.cache.view_scroll = (app.cache.view_scroll + 1)
                                            .min(app.cache.view_scroll_max.get());
                                    }
                                }
                            }
                            CacheScope::ICache => {
                                if app.cache.view_num_sets.get() == 0 {
                                    app.cache.view_scroll = app.cache.view_scroll.saturating_add(1);
                                } else {
                                    app.cache.view_scroll = (app.cache.view_scroll + 1)
                                        .min(app.cache.view_scroll_max.get());
                                }
                            }
                        }
                    } else {
                        if app.cache.view_num_sets.get() == 0 {
                            app.cache.view_scroll = app.cache.view_scroll.saturating_add(1);
                        } else {
                            app.cache.view_scroll =
                                (app.cache.view_scroll + 1).min(app.cache.view_scroll_max.get());
                        }
                    }
                }
                CacheSubtab::Config => {}
            }
            true
        }
        KeyCode::Left if matches!(app.cache.subtab, CacheSubtab::View) => {
            if app.cache.selected_level == 0 {
                match app.cache.scope {
                    CacheScope::DCache => {
                        app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_sub(3);
                    }
                    CacheScope::Both => {
                        if matches!(app.cache.view_focus, CacheViewFocus::DCache) {
                            app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_sub(3);
                        } else {
                            app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
                        }
                    }
                    CacheScope::ICache => {
                        app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
                    }
                }
            } else {
                app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
            }
            true
        }
        KeyCode::Right if matches!(app.cache.subtab, CacheSubtab::View) => {
            if app.cache.selected_level == 0 {
                match app.cache.scope {
                    CacheScope::DCache => {
                        app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_add(3);
                    }
                    CacheScope::Both => {
                        if matches!(app.cache.view_focus, CacheViewFocus::DCache) {
                            app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_add(3);
                        } else {
                            app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
                        }
                    }
                    CacheScope::ICache => {
                        app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
                    }
                }
            } else {
                app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
            }
            true
        }
        _ => false,
    }
}

fn handle_config_field_edit(app: &mut App, code: KeyCode) -> bool {
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

    true
}
