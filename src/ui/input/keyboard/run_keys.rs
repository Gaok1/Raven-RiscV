use crate::ui::app::{App, MemRegion};
use crossterm::event::{KeyCode, KeyEvent};

use crate::ui::input::max_regs_scroll;

pub(super) fn handle_execution_key(app: &mut App, code: KeyCode) -> bool {
    if !matches!(app.tab, crate::ui::app::Tab::Run) {
        return false;
    }

    match code {
        KeyCode::Char('s') => {
            if !app.run.faulted {
                app.single_step();
            }
            true
        }
        KeyCode::Char('r') => {
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
        KeyCode::Char('p') => {
            if app.run.is_running {
                app.run.is_running = false;
            }
            true
        }
        _ => false,
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
            true
        }
        KeyCode::Tab if app.run.show_registers && !app.run.show_dyn => {
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
        KeyCode::Char('P') if app.run.show_registers => {
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
        KeyCode::Up if app.run.show_registers => {
            let max_scroll = max_regs_scroll(app);
            app.run.regs_scroll = app.run.regs_scroll.saturating_sub(1);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.reg_cursor = app.run.reg_cursor.saturating_sub(1);
            true
        }
        KeyCode::Down if app.run.show_registers => {
            let max_scroll = max_regs_scroll(app);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
            app.run.reg_cursor = (app.run.reg_cursor + 1).min(32);
            true
        }
        KeyCode::PageUp if app.run.show_registers => {
            let max_scroll = max_regs_scroll(app);
            app.run.regs_scroll = app.run.regs_scroll.saturating_sub(10);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.reg_cursor = app.run.reg_cursor.saturating_sub(10);
            true
        }
        KeyCode::PageDown if app.run.show_registers => {
            let max_scroll = max_regs_scroll(app);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            app.run.regs_scroll = (app.run.regs_scroll + 10).min(max_scroll);
            app.run.reg_cursor = (app.run.reg_cursor + 10).min(32);
            true
        }
        KeyCode::Up if !app.run.show_registers => {
            app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(app.run.mem_view_bytes);
            app.run.mem_region = MemRegion::Custom;
            true
        }
        KeyCode::Down if !app.run.show_registers => {
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
        KeyCode::PageUp if !app.run.show_registers => {
            let delta = app.run.mem_view_bytes * 16;
            app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(delta);
            app.run.mem_region = MemRegion::Custom;
            true
        }
        KeyCode::PageDown if !app.run.show_registers => {
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
    app.sync_mem_focus_for_active_sidebar_mode();
}
