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
    let border_style = if app.run.hover_imem_bar {
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
    app.run.base_pc
        .saturating_add((app.run.imem_scroll as u32).saturating_mul(4))
}

fn instruction_items(inner: Rect, base: u32, app: &App) -> Vec<ListItem<'static>> {
    let lines = inner.height.saturating_sub(2) as u32;
    let mut items = Vec::new();
    let mut remaining = lines;
    let mut addr = base;

    while remaining > 0 && imem_address_in_range(app, addr) {
        // Show label headers (one per label at this address)
        if let Some(label_names) = app.run.labels.get(&addr) {
            for name in label_names {
                if remaining == 0 { break; }
                items.push(ListItem::new(
                    Line::from(vec![
                        Span::styled(format!("{name}:"), Style::default().fg(Color::Yellow)),
                    ])
                ));
                remaining -= 1;
            }
        }
        if remaining == 0 { break; }
        items.push(instruction_item(app, addr));
        remaining -= 1;
        addr = addr.wrapping_add(4);
    }
    items
}

/// Evaluate a B-type branch condition given current registers.
/// Returns `Some((taken, target))` for branch/jump instructions, `None` otherwise.
fn branch_outcome(word: u32, addr: u32, cpu: &crate::falcon::Cpu) -> Option<(bool, u32)> {
    use crate::falcon::decoder::decode;
    use crate::falcon::instruction::Instruction::*;
    match decode(word) {
        Ok(Beq { rs1, rs2, imm }) => {
            let taken = cpu.x[rs1 as usize] == cpu.x[rs2 as usize];
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Bne { rs1, rs2, imm }) => {
            let taken = cpu.x[rs1 as usize] != cpu.x[rs2 as usize];
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Blt { rs1, rs2, imm }) => {
            let taken = (cpu.x[rs1 as usize] as i32) < (cpu.x[rs2 as usize] as i32);
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Bge { rs1, rs2, imm }) => {
            let taken = (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32);
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Bltu { rs1, rs2, imm }) => {
            let taken = cpu.x[rs1 as usize] < cpu.x[rs2 as usize];
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Bgeu { rs1, rs2, imm }) => {
            let taken = cpu.x[rs1 as usize] >= cpu.x[rs2 as usize];
            Some((taken, addr.wrapping_add(imm as u32)))
        }
        Ok(Jal { imm, .. }) => Some((true, addr.wrapping_add(imm as u32))),
        Ok(Jalr { rs1, imm, .. }) => {
            let target = cpu.x[rs1 as usize].wrapping_add(imm as u32) & !1;
            Some((true, target))
        }
        _ => None,
    }
}

fn instruction_item(app: &App, addr: u32) -> ListItem<'static> {
    let word = app.run.mem.peek32(addr).unwrap_or(0);
    let is_bp = app.run.breakpoints.contains(&addr);
    let bp_marker = if is_bp { "●" } else { " " };
    let is_pc = addr == app.run.cpu.pc;
    let marker = if is_pc { "▶" } else { bp_marker };
    let value = format_u32_value(word, app.run.fmt_mode, app.run.show_signed);
    let disasm = disasm_word(word);

    let exec_suffix = app.run.exec_counts.get(&addr)
        .map(|&n| format!(" ×{n}"))
        .unwrap_or_default();

    let (line_bg, line_fg) = if is_pc {
        (Some(Color::Yellow), Some(Color::Black))
    } else if is_bp {
        (None, Some(Color::Red))
    } else {
        (None, None)
    };

    let base_text = format!("{marker} 0x{addr:08x}: {value}  {disasm}{exec_suffix}");

    // Build span list
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Main instruction text
    if let Some(comment) = app.run.comments.get(&addr) {
        let comment_style = if is_pc {
            Style::default().fg(Color::Rgb(80, 60, 0))
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::raw(base_text));
        spans.push(Span::styled(format!("  # {comment}"), comment_style));
    } else {
        spans.push(Span::raw(base_text));
    }

    // Branch/jump indicator on current PC instruction
    if is_pc {
        if let Some((taken, target)) = branch_outcome(word, addr, &app.run.cpu) {
            let label = app.run.labels.get(&target)
                .and_then(|v| v.first())
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let (arrow, color) = if taken {
                (format!("  → 0x{target:08x}{label}"), Color::Rgb(0, 200, 100))
            } else {
                ("  ↛ (not taken)".to_string(), Color::Rgb(150, 150, 150))
            };
            spans.push(Span::styled(arrow, Style::default().fg(color)));
        }
    }

    let line = Line::from(spans);
    let mut style = Style::default();
    if let Some(bg) = line_bg { style = style.bg(bg); }
    if let Some(fg) = line_fg { style = style.fg(fg); }
    ListItem::new(line).style(style)
}

fn render_instruction_hover(f: &mut Frame, inner: Rect, base: u32, app: &App) {
    if let Some(rect) = hover_highlight(inner, base, app) {
        let bar = Paragraph::new(" ".repeat(rect.width as usize))
            .style(Style::default().bg(Color::Rgb(180, 230, 255)));
        f.render_widget(bar, rect);
    }
}

fn hover_highlight(inner: Rect, base: u32, app: &App) -> Option<Rect> {
    let addr = app.run.hover_imem_addr?;
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
    let style = if app.run.hover_imem_bar {
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

/// Render the execution trace panel (last N executed instructions).
pub(super) fn render_exec_trace(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("Trace (last executed)", Style::default().fg(Color::Magenta)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height as usize;
    let total = app.run.exec_trace.len();
    let skip = total.saturating_sub(visible);

    let items: Vec<ListItem<'static>> = app.run.exec_trace.iter().skip(skip)
        .enumerate()
        .map(|(i, (addr, disasm))| {
            let style = if i + 1 == visible.min(total) {
                // Most recent entry
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let lbl = app.run.labels.get(addr)
                .and_then(|v| v.first())
                .map(|s| format!(" <{s}>"))
                .unwrap_or_default();
            ListItem::new(format!("0x{addr:08x}{lbl}  {disasm}")).style(style)
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
