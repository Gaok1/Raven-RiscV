use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Cell, List, ListItem, Row, Table};

use super::App;
use super::formatting::{format_memory_value, format_stale_value, format_u32_value};
use super::registers::reg_name;

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.run.show_stack {
        render_stack_view(f, area, app);
    } else if app.run.show_registers {
        render_register_table(f, area, app);
    } else {
        render_memory_view(f, area, app);
    }
}

// ── Register table ────────────────────────────────────────────────────────────

fn render_register_table(f: &mut Frame, area: Rect, app: &App) {
    let block = register_block();
    let inner = block.inner(area);
    let rows = build_register_rows(inner, app);
    let table = Table::new(rows, [Constraint::Length(14), Constraint::Length(20)]).block(block);
    f.render_widget(table, area);
}

fn register_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Registers  [p]=pin/unpin")
}

fn build_register_rows(inner: Rect, app: &App) -> Vec<Row<'static>> {
    // Total list: 0=PC, 1..=32=x0..x31
    let total = 33usize;
    let visible = inner.height.saturating_sub(2) as usize;
    let pinned = &app.run.pinned_regs;

    let mut rows: Vec<Row<'static>> = Vec::new();

    // ── Pinned registers (always at top) ─────────────────────────────────────
    for &reg_idx in pinned.iter() {
        let (label, value, age) = register_entry_reg(reg_idx, app);
        let pin_label = format!("◉ {label}");
        rows.push(Row::new(vec![
            Cell::from(pin_label).style(age_style(age).add_modifier(Modifier::BOLD)),
            Cell::from(value).style(age_style(age)),
        ]));
    }

    // Separator after pinned
    if !pinned.is_empty() && visible > pinned.len() {
        rows.push(Row::new(vec![
            Cell::from("─────────────"),
            Cell::from(""),
        ]).style(Style::default().fg(Color::DarkGray)));
    }

    // ── Regular scroll section ────────────────────────────────────────────────
    let max_scroll = total.saturating_sub(visible.saturating_sub(pinned.len() + if pinned.is_empty() { 0 } else { 1 }));
    let start = app.run.regs_scroll.min(max_scroll);
    let remaining = visible.saturating_sub(rows.len());
    let end = (start + remaining).min(total);

    for index in start..end {
        let is_cursor = index == app.run.reg_cursor;
        let (label, value, age) = register_entry(index, app);
        let is_pinned = if index >= 1 { pinned.contains(&((index - 1) as u8)) } else { false };
        let marker = if is_pinned { "◉ " } else { "  " };
        let full_label = format!("{marker}{label}");
        let base_style = age_style(age);
        let row_style = if is_cursor {
            base_style.bg(Color::Rgb(50, 50, 80))
        } else {
            base_style
        };
        rows.push(Row::new(vec![
            Cell::from(full_label).style(row_style),
            Cell::from(value).style(row_style),
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

fn register_entry(index: usize, app: &App) -> (String, String, u8) {
    if index == 0 {
        let age = if app.run.cpu.pc != app.run.prev_pc { 0 } else { 255 };
        (
            "PC".to_string(),
            format_u32_value(app.run.cpu.pc, app.run.fmt_mode, app.run.show_signed),
            age,
        )
    } else {
        let reg_index = (index - 1) as u8;
        let value = app.run.cpu.x[reg_index as usize];
        (
            format!("x{reg_index:02} ({})", reg_name(reg_index)),
            format_u32_value(value, app.run.fmt_mode, app.run.show_signed),
            app.run.reg_age[reg_index as usize],
        )
    }
}

fn register_entry_reg(reg_idx: u8, app: &App) -> (String, String, u8) {
    let value = app.run.cpu.x[reg_idx as usize];
    (
        format!("x{reg_idx:02} ({})", reg_name(reg_idx)),
        format_u32_value(value, app.run.fmt_mode, app.run.show_signed),
        app.run.reg_age[reg_idx as usize],
    )
}

// ── Memory view ───────────────────────────────────────────────────────────────

fn render_memory_view(f: &mut Frame, area: Rect, app: &App) {
    let block = memory_block();
    let inner = block.inner(area);
    let items = memory_items(inner, app);

    f.render_widget(block, area);
    f.render_widget(List::new(items), inner);
}

fn memory_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Memory  ● = dirty cache (cache → RAM stale)")
}

fn memory_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    let base = app.run.mem_view_addr;
    let lines = inner.height.saturating_sub(2) as u32;
    let bytes = app.run.mem_view_bytes;
    let max = app.run.mem_size.saturating_sub(bytes as usize) as u32;

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
    let is_dirty = app.run.mem.is_dirty_cached(addr, app.run.mem_view_bytes);
    let is_sp = addr == app.run.cpu.x[2];

    if !is_dirty {
        let text = format!("  0x{addr:08x}: {}", format_memory_value(app, addr));
        return if is_sp {
            ListItem::new(format!("{text}   ▶ sp")).style(Style::default().fg(Color::Yellow))
        } else {
            ListItem::new(text)
        };
    }

    let cache_val = format_memory_value(app, addr);
    let stale_val = format_stale_value(app, addr);
    let sp_suffix = if is_sp { "   ▶ sp" } else { "" };

    let line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled("● ", Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("0x{addr:08x}: "),
            Style::default().fg(PURPLE),
        ),
        ratatui::text::Span::styled(cache_val, Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("  ← RAM: {stale_val}{sp_suffix}"),
            Style::default().fg(STALE_COLOR),
        ),
    ]);
    ListItem::new(line)
}

// ── Stack view ────────────────────────────────────────────────────────────────

fn render_stack_view(f: &mut Frame, area: Rect, app: &App) {
    let sp = app.run.cpu.x[2];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!("Stack  SP=0x{sp:08x}"),
            Style::default().fg(Color::LightBlue),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height.saturating_sub(2) as i32;
    let _bytes = 4u32;
    let sp_aligned = sp & !3; // align to 4 bytes

    // Show visible/2 words below SP, SP itself, visible/2 words above
    let half = visible / 2;
    let items: Vec<ListItem<'static>> = (-half..=half)
        .filter_map(|delta| {
            let addr = (sp_aligned as i64 + delta as i64 * 4) as u64;
            if addr > u32::MAX as u64 { return None; }
            let addr = addr as u32;
            if addr as usize >= app.run.mem_size { return None; }

            let val = app.run.mem.peek32(addr).unwrap_or(0);
            let is_sp_row = addr == sp_aligned;
            let offset = delta * 4;
            let offset_str = if offset == 0 { "  +0".to_string() }
                else if offset > 0 { format!("{offset:+4}") }
                else { format!("{offset:+4}") };

            let style = if is_sp_row {
                Style::default().fg(Color::Black).bg(Color::LightBlue)
            } else {
                Style::default().fg(Color::White)
            };
            let sp_mark = if is_sp_row { " ◀ SP" } else { "" };
            Some(ListItem::new(
                format!("  {offset_str}  0x{addr:08x}: 0x{val:08x}{sp_mark}")
            ).style(style))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
