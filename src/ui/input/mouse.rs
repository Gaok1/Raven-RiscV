use crate::ui::{
    app::{App, CacheScope, CacheSubtab, ConfigField, DocsPage, EditorMode, FormatMode, MemRegion, PathInputAction, RunButton, Tab},
    editor::Editor,
};
use crate::ui::input::keyboard::{do_export_cfg, do_export_results, do_import_cfg};
use crate::ui::view::{ELF_BTN_CANCEL, ELF_BTN_DISCARD, ELF_BTN_EDIT, ELF_BTN_ROW, ELF_POPUP_H, ELF_POPUP_W};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use rfd::FileDialog as OSFileDialog;

use super::max_regs_scroll;
use crate::ui::view::docs::{docs_body_line_count, free_page_line_count};

pub fn handle_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    app.mouse_x = me.column;
    app.mouse_y = me.row;

    if app.show_exit_popup {
        handle_exit_popup_mouse(app, me, area);
        return;
    }

    if app.help_open {
        // Clicking anywhere outside the popup closes it; click inside does page nav
        handle_help_popup_mouse(app, me, area);
        return;
    }

    if app.editor.elf_prompt_open && matches!(app.tab, Tab::Editor) {
        handle_elf_prompt_mouse(app, me, area);
        return;
    }

    if app.path_input.open {
        return;
    }

    // Hover tabs — derived from Tab::all() so new tabs are automatically supported
    app.hover_tab = None;
    app.hover_run_button = None;
    app.hover_help = false;
    if me.row == area.y + 1 {
        // "?" help button is in the rightmost position of the tab bar row
        // It occupies 5 columns from the right edge: "[?]  " → columns [width-6..width-1]
        let help_col = area.x + area.width.saturating_sub(6);
        if me.column >= help_col && me.column < area.x + area.width.saturating_sub(1) {
            app.hover_help = true;
            if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                app.help_open = !app.help_open;
                app.help_page = 0;
            }
        } else {
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
                        if tab != app.tab && matches!(app.tab, Tab::Editor) && app.editor.dirty {
                            app.assemble_and_load();
                        }
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
    }

    // Scrolls
    match me.kind {
        MouseEventKind::ScrollUp => match app.tab {
            Tab::Editor => app.editor.buf.move_up(),
            Tab::Run => handle_run_scroll(app, me, area, true),
            Tab::Cache => match app.cache.subtab {
                CacheSubtab::Stats => {
                    app.cache.history_scroll = app.cache.history_scroll.saturating_sub(1);
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
                    if !app.cache.session_history.is_empty() {
                        app.cache.history_scroll = (app.cache.history_scroll + 1)
                            .min(app.cache.session_history.len() - 1);
                    }
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
                // Scroll the panel under the cursor: D-cache slot 1 if its track is set
                let tracks = app.cache.hscroll_tracks.get();
                let use_d = tracks[1].1 > 0 && me.column >= tracks[1].0
                    && me.column < tracks[1].0 + tracks[1].1;
                if use_d {
                    app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_sub(3);
                } else {
                    app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_sub(3);
                }
            }
        }
        MouseEventKind::ScrollRight => {
            if matches!(app.tab, Tab::Cache)
                && matches!(app.cache.subtab, CacheSubtab::View)
            {
                let tracks = app.cache.hscroll_tracks.get();
                let use_d = tracks[1].1 > 0 && me.column >= tracks[1].0
                    && me.column < tracks[1].0 + tracks[1].1;
                if use_d {
                    app.cache.view_h_scroll_d = app.cache.view_h_scroll_d.saturating_add(3);
                } else {
                    app.cache.view_h_scroll = app.cache.view_h_scroll.saturating_add(3);
                }
            }
        }
        _ => {}
    }

    // Cache tab interactions
    if let Tab::Cache = app.tab {
        update_cache_hover(app, me, area);
        update_cache_run_status_hover(app, me, area);
        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            handle_cache_run_status_click(app, me, area);
            // H-scrollbar: start drag or jump to click position
            if matches!(app.cache.subtab, CacheSubtab::View) && app.cache.hover_hscrollbar {
                let track_x = app.cache.hscroll_hover_track_x;
                let track_w = app.cache.hscroll_hover_track_w;
                let max_scroll = app.cache.hscroll_max.get();
                // Determine which panel (slot 0 = I-cache, slot 1 = D-cache)
                let tracks = app.cache.hscroll_tracks.get();
                let is_dcache = tracks[1].1 > 0 && track_x == tracks[1].0;
                app.cache.hscroll_drag_is_dcache = is_dcache;
                let cur_scroll = if is_dcache { app.cache.view_h_scroll_d } else { app.cache.view_h_scroll };
                // Jump to click ratio immediately
                if track_w > 0 {
                    let rel = me.column.saturating_sub(track_x).min(track_w - 1) as f64;
                    let new_scroll = (rel / (track_w as f64) * max_scroll as f64) as usize;
                    if is_dcache {
                        app.cache.view_h_scroll_d = new_scroll.min(max_scroll);
                    } else {
                        app.cache.view_h_scroll = new_scroll.min(max_scroll);
                    }
                }
                // Start drag state
                app.cache.hscroll_drag = true;
                app.cache.hscroll_drag_start_x = me.column;
                app.cache.hscroll_start = cur_scroll;
                app.cache.hscroll_drag_max = max_scroll;
                app.cache.hscroll_drag_track_w = track_w;
            } else {
                handle_cache_click(app, me, area);
            }
        }
        if matches!(me.kind, MouseEventKind::Up(MouseButton::Left)) {
            app.cache.hscroll_drag = false;
        }
        if matches!(me.kind, MouseEventKind::Drag(MouseButton::Left)) && app.cache.hscroll_drag {
            let track_w = app.cache.hscroll_drag_track_w;
            let max_scroll = app.cache.hscroll_drag_max;
            if track_w > 0 && max_scroll > 0 {
                let delta = me.column as i32 - app.cache.hscroll_drag_start_x as i32;
                let scale = max_scroll as f64 / track_w as f64;
                let new_scroll = (app.cache.hscroll_start as i64 + (delta as f64 * scale) as i64)
                    .max(0) as usize;
                if app.cache.hscroll_drag_is_dcache {
                    app.cache.view_h_scroll_d = new_scroll.min(max_scroll);
                } else {
                    app.cache.view_h_scroll = new_scroll.min(max_scroll);
                }
            }
        }
    }

    if let Tab::Docs = app.tab {
        if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
            handle_docs_click(app, me);
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

        // Use the stable scroll offset written by the renderer (consistent with display).
        let start = app.editor.buf.scroll_offset.get();

        let visible_h = editor_area.height.saturating_sub(2) as usize;
        let len = app.editor.buf.lines.len();
        let end = (start + visible_h).min(len);
        let num_width = end.to_string().len() as u16;
        let gutter = num_width + 3;

        let inner_top  = editor_area.y + 1;
        let inner_bot  = editor_area.y + editor_area.height.saturating_sub(1);
        let inner_left = editor_area.x + 1;
        let inner_right = editor_area.x + editor_area.width.saturating_sub(1);

        let within = |x: u16, y: u16| {
            x >= inner_left && x < inner_right && y >= inner_top && y < inner_bot
        };

        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if within(me.column, me.row) {
                    // ELF mode: clicking the editor body opens the prompt instead of editing.
                    if app.editor.last_ok_elf_bytes.is_some() {
                        app.editor.elf_prompt_open = true;
                    } else {
                        let y = (me.row - inner_top) as usize;
                        let row = (start + y).min(app.editor.buf.lines.len().saturating_sub(1));
                        let x = me.column.saturating_sub(inner_left + gutter) as usize;
                        let col = x.min(Editor::char_count(&app.editor.buf.lines[row]));
                        app.editor.buf.cursor_row = row;
                        app.editor.buf.cursor_col = col;
                        app.editor.buf.selection_anchor = Some((row, col));
                        if app.mode == EditorMode::Command {
                            app.mode = EditorMode::Insert;
                        }
                    }
                } else if app.mode == EditorMode::Insert {
                    app.mode = EditorMode::Command;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Use stable scroll_offset so view doesn't jump during drag.
                let drag_start = app.editor.buf.scroll_offset.get();
                if within(me.column, me.row) {
                    let y = (me.row - inner_top) as usize;
                    let row = (drag_start + y).min(app.editor.buf.lines.len().saturating_sub(1));
                    let x = me.column.saturating_sub(inner_left + gutter) as usize;
                    let col = x.min(Editor::char_count(&app.editor.buf.lines[row]));
                    app.editor.buf.cursor_row = row;
                    app.editor.buf.cursor_col = col;
                } else if me.row < inner_top && app.editor.buf.cursor_row > 0 {
                    // Dragged above: scroll up one line per event
                    app.editor.buf.cursor_row -= 1;
                } else if me.row >= inner_bot {
                    // Dragged below: scroll down one line per event
                    let max = app.editor.buf.lines.len().saturating_sub(1);
                    if app.editor.buf.cursor_row < max {
                        app.editor.buf.cursor_row += 1;
                    }
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
        update_sidebar_hover(app, me, area);
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                handle_run_status_click(app, me, area);
                handle_panel_title_click(app, me, area);
                start_sidebar_drag(app, me, area);
                start_imem_drag(app, me, area);
                handle_imem_bp_click(app, me, area);
                handle_imem_click(app, me, area);
                handle_console_clear(app, me, area);
                start_console_drag(app, me, area);
                handle_register_click(app, me, area);
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

fn apply_run_button(app: &mut App, btn: RunButton) {
    match btn {
        RunButton::View => {
            if app.run.show_dyn {
                app.run.show_dyn = false;
            } else if app.run.show_registers {
                app.run.show_registers = false;
                app.run.show_dyn = true;
            } else {
                app.run.show_registers = true;
            }
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
            match app.run.mem_region {
                MemRegion::Data | MemRegion::Custom => {
                    let sp = app.run.cpu.x[2];
                    app.run.mem_view_addr = sp & !(app.run.mem_view_bytes - 1);
                    app.run.mem_region = MemRegion::Stack;
                }
                MemRegion::Stack => {
                    app.run.mem_region = MemRegion::Access;
                }
                MemRegion::Access => {
                    let hb = app.run.cpu.heap_break;
                    app.run.mem_view_addr = hb & !(app.run.mem_view_bytes - 1);
                    app.run.mem_region = MemRegion::Heap;
                }
                MemRegion::Heap => {
                    app.run.mem_view_addr = app.run.data_base;
                    app.run.mem_region = MemRegion::Data;
                }
            }
            app.run.show_registers = false;
            app.run.show_dyn = false;
        }
        RunButton::Speed => {
            app.run.speed = app.run.speed.cycle();
        }
        RunButton::ExecCount => { app.run.show_exec_count = !app.run.show_exec_count; }
        RunButton::InstrType => { app.run.show_instr_type = !app.run.show_instr_type; }
        RunButton::State => {
            if app.run.is_running {
                app.run.is_running = false;
            } else if !app.run.faulted {
                app.run.is_running = true;
            }
        }
        RunButton::Reset => {
            app.restart_simulation();
        }
    }
}

fn handle_run_status_click(app: &mut App, me: MouseEvent, area: Rect) {
    let status = run_status_area(app, area);
    if me.row != status.y + 1 { return; }
    if let Some(btn) = run_status_hit(app, status, me.column) {
        apply_run_button(app, btn);
    }
}

fn update_run_status_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let status = run_status_area(app, area);
    if me.row != status.y + 1 { return; }
    app.hover_run_button = run_status_hit(app, status, me.column);
}

/// Area of the run-controls widget (always visible, above subtab content).
fn cache_run_status_area(area: Rect) -> Rect {
    let (_, _, run_controls, _, _) = cache_content_area(area);
    run_controls
}

fn update_cache_run_status_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let status = cache_run_status_area(area);
    if me.row != status.y + 1 { return; }
    app.hover_run_button = cache_exec_hit(app, status, me.column);
}

fn handle_cache_run_status_click(app: &mut App, me: MouseEvent, area: Rect) {
    let status = cache_run_status_area(area);
    if me.row != status.y + 1 { return; }
    if let Some(btn) = cache_exec_hit(app, status, me.column) {
        apply_run_button(app, btn);
    }
}

/// Hit-test for the cache exec-controls widget (Reset + Speed + State buttons).
fn cache_exec_hit(app: &App, status: Rect, col: u16) -> Option<RunButton> {
    let speed_text = app.run.speed.label();
    let state_text = if app.run.is_running { "RUN" } else { "PAUSE" };

    let mut pos = status.x + 1; // inner x (after block border)
    let skip  = |p: &mut u16, s: &str| { *p += s.len() as u16; };
    let range = |p: &mut u16, label: &str| -> (u16, u16) {
        let s = *p;
        *p += 1 + label.len() as u16 + 1; // [label]
        (s, *p)
    };

    // " [Reset]  Speed [X1]  State [PAUSE]  ..."
    skip(&mut pos, " ");
    let (reset_s, reset_e) = range(&mut pos, "Reset");
    skip(&mut pos, "  Speed ");
    let (speed_s, speed_e) = range(&mut pos, speed_text);
    skip(&mut pos, "  State ");
    let (state_s, state_e) = range(&mut pos, state_text);

    if col >= reset_s && col < reset_e { Some(RunButton::Reset) }
    else if col >= speed_s && col < speed_e { Some(RunButton::Speed) }
    else if col >= state_s && col < state_e { Some(RunButton::State) }
    else { None }
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(root_chunks[1]);
    run_chunks[0]
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
        MemRegion::Data | MemRegion::Custom => "DATA",
        MemRegion::Stack => "STACK",
        MemRegion::Access => "R/W",
        MemRegion::Heap => "HEAP",
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

    let (region_start, region_end) = if !app.run.show_registers && !app.run.show_dyn {
        skip(&mut pos, "  Region ");
        range(&mut pos, region_text)
    } else {
        (0, 0)
    };

    skip(&mut pos, "  Format ");
    let (fmt_start, fmt_end) = range(&mut pos, fmt_text);

    skip(&mut pos, "  Sign ");
    let (sign_start, sign_end) = range(&mut pos, sign_text);

    let (bytes_start, bytes_end) = if !app.run.show_registers && !app.run.show_dyn {
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

    let count_text = if app.run.show_exec_count { "ON" } else { "OFF" };
    skip(&mut pos, "  Count ");
    let (count_start, count_end) = range(&mut pos, count_text);

    let type_text = if app.run.show_instr_type { "ON" } else { "OFF" };
    skip(&mut pos, "  Type ");
    let (type_start, type_end) = range(&mut pos, type_text);

    skip(&mut pos, "  ");
    let (reset_start, reset_end) = range(&mut pos, "Reset");

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
    } else if col >= count_start && col < count_end {
        Some(RunButton::ExecCount)
    } else if col >= type_start && col < type_end {
        Some(RunButton::InstrType)
    } else if col >= reset_start && col < reset_end {
        Some(RunButton::Reset)
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
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(run_area);
    run_chunks[1]
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
        let visible = inner.height as usize;
        let max_scroll = app.imem_total_visual_rows().saturating_sub(visible);
        if app.run.imem_scroll > max_scroll {
            app.run.imem_scroll = max_scroll;
        }
        let target_row = (me.row - inner.y) as usize;
        let (base, skip) = app.imem_addr_skip_for_scroll();
        app.run.hover_imem_addr = addr_at_visual_row(base, skip, target_row, app);
    } else {
        app.run.hover_imem_addr = None;
    }
}

/// Map a visual row within the displayed instruction panel to the instruction address,
/// accounting for block_comment/label rows.  `skip` = header rows hidden at the top of
/// the first block (matches the value returned by `imem_addr_skip_for_scroll`).
fn addr_at_visual_row(base: u32, skip: usize, target_row: usize, app: &App) -> Option<u32> {
    let mem_end = if let Some(text) = &app.editor.last_ok_text {
        app.run.base_pc.saturating_add((text.len() as u32).saturating_mul(4))
    } else {
        app.run.mem_size as u32
    };
    let mut vrow = 0usize;
    let mut addr = base;
    let mut skip_rem = skip;
    loop {
        if addr >= mem_end || (addr as usize) + 4 > app.run.mem_size {
            return None;
        }
        if app.run.block_comments.contains_key(&addr) {
            if skip_rem > 0 { skip_rem -= 1; }
            else {
                if vrow == target_row { return Some(addr); }
                vrow += 1;
            }
        }
        if let Some(names) = app.run.labels.get(&addr) {
            for _ in names {
                if skip_rem > 0 { skip_rem -= 1; continue; }
                if vrow == target_row { return Some(addr); }
                vrow += 1;
            }
        }
        if vrow == target_row { return Some(addr); }
        vrow += 1;
        addr = addr.wrapping_add(4);
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

/// Handle left-click on the Docs tab: page tabs and filter bar.
fn handle_docs_click(app: &mut App, me: MouseEvent) {
    use crate::ui::view::docs::{ALL_MASK, FILTER_ITEMS};

    let col = me.column;
    let row = me.row;

    // ── Page tab bar ──
    let tab_bar_y = app.docs.tab_bar_y.get();
    if row == tab_bar_y {
        let xs = app.docs.tab_bar_xs.get();
        let pages = [DocsPage::InstrRef, DocsPage::Syscalls, DocsPage::MemoryMap, DocsPage::FcacheRef];
        for (i, &(x_start, x_end)) in xs.iter().enumerate() {
            if col >= x_start && col < x_end {
                if app.docs.page != pages[i] {
                    app.docs.page = pages[i];
                    app.docs.scroll = 0;
                }
                return;
            }
        }
    }

    // ── Filter bar (InstrRef only) ──
    if matches!(app.docs.page, DocsPage::InstrRef) {
        let filter_y = app.docs.filter_bar_y.get();
        if row == filter_y {
            // Compute cumulative x-ranges for each filter item
            let mut x: u16 = 0;
            for (idx, &(label, bit, _)) in FILTER_ITEMS.iter().enumerate() {
                let w = (label.chars().count() + 3) as u16; // " ●Label " = label + 3
                let x_end = x + w;
                if col >= x && col < x_end {
                    app.docs.filter_cursor = idx;
                    if idx == 0 {
                        // "All" toggle
                        if app.docs.type_filter == ALL_MASK {
                            app.docs.type_filter = 0;
                        } else {
                            app.docs.type_filter = ALL_MASK;
                        }
                    } else {
                        app.docs.type_filter ^= bit;
                    }
                    app.docs.scroll = 0;
                    return;
                }
                x = x_end;
            }
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

    let max_start = match app.docs.page {
        DocsPage::InstrRef => {
            // tab_bar(1) + legend(2) + search(0|1) + filter(1) + col_hdr(1) + sep(1)
            let search_h: u16 = if app.docs.search_open { 1 } else { 0 };
            let fixed: u16 = 1 + 2 + search_h + 1 + 1 + 1;
            let viewport_h = docs_area.height.saturating_sub(fixed) as usize;
            if viewport_h == 0 { app.docs.scroll = 0; return; }
            let q = app.docs.search_query.clone();
            let f = app.docs.type_filter;
            let w = docs_area.width;
            docs_body_line_count(w, &q, f).saturating_sub(viewport_h)
        }
        p => {
            // header(2) consumed by render_free_page
            let viewport_h = docs_area.height.saturating_sub(2) as usize;
            if viewport_h == 0 { app.docs.scroll = 0; return; }
            free_page_line_count(p, app.docs.lang).saturating_sub(viewport_h)
        }
    };
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
    let btn_run = "[▶ RUN]";
    let btn_fmt = "[FORMAT]";

    x += import_label.len() as u16;
    let ibin_start = x; let ibin_end = x + btn_ibin.len() as u16; x = ibin_end + 1;
    let icode_start = x; let icode_end = x + btn_icode.len() as u16; x = icode_end;
    x += gap.len() as u16;
    x += export_label.len() as u16;
    let ebin_start = x; let ebin_end = x + btn_ebin.len() as u16; x = ebin_end + 1;
    let ecode_start = x; let ecode_end = x + btn_ecode.len() as u16; x = ecode_end;
    x += gap.len() as u16;
    // btn_run uses char count because it contains multi-byte '▶'
    let run_start = x; let run_end = x + btn_run.chars().count() as u16; x = run_end + 1;
    let fmt_start = x; let fmt_end = x + btn_fmt.len() as u16;

    let col = me.column;
    if col >= ibin_start && col < ibin_end {
        if let Some(path) = OSFileDialog::new()
            .add_filter("ELF / Binary", &["elf", "bin", "img"])
            .add_filter("All Files", &["*"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(path) {
                app.load_binary(&bytes);
                // Build editor disassembly from the already-decoded text words (ELF text
                // segment or FALC/flat text section), not from raw file bytes.
                use crate::ui::view::disasm::disasm_word;
                let lines: Vec<String> = if let Some(ref words) = app.editor.last_ok_text {
                    words.iter().map(|&w| disasm_word(w)).collect()
                } else {
                    bytes.chunks(4).map(|chunk| {
                        let mut b = [0u8; 4];
                        for (i, &v) in chunk.iter().enumerate() { b[i] = v; }
                        disasm_word(u32::from_le_bytes(b))
                    }).collect()
                };
                app.editor.buf.lines = lines;
                app.editor.buf.cursor_row = 0;
                app.editor.buf.cursor_col = 0;
            }
        } else {
            super::keyboard::open_path_input(app, PathInputAction::OpenBin);
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
        } else {
            super::keyboard::open_path_input(app, PathInputAction::OpenFas);
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
        } else {
            super::keyboard::open_path_input(app, PathInputAction::SaveBin);
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
        } else {
            super::keyboard::open_path_input(app, PathInputAction::SaveFas);
        }
        return;
    }
    // [▶ RUN]: assemble and switch to Run tab (B1)
    if col >= run_start && col < run_end {
        app.assemble_and_load();
        if app.editor.last_compile_ok == Some(true) {
            app.tab = crate::ui::app::Tab::Run;
        }
        return;
    }
    // [FORMAT]: auto-format assembly (B2)
    if col >= fmt_start && col < fmt_end {
        app.format_code();
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
        let visible = app.run.imem_inner_height.get().max(1);
        let total = app.imem_total_visual_rows();
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

fn handle_imem_bp_click(app: &mut App, me: MouseEvent, area: Rect) {
    let cols = run_cols(app, area);
    let imem = cols[1];
    if app.run.imem_collapsed { return; }
    let inner = Rect::new(
        imem.x + 1,
        imem.y + 1,
        imem.width.saturating_sub(2),
        imem.height.saturating_sub(2),
    );
    // Only toggle breakpoint when clicking exactly on the marker column (inner.x)
    if me.column == inner.x
        && me.row >= inner.y
        && me.row < inner.y + inner.height
    {
        if let Some(addr) = app.run.hover_imem_addr {
            if app.run.breakpoints.contains(&addr) {
                app.run.breakpoints.remove(&addr);
            } else {
                app.run.breakpoints.insert(addr);
            }
        }
    }
}

/// Track which visual row in the register sidebar the mouse is hovering over.
fn update_sidebar_hover(app: &mut App, me: MouseEvent, area: Rect) {
    if app.run.sidebar_collapsed || !app.run.show_registers {
        app.run.hover_reg_row = None;
        return;
    }
    let cols = run_cols(app, area);
    let sidebar = cols[0];
    let inner = Rect::new(
        sidebar.x + 1,
        sidebar.y + 1,
        sidebar.width.saturating_sub(2),
        sidebar.height.saturating_sub(2),
    );
    if me.column >= inner.x
        && me.column < inner.x + inner.width
        && me.row >= inner.y
        && me.row < inner.y + inner.height
    {
        app.run.hover_reg_row = Some((me.row - inner.y) as usize);
    } else {
        app.run.hover_reg_row = None;
    }
}

fn handle_register_click(app: &mut App, me: MouseEvent, area: Rect) {
    if !app.run.show_registers { return; }
    let cols = run_cols(app, area);
    let sidebar = cols[0];
    if app.run.sidebar_collapsed { return; }
    let inner = Rect::new(
        sidebar.x + 1,
        sidebar.y + 1,
        sidebar.width.saturating_sub(2),
        sidebar.height.saturating_sub(2),
    );
    if me.column < inner.x || me.column >= inner.x + inner.width { return; }
    if me.row < inner.y || me.row >= inner.y + inner.height { return; }

    let visual_row = (me.row - inner.y) as usize;
    let pinned = &app.run.pinned_regs;
    let sep_row = if pinned.is_empty() { usize::MAX } else { pinned.len() };

    // Click on a pinned register row → unpin it
    if visual_row < pinned.len() {
        let reg = pinned[visual_row];
        app.run.pinned_regs.retain(|&r| r != reg);
        return;
    }
    // Click on separator → ignore
    if visual_row == sep_row { return; }

    // Click on a regular (scrolled) register row → pin/unpin
    let offset = if pinned.is_empty() { 0 } else { pinned.len() + 1 };
    let row_in_scroll = visual_row.saturating_sub(offset);
    let visible_rows = inner.height as usize;
    let total = 33usize;
    let max_scroll = total.saturating_sub(visible_rows.saturating_sub(offset));
    let start = app.run.regs_scroll.min(max_scroll);
    let reg_idx = start + row_in_scroll; // 0 = PC, 1..=32 = x0..x31

    if reg_idx == 0 { return; } // PC can't be pinned
    if reg_idx > 32 { return; }
    let reg = (reg_idx - 1) as u8;
    if let Some(pos) = app.run.pinned_regs.iter().position(|&r| r == reg) {
        app.run.pinned_regs.remove(pos);
    } else {
        app.run.pinned_regs.push(reg);
    }
}

fn handle_help_popup_mouse(app: &mut App, me: MouseEvent, _area: Rect) {
    // Any left click closes the help popup (or could navigate pages)
    if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
        app.help_open = false;
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

fn handle_elf_prompt_mouse(app: &mut App, me: MouseEvent, area: Rect) {
    let popup_w = ELF_POPUP_W.min(area.width.saturating_sub(4));
    let popup = centered_rect(popup_w, ELF_POPUP_H, area);

    if me.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    // Click outside → cancel
    if me.column < popup.x + 1
        || me.column >= popup.x + popup.width - 1
        || me.row < popup.y + 1
        || me.row >= popup.y + popup.height - 1
    {
        app.editor.elf_prompt_open = false;
        return;
    }

    let inner_y = me.row.saturating_sub(popup.y + 1);
    if inner_y != ELF_BTN_ROW {
        return;
    }

    let inner_w = popup_w.saturating_sub(2);
    const GAP: u16 = 2;
    let total_btns = ELF_BTN_CANCEL.len() as u16
        + GAP
        + ELF_BTN_EDIT.len() as u16
        + GAP
        + ELF_BTN_DISCARD.len() as u16;
    let x_cancel  = popup.x + 1 + inner_w.saturating_sub(total_btns) / 2;
    let x_edit    = x_cancel  + ELF_BTN_CANCEL.len()  as u16 + GAP;
    let x_discard = x_edit    + ELF_BTN_EDIT.len()    as u16 + GAP;

    let col = me.column;
    if col >= x_cancel && col < x_cancel + ELF_BTN_CANCEL.len() as u16 {
        // Cancelar
        app.editor.elf_prompt_open = false;
    } else if col >= x_edit && col < x_edit + ELF_BTN_EDIT.len() as u16 {
        // Editar opcodes: unlock editor, keep current disassembly text
        app.editor.elf_prompt_open = false;
        app.editor.last_ok_elf_bytes = None;
        app.editor.dirty = true;
        app.editor.last_edit_at = Some(std::time::Instant::now());
        app.editor.last_assemble_msg = Some("ELF unloaded — edit opcodes and save.".to_string());
        app.mode = EditorMode::Insert;
    } else if col >= x_discard && col < x_discard + ELF_BTN_DISCARD.len() as u16 {
        // Descartar ELF: clear editor, unlock
        app.editor.elf_prompt_open = false;
        app.editor.last_ok_elf_bytes = None;
        app.editor.buf.lines = vec![String::new()];
        app.editor.buf.cursor_row = 0;
        app.editor.buf.cursor_col = 0;
        app.editor.last_ok_text = None;
        app.editor.last_ok_data = None;
        app.editor.last_compile_ok = None;
        app.editor.dirty = false;
        app.editor.last_assemble_msg = Some("ELF discarded — editor cleared.".to_string());
        app.mode = EditorMode::Insert;
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

/// Returns (level_selector, subtab_header, exec_controls, content, controls_bar).
fn cache_content_area(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
        .split(area);
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // level selector bar
            Constraint::Length(3), // subtab header
            Constraint::Length(4), // exec controls (Speed / State / Cycles)
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar
        ])
        .split(root[1]);
    (parts[0], parts[1], parts[2], parts[3], parts[4])
}

fn update_cache_hover(app: &mut App, me: MouseEvent, area: Rect) {
    let (level_area, header_area, _, content_area, controls_area) = cache_content_area(area);

    // Reset all hover flags
    app.cache.hover_subtab_stats = false;
    app.cache.hover_subtab_config = false;
    app.cache.hover_subtab_view = false;
    app.cache.hover_export_results = false;
    app.cache.hover_export_cfg = false;
    app.cache.hover_import_cfg = false;
    app.cache.hover_scope_i = false;
    app.cache.hover_scope_d = false;
    app.cache.hover_scope_both = false;
    app.cache.hover_apply = false;
    app.cache.hover_apply_keep = false;
    app.cache.hover_preset_i = None;
    app.cache.hover_preset_d = None;
    app.cache.hover_config_field = None;
    app.cache.hover_cpi_field = None;
    app.cache.hover_hscrollbar = false;
    app.cache.hover_view_fmt   = false;
    app.cache.hover_view_group = false;
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
    // Without cfg buttons: " [⬆ Results]    View: [I-Cache] [D-Cache] [Both]"
    //   x=1..12                   x=22..31    x=32..41    x=42..48
    // With cfg buttons (Config subtab): " [⬆ Results]  [⬇ Import cfg]  [⬆ Export cfg]    View: ..."
    //   x=1..12         x=14..28        x=30..44         scope shifts to x=54+
    let ctrl_y = controls_area.y + 1;
    if me.row == ctrl_y {
        let x = me.column.saturating_sub(controls_area.x + 1);
        let show_cfg_btns = matches!(app.cache.subtab, CacheSubtab::Config);
        if x >= 1 && x < 12 {
            app.cache.hover_export_results = true;
        } else if show_cfg_btns {
            if x >= 14 && x < 28      { app.cache.hover_import_cfg = true; }
            else if x >= 30 && x < 44 { app.cache.hover_export_cfg = true; }
            else if x >= 54 && x < 63 { app.cache.hover_scope_i = true; }
            else if x >= 64 && x < 73 { app.cache.hover_scope_d = true; }
            else if x >= 74 && x < 80 { app.cache.hover_scope_both = true; }
        } else {
            if x >= 22 && x < 31      { app.cache.hover_scope_i = true; }
            else if x >= 32 && x < 41 { app.cache.hover_scope_d = true; }
            else if x >= 42 && x < 48 { app.cache.hover_scope_both = true; }
        }
    }

    // Config panel controls
    if matches!(app.cache.subtab, CacheSubtab::Config) {
        let selected = app.cache.selected_level;

        if selected == 0 {
            // L1 three-column layout: I-Cache(38%) | D-Cache(38%) | CPI(24%)
            let i_w = content_area.width * 38 / 100;
            let d_w = content_area.width * 38 / 100;
            let cpi_x = content_area.x + i_w + d_w;

            let fields_y0 = content_area.y + 1;
            let fields_y1 = content_area.y + content_area.height.saturating_sub(7);
            if me.row >= fields_y0 && me.row < fields_y1 {
                let row_idx = (me.row - fields_y0) as usize;
                if me.column >= cpi_x {
                    // CPI panel hover
                    if row_idx < 9 {
                        app.cache.hover_cpi_field = Some(row_idx);
                    }
                } else if let Some(field) = ConfigField::from_list_row(row_idx) {
                    let is_icache = me.column < content_area.x + i_w;
                    app.cache.hover_config_field = Some((is_icache, field));
                }
            }

            let apply_y = content_area.y + content_area.height.saturating_sub(3);
            if me.row == apply_y && me.column < cpi_x {
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
                app.cache.hover_preset_i = check_preset(content_area.x, i_w);
                app.cache.hover_preset_d = check_preset(content_area.x + i_w, d_w);
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

    // H-scrollbar hover (View subtab only) — check both track slots
    if matches!(app.cache.subtab, CacheSubtab::View) {
        let sb_row = app.cache.hscroll_row.get();
        let tracks = app.cache.hscroll_tracks.get();
        if sb_row > 0 && me.row == sb_row {
            for (track_x, track_w) in &tracks {
                if *track_w > 0 && me.column >= *track_x && me.column < track_x + track_w {
                    app.cache.hover_hscrollbar = true;
                    app.cache.hscroll_hover_track_x = *track_x;
                    app.cache.hscroll_hover_track_w = *track_w;
                    break;
                }
            }
        }

        // Legend bar buttons: [FMT] [GROUP]
        let (fmt_y, fmt_x0, fmt_x1) = app.cache.view_fmt_btn.get();
        let (grp_y, grp_x0, grp_x1) = app.cache.view_group_btn.get();
        if fmt_x1 > fmt_x0 && me.row == fmt_y && me.column >= fmt_x0 && me.column < fmt_x1 {
            app.cache.hover_view_fmt = true;
        }
        if grp_x1 > grp_x0 && me.row == grp_y && me.column >= grp_x0 && me.column < grp_x1 {
            app.cache.hover_view_group = true;
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
    let (level_area, header_area, _, content_area, controls_area) = cache_content_area(area);

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
        let show_cfg_btns = matches!(app.cache.subtab, CacheSubtab::Config);
        if x >= 1 && x < 12 { do_export_results(app); return; }
        if show_cfg_btns {
            if x >= 14 && x < 28      { do_import_cfg(app); return; }
            if x >= 30 && x < 44      { do_export_cfg(app); return; }
            if app.cache.selected_level == 0 {
                if x >= 54 && x < 63  { app.cache.scope = CacheScope::ICache; return; }
                if x >= 64 && x < 73  { app.cache.scope = CacheScope::DCache; return; }
                if x >= 74 && x < 80  { app.cache.scope = CacheScope::Both;   return; }
            }
        } else if app.cache.selected_level == 0 {
            if x >= 22 && x < 31      { app.cache.scope = CacheScope::ICache; return; }
            if x >= 32 && x < 41      { app.cache.scope = CacheScope::DCache; return; }
            if x >= 42 && x < 48      { app.cache.scope = CacheScope::Both;   return; }
        }
    }

    // View legend bar button clicks: [FMT] and [GROUP]
    if matches!(app.cache.subtab, CacheSubtab::View) {
        let (fmt_y, fmt_x0, fmt_x1) = app.cache.view_fmt_btn.get();
        let (grp_y, grp_x0, grp_x1) = app.cache.view_group_btn.get();
        if fmt_x1 > fmt_x0 && me.row == fmt_y && me.column >= fmt_x0 && me.column < fmt_x1 {
            app.cache.data_fmt = app.cache.data_fmt.cycle();
            return;
        }
        if grp_x1 > grp_x0 && me.row == grp_y && me.column >= grp_x0 && me.column < grp_x1 {
            use crate::ui::app::CacheDataFmt;
            if app.cache.data_fmt != CacheDataFmt::Float {
                app.cache.data_group = app.cache.data_group.cycle();
            }
            return;
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
    let i_w = content_area.width * 38 / 100;
    let d_w = content_area.width * 38 / 100;
    let cpi_x = content_area.x + i_w + d_w;

    let fields_y0 = content_area.y + 1;
    let fields_y1 = content_area.y + content_area.height.saturating_sub(7);
    if me.row >= fields_y0 && me.row < fields_y1 {
        let row_idx = (me.row - fields_y0) as usize;
        if me.column >= cpi_x {
            // CPI field click: select + start editing
            if row_idx < 9 {
                app.cache.cpi_selected = row_idx;
                app.cache.cpi_edit_buf = app.run.cpi_config.get(row_idx).to_string();
                app.cache.cpi_editing = true;
            }
            return;
        }
        if let Some(field) = ConfigField::from_list_row(row_idx) {
            let is_icache = me.column < content_area.x + i_w;
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
    if me.row == apply_y && me.column < cpi_x {
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
        if let Some(idx) = apply_preset(content_area.x, i_w) {
            app.cache.pending_icache = cache_presets(true)[idx].clone();
            app.cache.config_error = None;
            app.cache.config_status = None;
        }
        if let Some(idx) = apply_preset(content_area.x + i_w, d_w) {
            app.cache.pending_dcache = cache_presets(false)[idx].clone();
            app.cache.config_error = None;
            app.cache.config_status = None;
        }
    }
}

fn apply_l1_config(app: &mut App, keep_history: bool) {
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
    let extra = app.cache.extra_pending.clone();
    if keep_history {
        app.cache.config_status = Some("Config applied (history kept).".to_string());
        let old_istats = std::mem::take(&mut app.run.mem.icache.stats);
        let old_dstats = std::mem::take(&mut app.run.mem.dcache.stats);
        app.run.mem.apply_config(icfg, dcfg, extra);
        app.run.mem.icache.stats.history = old_istats.history;
        app.run.mem.dcache.stats.history = old_dstats.history;
    } else {
        app.cache.config_status = Some("Config applied (stats reset).".to_string());
        app.run.mem.apply_config(icfg, dcfg, extra);
    }
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

fn apply_extra_config(app: &mut App, extra_idx: usize, keep_history: bool) {
    if extra_idx >= app.cache.extra_pending.len() { return; }
    let cfg = app.cache.extra_pending[extra_idx].clone();
    if let Err(msg) = cfg.validate() {
        app.cache.config_error = Some(format!("L{} Cache: {msg}", extra_idx + 2));
        return;
    }
    app.cache.config_error = None;
    if keep_history {
        app.cache.config_status = Some("Config applied (history kept).".to_string());
        let old_stats = if extra_idx < app.run.mem.extra_levels.len() {
            Some(std::mem::take(&mut app.run.mem.extra_levels[extra_idx].stats))
        } else { None };
        if extra_idx < app.run.mem.extra_levels.len() {
            app.run.mem.extra_levels[extra_idx] = crate::falcon::cache::Cache::new(cfg);
            if let Some(s) = old_stats {
                app.run.mem.extra_levels[extra_idx].stats.history = s.history;
            }
        }
    } else {
        app.cache.config_status = Some("Config applied (stats reset).".to_string());
        if extra_idx < app.run.mem.extra_levels.len() {
            app.run.mem.extra_levels[extra_idx] = crate::falcon::cache::Cache::new(cfg);
        }
    }
    app.cache.view_scroll = 0;
    app.cache.stats_scroll = 0;
}





