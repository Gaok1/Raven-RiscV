use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Cell, List, ListItem, Paragraph, Row, Table};

use super::formatting::{format_memory_value, format_register_value, format_stale_value};
use super::{App, MemRegion};
use crate::ui::app::{NO_REG_AGE, RunEditTarget};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{SbGeom, vertical_scrollbar};
use crate::ui::view::style;
use raven_engine::capability::{RegisterEntry, RegisterId};

/// The cursor-suffixed edit buffer to paint in a cell, when it is the target of
/// the open inline editor. `None` means render the cell's value normally.
fn edit_overlay(app: &App, target: Option<RunEditTarget>) -> Option<String> {
    (target.is_some() && app.run.run_edit == target).then(|| format!("{}█", app.run.run_edit_buf))
}

/// Like [`edit_overlay`] for the memory cell at `addr`.
fn mem_edit_overlay(app: &App, addr: u32) -> Option<String> {
    match app.run.run_edit {
        Some(RunEditTarget::Mem { addr: a, .. }) if a == addr => {
            Some(format!("{}█", app.run.run_edit_buf))
        }
        _ => None,
    }
}

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    // STORE → show where data was written; LOAD/ALU/branch → show registers.
    let dyn_shows_memory =
        app.run.show_dyn && matches!(app.session.dyn_mem_access, Some((_, _, true)));
    if dyn_shows_memory || !(app.run.show_dyn || app.run.show_registers) {
        render_memory_view(f, area, app);
    } else if app.visible_register_bank() == 0 {
        render_register_table(f, area, app);
    } else {
        render_secondary_bank_table(f, area, app);
    }
}

// ── Register table ────────────────────────────────────────────────────────────

/// Number of rows the primary table holds: the PC, then every register in the
/// visible bank.
fn primary_row_count(app: &App) -> usize {
    1 + app.visible_register_entries().len()
}

fn render_register_table(f: &mut Frame, area: Rect, app: &App) {
    // The title's right slot says what the table is *showing*: every bank the
    // backend offers, with the visible one lit. The `[P]=pin` / `[Tab]=Float`
    // key hints that used to live here moved to the footer, so a bracket now
    // only ever means a key — and the alternative banks are still advertised,
    // because they are data rather than a hint.
    let mut state: Vec<Span<'static>> = Vec::new();
    if app.run.show_dyn {
        state.push(Span::styled("dyn", style::value().bold()));
    } else {
        let visible = app.visible_register_bank();
        for (i, bank) in app.register_banks().iter().enumerate() {
            if i > 0 {
                state.push(Span::styled(" · ", style::idle()));
            }
            state.push(if i == visible {
                Span::styled(bank.label.to_string(), style::value().bold())
            } else {
                Span::styled(bank.label.to_string(), style::idle())
            });
        }
    }
    if let Some(pc) = cursor_register(app).and_then(|id| app.register_last_write(id)) {
        state.push(Span::styled(
            format!("  last write 0x{pc:08x}"),
            style::label(),
        ));
    }
    let block = panel::panel_state_spans("Registers", state, PanelKind::Plain);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // The rows stop one column short of the scrollbar track.
    //
    // They used to fill the panel, and a selected or hovered row carries a
    // background — which ratatui's `Scrollbar` does not overwrite, since it only
    // paints a foreground glyph. So the highlight showed *through* the bar on
    // whichever row was lit, and the bar looked like it changed colour at
    // random. Reserving the column is what `vertical_scrollbar` asks callers to
    // do for exactly this reason.
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    let rows = build_register_rows(body, app);
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(0)]);
    f.render_widget(table, body);

    // Draggable scrollbar over the regular (non-pinned) section; window math
    // mirrors `build_register_rows`.
    let total = primary_row_count(app);
    let visible = inner.height.saturating_sub(2) as usize;
    let pins = app.run.pinned_regs.len();
    let offset = if pins == 0 { 0 } else { pins + 1 };
    let viewport = visible.saturating_sub(offset);
    if total > viewport && viewport > 0 {
        let max_scroll = total - viewport;
        let start = app.run.regs_scroll.min(max_scroll);
        let sb_area = Rect::new(
            inner.x,
            inner.y + offset as u16,
            inner.width,
            viewport as u16,
        );
        vertical_scrollbar(f, sb_area, total, viewport, start);
        app.run.regs_sb.set(Some(SbGeom {
            start: sb_area.y,
            len: sb_area.height,
            cross: inner.x + inner.width.saturating_sub(1),
            content: total,
            viewport,
            offset: start,
            max: max_scroll,
        }));
    }
}

fn build_register_rows(inner: Rect, app: &App) -> Vec<Row<'static>> {
    // Total list: row 0 = PC, then one row per register in the visible bank.
    let total = primary_row_count(app);
    let visible = inner.height.saturating_sub(2) as usize;
    let pinned = &app.run.pinned_regs;
    let hover = app.run.hover_reg_row;

    let mut rows: Vec<Row<'static>> = Vec::new();

    // ── Pinned registers (always at top) ─────────────────────────────────────
    for (pin_i, &reg_idx) in pinned.iter().enumerate() {
        let is_hover = hover == Some(pin_i);
        let (label, val, age) = register_entry_reg(reg_idx, app);
        let pin_label = format!("◉ {label}");
        let base = age_style(age).add_modifier(Modifier::BOLD);
        let style = if is_hover {
            base.bg(theme::PIN_HOVER_BG)
        } else {
            base
        };
        let target = app.register_edit_target(usize::from(reg_idx) + 1);
        let (val, value_style) = match edit_overlay(app, target) {
            Some(overlay) => (overlay, edit_value_style()),
            None => (val, style),
        };
        rows.push(Row::new(vec![
            Cell::from(pin_label).style(style),
            Cell::from(val).style(value_style),
        ]));
    }

    // Separator after pinned
    let sep_visual_row = pinned.len();
    if !pinned.is_empty() && visible > pinned.len() {
        rows.push(
            Row::new(vec![Cell::from("───────────────"), Cell::from("")])
                .style(Style::default().fg(theme::BORDER)),
        );
    }

    // ── Regular scroll section ────────────────────────────────────────────────
    let offset = if pinned.is_empty() {
        0
    } else {
        pinned.len() + 1
    };
    let max_scroll = total.saturating_sub(visible.saturating_sub(offset));
    let start = app.run.regs_scroll.min(max_scroll);
    let remaining = visible.saturating_sub(rows.len());
    let end = (start + remaining).min(total);

    for (i, index) in (start..end).enumerate() {
        let visual_row = offset + i;
        let is_cursor = index == app.run.reg_cursor;
        let is_hover = hover == Some(visual_row) && visual_row != sep_visual_row;
        let (label, val, age) = register_entry(index, app);
        let is_pinned = if index >= 1 {
            pinned.contains(&((index - 1) as u8))
        } else {
            false
        };
        let marker = if is_pinned { "◉ " } else { "  " };
        let full_label = format!("{marker}{label}");
        let bg = |style: Style| {
            if is_cursor {
                style.bg(theme::SEL_ROW_BG)
            } else if is_hover {
                style.bg(theme::HOVER_ROW_BG)
            } else {
                style
            }
        };
        // The name stays legible even when its value is dimmed as an untouched
        // zero — the row is still there to be found by name.
        let row_style = bg(age_style(age));
        let target = app.register_edit_target(index);
        let (val, val_style) = match edit_overlay(app, target) {
            Some(overlay) => (overlay, edit_value_style()),
            None => {
                let style = bg(value_style(age, &val));
                (val, style)
            }
        };
        rows.push(Row::new(vec![
            Cell::from(full_label).style(row_style),
            Cell::from(val).style(val_style),
        ]));
    }

    rows
}

/// Accent style for a cell currently being inline-edited.
fn edit_value_style() -> Style {
    Style::default().fg(theme::ACCENT).bold()
}

/// Style based on register age (0 = just changed → bright yellow, fades over steps).
fn age_style(age: u8) -> Style {
    match age {
        0 => Style::default().fg(Color::Yellow),
        1 => Style::default().fg(Color::Rgb(210, 170, 0)),
        2 => Style::default().fg(Color::Rgb(160, 130, 0)),
        3 => Style::default().fg(Color::Rgb(110, 90, 0)),
        _ => Style::default().fg(Color::White),
    }
}

/// The register the row cursor is on, or `None` when it sits on the PC row.
fn cursor_register(app: &App) -> Option<RegisterId> {
    let index = app.run.reg_cursor.checked_sub(1)?;
    app.visible_register_entries()
        .get(index)
        .map(|entry| entry.id)
}

/// One register drawn the way its bank asks: a float bank supplies its own
/// decimal, an integer bank defers to the pane's hex/dec/bin setting.
fn entry_value(app: &App, entry: &RegisterEntry) -> String {
    let format = app
        .register_banks()
        .get(entry.id.bank)
        .map(|bank| bank.format)
        .unwrap_or_default();
    entry.formatted(format).unwrap_or_else(|| {
        format_register_value(
            entry.value,
            entry.bits,
            app.run.fmt_mode,
            app.run.show_signed,
        )
    })
}

/// `alias name` when the ISA has both, otherwise just the name — so RV32 shows
/// `a0    x10` while a toy ISA shows a bare `r2`.
///
/// The ABI alias leads because that is the name the code being debugged uses;
/// `x10 (a0)` made the reader skip past the number to reach the word they were
/// actually looking for. Padding the alias keeps the numbers in a column.
fn entry_label(entry: &RegisterEntry) -> String {
    match &entry.alias {
        Some(alias) => format!("{alias:<5} {}", entry.name),
        None => entry.name.clone(),
    }
}

/// Whether a rendered value is zero, in whichever format the sidebar is showing
/// (`0x00000000`, `0`, `0b0000…`).
fn renders_as_zero(value: &str) -> bool {
    let digits = value
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0b");
    !digits.is_empty() && digits.chars().all(|c| c == '0')
}

/// The style for a register value, dimming long-untouched zeros.
///
/// Thirty rows of `0x00000000` at full brightness drown the handful of values
/// that actually matter. A register the program has written keeps its colour
/// even when it wrote a zero — that is a fact about the run, not background.
fn value_style(age: u8, value: &str) -> Style {
    if age > 3 && renders_as_zero(value) {
        style::idle()
    } else {
        age_style(age)
    }
}

/// Returns (label, value, age) for row `index`: 0 is the PC, the rest index the
/// visible bank.
fn register_entry(index: usize, app: &App) -> (String, String, u8) {
    let Some(entry) = index
        .checked_sub(1)
        .and_then(|i| app.visible_register_entries().into_iter().nth(i))
    else {
        let pc = app.registers().map_or(0, |file| file.program_counter());
        let age = if pc != u64::from(app.session.prev_pc) {
            0
        } else {
            NO_REG_AGE
        };
        let bits = app.register_banks().first().map_or(32, |bank| bank.bits);
        let val = format_register_value(pc, bits, app.run.fmt_mode, app.run.show_signed);
        return ("PC".to_string(), val, age);
    };
    let age = app.register_age(entry.id);
    (entry_label(&entry), entry_value(app, &entry), age)
}

/// Returns (label, value, age) for a pinned register.
fn register_entry_reg(reg_idx: u8, app: &App) -> (String, String, u8) {
    let Some(entry) = app
        .visible_register_entries()
        .into_iter()
        .nth(usize::from(reg_idx))
    else {
        return (format!("x{reg_idx:02}"), String::new(), NO_REG_AGE);
    };
    let age = app.register_age(entry.id);
    (entry_label(&entry), entry_value(app, &entry), age)
}

// ── Float register table (RV32F) ──────────────────────────────────────────────

/// Any bank other than the primary one, drawn as a plain scrolling list.
///
/// No pins and no PC row: those belong to the bank the ISA executes against.
/// The heading is whatever the backend calls the bank, so this is the same code
/// for RV32's float file as for a flag bank on an 8-bit ISA.
fn render_secondary_bank_table(f: &mut Frame, area: Rect, app: &App) {
    let banks = app.register_banks();
    let bank_index = app.visible_register_bank();
    let Some(bank) = banks.get(bank_index) else {
        return;
    };
    let entries = app.visible_register_entries();
    let total = entries.len();
    let block = panel::panel_state(
        bank.label,
        format!("{total} entries"),
        PanelKind::Plain,
    );
    let inner = block.inner(area);

    let visible = inner.height.saturating_sub(2) as usize;
    let scroll = app.run.regs_scroll.min(total.saturating_sub(visible));

    let rows: Vec<Row<'static>> = entries
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|entry| {
            let age = app.register_age(entry.id);
            let style = age_style(age);
            let label = format!("{} ", entry_label(entry));
            let (value, value_style) =
                match edit_overlay(app, Some(RunEditTarget::Register(entry.id))) {
                    Some(overlay) => (overlay, edit_value_style()),
                    None => (entry_value(app, entry), style),
                };
            Row::new(vec![
                Cell::from(label).style(style),
                Cell::from(value).style(value_style),
            ])
        })
        .collect();

    // Same reserved column as the primary table, for the same reason: a row
    // background must not show through the scrollbar track.
    f.render_widget(block, area);
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    let table = Table::new(rows, [Constraint::Length(13), Constraint::Min(0)]);
    f.render_widget(table, body);

    // Draggable scrollbar (shares `regs_scroll` with the primary table).
    if total > visible && visible > 0 {
        let max_scroll = total - visible;
        let sb_area = Rect::new(inner.x, inner.y, inner.width, visible as u16);
        vertical_scrollbar(f, sb_area, total, visible, scroll);
        app.run.regs_sb.set(Some(SbGeom {
            start: sb_area.y,
            len: sb_area.height,
            cross: inner.x + inner.width.saturating_sub(1),
            content: total,
            viewport: visible,
            offset: scroll,
            max: max_scroll,
        }));
    }
}

fn freg_name_short(i: u8) -> &'static str {
    match i {
        0 => "ft0",
        1 => "ft1",
        2 => "ft2",
        3 => "ft3",
        4 => "ft4",
        5 => "ft5",
        6 => "ft6",
        7 => "ft7",
        8 => "fs0",
        9 => "fs1",
        10 => "fa0",
        11 => "fa1",
        12 => "fa2",
        13 => "fa3",
        14 => "fa4",
        15 => "fa5",
        16 => "fa6",
        17 => "fa7",
        18 => "fs2",
        19 => "fs3",
        20 => "fs4",
        21 => "fs5",
        22 => "fs6",
        23 => "fs7",
        24 => "fs8",
        25 => "fs9",
        26 => "fs10",
        27 => "fs11",
        28 => "ft8",
        29 => "ft9",
        30 => "ft10",
        31 => "ft11",
        _ => "f?",
    }
}

// ── Memory view (Data + Stack region) ────────────────────────────────────────

fn render_memory_view(f: &mut Frame, area: Rect, app: &App) {
    let block = memory_block(app);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Reserve 1 line at the top for the search bar when open
    let (search_area, list_area) = if app.run.mem_search_open && inner.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner)
    };

    // One column reserved for the bar, so a highlighted row cannot bleed under
    // it (see `render_register_view`).
    let body = Rect::new(
        list_area.x,
        list_area.y,
        list_area.width.saturating_sub(1),
        list_area.height,
    );
    let items = memory_items(body, app);
    f.render_widget(List::new(items), body);

    // Where this window sits in RAM. Counted in rows of `mem_view_bytes`, which
    // is the unit the wheel and the arrow keys already move in.
    let bytes = app.run.mem_view_bytes.max(1) as usize;
    let total = app.session.mem_size / bytes;
    let viewport = list_area.height as usize;
    let base = app.visible_memory_base_addr(Some(list_area.height as u32)) as usize;
    let offset = (base / bytes).min(total.saturating_sub(viewport));
    vertical_scrollbar(f, list_area, total, viewport, offset);
    app.run.mem_sb.set((total > viewport && viewport > 0).then(|| SbGeom {
        start: list_area.y,
        len: list_area.height,
        cross: list_area.x + list_area.width.saturating_sub(1),
        content: total,
        viewport,
        offset,
        max: total - viewport,
    }));

    if let Some(bar) = search_area {
        render_mem_search_bar(f, bar, app);
    }
}

fn render_mem_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(20, 22, 40);
    let q = &app.run.mem_search_query;

    let parsed = u32::from_str_radix(q.trim_start_matches("0x").trim_start_matches("0X"), 16).ok();

    let valid_span = if let Some(addr) = parsed {
        Span::styled(format!("  →  0x{addr:08X}"), style::success().bg(bg))
    } else if !q.is_empty() {
        Span::styled("  ✗", Style::default().fg(Color::Red).bg(bg))
    } else {
        Span::styled("", Style::default().bg(bg))
    };

    let line = Line::from(vec![
        Span::styled(
            " Go to: 0x",
            Style::default().fg(theme::ACCENT).bg(bg).bold(),
        ),
        Span::styled(q.clone(), Style::default().fg(theme::LABEL_Y).bg(bg)),
        valid_span,
        // A modal prompt is the one place besides the footer where keys belong:
        // they are the *current* context and the footer still shows the tab's.
        // Same bracket notation, so the convention holds.
        Span::styled(
            "  [ctrl+v] paste  [esc] close  [enter] ok",
            style::idle().bg(bg),
        ),
    ]);

    f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);

    // Blinking cursor after typed text
    let prefix = " Go to: 0x".len() as u16;
    let cx =
        (area.x + prefix + q.chars().count() as u16).min(area.x + area.width.saturating_sub(1));
    if area.height > 0 {
        f.set_cursor_position((cx, area.y));
    }
}

fn memory_block(app: &App) -> Block<'static> {
    let base_addr = app.visible_memory_base_addr(None);
    let section = memory_title_section(app, base_addr);
    let accent = memory_accent_color(app, section);
    // Region and address are state, so they belong in the title's right slot —
    // and the region loses its brackets, which mean "key" everywhere now.
    panel::panel_state(
        "Memory",
        format!("{section}  0x{base_addr:08x}"),
        PanelKind::Custom(accent),
    )
}

fn memory_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    let base = app.visible_memory_base_addr(Some(inner.height as u32));
    let bytes = app.run.mem_view_bytes;
    let lines = inner.height as u32;
    let max = app.session.mem_size.saturating_sub(bytes as usize) as u32;

    (0..lines)
        .map(|i| i * bytes)
        .map(|offset| base.wrapping_add(offset))
        .filter(|&addr| addr <= max)
        .map(|addr| memory_line(app, addr))
        .collect()
}

fn memory_title_section<'a>(app: &'a App, addr: u32) -> &'a str {
    classify_memory_section(app, addr)
}

/// The stack pointer and program break as RV32 reports them. Both are RV32
/// ideas — a backend with neither simply never paints those markers, which is
/// what a zero here means.
fn stack_pointer(app: &App) -> u32 {
    app.rv32().map_or(0, |rv32| rv32.cpu().x[2])
}

fn heap_break(app: &App) -> u32 {
    app.rv32().map_or(0, |rv32| rv32.cpu().heap_break)
}

fn classify_memory_section<'a>(app: &'a App, addr: u32) -> &'a str {
    let sp_aligned = stack_pointer(app) & !(app.run.mem_view_bytes.saturating_sub(1));
    if addr >= sp_aligned && (addr as usize) < app.session.mem_size {
        return "stack";
    }

    for section in &app.session.elf_sections {
        let end = section.addr.saturating_add(section.size);
        if addr >= section.addr && addr < end {
            return section.name.as_str();
        }
    }

    let data_base = app
        .editor
        .last_ok_data_base
        .unwrap_or(app.session.data_base);
    let data_len = app
        .editor
        .last_ok_data
        .as_ref()
        .map(|bytes| bytes.len() as u32)
        .unwrap_or(0);
    let bss_size = app.editor.last_ok_bss_size.unwrap_or(0);
    let data_end = data_base.saturating_add(data_len);
    let bss_end = data_end.saturating_add(bss_size);

    if addr >= app.session.base_pc && super::memory::imem_address_in_range(app, addr) {
        return ".text";
    }
    if addr >= data_base && addr < data_end {
        return ".data";
    }
    if addr >= data_end && addr < bss_end {
        return ".bss";
    }
    if addr >= app.session.heap_start && addr < heap_break(app) {
        return "heap";
    }

    "free"
}

fn memory_accent_color(_app: &App, section: &str) -> Color {
    match section {
        ".text" => theme::CACHE_I,
        ".data" | ".rodata" => theme::CACHE_D,
        s if s.starts_with(".rodata.") => theme::CACHE_D,
        s if s.starts_with(".data.") => theme::CACHE_D,
        ".bss" => theme::PAUSED,
        s if s.starts_with(".bss.") => theme::PAUSED,
        "heap" => theme::RUNNING,
        "stack" => theme::ACCENT,
        "free" => theme::LABEL,
        _ => theme::ACCENT,
    }
}

const PURPLE: Color = theme::DIRTY;
const STALE_COLOR: Color = theme::DIRTY_DIM;

/// Style for recently accessed memory (cyan fade, disappears after 3 steps).
fn mem_age_style(age: u8) -> Option<Style> {
    match age {
        0 => Some(Style::default().fg(Color::Cyan)),
        1 => Some(Style::default().fg(Color::Rgb(0, 180, 180))),
        2 => Some(Style::default().fg(Color::Rgb(0, 110, 110))),
        _ => None,
    }
}

const HEAP_COLOR: Color = Color::Rgb(80, 200, 120);

fn memory_line(app: &App, addr: u32) -> ListItem<'static> {
    // When this cell is the open inline editor, paint the buffer + cursor and
    // skip the usual cache/marker decorations (transient while editing).
    if let Some(overlay) = mem_edit_overlay(app, addr) {
        return ListItem::new(format!("  0x{addr:08x}: {overlay}")).style(edit_value_style());
    }

    let sp = stack_pointer(app);
    let sp_aligned = sp & !(app.run.mem_view_bytes - 1);
    let is_sp = addr == sp_aligned;
    let is_stack = app.run.mem_region == MemRegion::Stack;

    let hb = heap_break(app);
    let hb_aligned = hb & !(app.run.mem_view_bytes - 1);
    let is_heap_mode = app.run.mem_region == MemRegion::Heap;
    let is_hb = addr == hb_aligned;

    let cache_presence = if app.session.cache_enabled {
        app.rv32()
            .and_then(|rv32| cache_presence_label(rv32.mem(), addr))
    } else {
        None
    };
    let data_cache_loc = if app.session.cache_enabled {
        app.rv32()
            .and_then(|rv32| rv32.mem().data_cache_location(addr))
    } else {
        None
    };
    let is_dirty = app.session.cache_enabled
        && app
            .rv32()
            .is_some_and(|rv32| rv32.mem().is_dirty_cached(addr, app.run.mem_view_bytes));

    // Check if any recent memory access overlaps this row's byte range
    let row_end = addr.wrapping_add(app.run.mem_view_bytes);
    let access_highlight = app
        .session
        .mem_access_log
        .iter()
        .filter(|(a, s, _)| {
            let end = a.wrapping_add(*s);
            *a < row_end && end > addr
        })
        .map(|(_, _, age)| *age)
        .min()
        .and_then(mem_age_style);

    // SP offset annotation (trailing, only for non-SP rows in stack view)
    let sp_offset_ann = if is_stack && !is_sp {
        let offset = addr as i64 - sp_aligned as i64;
        format!("  SP{offset:+}")
    } else {
        String::new()
    };

    // HB offset annotation (trailing, only for non-HB rows in heap view)
    let hb_offset_ann = if is_heap_mode && !is_hb {
        let offset = addr as i64 - hb_aligned as i64;
        format!("  HB{offset:+}")
    } else {
        String::new()
    };

    let trailing_ann = if !sp_offset_ann.is_empty() {
        sp_offset_ann
    } else {
        hb_offset_ann
    };

    // Leading prefix — SP takes priority if both happen to coincide
    let marker: Option<ratatui::text::Span<'static>> = if is_sp {
        Some(ratatui::text::Span::styled(
            "\u{25b6}SP ".to_string(),
            style::warning().bold(),
        ))
    } else if is_hb {
        Some(ratatui::text::Span::styled(
            "\u{25b6}HB ".to_string(),
            Style::default().fg(HEAP_COLOR).bold(),
        ))
    } else {
        None
    };

    // Row fg and background
    let marker_fg = if is_sp { theme::PAUSED } else { HEAP_COLOR };
    let row_bg = if is_sp || is_hb {
        Some(theme::BG_HOVER)
    } else {
        None
    };

    if !is_dirty {
        let val = format_memory_value(app, addr);
        let cache_label = cache_presence
            .as_deref()
            .map(|label| format!("{label} "))
            .unwrap_or_default();
        let addr_text = format!("{cache_label}0x{addr:08x}: {val}{trailing_ann}");
        let fg = if is_sp || is_hb {
            marker_fg
        } else if let Some(s) = access_highlight {
            return ListItem::new(format!("  {addr_text}")).style(s);
        } else {
            if cache_presence.is_some() {
                PURPLE
            } else {
                theme::TEXT
            }
        };
        let line = if let Some(prefix) = marker {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw(" "),
                prefix,
                ratatui::text::Span::styled(addr_text, Style::default().fg(fg)),
            ])
        } else if cache_presence.is_some() {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("\u{25cf} ", Style::default().fg(PURPLE).bold()),
                ratatui::text::Span::styled(addr_text, Style::default().fg(fg)),
            ])
        } else {
            ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("  {addr_text}"),
                Style::default().fg(fg),
            ))
        };
        let mut style = Style::default();
        if let Some(bg) = row_bg {
            style = style.bg(bg);
        }
        return ListItem::new(line).style(style);
    }

    let cache_val = format_memory_value(app, addr);
    let stale_val = format_stale_value(app, addr);
    let level_label = cache_presence
        .as_deref()
        .map(|label| format!("{label} "))
        .unwrap_or_else(|| {
            data_cache_loc
                .map(|(n, _)| format!("D{n} "))
                .unwrap_or_default()
        });

    let addr_style = if is_sp || is_hb {
        Style::default().fg(PURPLE)
    } else {
        access_highlight.unwrap_or(Style::default().fg(PURPLE))
    };
    let mut spans: Vec<ratatui::text::Span<'static>> = Vec::new();
    if let Some(prefix) = marker {
        spans.push(ratatui::text::Span::raw(" "));
        spans.push(prefix);
    } else {
        spans.push(ratatui::text::Span::styled(
            "\u{25cf} ",
            Style::default().fg(PURPLE).bold(),
        ));
    }
    spans.push(ratatui::text::Span::styled(
        format!("{level_label}0x{addr:08x}: "),
        addr_style,
    ));
    spans.push(ratatui::text::Span::styled(
        cache_val,
        Style::default().fg(PURPLE).bold(),
    ));
    spans.push(ratatui::text::Span::styled(
        format!("  \u{2190} RAM: {stale_val}{trailing_ann}"),
        Style::default().fg(STALE_COLOR),
    ));
    let mut style = Style::default();
    if let Some(bg) = row_bg {
        style = style.bg(bg);
    }
    ListItem::new(ratatui::text::Line::from(spans)).style(style)
}

fn cache_presence_label(mem: &crate::falcon::cache::CacheController, addr: u32) -> Option<String> {
    let mut labels: Vec<String> = Vec::new();

    if let Some(level) = mem.instruction_cache_location(addr) {
        if level == 1 {
            labels.push("I1".to_string());
        } else {
            labels.push(format!("U{level}"));
        }
    }

    if let Some((level, _dirty)) = mem.data_cache_location(addr) {
        let label = if level == 1 {
            "D1".to_string()
        } else {
            format!("U{level}")
        };
        if !labels.iter().any(|existing| existing == &label) {
            labels.push(label);
        }
    }

    if labels.is_empty() {
        None
    } else {
        Some(labels.join("/"))
    }
}

// ── ELF Sections viewer ───────────────────────────────────────────────────────

const MAX_LINES_PER_SECTION: usize = 16;

/// Compute the height (in terminal rows) needed by the sections viewer.
fn elf_sections_height(app: &App) -> u16 {
    // border (2) + header line per section + label lines + data lines per section
    let mut lines = 2usize; // block border
    for sec in &app.session.elf_sections {
        lines += 1; // section header line
        if sec.bytes.is_empty() {
            // Count any symbol label at the section base address
            lines += app
                .session
                .labels
                .get(&sec.addr)
                .map(|v| v.len())
                .unwrap_or(0);
            lines += 1; // bss placeholder line
        } else {
            let word_count = (sec.bytes.len() / 4).min(MAX_LINES_PER_SECTION);
            // Count symbol labels that fall in this section
            let label_lines: usize = (0..word_count)
                .map(|i| {
                    let addr = sec.addr + (i * 4) as u32;
                    app.session.labels.get(&addr).map(|v| v.len()).unwrap_or(0)
                })
                .sum();
            lines += word_count + label_lines;
        }
    }
    lines.min(50) as u16
}

fn render_elf_sections(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel("ELF Sections", PanelKind::Plain),
    );

    let mut items: Vec<ListItem<'static>> = Vec::new();
    for sec in &app.session.elf_sections {
        // Section header line
        let header = format!("{:<10} 0x{:08x}  {} B", sec.name, sec.addr, sec.size);
        items.push(
            ListItem::new(header).style(
                Style::default()
                    .fg(theme::LABEL_Y)
                    .add_modifier(Modifier::BOLD),
            ),
        );

        if sec.bytes.is_empty() {
            // .bss or no-data section: show symbol labels if any fall inside this range
            if let Some(names) = app.session.labels.get(&sec.addr) {
                for name in names {
                    items.push(
                        ListItem::new(format!("  {name}:"))
                            .style(Style::default().fg(theme::LABEL_Y)),
                    );
                }
            }
            items.push(
                ListItem::new(format!("  0x{:08x}: (zeroed, {} B)", sec.addr, sec.size))
                    .style(style::label()),
            );
        } else {
            let chunks = sec.bytes.chunks(4).take(MAX_LINES_PER_SECTION);
            for (i, chunk) in chunks.enumerate() {
                let addr = sec.addr + (i * 4) as u32;
                // Symbol label at this address
                if let Some(names) = app.session.labels.get(&addr) {
                    for name in names {
                        items.push(
                            ListItem::new(format!("  {name}:"))
                                .style(Style::default().fg(theme::LABEL_Y)),
                        );
                    }
                }
                let hex: String = chunk
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let hint = type_hint(chunk);
                items.push(
                    ListItem::new(format!("  0x{addr:08x}: {hex:<11} │ {hint}"))
                        .style(style::value()),
                );
            }
            if sec.bytes.len() / 4 > MAX_LINES_PER_SECTION {
                items.push(
                    ListItem::new(format!(
                        "  … {} more bytes",
                        sec.bytes.len() - MAX_LINES_PER_SECTION * 4
                    ))
                    .style(style::label()),
                );
            }
        }
    }

    f.render_widget(List::new(items), inner);
}

/// Classify a 1-4 byte chunk for display hint.
fn type_hint(chunk: &[u8]) -> String {
    if chunk.len() == 4 {
        let mut b = [0u8; 4];
        b.copy_from_slice(chunk);
        // Try f32
        let v = f32::from_le_bytes(b);
        if !v.is_nan() && !v.is_infinite() && (v == 0.0 || (v.abs() > 1e-30 && v.abs() < 1e30)) {
            return format!("{v:.4} (f32)");
        }
    }
    // Try ASCII
    if chunk.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        let s: String = chunk.iter().map(|&b| b as char).collect();
        return format!("\"{}\"  (ASCII)", s);
    }
    // Default: raw hex
    chunk
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{classify_memory_section, memory_title_section};
    use crate::falcon::program::ElfSection;
    use crate::ui::app::App;

    fn make_app() -> App {
        let mut app = App::new(Some(0x2000));
        app.editor.last_ok_text = Some(vec![0; 4]);
        app.editor.last_ok_data = Some(vec![0; 0x20]);
        app.editor.last_ok_data_base = Some(0x1000);
        app.editor.last_ok_bss_size = Some(0x20);
        app.session.base_pc = 0x0000;
        app.session.data_base = 0x1000;
        app.session.heap_start = 0x1040;
        app.rv32_mut().unwrap().cpu_mut_unjournaled().heap_break = 0x1080;
        app.rv32_mut()
            .unwrap()
            .cpu_mut_unjournaled()
            .write(2, 0x2000);
        app.session.mem_size = 0x2000;
        app.run.mem_view_bytes = 4;
        app
    }

    #[test]
    fn classifies_text_data_bss_heap_and_free_from_real_layout() {
        let app = make_app();
        assert_eq!(classify_memory_section(&app, 0x0004), ".text");
        assert_eq!(classify_memory_section(&app, 0x1008), ".data");
        assert_eq!(classify_memory_section(&app, 0x1024), ".bss");
        assert_eq!(classify_memory_section(&app, 0x1050), "heap");
        assert_eq!(classify_memory_section(&app, 0x1100), "free");
    }

    #[test]
    fn stack_classification_uses_current_sp_boundary() {
        let mut app = make_app();
        app.rv32_mut()
            .unwrap()
            .cpu_mut_unjournaled()
            .write(2, 0x1ff0);
        assert_eq!(classify_memory_section(&app, 0x1ff0), "stack");
        assert_eq!(classify_memory_section(&app, 0x1fec), "free");
    }

    #[test]
    fn elf_sections_override_generic_data_buckets() {
        let mut app = make_app();
        app.session.elf_sections = vec![ElfSection {
            name: ".rodata".to_string(),
            addr: 0x1000,
            size: 0x20,
            bytes: vec![0; 0x20],
        }];
        assert_eq!(memory_title_section(&app, 0x1008), ".rodata");
    }
}
