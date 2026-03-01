use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table};

use super::App;
use super::formatting::{format_memory_value, format_stale_value, format_u32_value};
use super::registers::reg_name;

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.run.show_bp_list {
        render_bp_list(f, area, app);
    } else if app.run.show_stack {
        render_stack_view(f, area, app);
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
    // Feature 7: 3-column table (name, hex, dec)
    let table = Table::new(rows, [Constraint::Length(14), Constraint::Length(11), Constraint::Length(12)]).block(block);
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
        let (label, hex_val, dec_val, age) = register_entry_reg(reg_idx, app);
        let pin_label = format!("◉ {label}");
        let base = age_style(age).add_modifier(Modifier::BOLD);
        let style = if is_hover { base.bg(Color::Rgb(60, 80, 60)) } else { base };
        let val_style = if is_hover { age_style(age).bg(Color::Rgb(60, 80, 60)) } else { age_style(age) };
        rows.push(Row::new(vec![
            Cell::from(pin_label).style(style),
            Cell::from(hex_val).style(val_style),
            Cell::from(dec_val).style(val_style),
        ]));
    }

    // Separator after pinned
    let sep_visual_row = pinned.len();
    if !pinned.is_empty() && visible > pinned.len() {
        rows.push(Row::new(vec![
            Cell::from("─────────────"),
            Cell::from(""),
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
        let (label, hex_val, dec_val, age) = register_entry(index, app);
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
            Cell::from(hex_val).style(row_style),
            Cell::from(dec_val).style(row_style),
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

/// Feature 7: returns (label, hex_value, dec_value, age)
fn register_entry(index: usize, app: &App) -> (String, String, String, u8) {
    if index == 0 {
        let age = if app.run.cpu.pc != app.run.prev_pc { 0 } else { 255 };
        (
            "PC".to_string(),
            format!("0x{:08x}", app.run.cpu.pc),
            format!("{}", app.run.cpu.pc as i32),
            age,
        )
    } else {
        let reg_index = (index - 1) as u8;
        let value = app.run.cpu.x[reg_index as usize];
        (
            format!("x{reg_index:02} ({})", reg_name(reg_index)),
            format!("0x{value:08x}"),
            format!("{}", value as i32),
            app.run.reg_age[reg_index as usize],
        )
    }
}

/// Feature 7: returns (label, hex_value, dec_value, age) for pinned register
fn register_entry_reg(reg_idx: u8, app: &App) -> (String, String, String, u8) {
    let value = app.run.cpu.x[reg_idx as usize];
    (
        format!("x{reg_idx:02} ({})", reg_name(reg_idx)),
        format!("0x{value:08x}"),
        format!("{}", value as i32),
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
            ListItem::new(format!("{text}   \u{25b6} sp")).style(Style::default().fg(Color::Yellow))
        } else {
            ListItem::new(text)
        };
    }

    let cache_val = format_memory_value(app, addr);
    let stale_val = format_stale_value(app, addr);
    let sp_suffix = if is_sp { "   \u{25b6} sp" } else { "" };

    let line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled("\u{25cf} ", Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("0x{addr:08x}: "),
            Style::default().fg(PURPLE),
        ),
        ratatui::text::Span::styled(cache_val, Style::default().fg(PURPLE).bold()),
        ratatui::text::Span::styled(
            format!("  \u{2190} RAM: {stale_val}{sp_suffix}"),
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
            let sp_mark = if is_sp_row { " \u{25c0} SP" } else { "" };
            Some(ListItem::new(
                format!("  {offset_str}  0x{addr:08x}: 0x{val:08x}{sp_mark}")
            ).style(style))
        })
        .collect();

    f.render_widget(List::new(items), inner);
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
