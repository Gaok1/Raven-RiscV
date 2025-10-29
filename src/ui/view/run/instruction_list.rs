use crate::falcon::memory::Bus;
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use super::App;
use super::formatting::format_u32_value;
use super::instruction_details::disasm_word;
use super::memory::imem_address_in_range;

pub(super) fn render_instruction_memory(f: &mut Frame, area: Rect, app: &App) {
    let block = instruction_block(app);
    let inner = block.inner(area);
    let base = instruction_list_base(app);
    let items = instruction_items(inner, base, app);

    f.render_widget(block, area);
    f.render_widget(List::new(items), inner);
    render_instruction_hover(f, inner, base, app);
    render_instruction_drag_arrow(f, area, app);
}

fn instruction_block(app: &App) -> Block<'static> {
    let border_style = if app.hover_imem_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title("Instruction Memory")
}

fn instruction_list_base(app: &App) -> u32 {
    app.base_pc
        .saturating_add((app.imem_scroll as u32).saturating_mul(4))
}

fn instruction_items(inner: Rect, base: u32, app: &App) -> Vec<ListItem<'static>> {
    let lines = inner.height.saturating_sub(2) as u32;

    (0..lines)
        .map(|i| i * 4)
        .map(|offset| base.wrapping_add(offset))
        .filter(|&addr| imem_address_in_range(app, addr))
        .map(|addr| instruction_item(app, addr))
        .collect()
}

fn instruction_item(app: &App, addr: u32) -> ListItem<'static> {
    let word = app.mem.load32(addr).unwrap_or(0);
    let marker = if addr == app.cpu.pc { "▶" } else { " " };
    let value = format_u32_value(word, app.fmt_mode, app.show_signed);
    let disasm = disasm_word(word);
    let mut item = ListItem::new(format!("{marker} 0x{addr:08x}: {value}  {disasm}"));

    if addr == app.cpu.pc {
        item = item.style(Style::default().bg(Color::Yellow).fg(Color::Black));
    }

    item
}

fn render_instruction_hover(f: &mut Frame, inner: Rect, base: u32, app: &App) {
    if let Some(rect) = hover_highlight(inner, base, app) {
        let bar = Paragraph::new(" ".repeat(rect.width as usize))
            .style(Style::default().bg(Color::Rgb(180, 230, 255)));
        f.render_widget(bar, rect);
    }
}

fn hover_highlight(inner: Rect, base: u32, app: &App) -> Option<Rect> {
    let addr = app.hover_imem_addr?;
    let visible_rows = inner.height.saturating_sub(2) as u32;
    let end_addr = base.saturating_add(visible_rows.saturating_mul(4));

    if addr < base || addr >= end_addr {
        return None;
    }

    let row = (addr.saturating_sub(base) / 4) as u16;
    let seg_width = 2u16.min(inner.width);
    if seg_width == 0 {
        None
    } else {
        Some(Rect::new(inner.x, inner.y + row, seg_width, 1))
    }
}

fn render_instruction_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    let style = if app.hover_imem_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let arrow_area = Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y + area.height / 2,
        1,
        1,
    );
    f.render_widget(Paragraph::new(">").style(style), arrow_area);
}
