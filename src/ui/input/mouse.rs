use crate::ui::app::{App, MemRegion, RunHover, Tab};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

    // Hover tabs
    app.hover_tab = None;
    if me.row == area.y + 1 {
        let x = me.column.saturating_sub(area.x + 1);
        let titles = [
            ("Editor", Tab::Editor),
            ("Run", Tab::Run),
            ("Docs", Tab::Docs),
        ];
        let divider = " │ ".len() as u16;
        let mut pos: u16 = 0;
        for (i, (title, tab)) in titles.iter().enumerate() {
            let w = title.len() as u16;
            if x >= pos && x < pos + w {
                app.hover_tab = Some(*tab);
                if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                    app.tab = *tab;
                }
                break;
            }
            pos += w;
            if i + 1 < titles.len() {
                pos += divider;
            }
        }
    }

    // Scrolls
    match me.kind {
        MouseEventKind::ScrollUp => match app.tab {
            Tab::Editor => app.editor.move_up(),
            Tab::Run => {
                if app.show_registers {
                    app.regs_scroll = app.regs_scroll.saturating_sub(1);
                } else {
                    app.mem_view_addr = app.mem_view_addr.saturating_sub(app.mem_view_bytes);
                    app.mem_region = MemRegion::Custom;
                }
            }
            Tab::Docs => app.docs_scroll = app.docs_scroll.saturating_sub(1),
        },
        MouseEventKind::ScrollDown => match app.tab {
            Tab::Editor => app.editor.move_down(),
            Tab::Run => {
                if app.show_registers {
                    app.regs_scroll = app.regs_scroll.saturating_add(1);
                } else {
                    let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                    if app.mem_view_addr < max {
                        app.mem_view_addr = app
                            .mem_view_addr
                            .saturating_add(app.mem_view_bytes)
                            .min(max);
                    }
                    app.mem_region = MemRegion::Custom;
                }
            }
            Tab::Docs => app.docs_scroll += 1,
        },
        _ => {}
    }

    // Run tab interactions
    if let Tab::Run = app.tab {
        if matches!(me.kind, MouseEventKind::Moved) {
            app.hover_run = if app.show_menu {
                compute_run_menu_hover(app, me, area)
            } else {
                compute_run_status_hover(app, me, area)
            };
        }

        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            if app.show_menu {
                handle_run_menu_click(app, me, area);
            } else {
                handle_run_status_click(app, me, area);
            }
        }
    }
}

fn compute_run_status_hover(app: &App, me: MouseEvent, area: Rect) -> RunHover {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(root[1]);
    let status = run[1];
    if me.row != status.y + 1 {
        return RunHover::None;
    }

    let view = if app.show_registers { "REGS" } else { "RAM" };
    let fmt = if app.show_hex { "HEX" } else { "DEC" };
    let bytes = match app.mem_view_bytes {
        4 => "4B",
        2 => "2B",
        _ => "1B",
    };
    let region = match app.mem_region {
        MemRegion::Data => "DATA",
        MemRegion::Stack => "STACK",
        MemRegion::Custom => "ADDR",
    };
    let state = if app.is_running { "RUN" } else { "PAUSE" };

    let mut pos = status.x + 1;
    let col = me.column;

    let range = |start: &mut u16, label: &str| {
        let s = *start;
        *start += label.len() as u16;
        (s, *start)
    };
    let skip = |start: &mut u16, s: &str| {
        *start += s.len() as u16;
    };

    skip(&mut pos, "View: ");
    let (v0, v1) = range(&mut pos, view);

    skip(&mut pos, "  Format: ");
    let (f0, f1) = range(&mut pos, fmt);

    let (b0, b1) = if !app.show_registers {
        skip(&mut pos, "  Bytes: ");
        range(&mut pos, bytes)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  Region: ");
    let (r0, r1) = range(&mut pos, region);

    skip(&mut pos, "  State: ");
    let (s0, s1) = range(&mut pos, state);

    if col >= v0 && col < v1 {
        RunHover::View
    } else if col >= f0 && col < f1 {
        RunHover::Format
    } else if !app.show_registers && col >= b0 && col < b1 {
        RunHover::Bytes
    } else if col >= r0 && col < r1 {
        RunHover::Region
    } else if col >= s0 && col < s1 {
        RunHover::State
    } else {
        RunHover::None
    }
}

fn compute_run_menu_hover(app: &App, me: MouseEvent, area: Rect) -> RunHover {
    let popup = centered_rect(area.width / 2, area.height / 2, area);
    if me.column < popup.x + 1
        || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1
        || me.row >= popup.y + popup.height - 1
    {
        return RunHover::None;
    }
    let x = me.column - (popup.x + 1);
    let y = me.row - (popup.y + 1);

    match y {
        2 => {
            let step = "[s] Step";
            let run = "[r] Run";
            let pause = "[p] Pause";
            let mut pos: u16 = 0;
            if x < step.len() as u16 {
                return RunHover::MStep;
            }
            pos += step.len() as u16 + 2;
            if x >= pos && x < pos + run.len() as u16 {
                return RunHover::MRun;
            }
            pos += run.len() as u16 + 2;
            if x >= pos && x < pos + pause.len() as u16 {
                return RunHover::MPause;
            }
            RunHover::None
        }
        3 => {
            let d = "[d] View data";
            let k = "[k] View stack";
            let mut pos: u16 = 0;
            if x < d.len() as u16 {
                return RunHover::MViewData;
            }
            pos += d.len() as u16 + 2;
            if x >= pos && x < pos + k.len() as u16 {
                return RunHover::MViewStack;
            }
            RunHover::None
        }
        4 => RunHover::MToggleView,
        5 => RunHover::MToggleFormat,
        6 => {
            if app.show_registers {
                RunHover::None
            } else {
                RunHover::MCycleBytes
            }
        }
        7 => RunHover::MClose,
        _ => RunHover::None,
    }
}

fn handle_run_status_click(app: &mut App, me: MouseEvent, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(root_chunks[1]);
    let status = run_chunks[1];
    if me.row != status.y + 1 {
        return;
    }

    let view_text = if app.show_registers { "REGS" } else { "RAM" };
    let fmt_text = if app.show_hex { "HEX" } else { "DEC" };
    let bytes_text = match app.mem_view_bytes {
        4 => "4B",
        2 => "2B",
        _ => "1B",
    };
    let region_text = match app.mem_region {
        MemRegion::Data => "DATA",
        MemRegion::Stack => "STACK",
        MemRegion::Custom => "ADDR",
    };
    let run_text = if app.is_running { "RUN" } else { "PAUSE" };

    let mut pos = status.x + 1;

    let range = |start: &mut u16, label: &str| {
        let s = *start;
        *start += label.len() as u16;
        (s, *start)
    };
    let skip = |start: &mut u16, s: &str| {
        *start += s.len() as u16;
    };

    skip(&mut pos, "View: ");
    let (view_start, view_end) = range(&mut pos, view_text);

    skip(&mut pos, "  Format: ");
    let (fmt_start, fmt_end) = range(&mut pos, fmt_text);

    let (bytes_start, bytes_end) = if !app.show_registers {
        skip(&mut pos, "  Bytes: ");
        range(&mut pos, bytes_text)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  Region: ");
    let region_start = pos;
    pos += region_text.len() as u16;
    let region_end = pos;

    skip(&mut pos, "  State: ");
    let state_start = pos;
    pos += run_text.len() as u16;
    let state_end = pos;

    let col = me.column;
    if col >= view_start && col < view_end {
        app.show_registers = !app.show_registers;
    } else if col >= fmt_start && col < fmt_end {
        app.show_hex = !app.show_hex;
    } else if !app.show_registers && col >= bytes_start && col < bytes_end {
        app.mem_view_bytes = match app.mem_view_bytes {
            4 => 2,
            2 => 1,
            _ => 4,
        };
    } else if !app.show_registers && col >= region_start && col < region_end {
        app.mem_region = match app.mem_region {
            MemRegion::Data => {
                app.mem_view_addr = app.cpu.x[2];
                MemRegion::Stack
            }
            MemRegion::Stack => MemRegion::Custom,
            MemRegion::Custom => {
                app.mem_view_addr = app.data_base;
                MemRegion::Data
            }
        };
    } else if col >= state_start && col < state_end {
        if app.is_running {
            app.is_running = false;
        } else if !app.faulted {
            app.is_running = true;
        }
    }
}

fn handle_run_menu_click(app: &mut App, me: MouseEvent, area: Rect) {
    let popup = centered_rect(area.width / 2, area.height / 2, area);
    if me.column < popup.x + 1
        || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1
        || me.row >= popup.y + popup.height - 1
    {
        app.show_menu = false;
        return;
    }
    let inner_x = me.column - (popup.x + 1);
    let inner_y = me.row - (popup.y + 1);
    match inner_y {
        2 => {
            const STEP: &str = "[s] Step";
            const RUN: &str = "[r] Run";
            const PAUSE: &str = "[p] Pause";
            let mut x = 0u16;
            if inner_x < STEP.len() as u16 {
                app.single_step();
            } else {
                x += STEP.len() as u16 + 2;
                if inner_x >= x && inner_x < x + RUN.len() as u16 {
                    if !app.faulted {
                        app.is_running = true;
                    }
                } else {
                    x += RUN.len() as u16 + 2;
                    if inner_x >= x && inner_x < x + PAUSE.len() as u16 {
                        app.is_running = false;
                    }
                }
            }
        }
        3 => {
            const DATA: &str = "[d] View data";
            const STACK: &str = "[k] View stack";
            let mut x = 0u16;
            if inner_x < DATA.len() as u16 {
                app.mem_view_addr = app.data_base;
                app.mem_region = MemRegion::Data;
                app.show_registers = false;
            } else {
                x += DATA.len() as u16 + 2;
                if inner_x >= x && inner_x < x + STACK.len() as u16 {
                    app.mem_view_addr = app.cpu.x[2];
                    app.mem_region = MemRegion::Stack;
                    app.show_registers = false;
                }
            }
        }
        4 => {
            app.show_registers = !app.show_registers;
        }
        5 => {
            app.show_hex = !app.show_hex;
        }
        6 => {
            if !app.show_registers {
                app.mem_view_bytes = match app.mem_view_bytes {
                    4 => 2,
                    2 => 1,
                    _ => 4,
                };
            }
        }
        7 => {
            app.show_menu = false;
        }
        _ => {}
    }
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Rect::new(
        r.x + (r.width.saturating_sub(width)) / 2,
        r.y + (r.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}
