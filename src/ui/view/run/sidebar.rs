use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table};

use crate::ui::theme;
use super::{App, MemRegion};
use super::formatting::{format_memory_value, format_stale_value, format_u32_value};
use super::registers::reg_name;

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.run.show_dyn {
        // STORE → show where data was written; LOAD/ALU/branch → show registers
        let show_mem = matches!(app.run.dyn_mem_access, Some((_, _, true)));
        if show_mem {
            render_memory_view(f, area, app);
        } else if app.run.show_float_regs {
            render_float_register_table(f, area, app);
        } else {
            render_register_table(f, area, app);
        }
    } else if app.run.show_registers {
        if app.run.show_float_regs {
            render_float_register_table(f, area, app);
        } else {
            render_register_table(f, area, app);
        }
    } else {
        render_memory_view(f, area, app);
    }
}

// ── Register table ────────────────────────────────────────────────────────────

fn render_register_table(f: &mut Frame, area: Rect, app: &App) {
    // Feature 8: show last write PC in title
    let cursor_info = if app.run.reg_cursor >= 1 && app.run.reg_cursor <= 32 {
        let reg = (app.run.reg_cursor - 1) as usize;
        match app.run.reg_last_write_pc[reg] {
            Some(pc) => format!("  [last write @ 0x{pc:08x}]"),
            None => String::new(),
        }
    } else { String::new() };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title(if app.run.show_dyn {
            format!("Registers [Dyn]{cursor_info}")
        } else {
            format!("Registers  [p]=pin  [Tab]=float{cursor_info}")
        });
    let inner = block.inner(area);
    let rows = build_register_rows(inner, app);
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(0)]).block(block);
    f.render_widget(table, area);
}

fn build_register_rows(inner: Rect, app: &App) -> Vec<Row<'static>> {
    // Total list: 0=PC, 1..=32=x0..x31
    let total = 33usize;
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
        let style = if is_hover { base.bg(Color::Rgb(60, 80, 60)) } else { base };
        rows.push(Row::new(vec![
            Cell::from(pin_label).style(style),
            Cell::from(val).style(style),
        ]));
    }

    // Separator after pinned
    let sep_visual_row = pinned.len();
    if !pinned.is_empty() && visible > pinned.len() {
        rows.push(Row::new(vec![
            Cell::from("───────────────"),
            Cell::from(""),
        ]).style(Style::default().fg(theme::BORDER)));
    }

    // ── Regular scroll section ────────────────────────────────────────────────
    let offset = if pinned.is_empty() { 0 } else { pinned.len() + 1 };
    let max_scroll = total.saturating_sub(visible.saturating_sub(offset));
    let start = app.run.regs_scroll.min(max_scroll);
    let remaining = visible.saturating_sub(rows.len());
    let end = (start + remaining).min(total);

    for (i, index) in (start..end).enumerate() {
        let visual_row = offset + i;
        let is_cursor = index == app.run.reg_cursor;
        let is_hover = hover == Some(visual_row) && visual_row != sep_visual_row;
        let (label, val, age) = register_entry(index, app);
        let is_pinned = if index >= 1 { pinned.contains(&((index - 1) as u8)) } else { false };
        let marker = if is_pinned { "◉ " } else { "  " };
        let full_label = format!("{marker}{label}");
        let base_style = age_style(age);
        let row_style = if is_cursor {
            base_style.bg(Color::Rgb(50, 50, 80))
        } else if is_hover {
            base_style.bg(Color::Rgb(40, 60, 40))
        } else {
            base_style
        };
        rows.push(Row::new(vec![
            Cell::from(full_label).style(row_style),
            Cell::from(val).style(row_style),
        ]));
    }

    rows
}

/// Style based on register age (0 = just changed → bright yellow, fades over steps).
fn age_style(age: u8) -> Style {
    match age {
        0   => Style::default().fg(Color::Yellow),
        1   => Style::default().fg(Color::Rgb(210, 170, 0)),
        2   => Style::default().fg(Color::Rgb(160, 130, 0)),
        3   => Style::default().fg(Color::Rgb(110, 90, 0)),
        _   => Style::default().fg(Color::White),
    }
}

/// Returns (label, value, age).
fn register_entry(index: usize, app: &App) -> (String, String, u8) {
    if index == 0 {
        let age = if app.run.cpu.pc != app.run.prev_pc { 0 } else { 255 };
        let val = format_u32_value(app.run.cpu.pc, app.run.fmt_mode, app.run.show_signed);
        ("PC".to_string(), val, age)
    } else {
        let reg_index = (index - 1) as u8;
        let val = format_u32_value(app.run.cpu.x[reg_index as usize], app.run.fmt_mode, app.run.show_signed);
        (
            format!("x{reg_index:02} ({})", reg_name(reg_index)),
            val,
            app.run.reg_age[reg_index as usize],
        )
    }
}

/// Returns (label, value, age) for pinned register.
fn register_entry_reg(reg_idx: u8, app: &App) -> (String, String, u8) {
    let val = format_u32_value(app.run.cpu.x[reg_idx as usize], app.run.fmt_mode, app.run.show_signed);
    (
        format!("x{reg_idx:02} ({})", reg_name(reg_idx)),
        val,
        app.run.reg_age[reg_idx as usize],
    )
}

// ── Float register table (RV32F) ──────────────────────────────────────────────

fn render_float_register_table(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title("Float Regs (f0–f31)  [Tab]=int regs");
    let inner = block.inner(area);

    let visible = inner.height.saturating_sub(2) as usize;
    let scroll = app.run.regs_scroll.min(32usize.saturating_sub(visible));

    let rows: Vec<Row<'static>> = (0u8..32u8)
        .skip(scroll)
        .take(visible)
        .map(|i| {
            let age   = app.run.f_age[i as usize];
            let bits  = app.run.cpu.f[i as usize];
            let val   = f32::from_bits(bits);
            let label = format!("f{i:02} ({}) ", freg_name_short(i));
            let value = if val.is_nan() {
                "NaN".to_string()
            } else if val.is_infinite() {
                if val.is_sign_positive() { "+Inf".to_string() } else { "-Inf".to_string() }
            } else {
                format!("{val:.6}")
            };
            let style = age_style(age);
            Row::new(vec![
                Cell::from(label).style(style),
                Cell::from(value).style(style),
            ])
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(13), Constraint::Min(0)]).block(block);
    f.render_widget(table, area);
}

fn freg_name_short(i: u8) -> &'static str {
    match i {
        0  => "ft0",  1  => "ft1",  2  => "ft2",  3  => "ft3",
        4  => "ft4",  5  => "ft5",  6  => "ft6",  7  => "ft7",
        8  => "fs0",  9  => "fs1",
        10 => "fa0",  11 => "fa1",  12 => "fa2",  13 => "fa3",
        14 => "fa4",  15 => "fa5",  16 => "fa6",  17 => "fa7",
        18 => "fs2",  19 => "fs3",  20 => "fs4",  21 => "fs5",
        22 => "fs6",  23 => "fs7",  24 => "fs8",  25 => "fs9",
        26 => "fs10", 27 => "fs11",
        28 => "ft8",  29 => "ft9",  30 => "ft10", 31 => "ft11",
        _  => "f?",
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

    let items = memory_items(list_area, app);
    f.render_widget(List::new(items), list_area);

    if let Some(bar) = search_area {
        render_mem_search_bar(f, bar, app);
    }
}

fn render_mem_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(20, 22, 40);
    let q = &app.run.mem_search_query;

    let parsed = u32::from_str_radix(
        q.trim_start_matches("0x").trim_start_matches("0X"), 16
    ).ok();

    let valid_span = if let Some(addr) = parsed {
        Span::styled(
            format!("  →  0x{addr:08X}"),
            Style::default().fg(theme::RUNNING).bg(bg),
        )
    } else if !q.is_empty() {
        Span::styled("  ✗", Style::default().fg(Color::Red).bg(bg))
    } else {
        Span::styled("", Style::default().bg(bg))
    };

    let line = Line::from(vec![
        Span::styled(" Go to: 0x", Style::default().fg(theme::ACCENT).bg(bg).bold()),
        Span::styled(q.clone(), Style::default().fg(theme::LABEL_Y).bg(bg)),
        valid_span,
        Span::styled("  Esc=close  Enter=ok", Style::default().fg(theme::IDLE).bg(bg)),
    ]);

    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(bg)),
        area,
    );

    // Blinking cursor after typed text
    let prefix = " Go to: 0x".len() as u16;
    let cx = (area.x + prefix + q.chars().count() as u16)
        .min(area.x + area.width.saturating_sub(1));
    if area.height > 0 {
        f.set_cursor_position((cx, area.y));
    }
}

fn memory_block(app: &App) -> Block<'static> {
    let title = if app.run.show_dyn {
        if let Some((addr, _, true)) = app.run.dyn_mem_access {
            format!("Memory [Dyn]  @0x{addr:08x}")
        } else {
            "Memory [Dyn]".to_string()
        }
    } else {
        match app.run.mem_region {
            MemRegion::Stack => {
                let sp = app.run.cpu.x[2];
                format!("Memory [Stack]  SP=0x{sp:08x}")
            }
            MemRegion::Access => format!("Memory [R/W]  @0x{:08x}", app.run.mem_view_addr),
            MemRegion::Heap => {
                let hb = app.run.cpu.heap_break;
                format!("Memory [Heap]  HB=0x{hb:08x}")
            }
            _ => "Memory".to_string(),
        }
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title(title)
}

fn memory_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    let bytes = app.run.mem_view_bytes;
    let lines = inner.height as u32;
    let max = app.run.mem_size.saturating_sub(bytes as usize) as u32;

    // Center the view for Stack, R/W (Access), Heap, and Dyn-store modes
    let center = app.run.mem_region == MemRegion::Stack
        || app.run.mem_region == MemRegion::Access
        || app.run.mem_region == MemRegion::Heap
        || (app.run.show_dyn && matches!(app.run.dyn_mem_access, Some((_, _, true))));
    let base = if center {
        let half = lines / 2;
        app.run.mem_view_addr.saturating_sub(half * bytes)
    } else {
        app.run.mem_view_addr
    };

    (0..lines)
        .map(|i| i * bytes)
        .map(|offset| base.wrapping_add(offset))
        .filter(|&addr| addr <= max)
        .map(|addr| memory_line(app, addr))
        .collect()
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
    let sp = app.run.cpu.x[2];
    let sp_aligned = sp & !(app.run.mem_view_bytes - 1);
    let is_sp = addr == sp_aligned;
    let is_stack = app.run.mem_region == MemRegion::Stack;

    let hb = app.run.cpu.heap_break;
    let hb_aligned = hb & !(app.run.mem_view_bytes - 1);
    let is_heap_mode = app.run.mem_region == MemRegion::Heap;
    let is_hb = addr == hb_aligned;

    let cache_loc = if app.run.cache_enabled { app.run.mem.data_cache_location(addr) } else { None };
    let is_dirty  = app.run.cache_enabled && app.run.mem.is_dirty_cached(addr, app.run.mem_view_bytes);

    // Check if any recent memory access overlaps this row's byte range
    let row_end = addr.wrapping_add(app.run.mem_view_bytes);
    let access_highlight = app.run.mem_access_log.iter()
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

    let trailing_ann = if !sp_offset_ann.is_empty() { sp_offset_ann }
                       else { hb_offset_ann };

    // Leading prefix — SP takes priority if both happen to coincide
    let marker: Option<ratatui::text::Span<'static>> = if is_sp {
        Some(ratatui::text::Span::styled(
            "\u{25b6}SP ".to_string(),
            Style::default().fg(theme::PAUSED).bold(),
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
    let row_bg = if is_sp || is_hb { Some(theme::BG_HOVER) } else { None };

    if !is_dirty {
        let val = format_memory_value(app, addr);
        let addr_text = format!("0x{addr:08x}: {val}{trailing_ann}");
        let fg = if is_sp || is_hb {
            marker_fg
        } else if let Some(s) = access_highlight {
            return ListItem::new(format!("  {addr_text}")).style(s);
        } else {
            theme::TEXT
        };
        let line = if let Some(prefix) = marker {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw(" "),
                prefix,
                ratatui::text::Span::styled(addr_text, Style::default().fg(fg)),
            ])
        } else {
            ratatui::text::Line::from(
                ratatui::text::Span::styled(format!("  {addr_text}"), Style::default().fg(fg))
            )
        };
        let mut style = Style::default();
        if let Some(bg) = row_bg { style = style.bg(bg); }
        return ListItem::new(line).style(style);
    }

    let cache_val = format_memory_value(app, addr);
    let stale_val = format_stale_value(app, addr);
    let level_label = cache_loc.map(|(n, _)| format!("L{n} ")).unwrap_or_default();

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
        spans.push(ratatui::text::Span::styled("\u{25cf} ", Style::default().fg(PURPLE).bold()));
    }
    spans.push(ratatui::text::Span::styled(
        format!("{level_label}0x{addr:08x}: "),
        addr_style,
    ));
    spans.push(ratatui::text::Span::styled(cache_val, Style::default().fg(PURPLE).bold()));
    spans.push(ratatui::text::Span::styled(
        format!("  \u{2190} RAM: {stale_val}{trailing_ann}"),
        Style::default().fg(STALE_COLOR),
    ));
    let mut style = Style::default();
    if let Some(bg) = row_bg { style = style.bg(bg); }
    ListItem::new(ratatui::text::Line::from(spans)).style(style)
}

// Keep the old format_u32_value usage for format_memory_value compatibility
#[allow(dead_code)]
fn _unused_format(app: &App, addr: u32) -> String {
    format_u32_value(app.run.mem.peek32(addr).unwrap_or(0), app.run.fmt_mode, app.run.show_signed)
}

// ── ELF Sections viewer ───────────────────────────────────────────────────────

const MAX_LINES_PER_SECTION: usize = 16;

/// Compute the height (in terminal rows) needed by the sections viewer.
fn elf_sections_height(app: &App) -> u16 {
    // border (2) + header line per section + data lines per section
    let mut lines = 2usize; // block border
    for sec in &app.run.elf_sections {
        lines += 1; // section header line
        let data_lines = if sec.bytes.is_empty() {
            1 // .bss: just show a single placeholder line
        } else {
            (sec.bytes.len() / 4).min(MAX_LINES_PER_SECTION)
        };
        lines += data_lines;
    }
    lines.min(40) as u16
}

fn render_elf_sections(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title("ELF Sections");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut items: Vec<ListItem<'static>> = Vec::new();
    for sec in &app.run.elf_sections {
        // Section header line
        let header = format!(
            "{:<10} 0x{:08x}  {} B",
            sec.name, sec.addr, sec.size
        );
        items.push(ListItem::new(header)
            .style(Style::default().fg(theme::LABEL_Y).add_modifier(Modifier::BOLD)));

        if sec.bytes.is_empty() {
            // .bss or no-data section
            items.push(ListItem::new(format!(
                "  0x{:08x}: (zeroed, {} B)", sec.addr, sec.size
            )).style(Style::default().fg(theme::LABEL)));
        } else {
            let chunks = sec.bytes.chunks(4).take(MAX_LINES_PER_SECTION);
            for (i, chunk) in chunks.enumerate() {
                let addr = sec.addr + (i * 4) as u32;
                let mut padded = [0u8; 4];
                for (j, &b) in chunk.iter().enumerate() { padded[j] = b; }
                let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
                let hint = type_hint(chunk);
                items.push(ListItem::new(format!("  0x{addr:08x}: {hex:<11} │ {hint}"))
                    .style(Style::default().fg(theme::TEXT)));
            }
            if sec.bytes.len() / 4 > MAX_LINES_PER_SECTION {
                items.push(ListItem::new(format!(
                    "  … {} more bytes",
                    sec.bytes.len() - MAX_LINES_PER_SECTION * 4
                )).style(Style::default().fg(theme::LABEL)));
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
    chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}
