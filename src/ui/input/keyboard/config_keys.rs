use crate::falcon::jit::BackendKind;
use crate::ui::app::{
    App, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_JIT_MODE, SETTINGS_ROW_MAX_CORES,
    SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED, SETTINGS_ROW_RUN_SCOPE,
    SETTINGS_ROW_SCREEN_TARGET, SETTINGS_ROW_TLB_ENABLED, SETTINGS_ROW_TRACE_SYSCALLS,
    SETTINGS_ROW_VM_ENABLED, SETTINGS_ROWS,
};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    if matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A')) {
        app.cycle_architecture();
        return true;
    }
    if app.settings.num_editing {
        return handle_numeric_edit(app, key.code);
    }

    // The rows are contiguous again now that the CPI block has moved to the
    // Pipeline tab — the blank-row skip that used to sit here existed only to
    // step over the separator between the toggles and that block.
    match key.code {
        KeyCode::Up => {
            app.settings.selected = app.settings.selected.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            app.settings.selected = (app.settings.selected + 1).min(SETTINGS_ROWS - 1);
            true
        }
        KeyCode::Left | KeyCode::Right => true,
        KeyCode::Enter | KeyCode::Char(' ') => {
            if app.settings.selected == SETTINGS_ROW_CACHE_ENABLED {
                app.set_cache_enabled(!app.session.cache_enabled);
            } else if app.settings.selected == SETTINGS_ROW_MAX_CORES {
                app.settings.num_edit_buf = app.max_cores.to_string();
                app.settings.num_editing = true;
            } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
                app.settings.num_edit_buf = (app.session.mem_size / 1024).to_string();
                app.settings.num_editing = true;
            } else if app.settings.selected == SETTINGS_ROW_RUN_SCOPE {
                app.run_scope = app.run_scope.cycle();
            } else if app.settings.selected == SETTINGS_ROW_PIPELINE_ENABLED {
                app.set_pipeline_enabled(
                    !app.pipeline_status().is_some_and(|status| status.enabled),
                );
            } else if app.settings.selected == SETTINGS_ROW_VM_ENABLED {
                app.set_vm_mode(app.vm_mode().cycle());
            } else if app.settings.selected == SETTINGS_ROW_TLB_ENABLED {
                app.set_tlb_enabled(!app.session.tlb_enabled);
            } else if app.settings.selected == SETTINGS_ROW_JIT_MODE {
                let next = match app.session.jit_kind {
                    BackendKind::None => BackendKind::Hot,
                    BackendKind::Hot => BackendKind::Full,
                    BackendKind::Full => BackendKind::None,
                };
                app.set_jit_mode(next);
            } else if app.settings.selected == SETTINGS_ROW_TRACE_SYSCALLS {
                app.set_trace_syscalls(!app.session.trace_syscalls);
            } else if app.settings.selected == SETTINGS_ROW_SCREEN_TARGET {
                app.console.screen_target = app.console.screen_target.cycle();
            }
            true
        }
        KeyCode::Char(c)
            if (app.settings.selected == SETTINGS_ROW_MAX_CORES
                || app.settings.selected == SETTINGS_ROW_MEM_SIZE)
                && c.is_ascii_digit() =>
        {
            app.settings.num_edit_buf.clear();
            app.settings.num_edit_buf.push(c);
            app.settings.num_editing = true;
            true
        }
        _ => false,
    }
}

fn handle_numeric_edit(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc => {
            app.settings.num_editing = false;
            app.settings.num_edit_buf.clear();
        }
        KeyCode::Enter => {
            commit_numeric_edit(app);
            app.settings.num_editing = false;
            app.settings.num_edit_buf.clear();
        }
        KeyCode::Up => {
            commit_numeric_edit(app);
            app.settings.num_editing = false;
            app.settings.num_edit_buf.clear();
            app.settings.selected = app.settings.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Tab => {
            commit_numeric_edit(app);
            app.settings.num_editing = false;
            app.settings.num_edit_buf.clear();
            app.settings.selected = (app.settings.selected + 1).min(SETTINGS_ROWS - 1);
        }
        KeyCode::Char(c)
            if c.is_ascii_digit()
                || (app.settings.selected == SETTINGS_ROW_MEM_SIZE
                    && matches!(c, 'k' | 'K' | 'm' | 'M' | 'b' | 'B')) =>
        {
            app.settings.num_edit_buf.push(c);
        }
        KeyCode::Backspace => {
            app.settings.num_edit_buf.pop();
        }
        _ => {}
    }

    true
}

fn commit_numeric_edit(app: &mut App) {
    if app.settings.selected == SETTINGS_ROW_MAX_CORES {
        if let Ok(v) = app.settings.num_edit_buf.trim().parse::<usize>() {
            if (1..=32).contains(&v) && v != app.max_cores {
                app.max_cores = v;
                app.restart_simulation();
            }
        }
    } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
        let raw = app.settings.num_edit_buf.trim().to_lowercase();
        let kb = if let Some(n) = raw.strip_suffix("mb") {
            n.trim().parse::<usize>().ok().map(|v| v * 1024)
        } else if let Some(n) = raw.strip_suffix("kb") {
            n.trim().parse::<usize>().ok()
        } else {
            raw.parse::<usize>().ok()
        };
        if let Some(v) = kb {
            let snapped = crate::ui::app::nearest_pow2_clamp(v.max(4), 4, 4 * 1024 * 1024);
            let new_bytes = snapped * 1024;
            if new_bytes != app.session.mem_size {
                app.ram_override = Some(new_bytes);
                app.restart_simulation();
            }
        }
    }
    // Only max cores and memory size are typed here now; the CPI fields moved
    // to the Pipeline tab and commit through `pipeline_keys::commit_cpi_edit`.
}
