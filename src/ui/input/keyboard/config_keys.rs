use crate::ui::app::{
    App, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_CPI_START, SETTINGS_ROW_MAX_CORES,
    SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED, SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROWS,
};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    if app.settings.cpi_editing {
        return handle_numeric_edit(app, key.code);
    }

    match key.code {
        KeyCode::Up => {
            if app.settings.selected > 0 {
                app.settings.selected -= 1;
                if app.settings.selected == 5 {
                    app.settings.selected = SETTINGS_ROW_PIPELINE_ENABLED;
                }
            }
            true
        }
        KeyCode::Down => {
            if app.settings.selected + 1 < SETTINGS_ROWS {
                app.settings.selected += 1;
                if app.settings.selected == 5 {
                    app.settings.selected = SETTINGS_ROW_CPI_START;
                }
            }
            true
        }
        KeyCode::Left | KeyCode::Right => true,
        KeyCode::Enter | KeyCode::Char(' ') => {
            if app.settings.selected == SETTINGS_ROW_CACHE_ENABLED {
                app.set_cache_enabled(!app.run.cache_enabled);
            } else if app.settings.selected == SETTINGS_ROW_MAX_CORES {
                app.settings.cpi_edit_buf = app.max_cores.to_string();
                app.settings.cpi_editing = true;
            } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
                app.settings.cpi_edit_buf = (app.run.mem_size / 1024).to_string();
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
            true
        }
        KeyCode::Char(c)
            if (app.settings.selected == SETTINGS_ROW_MAX_CORES
                || app.settings.selected == SETTINGS_ROW_MEM_SIZE)
                && c.is_ascii_digit() =>
        {
            app.settings.cpi_edit_buf.clear();
            app.settings.cpi_edit_buf.push(c);
            app.settings.cpi_editing = true;
            true
        }
        _ => false,
    }
}

fn handle_numeric_edit(app: &mut App, code: KeyCode) -> bool {
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
        KeyCode::Char(c)
            if c.is_ascii_digit()
                || (app.settings.selected == SETTINGS_ROW_MEM_SIZE
                    && matches!(c, 'k' | 'K' | 'm' | 'M' | 'b' | 'B')) =>
        {
            app.settings.cpi_edit_buf.push(c);
        }
        KeyCode::Backspace => {
            app.settings.cpi_edit_buf.pop();
        }
        _ => {}
    }

    true
}

fn commit_numeric_edit(app: &mut App) {
    if app.settings.selected == SETTINGS_ROW_MAX_CORES {
        if let Ok(v) = app.settings.cpi_edit_buf.trim().parse::<usize>() {
            if (1..=32).contains(&v) && v != app.max_cores {
                app.max_cores = v;
                app.restart_simulation();
            }
        }
    } else if app.settings.selected == SETTINGS_ROW_MEM_SIZE {
        let raw = app.settings.cpi_edit_buf.trim().to_lowercase();
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
            if new_bytes != app.run.mem_size {
                app.ram_override = Some(new_bytes);
                app.restart_simulation();
            }
        }
    } else {
        let cpi_idx = app.settings.selected.saturating_sub(SETTINGS_ROW_CPI_START);
        if let Ok(v) = app.settings.cpi_edit_buf.trim().parse::<u64>() {
            app.run.cpi_config.set(cpi_idx, v);
        }
    }
}
