use crate::ui::app::{App, MemRegion};
use crossterm::event::{KeyCode, KeyEvent};

use crate::ui::input::max_regs_scroll;

fn close_mem_search(app: &mut App) {
    app.run.mem_search_open = false;
    app.run.mem_search_query.clear();
}

pub(super) fn handle_execution_key(app: &mut App, code: KeyCode) -> bool {
    if !matches!(app.tab, crate::ui::app::Tab::Run) {
        return false;
    }

    // An open inline editor claims keystrokes first, so the Run shortcuts
    // (`s`, `b`, `r`, …) feed the value buffer instead of stepping.
    if app.run.run_edit.is_some() {
        return handle_run_edit_key(app, code);
    }

    match code {
        KeyCode::Char('s') => {
            if !app.run.faulted {
                app.single_step();
            }
            true
        }
        KeyCode::Char('b') => {
            app.stepback_one();
            true
        }
        KeyCode::Char('r') => {
            app.restart_simulation();
            true
        }
        KeyCode::Char('p') | KeyCode::Char(' ') => {
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
        _ => false,
    }
}

/// Route a keystroke into the open inline editor: Esc cancels, Enter commits,
/// Backspace trims, and characters valid for the active format extend the
/// buffer. Any keystroke clears a stale rejection message.
fn handle_run_edit_key(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc => app.cancel_run_edit(),
        KeyCode::Enter => app.commit_run_edit(),
        KeyCode::Backspace => {
            app.run.run_edit_buf.pop();
            app.run.run_edit_error = None;
        }
        KeyCode::Char(c) if edit_char_allowed(app, c) => {
            app.run.run_edit_buf.push(c);
            app.run.run_edit_error = None;
        }
        _ => {}
    }
    true
}

/// Whether `c` is a legal character for the cell currently being edited. Floats
/// accept a free decimal/scientific form; integer and memory cells follow the
/// Run tab's display format (hex / decimal / raw string).
fn edit_char_allowed(app: &App, c: char) -> bool {
    use crate::ui::app::{FormatMode, RunEditTarget};
    if matches!(app.run.run_edit, Some(RunEditTarget::FReg(_))) {
        return c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+');
    }
    match app.run.fmt_mode {
        FormatMode::Hex => c.is_ascii_hexdigit() || matches!(c, 'x' | 'X' | '_'),
        FormatMode::Dec => c.is_ascii_digit() || matches!(c, '-' | '_'),
        FormatMode::Str => !c.is_control(),
    }
}

pub(super) fn handle(app: &mut App, key: KeyEvent, ctrl: bool) -> bool {
    match key.code {
        KeyCode::Char('v') => {
            if app.run.show_dyn {
                app.run.show_dyn = false;
            } else if app.run.show_registers {
                app.run.show_registers = false;
                app.run.show_dyn = true;
            } else {
                app.run.show_registers = true;
            }
            app.sync_mem_focus_for_active_sidebar_mode();
            if !app.run_sidebar_shows_memory() {
                close_mem_search(app);
            }
            true
        }
        KeyCode::Tab if app.run_sidebar_shows_registers() => {
            app.run.show_float_regs = !app.run.show_float_regs;
            true
        }
        KeyCode::Char('t') => {
            app.run.show_trace = !app.run.show_trace;
            true
        }
        KeyCode::Char('e') => {
            app.run.show_exec_count = !app.run.show_exec_count;
            true
        }
        KeyCode::Char('y') => {
            app.run.show_instr_type = !app.run.show_instr_type;
            true
        }
        KeyCode::Char('k') => {
            cycle_memory_region(app);
            true
        }
        KeyCode::Char('P') if app.run_sidebar_shows_registers() && !app.run.show_float_regs => {
            let idx = app.run.reg_cursor;
            if idx >= 1 {
                let reg = (idx - 1) as u8;
                if let Some(pos) = app.run.pinned_regs.iter().position(|&r| r == reg) {
                    app.run.pinned_regs.remove(pos);
                } else {
                    app.run.pinned_regs.push(reg);
                }
            }
            true
        }
        KeyCode::Char('f') => {
            app.run.speed = app.run.speed.cycle();
            true
        }
        KeyCode::Up if ctrl => {
            let visible = app.run.console_height.saturating_sub(3) as usize;
            let max_scroll = app.console.lines.len().saturating_sub(visible);
            if app.console.scroll > max_scroll {
                app.console.scroll = max_scroll;
            }
            app.console.scroll = (app.console.scroll + 1).min(max_scroll);
            true
        }
        KeyCode::Down if ctrl => {
            let visible = app.run.console_height.saturating_sub(3) as usize;
            let max_scroll = app.console.lines.len().saturating_sub(visible);
            if app.console.scroll > max_scroll {
                app.console.scroll = max_scroll;
            }
            app.console.scroll = app.console.scroll.saturating_sub(1);
            true
        }
        KeyCode::Up if app.run_sidebar_shows_registers() => {
            let max_scroll = max_regs_scroll(app);
            app.run.regs_scroll = app.run.regs_scroll.saturating_sub(1);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.reg_cursor = app.run.reg_cursor.saturating_sub(1);
            true
        }
        KeyCode::Down if app.run_sidebar_shows_registers() => {
            let max_scroll = max_regs_scroll(app);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
            app.run.reg_cursor = (app.run.reg_cursor + 1).min(32);
            true
        }
        KeyCode::PageUp if app.run_sidebar_shows_registers() => {
            let max_scroll = max_regs_scroll(app);
            app.run.regs_scroll = app.run.regs_scroll.saturating_sub(10);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.reg_cursor = app.run.reg_cursor.saturating_sub(10);
            true
        }
        KeyCode::PageDown if app.run_sidebar_shows_registers() => {
            let max_scroll = max_regs_scroll(app);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.regs_scroll = (app.run.regs_scroll + 10).min(max_scroll);
            app.run.reg_cursor = (app.run.reg_cursor + 10).min(32);
            true
        }
        KeyCode::Up if app.run_sidebar_shows_memory() => {
            app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(app.run.mem_view_bytes);
            app.run.mem_region = MemRegion::Custom;
            true
        }
        KeyCode::Down if app.run_sidebar_shows_memory() => {
            let max = app
                .run
                .mem_size
                .saturating_sub(app.run.mem_view_bytes as usize) as u32;
            if app.run.mem_view_addr < max {
                app.run.mem_view_addr = app
                    .run
                    .mem_view_addr
                    .saturating_add(app.run.mem_view_bytes)
                    .min(max);
            }
            app.run.mem_region = MemRegion::Custom;
            true
        }
        KeyCode::PageUp if app.run_sidebar_shows_memory() => {
            let delta = app.run.mem_view_bytes * 16;
            app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(delta);
            app.run.mem_region = MemRegion::Custom;
            true
        }
        KeyCode::PageDown if app.run_sidebar_shows_memory() => {
            let delta = app.run.mem_view_bytes * 16;
            let max = app
                .run
                .mem_size
                .saturating_sub(app.run.mem_view_bytes as usize) as u32;
            let new = app.run.mem_view_addr.saturating_add(delta);
            app.run.mem_view_addr = new.min(max);
            app.run.mem_region = MemRegion::Custom;
            true
        }
        _ => false,
    }
}

pub(super) fn cycle_memory_region(app: &mut App) {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => {
            app.run.mem_region = MemRegion::Stack;
            let sp = app.run.cpu().x[2];
            app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
        }
        MemRegion::Stack => {
            app.run.mem_region = MemRegion::Access;
        }
        MemRegion::Access => {
            app.run.mem_region = MemRegion::Heap;
            let hb = app.run.cpu().heap_break;
            app.run.mem_view_addr = hb & !(app.run.mem_view_bytes - 1);
        }
        MemRegion::Heap => {
            app.run.mem_region = MemRegion::Data;
            app.run.mem_view_addr = app.run.data_base;
        }
    }
    app.run.show_registers = false;
    app.run.show_dyn = false;
    app.sync_mem_focus_for_active_sidebar_mode();
}
