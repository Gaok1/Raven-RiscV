use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table};

use super::{App, MemRegion};
use super::formatting::{format_memory_value, format_stale_value, format_u32_value};
use super::registers::reg_name;

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.run.show_bp_list {
        render_bp_list(f, area, app);
    } else if app.run.show_registers {
        render_register_table(f, area, app);
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
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title(format!("Registers  [p]/click=pin{cursor_info}"));
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
        ]).style(Style::default().fg(Color::DarkGray)));
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

// ── Memory view (Data + Stack region) ────────────────────────────────────────

fn render_memory_view(f: &mut Frame, area: Rect, app: &App) {
    let block = memory_block(app);
    let inner = block.inner(area);
    let items = memory_items(inner, app);

    f.render_widget(block, area);
    f.render_widget(List::new(items), inner);
}

fn memory_block(app: &App) -> Block<'static> {
    let title = if app.run.mem_region == MemRegion::Stack {
        let sp = app.run.cpu.x[2];
        format!("Memory [Stack]  SP=0x{sp:08x}  ● dirty")
    } else {
        "Memory  ● = dirty cache (cache → RAM stale)".to_string()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title(title)
}

fn memory_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    let bytes = app.run.mem_view_bytes;
    let lines = inner.height as u32;
    let max = app.run.mem_size.saturating_sub(bytes as usize) as u32;

    // In Stack region: center view on mem_view_addr (SP-aligned) by shifting back half
    let base = if app.run.mem_region == MemRegion::Stack {
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

const PURPLE: Color = Color::Rgb(180, 100, 255);
const STALE_COLOR: Color = Color::Rgb(110, 70, 160);

fn memory_line(app: &App, addr: u32) -> ListItem<'static> {
    let sp = app.run.cpu.x[2];
    let sp_aligned = sp & !(app.run.mem_view_bytes - 1);
    let is_sp = addr == sp_aligned;
    let is_stack = app.run.mem_region == MemRegion::Stack;
    let cache_loc = app.run.mem.data_cache_location(addr);
    let is_dirty = app.run.mem.is_dirty_cached(addr, app.run.mem_view_bytes);

    // Build SP annotation: offset for stack region, or ▶ sp marker otherwise
    let sp_ann = if is_stack {
        let offset = addr as i64 - sp_aligned as i64;
        if is_sp {
            format!("  SP+0 \u{25c0}")
        } else {
            format!("  SP{offset:+}")
        }
    } else if is_sp {
        "  \u{25b6} sp".to_string()
    } else {
        String::new()
    };

    if !is_dirty {
        let val = format_memory_value(app, addr);
        let text = format!("  0x{addr:08x}: {val}{sp_ann}");
        return if is_sp {
            ListItem::new(text).style(Style::default().fg(Color::Yellow))
        } else {
            ListItem::new(text)
        };
    }

    let cache_val = format_memory_value(app, addr);
    let stale_val = format_stale_value(app, addr);
    let level_label = cache_loc.map(|(n, _)| format!("L{n} ")).unwrap_or_default();

    let line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled("\u{25cf} ", Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("{level_label}0x{addr:08x}: "),
            Style::default().fg(PURPLE),
        ),
        ratatui::text::Span::styled(cache_val, Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("  \u{2190} RAM: {stale_val}{sp_ann}"),
            Style::default().fg(STALE_COLOR),
        ),
    ]);
    ListItem::new(line)
}

// ── Breakpoint list view (Feature 10) ─────────────────────────────────────────

fn render_bp_list(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("Breakpoints  F9=toggle", Style::default().fg(Color::Red)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.run.breakpoints.is_empty() {
        f.render_widget(
            Paragraph::new("No breakpoints set.\nF9 to toggle at current PC.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let mut addrs: Vec<u32> = app.run.breakpoints.iter().cloned().collect();
    addrs.sort();
    let items: Vec<ListItem<'static>> = addrs.iter().map(|&addr| {
        let word = app.run.mem.peek32(addr).unwrap_or(0);
        let disasm = super::instruction_details::disasm_word(word);
        let label = app.run.labels.get(&addr)
            .and_then(|v| v.first())
            .map(|l| format!(" <{l}>"))
            .unwrap_or_default();
        let is_pc = addr == app.run.cpu.pc;
        let text = format!("\u{25cf} 0x{addr:08x}{label}  {disasm}");
        let style = if is_pc {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::LightRed)
        };
        ListItem::new(text).style(style)
    }).collect();
    f.render_widget(List::new(items), inner);
}

// Keep the old format_u32_value usage for format_memory_value compatibility
#[allow(dead_code)]
fn _unused_format(app: &App, addr: u32) -> String {
    format_u32_value(app.run.mem.peek32(addr).unwrap_or(0), app.run.fmt_mode, app.run.show_signed)
}
