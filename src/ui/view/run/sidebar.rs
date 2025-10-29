use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Cell, List, ListItem, Row, Table};

use super::App;
use super::formatting::{format_memory_value, format_u32_value};
use super::registers::reg_name;

pub(super) fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.show_registers {
        render_register_table(f, area, app);
    } else {
        render_memory_view(f, area, app);
    }
}

fn render_register_table(f: &mut Frame, area: Rect, app: &App) {
    let block = register_block();
    let inner = block.inner(area);
    let (start, end) = register_visible_range(inner, app);
    let rows = register_rows(start, end, app);
    let table = Table::new(rows, [Constraint::Length(14), Constraint::Length(20)]).block(block);
    f.render_widget(table, area);
}

fn register_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Registers - s:step r:run p:pause Ctrl+R:restart")
}

fn register_visible_range(inner: Rect, app: &App) -> (usize, usize) {
    let visible_rows = inner.height.saturating_sub(2) as usize;
    let total_rows = 33usize; // PC + x0..x31
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let start = app.regs_scroll.min(max_scroll);
    let end = (start + visible_rows).min(total_rows);
    (start, end)
}

fn register_rows(start: usize, end: usize, app: &App) -> Vec<Row<'static>> {
    (start..end).map(|index| register_row(index, app)).collect()
}

fn register_row(index: usize, app: &App) -> Row<'static> {
    let (label, value, changed) = register_entry(index, app);
    let style = if changed {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    Row::new(vec![
        Cell::from(label).style(style),
        Cell::from(value).style(style),
    ])
}

fn register_entry(index: usize, app: &App) -> (String, String, bool) {
    if index == 0 {
        (
            "PC".to_string(),
            format_u32_value(app.cpu.pc, app.fmt_mode, app.show_signed),
            app.cpu.pc != app.prev_pc,
        )
    } else {
        let reg_index = (index - 1) as u8;
        let value = app.cpu.x[reg_index as usize];
        (
            format!("x{reg_index:02} ({})", reg_name(reg_index)),
            format_u32_value(value, app.fmt_mode, app.show_signed),
            value != app.prev_x[reg_index as usize],
        )
    }
}

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
        .title("RAM Memory - s:step r:run p:pause Ctrl+R:restart")
}

fn memory_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    let base = app.mem_view_addr;
    let lines = inner.height.saturating_sub(2) as u32;
    let bytes = app.mem_view_bytes;
    let max = app.mem_size.saturating_sub(bytes as usize) as u32;

    (0..lines)
        .map(|i| i * bytes)
        .map(|offset| base.wrapping_add(offset))
        .filter(|&addr| addr <= max)
        .map(|addr| memory_line(app, addr))
        .collect()
}

fn memory_line(app: &App, addr: u32) -> ListItem<'static> {
    let mut text = format!("0x{addr:08x}: {}", format_memory_value(app, addr));
    if addr == app.cpu.x[2] {
        text.push_str("   ▶ sp");
        ListItem::new(text).style(Style::default().fg(Color::Yellow))
    } else {
        ListItem::new(text)
    }
}
