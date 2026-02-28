use crate::ui::{
    app::{App, CacheScope, CacheSubtab, ConfigField, EditorMode, FormatMode, MemRegion, RunButton, RunSpeed, Tab},
    editor::Editor,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use rfd::FileDialog as OSFileDialog;

use super::max_regs_scroll;
use crate::ui::view::docs::docs_body_line_count;

pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

    if app.show_exit_popup {
        handle_exit_popup_mouse(app, me, area);
        return;
    }

    // Hover tabs — derived from Tab::all() so new tabs are automatically supported
    app.hover_tab = None;
    app.hover_run_button = None;
    if me.row == area.y + 1 {
        let x = me.column.saturating_sub(area.x + 1);
        let divider = 3u16; // " │ "
        let pad_left = 1u16;
        let pad_right = 1u16;
        let mut pos: u16 = 0;
        for (i, &tab) in Tab::all().iter().enumerate() {
            let label = tab.label();
            let w = pad_left + label.len() as u16 + pad_right;
            if x >= pos && x < pos + w {
                app.hover_tab = Some(tab);
                if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                    app.tab = tab;
                    app.mode = EditorMode::Command;
                }
                break;
            }
            pos += w;
            if i + 1 < Tab::all().len() {
                pos += divider;
            }
        }
    }

    // Scrolls
    match me.kind {
        MouseEventKind::ScrollUp => match app.tab {
            Tab::Editor => app.editor.buf.move_up(),
            Tab::Run => handle_run_scroll(app, me, area, true),
            Tab::Cache => match app.cache.subtab {
                CacheSubtab::Stats => {
                    app.cache.stats_scroll = app.cache.stats_scroll.saturating_sub(1);
                }
                CacheSubtab::View => {
                    app.cache.view_scroll = app.cache.view_scroll.saturating_sub(1);
                }
                _ => {}
            },
            Tab::Docs => {
                app.docs.scroll = app.docs.scroll.saturating_sub(1);
                clamp_docs_scroll(app, area);
            }
        },
        MouseEventKind::ScrollDown => match app.tab {
            Tab::Editor => app.editor.buf.move_down(),
            Tab::Run => handle_run_scroll(app, me, area, false),
            Tab::Cache => match app.cache.subtab {
                CacheSubtab::Stats => {
                    app.cache.stats_scroll = app.cache.stats_scroll.saturating_add(1);
                }
                CacheSubtab::View => {
                    app.cache.view_scroll = app.cache.view_scroll.saturating_add(1);
                }
                _ => {}
            },
            Tab::Docs => {
                app.docs.scroll = app.docs.scroll.saturating_add(1);
                clamp_docs_scroll(app, area);
            }
        },
        MouseEventKind::ScrollLeft => {
            if matches!(app.tab, Tab::Cache)
                && matches!(app.cache.subtab, CacheSubtab::View)
            {
                app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
            }
        }
        MouseEventKind::ScrollRight => {
            if matches!(app.tab, Tab::Cache)
                && matches!(app.cache.subtab, CacheSubtab::View)
            {
                app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
            }
        }
        _ => {}
    }

    // Cache tab interactions
    if let Tab::Cache = app.tab {
        update_cache_hover(app, me, area);
        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            handle_cache_click(app, me, area);
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
        let status_area = editor_chunks[0];
        let editor_area = editor_chunks[1];

        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            handle_editor_status_click(app, me, status_area);
        }

        let start = {
            let visible_h = editor_area.height.saturating_sub(2) as usize;
            let len = app.editor.buf.lines.len();
            let mut s = 0usize;
            if len > visible_h {
                if app.editor.buf.cursor_row <= visible_h / 2 {
                    s = 0;
                } else if app.editor.buf.cursor_row >= len.saturating_sub(visible_h / 2) {
                    s = len.saturating_sub(visible_h);
                } else {
                    s = app.editor.buf.cursor_row - visible_h / 2;
                }
            }
            s
        };

        let visible_h = editor_area.height.saturating_sub(2) as usize;
        let len = app.editor.buf.lines.len();
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
                    let row = (start + y).min(app.editor.buf.lines.len().saturating_sub(1));
                    let x = me.column.saturating_sub(editor_area.x + 1 + gutter) as usize;
                    let col = x.min(Editor::char_count(&app.editor.buf.lines[row]));
                    app.editor.buf.cursor_row = row;
                    app.editor.buf.cursor_col = col;
                    app.editor.buf.selection_anchor = Some((row, col));
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
                    let row = (start + y).min(app.editor.buf.lines.len().saturating_sub(1));
                    let x = me.column.saturating_sub(editor_area.x + 1 + gutter) as usize;
                    let col = x.min(Editor::char_count(&app.editor.buf.lines[row]));
                    app.editor.buf.cursor_row = row;
                    app.editor.buf.cursor_col = col;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some((r, c)) = app.editor.buf.selection_anchor {
                    if r == app.editor.buf.cursor_row && c == app.editor.buf.cursor_col {
                        app.editor.buf.clear_selection();
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
                handle_panel_title_click(app, me, area);
                start_sidebar_drag(app, me, area);
                start_imem_drag(app, me, area);
                handle_imem_click(app, me, area);
                handle_console_clear(app, me, area);
                start_console_drag(app, me, area);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if app.run.sidebar_drag {
                    handle_sidebar_drag(app, me, area);
                }
                if app.run.imem_drag {
                    handle_imem_drag(app, me, area);
                }
                if app.run.console_drag {
                    handle_console_drag(app, me, area);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                app.run.sidebar_drag = false;
                app.run.imem_drag = false;
                app.run.console_drag = false;
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
                app.run.show_registers = !app.run.show_registers;
            }
            RunButton::Format => {
                app.run.fmt_mode = match app.run.fmt_mode {
                    FormatMode::Hex => FormatMode::Dec,
                    FormatMode::Dec => FormatMode::Str,
                    FormatMode::Str => FormatMode::Hex,
                };
            }
            RunButton::Sign => {
                if matches!(app.run.fmt_mode, FormatMode::Dec) {
                    app.run.show_signed = !app.run.show_signed;
                }
            }
            RunButton::Bytes => {
                let next = match app.run.mem_view_bytes {
                    4 => 2,
                    2 => 1,
                    _ => 4,
                };
                app.run.mem_view_bytes = next;
                if next > 1 {
                    let mask = !(next as u32 - 1);
                    app.run.mem_view_addr &= mask;
                }
            }
            RunButton::Region => {
                app.run.mem_region = match app.run.mem_region {
                    MemRegion::Data => {
                        app.run.mem_view_addr = app.run.cpu.x[2];
                        MemRegion::Stack
                    }
                    _ => {
                        app.run.mem_view_addr = app.run.data_base;
                        MemRegion::Data
                    }
                };
            }
            RunButton::Speed => {
                // Locked while running in Instant mode
                if !(matches!(app.run.speed, RunSpeed::Instant) && app.run.is_running) {
                    app.run.speed = app.run.speed.cycle();
                }
            }
            RunButton::State => {
                // Pause/resume blocked while running in Instant mode
                if matches!(app.run.speed, RunSpeed::Instant) && app.run.is_running {
                    return;
                }
                if app.run.is_running {
                    app.run.is_running = false;
                } else if !app.run.faulted {
                    app.run.is_running = true;
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(root_chunks[1]);
    run_chunks[1]
}

fn run_status_hit(app: &App, status: Rect, col: u16) -> Option<RunButton> {
    let view_text = if app.run.show_registers { "REGS" } else { "RAM" };
    let fmt_text = match app.run.fmt_mode {
        FormatMode::Hex => "HEX",
        FormatMode::Dec => "DEC",
        FormatMode::Str => "STR",
    };
    let sign_text = if app.run.show_signed { "SGN" } else { "UNS" };
    let bytes_text = match app.run.mem_view_bytes {
        4 => "4B",
        2 => "2B",
        _ => "1B",
    };
    let region_text = match app.run.mem_region {
        MemRegion::Data => "DATA",
        MemRegion::Stack => "STACK",
        MemRegion::Custom => "DATA",
    };
    let run_text = if app.run.is_running { "RUN" } else { "PAUSE" };

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

    let (region_start, region_end) = if !app.run.show_registers {
        skip(&mut pos, "  Region ");
        range(&mut pos, region_text)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  Format ");
    let (fmt_start, fmt_end) = range(&mut pos, fmt_text);

    skip(&mut pos, "  Sign ");
    let (sign_start, sign_end) = range(&mut pos, sign_text);

    let (bytes_start, bytes_end) = if !app.run.show_registers {
        skip(&mut pos, "  Bytes ");
        range(&mut pos, bytes_text)
    } else {
        (0, 0)
    };

    let speed_text = app.run.speed.label();
    skip(&mut pos, "  Speed ");
    let (speed_start, speed_end) = range(&mut pos, speed_text);

    skip(&mut pos, "  State ");
    let (state_start, state_end) = range(&mut pos, run_text);

    if col >= view_start && col < view_end {
        Some(RunButton::View)
    } else if !app.run.show_registers && col >= region_start && col < region_end {
        Some(RunButton::Region)
    } else if col >= fmt_start && col < fmt_end {
        Some(RunButton::Format)
    } else if col >= sign_start && col < sign_end {
        if matches!(app.run.fmt_mode, FormatMode::Dec) {
            Some(RunButton::Sign)
        } else {
            None
        }
    } else if !app.run.show_registers && col >= bytes_start && col < bytes_end {
        Some(RunButton::Bytes)
    } else if col >= speed_start && col < speed_end {
        Some(RunButton::Speed)
    } else if col >= state_start && col < state_end {
        Some(RunButton::State)
    } else {
        None
    }
}

fn run_main_area(app: &App, area: Rect) -> Rect {
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(run_area);
    run_chunks[2]
}

fn run_cols(app: &App, area: Rect) -> Vec<Rect> {
    let main = run_main_area(app, area);
    let sidebar_w = if app.run.sidebar_collapsed { 3 } else { app.run.sidebar_width };
    let imem_w    = if app.run.imem_collapsed    { 3 } else { app.run.imem_width    };
    let details_min = if app.run.details_collapsed { 3 } else { 40 };
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w),
            Constraint::Length(imem_w),
            Constraint::Min(details_min),
        ])
        .split(main)
        .to_vec()
}

fn update_imem_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);

    // Sidebar resize bar (right edge of col[0]) — full height hit area
    let sidebar = cols[0];
    let sbar_x = sidebar.x + sidebar.width.saturating_sub(1);
    if !app.run.sidebar_collapsed && me.column == sbar_x
        && me.row >= area.y && me.row < area.y + area.height
    {
        app.run.hover_sidebar_bar = true;
    } else if !app.run.sidebar_drag {
        app.run.hover_sidebar_bar = false;
    }

    let imem = cols[1];
    let bar_x = imem.x + imem.width - 1;
    if !app.run.imem_collapsed && me.column == bar_x
        && me.row >= area.y && me.row < area.y + area.height
    {
        app.run.hover_imem_bar = true;
    } else if !app.run.imem_drag {
        app.run.hover_imem_bar = false;
    }

    if app.run.imem_collapsed {
        app.run.hover_imem_addr = None;
        return;
    }

    let inner = Rect::new(
        imem.x + 1,
        imem.y + 1,
        imem.width.saturating_sub(2),
        imem.height.saturating_sub(2),
    );
    if me.column >= inner.x
        && me.column < inner.x + inner.width
        && me.row >= inner.y
        && me.row < inner.y + inner.height
    {
        if let Some(ref text) = app.editor.last_ok_text {
            let rows = inner.height as usize;
            let total = text.len();
            let max_scroll = total.saturating_sub(rows);
            if app.run.imem_scroll > max_scroll {
                app.run.imem_scroll = max_scroll;
            }
            let row = (me.row - inner.y) as usize;
            let idx = app.run.imem_scroll + row;
            if idx < total {
                app.run.hover_imem_addr = Some(app.run.base_pc + (idx as u32) * 4);
            } else {
                app.run.hover_imem_addr = None;
            }
        } else {
            app.run.hover_imem_addr = None;
        }
    } else {
        app.run.hover_imem_addr = None;
    }
}

fn start_sidebar_drag(app: &mut App, me: MouseEvent, area: Rect) {
    if app.run.sidebar_collapsed { return; }
    let cols = run_cols(app, area);
    let sidebar = cols[0];
    let bar_x = sidebar.x + sidebar.width.saturating_sub(1);
    if me.column == bar_x && me.row >= area.y && me.row < area.y + area.height {
        app.run.sidebar_drag = true;
        app.run.sidebar_drag_start_x = me.column;
        app.run.sidebar_width_start = app.run.sidebar_width;
    }
}

fn handle_panel_title_click(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);
    // Click on top border row of a panel toggles collapse
    for (i, col) in cols.iter().enumerate() {
        if me.row == col.y && me.column >= col.x && me.column < col.x + col.width {
            match i {
                0 => app.run.sidebar_collapsed = !app.run.sidebar_collapsed,
                1 => app.run.imem_collapsed    = !app.run.imem_collapsed,
                2 => app.run.details_collapsed = !app.run.details_collapsed,
                _ => {}
            }
            return;
        }
    }
}

fn clamp_docs_scroll(app: &mut App, area: Rect) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let docs_area = root_chunks[1];
    let table_h = docs_area.height.saturating_sub(2);
    let viewport_h = table_h.saturating_sub(4) as usize;
    if viewport_h == 0 {
        app.docs.scroll = 0;
        return;
    }
    let total_body = docs_body_line_count(docs_area.width);
    let max_start = total_body.saturating_sub(viewport_h);
    if app.docs.scroll > max_start {
        app.docs.scroll = max_start;
    }
}

fn handle_editor_status_click(app: &mut App, me: MouseEvent, status_area: Rect) {
    let inner_x = status_area.x + 1;
    let actions_y = status_area.y + 1 + 1;
    if me.row != actions_y {
        return;
    }
    let mut x = inner_x;
    let import_label = "Import: ";
    let export_label = "Export: ";
    let gap = "   ";
    let btn_ibin = "[BIN]";
    let btn_icode = "[CODE]";
    let btn_ebin = "[BIN]";
    let btn_ecode = "[CODE]";

    x += import_label.len() as u16;
    let ibin_start = x; let ibin_end = x + btn_ibin.len() as u16; x = ibin_end + 1;
    let icode_start = x; let icode_end = x + btn_icode.len() as u16; x = icode_end;
    x += gap.len() as u16;
    x += export_label.len() as u16;
    let ebin_start = x; let ebin_end = x + btn_ebin.len() as u16; x = ebin_end + 1;
    let ecode_start = x; let ecode_end = x + btn_ecode.len() as u16;

    let col = me.column;
    if col >= ibin_start && col < ibin_end {
        if let Some(path) = OSFileDialog::new()
            .add_filter("Binary", &["bin", "img"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(path) {
                app.load_binary(&bytes);
                use crate::ui::view::disasm::disasm_word;
                let mut lines = Vec::new();
                for chunk in bytes.chunks(4) {
                    let mut b = [0u8; 4];
                    for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
                    let w = u32::from_le_bytes(b);
                    lines.push(disasm_word(w));
                }
                app.editor.buf.lines = lines;
                app.editor.buf.cursor_row = 0;
                app.editor.buf.cursor_col = 0;
            }
        }
        return;
    }
    if col >= icode_start && col < icode_end {
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
        return;
    }
    if col >= ebin_start && col < ebin_end {
        if let Some(path) = OSFileDialog::new()
            .add_filter("Binary", &["bin"])
            .set_file_name("program.bin")
            .save_file()
        {
            // Use cached result when available; otherwise re-assemble.
            let (words, data, bss_size) = match (
                app.editor.last_ok_text.as_ref(),
                app.editor.last_ok_data.as_ref(),
                app.editor.last_ok_bss_size,
            ) {
                (Some(t), Some(d), bss) => (t.clone(), d.clone(), bss.unwrap_or(0)),
                _ => match crate::falcon::asm::assemble(&app.editor.buf.text(), app.run.base_pc) {
                    Ok(p) => (p.text, p.data, p.bss_size),
                    Err(e) => {
                        app.console.push_error(format!("Cannot export: assemble error at line {}: {}", e.line + 1, e.msg));
                        return;
                    }
                },
            };
            // FALC format: "FALC" + text_size(u32LE) + data_size(u32LE) + bss_size(u32LE)
            //              + text_bytes + data_bytes
            // BSS is NOT stored — loader zero-initialises it at runtime.
            let text_bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            let text_size = text_bytes.len() as u32;
            let data_size = data.len() as u32;
            let mut bytes: Vec<u8> = Vec::with_capacity(16 + text_bytes.len() + data.len());
            bytes.extend_from_slice(b"FALC");
            bytes.extend_from_slice(&text_size.to_le_bytes());
            bytes.extend_from_slice(&data_size.to_le_bytes());
            bytes.extend_from_slice(&bss_size.to_le_bytes());
            bytes.extend_from_slice(&text_bytes);
            bytes.extend_from_slice(&data);
            let _ = std::fs::write(path, bytes);
        }
        return;
    }
    if col >= ecode_start && col < ecode_end {
        if let Some(path) = OSFileDialog::new()
            .add_filter("Falcon ASM", &["fas", "asm"])
            .set_file_name("program.fas")
            .save_file()
        {
            let _ = std::fs::write(path, app.editor.buf.text());
        }
        return;
    }
}

fn start_imem_drag(app: &mut App, me: MouseEvent, area: Rect) {
    if app.run.imem_collapsed { return; }
    let cols = run_cols(app, area);
    let imem = cols[1];
    let bar_x = imem.x + imem.width - 1;
    if me.column == bar_x && me.row >= area.y && me.row < area.y + area.height {
        app.run.imem_drag = true;
        app.run.imem_drag_start_x = me.column;
        app.run.imem_width_start = app.run.imem_width;
    }
}

fn handle_imem_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let delta = me.column as i32 - app.run.imem_drag_start_x as i32;
    let main = run_main_area(app, area);
    let sidebar_w = if app.run.sidebar_collapsed { 3 } else { app.run.sidebar_width };
    let available = main.width.saturating_sub(sidebar_w + 40);
    let max = (available as i32).max(20);
    let new_width = (app.run.imem_width_start as i32 + delta).clamp(20, max);
    app.run.imem_width = new_width as u16;
}

fn handle_sidebar_drag(app: &mut App, me: MouseEvent, area: Rect) {
    let delta = me.column as i32 - app.run.sidebar_drag_start_x as i32;
    let main = run_main_area(app, area);
    let imem_w = if app.run.imem_collapsed { 3 } else { app.run.imem_width };
    let available = main.width.saturating_sub(imem_w + 40);
    let max = (available as i32).max(20);
    let new_width = (app.run.sidebar_width_start as i32 + delta).clamp(20, max);
    app.run.sidebar_width = new_width as u16;
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(run_area);
    let console = run_chunks[3];
    let bar_y = console.y;
    let clear_start = console.x + console.width.saturating_sub(6);
    let clear_end = clear_start + 5;
    if me.row == bar_y {
        if me.column >= clear_start && me.column < clear_end {
            app.run.hover_console_clear = true;
            app.run.hover_console_bar = false;
        } else if me.column >= console.x && me.column < console.x + console.width {
            app.run.hover_console_bar = true;
            app.run.hover_console_clear = false;
        } else if !app.run.console_drag {
            app.run.hover_console_bar = false;
            app.run.hover_console_clear = false;
        }
    } else if !app.run.console_drag {
        app.run.hover_console_bar = false;
        app.run.hover_console_clear = false;
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
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
        app.run.console_drag = true;
        app.run.console_drag_start_y = me.row;
        app.run.console_height_start = app.run.console_height;
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
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
    let delta = app.run.console_drag_start_y as i32 - me.row as i32;
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let run_area = root_chunks[1];
    let max = run_area.height.saturating_sub(3 + 5);
    let mut new_h = app.run.console_height_start as i32 + delta;
    if new_h < 1 { new_h = 1; }
    if new_h as u16 > max { new_h = max as i32; }
    app.run.console_height = new_h as u16;
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(run_area);
    let _main = run_chunks[2];
    let console = run_chunks[3];

    if me.row >= console.y && me.row < console.y + console.height {
        let total = app.console.lines.len();
        let visible = app.run.console_height.saturating_sub(3) as usize;
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

    let cols = run_cols(app, area);
    let side = cols[0];
    let imem = cols[1];
    if me.column >= side.x
        && me.column < side.x + side.width
        && me.row >= side.y
        && me.row < side.y + side.height
    {
        if app.run.show_registers {
            let max_scroll = max_regs_scroll(app);
            if app.run.regs_scroll > max_scroll {
                app.run.regs_scroll = max_scroll;
            }
            if up {
                app.run.regs_scroll = app.run.regs_scroll.saturating_sub(1);
            } else {
                app.run.regs_scroll = (app.run.regs_scroll + 1).min(max_scroll);
            }
        } else {
            if up {
                app.run.mem_view_addr = app.run.mem_view_addr.saturating_sub(app.run.mem_view_bytes);
            } else {
                let max = app.run.mem_size.saturating_sub(app.run.mem_view_bytes as usize) as u32;
                if app.run.mem_view_addr < max {
                    app.run.mem_view_addr = app.run.mem_view_addr
                        .saturating_add(app.run.mem_view_bytes)
                        .min(max);
                }
            }
            app.run.mem_region = MemRegion::Custom;
        }
    }

    if me.column >= imem.x
        && me.column < imem.x + imem.width
        && me.row >= imem.y
        && me.row < imem.y + imem.height
    {
        if app.run.is_running {
            return;
        }
        if let Some(ref text) = app.editor.last_ok_text {
            let visible = imem.height.saturating_sub(2) as usize;
            let total = text.len();
            let max_scroll = total.saturating_sub(visible);
            if app.run.imem_scroll > max_scroll {
                app.run.imem_scroll = max_scroll;
            }
            if up {
                app.run.imem_scroll = app.run.imem_scroll.saturating_sub(1);
            } else {
                app.run.imem_scroll = (app.run.imem_scroll + 1).min(max_scroll);
            }
        }
    }
}

fn handle_imem_click(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);
    let imem = cols[1];
    let bar_x = imem.x + imem.width - 1;
    if me.column == bar_x {
        return;
    }
    let inner = Rect::new(
        imem.x + 1,
        imem.y + 1,
        imem.width.saturating_sub(2),
        imem.height.saturating_sub(2),
    );
    if me.column >= inner.x
        && me.column < inner.x + inner.width
        && me.row >= inner.y
        && me.row < inner.y + inner.height
    {
        if let Some(addr) = app.run.hover_imem_addr {
            app.run.prev_pc = app.run.cpu.pc;
            app.run.cpu.pc = addr;
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

// ── Cache tab mouse handlers ─────────────────────────────────────────────────

/// Returns (level_selector, subtab_header, content, controls_bar).
fn cache_content_area(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
        .split(area);
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // level selector bar
            Constraint::Length(3), // subtab header
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar
        ])
        .split(root[1]);
    (parts[0], parts[1], parts[2], parts[3])
}

fn update_cache_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let (level_area, header_area, content_area, controls_area) = cache_content_area(area);

    // Reset all hover flags
    app.cache.hover_subtab_stats = false;
    app.cache.hover_subtab_config = false;
    app.cache.hover_subtab_view = false;
    app.cache.hover_reset = false;
    app.cache.hover_pause = false;
    app.cache.hover_scope_i = false;
    app.cache.hover_scope_d = false;
    app.cache.hover_scope_both = false;
    app.cache.hover_apply = false;
    app.cache.hover_apply_keep = false;
    app.cache.hover_preset_i = None;
    app.cache.hover_preset_d = None;
    app.cache.hover_config_field = None;
    for h in app.cache.hover_level.iter_mut() { *h = false; }
    app.cache.hover_add_level = false;
    app.cache.hover_remove_level = false;

    // Level selector bar
    if me.row == level_area.y {
        update_level_selector_hover(app, me, level_area);
    }

    // Header row (subtab buttons)
    let inner_header = Rect::new(
        header_area.x + 1,
        header_area.y + 1,
        header_area.width.saturating_sub(2),
        1,
    );
    if me.row == inner_header.y && me.column >= inner_header.x {
        let x = me.column - inner_header.x;
        // " Stats " (x 1..8) | " View " (x 10..16) | " Config " (x 18..26)
        if x >= 1 && x < 8 {
            app.cache.hover_subtab_stats = true;
        } else if x >= 10 && x < 16 {
            app.cache.hover_subtab_view = true;
        } else if x >= 18 && x < 26 {
            app.cache.hover_subtab_config = true;
        }
    }

    // Shared controls bar — active for all subtabs
    let ctrl_y = controls_area.y + 1;
    if me.row == ctrl_y {
        let x = me.column.saturating_sub(controls_area.x + 1);
        // " [Reset]  [Pause]    View: [I-Cache] [D-Cache] [Both]  ..."
        // [Reset]=1..8  [Pause/Resume]=10..18  [I-Cache]=27..36  [D-Cache]=37..46  [Both]=47..53
        if x >= 1 && x < 8 { app.cache.hover_reset = true; }
        else if x >= 10 && x < 18 { app.cache.hover_pause = true; }
        else if x >= 27 && x < 36 { app.cache.hover_scope_i = true; }
        else if x >= 37 && x < 46 { app.cache.hover_scope_d = true; }
        else if x >= 47 && x < 53 { app.cache.hover_scope_both = true; }
    }

    // Config panel controls
    if matches!(app.cache.subtab, CacheSubtab::Config) {
        let selected = app.cache.selected_level;

        if selected == 0 {
            // L1 two-column layout
            let half_w = content_area.width / 2;

            let fields_y0 = content_area.y + 1;
            let fields_y1 = content_area.y + content_area.height.saturating_sub(7);
            if me.row >= fields_y0 && me.row < fields_y1 {
                let row_idx = (me.row - fields_y0) as usize;
                if let Some(field) = ConfigField::from_list_row(row_idx) {
                    let is_icache = me.column < content_area.x + half_w;
                    app.cache.hover_config_field = Some((is_icache, field));
                }
            }

            let apply_y = content_area.y + content_area.height.saturating_sub(3);
            if me.row == apply_y {
                let x = me.column.saturating_sub(content_area.x + 1);
                if x >= 1 && x < 22 { app.cache.hover_apply = true; }
                else if x >= 24 && x < 43 { app.cache.hover_apply_keep = true; }
            }

            let preset_y = content_area.y + content_area.height.saturating_sub(6);
            if me.row == preset_y {
                let col_x = me.column;
                let check_preset = |panel_x: u16, panel_w: u16| -> Option<usize> {
                    if col_x < panel_x || col_x >= panel_x + panel_w { return None; }
                    let x = col_x - panel_x;
                    if x >= 10 && x < 17 { Some(0) }
                    else if x >= 18 && x < 26 { Some(1) }
                    else if x >= 27 && x < 34 { Some(2) }
                    else { None }
                };
                app.cache.hover_preset_i = check_preset(content_area.x, half_w);
                app.cache.hover_preset_d = check_preset(content_area.x + half_w, half_w);
            }
        } else {
            // L2+ single-column unified layout (centered, max 60 wide)
            let col_w = content_area.width.min(60);
            let col_x = content_area.x + (content_area.width.saturating_sub(col_w)) / 2;
            let col_area = Rect::new(col_x, content_area.y, col_w, content_area.height);

            let fields_y0 = col_area.y + 1;
            let fields_y1 = col_area.y + col_area.height.saturating_sub(7);
            if me.row >= fields_y0 && me.row < fields_y1 {
                let row_idx = (me.row - fields_y0) as usize;
                if let Some(field) = ConfigField::from_list_row(row_idx) {
                    if me.column >= col_area.x && me.column < col_area.x + col_area.width {
                        app.cache.hover_config_field = Some((false, field));
                    }
                }
            }

            let apply_y = col_area.y + col_area.height.saturating_sub(3);
            if me.row == apply_y {
                let x = me.column.saturating_sub(col_area.x + 1);
                if x >= 1 && x < 22 { app.cache.hover_apply = true; }
                else if x >= 24 && x < 43 { app.cache.hover_apply_keep = true; }
            }

            let preset_y = col_area.y + col_area.height.saturating_sub(6);
            if me.row == preset_y && me.column >= col_area.x && me.column < col_area.x + col_area.width {
                let x = me.column - col_area.x;
                // " Presets: [Small NKB] [Med NKB] [Large NKB]"
                // Approximate positions (labels vary); use same offsets as render_unified_presets
                if x >= 10 && x < 22 { app.cache.hover_preset_d = Some(0); }
                else if x >= 23 && x < 33 { app.cache.hover_preset_d = Some(1); }
                else if x >= 34 && x < 47 { app.cache.hover_preset_d = Some(2); }
            }
        }
    }
}

/// Update hover state for the level selector bar (row = level_area.y).
fn update_level_selector_hover(app: &mut App, me: MouseEvent, level_area: Rect) {
    let mut x = me.column.saturating_sub(level_area.x + 1); // relative x
    // " [ L1 ] [ L2 ] ... [+ Add] [- Remove]   ..."
    // Each level button: "[ LN ]" = 6 chars, " " separator
    let num_levels = 1 + app.cache.extra_pending.len(); // L1 + extras
    for i in 0..num_levels {
        let btn_w: u16 = 6; // "[ L1 ]"
        if x < btn_w {
            if i < app.cache.hover_level.len() {
                app.cache.hover_level[i] = true;
            }
            return;
        }
        x = x.saturating_sub(btn_w + 1); // skip button + " "
    }
    // "  " gap before [+ Add]
    x = x.saturating_sub(2);
    let add_w: u16 = 7; // "[+ Add]"
    if x < add_w {
        app.cache.hover_add_level = true;
        return;
    }
    x = x.saturating_sub(add_w + 1);
    let rem_w: u16 = 10; // "[- Remove]"
    if x < rem_w && !app.cache.extra_pending.is_empty() {
        app.cache.hover_remove_level = true;
    }
}

fn handle_cache_click(app: &mut App, me: MouseEvent, area: Rect) {
    let (level_area, header_area, content_area, controls_area) = cache_content_area(area);

    // Level selector bar clicks
    if me.row == level_area.y {
        handle_level_selector_click(app, me, level_area);
        return;
    }

    // Subtab header clicks — Stats | View | Config
    let inner_header = Rect::new(
        header_area.x + 1,
        header_area.y + 1,
        header_area.width.saturating_sub(2),
        1,
    );
    if me.row == inner_header.y && me.column >= inner_header.x {
        let x = me.column - inner_header.x;
        if x >= 1  && x < 8  { app.cache.subtab = CacheSubtab::Stats;  return; }
        if x >= 10 && x < 16 { app.cache.subtab = CacheSubtab::View;   return; }
        if x >= 18 && x < 26 { app.cache.subtab = CacheSubtab::Config; return; }
    }

    // Shared controls bar — available in all subtabs
    let ctrl_y = controls_area.y + 1;
    if me.row == ctrl_y {
        let x = me.column.saturating_sub(controls_area.x + 1);
        if x >= 1 && x < 8 { app.restart_simulation(); return; }
        if x >= 10 && x < 18 {
            if app.run.is_running {
                app.run.is_running = false;
            } else if !app.run.faulted {
                app.run.is_running = true;
            }
            return;
        }
        if app.cache.selected_level == 0 {
            if x >= 27 && x < 36 { app.cache.scope = CacheScope::ICache; return; }
            if x >= 37 && x < 46 { app.cache.scope = CacheScope::DCache; return; }
            if x >= 47 && x < 53 { app.cache.scope = CacheScope::Both;   return; }
        }
    }

    // Config controls
    if matches!(app.cache.subtab, CacheSubtab::Config) {
        let selected = app.cache.selected_level;

        if selected == 0 {
            handle_l1_config_click(app, me, content_area);
        } else {
            handle_unified_config_click(app, me, content_area, selected - 1);
        }
    }
}

fn handle_level_selector_click(app: &mut App, me: MouseEvent, level_area: Rect) {
    let mut x = me.column.saturating_sub(level_area.x + 1);
    let num_levels = 1 + app.cache.extra_pending.len();
    for i in 0..num_levels {
        let btn_w: u16 = 6;
        if x < btn_w {
            app.cache.selected_level = i;
            return;
        }
        x = x.saturating_sub(btn_w + 1);
    }
    x = x.saturating_sub(2); // "  " gap
    let add_w: u16 = 7;
    if x < add_w {
        app.add_cache_level();
        return;
    }
    x = x.saturating_sub(add_w + 1);
    let rem_w: u16 = 10;
    if x < rem_w && !app.cache.extra_pending.is_empty() {
        app.remove_last_cache_level();
    }
}

fn handle_l1_config_click(app: &mut App, me: MouseEvent, content_area: Rect) {
    let half_w = content_area.width / 2;

    let fields_y0 = content_area.y + 1;
    let fields_y1 = content_area.y + content_area.height.saturating_sub(7);
    if me.row >= fields_y0 && me.row < fields_y1 {
        let row_idx = (me.row - fields_y0) as usize;
        if let Some(field) = ConfigField::from_list_row(row_idx) {
            let is_icache = me.column < content_area.x + half_w;
            if field.is_numeric() {
                let initial = app.cache_field_value_str(is_icache, field);
                app.cache.edit_field = Some((is_icache, field));
                app.cache.edit_buf = initial;
                app.cache.config_error = None;
                app.cache.config_status = None;
            } else {
                app.cycle_cache_field(is_icache, field, true);
                app.cache.edit_field = Some((is_icache, field));
                app.cache.edit_buf.clear();
            }
            return;
        }
    }

    app.cache.edit_field = None;
    app.cache.edit_buf.clear();

    let apply_y = content_area.y + content_area.height.saturating_sub(3);
    if me.row == apply_y {
        let x = me.column.saturating_sub(content_area.x + 1);
        if x >= 1 && x < 22 {
            apply_l1_config(app, false);
            return;
        }
        if x >= 24 && x < 43 {
            apply_l1_config(app, true);
            return;
        }
    }

    let preset_y = content_area.y + content_area.height.saturating_sub(6);
    if me.row == preset_y {
        let col_x = me.column;
        let apply_preset = |panel_x: u16, panel_w: u16| -> Option<usize> {
            if col_x < panel_x || col_x >= panel_x + panel_w { return None; }
            let x = col_x - panel_x;
            if x >= 10 && x < 17 { Some(0) }
            else if x >= 18 && x < 26 { Some(1) }
            else if x >= 27 && x < 34 { Some(2) }
            else { None }
        };
        use crate::falcon::cache::cache_presets;
        if let Some(idx) = apply_preset(content_area.x, half_w) {
            app.cache.pending_icache = cache_presets(true)[idx].clone();
            app.cache.config_error = None;
            app.cache.config_status = None;
        }
        if let Some(idx) = apply_preset(content_area.x + half_w, half_w) {
            app.cache.pending_dcache = cache_presets(false)[idx].clone();
            app.cache.config_error = None;
            app.cache.config_status = None;
        }
    }
}

fn apply_l1_config(app: &mut App, _keep_history: bool) {
    let icfg = app.cache.pending_icache.clone();
    let dcfg = app.cache.pending_dcache.clone();
    if let Err(msg) = icfg.validate() {
        app.cache.config_error = Some(format!("I-Cache: {msg}"));
        return;
    }
    if let Err(msg) = dcfg.validate() {
        app.cache.config_error = Some(format!("D-Cache: {msg}"));
        return;
    }
    app.cache.config_error = None;
    app.cache.config_status = Some("Config applied — simulation restarted.".to_string());
    app.restart_simulation();
    app.cache.view_scroll = 0;
    app.cache.stats_scroll = 0;
}

fn handle_unified_config_click(app: &mut App, me: MouseEvent, content_area: Rect, extra_idx: usize) {
    let col_w = content_area.width.min(60);
    let col_x = content_area.x + (content_area.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, content_area.y, col_w, content_area.height);

    let fields_y0 = col_area.y + 1;
    let fields_y1 = col_area.y + col_area.height.saturating_sub(7);
    if me.row >= fields_y0 && me.row < fields_y1
        && me.column >= col_area.x && me.column < col_area.x + col_area.width
    {
        let row_idx = (me.row - fields_y0) as usize;
        if let Some(field) = ConfigField::from_list_row(row_idx) {
            if field.is_numeric() {
                let initial = app.cache_field_value_str(false, field);
                app.cache.edit_field = Some((false, field));
                app.cache.edit_buf = initial;
                app.cache.config_error = None;
                app.cache.config_status = None;
            } else {
                app.cycle_cache_field(false, field, true);
                app.cache.edit_field = Some((false, field));
                app.cache.edit_buf.clear();
            }
            return;
        }
    }

    app.cache.edit_field = None;
    app.cache.edit_buf.clear();

    // Apply buttons
    let apply_y = col_area.y + col_area.height.saturating_sub(3);
    if me.row == apply_y {
        let x = me.column.saturating_sub(col_area.x + 1);
        if x >= 1 && x < 22 {
            apply_extra_config(app, extra_idx, false);
            return;
        }
        if x >= 24 && x < 43 {
            apply_extra_config(app, extra_idx, true);
            return;
        }
    }

    // Presets
    let preset_y = col_area.y + col_area.height.saturating_sub(6);
    if me.row == preset_y && me.column >= col_area.x && me.column < col_area.x + col_area.width {
        let x = me.column - col_area.x;
        use crate::falcon::cache::extra_level_presets;
        let presets = extra_level_presets();
        let idx = if x >= 10 && x < 22 { Some(0) }
            else if x >= 23 && x < 33 { Some(1) }
            else if x >= 34 && x < 47 { Some(2) }
            else { None };
        if let Some(i) = idx {
            if extra_idx < app.cache.extra_pending.len() {
                app.cache.extra_pending[extra_idx] = presets[i].clone();
                app.cache.config_error = None;
                app.cache.config_status = None;
            }
        }
    }
}

fn apply_extra_config(app: &mut App, extra_idx: usize, _keep_history: bool) {
    if extra_idx >= app.cache.extra_pending.len() { return; }
    let cfg = app.cache.extra_pending[extra_idx].clone();
    if let Err(msg) = cfg.validate() {
        app.cache.config_error = Some(format!("L{} Cache: {msg}", extra_idx + 2));
        return;
    }
    app.cache.config_error = None;
    app.cache.config_status = Some("Config applied — simulation restarted.".to_string());
    app.restart_simulation();
    app.cache.view_scroll = 0;
    app.cache.stats_scroll = 0;
}





