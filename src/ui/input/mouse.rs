use crate::ui::{
    app::{App, EditorMode, FormatMode, MemRegion, RunButton, Tab, Lang},
    editor::Editor,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::max_regs_scroll;

pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

    if app.show_exit_popup {
        handle_exit_popup_mouse(app, me, area);
        return;
    }

    // Hover tabs
    app.hover_tab = None;
    app.hover_run_button = None;
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
                    app.mode = EditorMode::Command;
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
            Tab::Run => handle_run_scroll(app, me, area, true),
            Tab::Docs => app.docs_scroll = app.docs_scroll.saturating_sub(1),
        },
        MouseEventKind::ScrollDown => match app.tab {
            Tab::Editor => app.editor.move_down(),
            Tab::Run => handle_run_scroll(app, me, area, false),
            Tab::Docs => app.docs_scroll += 1,
        },
        _ => {}
    }

    // Docs: detect click on top-right language button
    if let Tab::Docs = app.tab {
        let root_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(area);
        let docs_area = root_chunks[1];
        // header is 1 row at top of docs_area
        let header_y = docs_area.y;
        let btn_width = 4u16; // "[EN]" or "[PT]"
        let btn_x_start = docs_area.x + docs_area.width.saturating_sub(btn_width);
        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left))
            && me.row == header_y
            && me.column >= btn_x_start
            && me.column < btn_x_start + btn_width
        {
            app.lang = match app.lang { Lang::EN => Lang::PT, Lang::PT => Lang::EN };
        }
    }

    if let Tab::Editor = app.tab {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(area);
        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(3)])
            .split(chunks[1]);
        let editor_area = editor_chunks[1];

        let start = {
            let visible_h = editor_area.height.saturating_sub(2) as usize;
            let len = app.editor.lines.len();
            let mut s = 0usize;
            if len > visible_h {
                if app.editor.cursor_row <= visible_h / 2 {
                    s = 0;
                } else if app.editor.cursor_row >= len.saturating_sub(visible_h / 2) {
                    s = len.saturating_sub(visible_h);
                } else {
                    s = app.editor.cursor_row - visible_h / 2;
                }
            }
            s
        };

        let visible_h = editor_area.height.saturating_sub(2) as usize;
        let len = app.editor.lines.len();
        let end = (start + visible_h).min(len);
        let num_width = end.to_string().len() as u16;
        let gutter = num_width + 3;

        let within = |x: u16, y: u16| {
            x >= editor_area.x + 1
                && x < editor_area.x + editor_area.width - 1
                && y >= editor_area.y + 1
                && y < editor_area.y + editor_area.height - 1
        };

        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if within(me.column, me.row) {
                    let y = (me.row - (editor_area.y + 1)) as usize;
                    let row = (start + y).min(app.editor.lines.len().saturating_sub(1));
                    let x = me.column.saturating_sub(editor_area.x + 1 + gutter) as usize;
                    let col = x.min(Editor::char_count(&app.editor.lines[row]));
                    app.editor.cursor_row = row;
                    app.editor.cursor_col = col;
                    app.editor.selection_anchor = Some((row, col));
                    if app.mode == EditorMode::Command {
                        app.mode = EditorMode::Insert;
                    }
                } else if app.mode == EditorMode::Insert {
                    app.mode = EditorMode::Command;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if within(me.column, me.row) {
                    let y = (me.row - (editor_area.y + 1)) as usize;
                    let row = (start + y).min(app.editor.lines.len().saturating_sub(1));
                    let x = me.column.saturating_sub(editor_area.x + 1 + gutter) as usize;
                    let col = x.min(Editor::char_count(&app.editor.lines[row]));
                    app.editor.cursor_row = row;
                    app.editor.cursor_col = col;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some((r, c)) = app.editor.selection_anchor {
                    if r == app.editor.cursor_row && c == app.editor.cursor_col {
                        app.editor.clear_selection();
                    }
                }
            }
            _ => {}
        }
    }

    // Run tab interactions
    if let Tab::Run = app.tab {
        update_run_status_hover(app, me, area);
        update_imem_hover(app, me, area);
        update_console_hover(app, me, area);
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                handle_run_status_click(app, me, area);
                start_imem_drag(app, me, area);
                handle_console_clear(app, me, area);
                start_console_drag(app, me, area);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if app.imem_drag {
                    handle_imem_drag(app, me, area);
                }
                if app.console_drag {
                    handle_console_drag(app, me, area);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                app.imem_drag = false;
                app.console_drag = false;
            }
            _ => {}
        }
    }
}

fn handle_run_status_click(app: &mut App, me: MouseEvent, area: Rect) {
    let status = run_status_area(app, area);
    if me.row != status.y + 1 {
        return;
    }
    if let Some(btn) = run_status_hit(app, status, me.column) {
        match btn {
            RunButton::View => {
                app.show_registers = !app.show_registers;
            }
            RunButton::Format => {
                app.fmt_mode = match app.fmt_mode {
                    FormatMode::Hex => FormatMode::Dec,
                    FormatMode::Dec => FormatMode::Str,
                    FormatMode::Str => FormatMode::Hex,
                };
            }
            RunButton::Sign => {
                // Disable sign toggle unless in decimal format
                if matches!(app.fmt_mode, FormatMode::Dec) {
                    app.show_signed = !app.show_signed;
                }
            }
            RunButton::Bytes => {
                // Cycle byte width 4 -> 2 -> 1 -> 4
                let next = match app.mem_view_bytes {
                    4 => 2,
                    2 => 1,
                    _ => 4,
                };
                app.mem_view_bytes = next;
                // Align base address to the new byte width so regrouping
                // always starts at proper boundaries (prevents mis-grouping
                // after scrolling in 1-byte mode and switching back).
                if next > 1 {
                    let mask = !(next as u32 - 1);
                    app.mem_view_addr &= mask;
                }
            }
            RunButton::Region => {
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
            }
            RunButton::State => {
                if app.is_running {
                    app.is_running = false;
                } else if !app.faulted {
                    app.is_running = true;
                }
            }
        }
    }
}

fn update_run_status_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let status = run_status_area(app, area);
    if me.row != status.y + 1 {
        return;
    }
    app.hover_run_button = run_status_hit(app, status, me.column);
}

fn run_status_area(app: &App, area: Rect) -> Rect {
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
            Constraint::Length(app.console_height),
        ])
        .split(root_chunks[1]);
    run_chunks[1]
}

fn run_status_hit(app: &App, status: Rect, col: u16) -> Option<RunButton> {
    let view_text = if app.show_registers { "REGS" } else { "RAM" };
    let fmt_text = match app.fmt_mode {
        FormatMode::Hex => "HEX",
        FormatMode::Dec => "DEC",
        FormatMode::Str => "STR",
    };
    let sign_text = if app.show_signed { "SGN" } else { "UNS" };
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
        *start += 1 + label.len() as u16 + 1;
        (s, *start)
    };
    let skip = |start: &mut u16, s: &str| {
        *start += s.len() as u16;
    };

    skip(&mut pos, "View ");
    let (view_start, view_end) = range(&mut pos, view_text);

    let (region_start, region_end) = if !app.show_registers {
        skip(&mut pos, "  Region ");
        range(&mut pos, region_text)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  Format ");
    let (fmt_start, fmt_end) = range(&mut pos, fmt_text);

    skip(&mut pos, "  Sign ");
    let (sign_start, sign_end) = range(&mut pos, sign_text);

    let (bytes_start, bytes_end) = if !app.show_registers {
        skip(&mut pos, "  Bytes ");
        range(&mut pos, bytes_text)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  State ");
    let (state_start, state_end) = range(&mut pos, run_text);

    if col >= view_start && col < view_end {
        Some(RunButton::View)
    } else if !app.show_registers && col >= region_start && col < region_end {
        Some(RunButton::Region)
    } else if col >= fmt_start && col < fmt_end {
        Some(RunButton::Format)
    } else if col >= sign_start && col < sign_end {
        // Only allow clicking Sign when in decimal format
        if matches!(app.fmt_mode, FormatMode::Dec) {
            Some(RunButton::Sign)
        } else {
            None
        }
    } else if !app.show_registers && col >= bytes_start && col < bytes_end {
        Some(RunButton::Bytes)
    } else if col >= state_start && col < state_end {
        Some(RunButton::State)
    } else {
        None
    }
}

fn run_cols(app: &App, area: Rect) -> Vec<Rect> {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let main = run_chunks[2];
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(38),
            Constraint::Length(app.imem_width),
            Constraint::Min(46),
        ])
        .split(main)
        .to_vec()
}

fn update_imem_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);
    let imem = cols[1];
    let bar_x = imem.x + imem.width - 1;
    if me.column == bar_x && me.row >= imem.y && me.row < imem.y + imem.height {
        app.hover_imem_bar = true;
    } else if !app.imem_drag {
        app.hover_imem_bar = false;
    }
}

fn start_imem_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);
    let imem = cols[1];
    let bar_x = imem.x + imem.width - 1;
    if me.column == bar_x && me.row >= imem.y && me.row < imem.y + imem.height {
        app.imem_drag = true;
        app.imem_drag_start_x = me.column;
        app.imem_width_start = app.imem_width;
    }
}

fn handle_imem_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let delta = me.column as i32 - app.imem_drag_start_x as i32;
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let main = run_chunks[2];
    let available = main.width.saturating_sub(38 + 46);
    let max = if available < 20 { 20 } else { available } as i32;
    let mut new_width = app.imem_width_start as i32 + delta;
    if new_width < 20 {
        new_width = 20;
    }
    if new_width > max {
        new_width = max;
    }
    app.imem_width = new_width as u16;
}

fn update_console_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let console = run_chunks[3];
    let bar_y = console.y;
    let clear_start = console.x + console.width.saturating_sub(6);
    let clear_end = clear_start + 5;
    if me.row == bar_y {
        if me.column >= clear_start && me.column < clear_end {
            app.hover_console_clear = true;
            app.hover_console_bar = false;
        } else if me.column >= console.x && me.column < console.x + console.width {
            app.hover_console_bar = true;
            app.hover_console_clear = false;
        } else if !app.console_drag {
            app.hover_console_bar = false;
            app.hover_console_clear = false;
        }
    } else if !app.console_drag {
        app.hover_console_bar = false;
        app.hover_console_clear = false;
    }
}

fn start_console_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let console = run_chunks[3];
    let bar_y = console.y;
    let clear_start = console.x + console.width.saturating_sub(6);
    let clear_end = clear_start + 5;
    if me.row == bar_y
        && me.column >= console.x
        && me.column < console.x + console.width
        && !(me.column >= clear_start && me.column < clear_end)
    {
        app.console_drag = true;
        app.console_drag_start_y = me.row;
        app.console_height_start = app.console_height;
    }
}

fn handle_console_clear(app: &mut App, me: MouseEvent, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let console = run_chunks[3];
    let bar_y = console.y;
    let clear_start = console.x + console.width.saturating_sub(6);
    let clear_end = clear_start + 5;
    if me.row == bar_y && me.column >= clear_start && me.column < clear_end {
        app.console.clear();
    }
}

fn handle_console_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let delta = app.console_drag_start_y as i32 - me.row as i32;
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let max = run_area.height.saturating_sub(3 + 4);
    let mut new_h = app.console_height_start as i32 + delta;
    if new_h < 1 {
        new_h = 1;
    }
    if new_h as u16 > max {
        new_h = max as i32;
    }
    app.console_height = new_h as u16;
}

fn handle_run_scroll(app: &mut App, me: MouseEvent, area: Rect, up: bool) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let run_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(run_area);
    let main = run_chunks[2];
    let console = run_chunks[3];

    if me.row >= console.y && me.row < console.y + console.height {
        let total = app.console.lines.len();
        let visible = app.console_height.saturating_sub(3) as usize;
        let max_scroll = total.saturating_sub(visible);
        if app.console.scroll > max_scroll {
            app.console.scroll = max_scroll;
        }
        if up {
            app.console.scroll = (app.console.scroll + 1).min(max_scroll);
        } else {
            app.console.scroll = app.console.scroll.saturating_sub(1);
        }
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(38),
            Constraint::Length(app.imem_width),
            Constraint::Min(46),
        ])
        .split(main);
    let side = cols[0];
    if me.column >= side.x
        && me.column < side.x + side.width
        && me.row >= side.y
        && me.row < side.y + side.height
    {
        if app.show_registers {
            let max_scroll = max_regs_scroll(app);
            if app.regs_scroll > max_scroll {
                app.regs_scroll = max_scroll;
            }
            if up {
                app.regs_scroll = app.regs_scroll.saturating_sub(1);
            } else {
                app.regs_scroll = (app.regs_scroll + 1).min(max_scroll);
            }
        } else {
            if up {
                app.mem_view_addr = app.mem_view_addr.saturating_sub(app.mem_view_bytes);
            } else {
                let max = app.mem_size.saturating_sub(app.mem_view_bytes as usize) as u32;
                if app.mem_view_addr < max {
                    app.mem_view_addr = app
                        .mem_view_addr
                        .saturating_add(app.mem_view_bytes)
                        .min(max);
                }
            }
            app.mem_region = MemRegion::Custom;
        }
    }
}

fn handle_exit_popup_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    let popup = centered_rect(area.width / 3, area.height / 4, area);
    if me.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    if me.column < popup.x + 1
        || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1
        || me.row >= popup.y + popup.height - 1
    {
        app.show_exit_popup = false;
        return;
    }
    let inner_x = me.column - (popup.x + 1);
    let inner_y = me.row - (popup.y + 1);
    const EXIT: &str = "[Exit]";
    const CANCEL: &str = "[Cancel]";
    const GAP: u16 = 3;
    if inner_y == 3 {
        let line_width = EXIT.len() as u16 + GAP + CANCEL.len() as u16;
        let start = ((popup.width - 2).saturating_sub(line_width)) / 2;
        if inner_x >= start && inner_x < start + EXIT.len() as u16 {
            app.should_quit = true;
        } else if inner_x >= start + EXIT.len() as u16 + GAP
            && inner_x < start + EXIT.len() as u16 + GAP + CANCEL.len() as u16
        {
            app.show_exit_popup = false;
        }
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
