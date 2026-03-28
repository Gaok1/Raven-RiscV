use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use super::App;
use super::instruction_details::disasm_word;
use super::memory::imem_address_in_range;
use crate::ui::theme;

pub(super) fn render_instruction_memory(f: &mut Frame, area: Rect, app: &App) {
    let block = instruction_block(app);
    let inner = block.inner(area);

    // Reserve 1 line at the top for the label search bar when open
    let (search_area, list_area) = if app.run.imem_search_open && inner.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner)
    };

    // Tell scroll/hover handlers the actual inner height each frame
    app.run.imem_inner_height.set(list_area.height as usize);
    let items = instruction_items(list_area, app);

    f.render_widget(block, area);
    f.render_widget(List::new(items), list_area);
    render_instruction_drag_arrow(f, area, app);

    if let Some(bar) = search_area {
        render_imem_search_bar(f, bar, app);
    }
}

fn render_imem_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(20, 22, 40);
    let q = &app.run.imem_search_query;

    let q_lower = q.to_lowercase();
    let match_count = if q.is_empty() {
        0usize
    } else {
        app.run
            .labels
            .iter()
            .filter(|(addr, labels)| {
                imem_address_in_range(app, **addr)
                    && labels.iter().any(|l| l.to_lowercase().contains(&q_lower))
            })
            .count()
    };

    let result_span = if q.is_empty() {
        Span::styled("", Style::default().bg(bg))
    } else if match_count > 0 {
        Span::styled(
            format!(
                "  →  {match_count} match{}",
                if match_count == 1 { "" } else { "es" }
            ),
            Style::default().fg(theme::RUNNING).bg(bg),
        )
    } else {
        Span::styled("  ✗ no match", Style::default().fg(Color::Red).bg(bg))
    };

    let line = Line::from(vec![
        Span::styled(" Label: ", Style::default().fg(theme::ACCENT).bg(bg).bold()),
        Span::styled(q.clone(), Style::default().fg(theme::LABEL_Y).bg(bg)),
        result_span,
        Span::styled("  Esc/Enter=close", Style::default().fg(theme::IDLE).bg(bg)),
    ]);

    f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);

    let prefix = " Label: ".len() as u16;
    let cx =
        (area.x + prefix + q.chars().count() as u16).min(area.x + area.width.saturating_sub(1));
    if area.height > 0 {
        f.set_cursor_position((cx, area.y));
    }
}

fn instruction_block(app: &App) -> Block<'static> {
    let border_style = if app.run.hover_imem_bar {
        Style::default().fg(theme::HOVER_BG)
    } else {
        Style::default().fg(theme::BORDER)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(BorderType::Rounded)
        .title("Instruction Memory")
}

fn instruction_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    // imem_scroll is now in visual rows; compute the starting address + how many
    // header rows (block_comment/labels) to skip at the first block.
    let (mut addr, mut skip) = app.imem_addr_skip_for_scroll();
    let lines = inner.height as u32;
    let mut items = Vec::new();
    let mut remaining = lines;

    while remaining > 0 && imem_address_in_range(app, addr) {
        // Block comment separator
        if let Some(bc) = app.run.block_comments.get(&addr) {
            if skip > 0 {
                skip -= 1;
            } else {
                let is_hover = app.run.hover_imem_addr == Some(addr);
                let bc_style = Style::default().fg(theme::COMMENT).patch(if is_hover {
                    Style::default().bg(HOVER_BG)
                } else {
                    Style::default()
                });
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!("▌ {bc}"),
                    bc_style,
                )])));
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
            }
        }

        // Label headers
        if let Some(label_names) = app.run.labels.get(&addr) {
            for name in label_names {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                if remaining == 0 {
                    break;
                }
                let is_hover = app.run.hover_imem_addr == Some(addr);
                let lbl_style = Style::default().fg(theme::LABEL_Y).patch(if is_hover {
                    Style::default().bg(HOVER_BG)
                } else {
                    Style::default()
                });
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!("{name}:"),
                    lbl_style,
                )])));
                remaining -= 1;
            }
        }
        if remaining == 0 {
            break;
        }
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

/// Feature 2: instruction type badge color
fn type_badge(word: u32) -> (&'static str, Color) {
    match word & 0x7f {
        0x33 => ("[R]", Color::LightRed),
        0x13 | 0x03 | 0x67 | 0x73 => ("[I]", Color::LightBlue),
        0x23 => ("[S]", Color::LightYellow),
        0x63 => ("[B]", Color::LightGreen),
        0x37 | 0x17 => ("[U]", Color::LightMagenta),
        0x6f => ("[J]", Color::LightCyan),
        _ => ("[ ]", Color::DarkGray),
    }
}

/// Feature 3: heat color based on exec count
fn heat_color(n: u64) -> Color {
    match n {
        0 => Color::DarkGray,
        1..=10 => Color::Cyan,
        11..=100 => Color::Green,
        101..=1000 => Color::Yellow,
        _ => Color::Red,
    }
}

const HOVER_BG: Color = theme::BG_HOVER;

fn instruction_item(app: &App, addr: u32) -> ListItem<'static> {
    let word = app.run.mem.peek32(addr).unwrap_or(0);
    let is_bp = app.run.breakpoints.contains(&addr);
    let is_pc = addr == app.run.cpu.pc;
    let is_hover = !is_pc && app.run.hover_imem_addr == Some(addr);

    // Collect non-selected harts that are currently at this address.
    let peer_ids = app.peer_hart_ids_at(addr);

    let marker = if is_pc && is_bp {
        "●▶"
    } else if is_pc {
        " ▶"
    } else if is_bp {
        "● "
    } else {
        "  "
    };
    let disasm = disasm_word(word);

    let exec_count = app.run.exec_counts.get(&addr).copied().unwrap_or(0);

    let (line_bg, line_fg) = if is_pc {
        (Some(Color::Yellow), Some(Color::Black))
    } else if is_bp {
        (None, Some(Color::Red))
    } else {
        (None, None)
    };

    let addr_part = format!("{marker}0x{addr:08x}:  {disasm}");

    // Build span list
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Type badge (before main text) — shown only if enabled
    if app.run.show_instr_type {
        let (badge_text, badge_color) = type_badge(word);
        spans.push(Span::styled(
            badge_text.to_string(),
            Style::default().fg(badge_color),
        ));
        spans.push(Span::raw(" "));
    }

    // Main instruction text
    if let Some(comment) = app.run.comments.get(&addr) {
        let comment_style = if is_pc {
            Style::default().fg(Color::Rgb(80, 60, 0))
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::raw(addr_part));
        spans.push(Span::styled(format!("  # {comment}"), comment_style));
    } else {
        spans.push(Span::raw(addr_part));
    }

    // Heat coloring on exec count — shown only if enabled
    if app.run.show_exec_count && exec_count > 0 {
        spans.push(Span::styled(
            format!(" \u{d7}{exec_count}"),
            Style::default().fg(heat_color(exec_count)),
        ));
    }

    // Branch/jump indicator on current PC instruction
    if is_pc {
        if let Some((taken, target)) = branch_outcome(word, addr, &app.run.cpu) {
            let label = app
                .run
                .labels
                .get(&target)
                .and_then(|v| v.first())
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let (arrow, color) = if taken {
                (
                    format!("  \u{2192} 0x{target:08x}{label}"),
                    Color::Rgb(0, 200, 100),
                )
            } else {
                (
                    "  \u{219b} (not taken)".to_string(),
                    Color::Rgb(150, 150, 150),
                )
            };
            spans.push(Span::styled(arrow, Style::default().fg(color)));
        }
    }

    // Peer-hart PC markers: show [Hn] for each non-selected hart at this address
    for id in &peer_ids {
        spans.push(Span::styled(
            format!(" [H{id}]"),
            Style::default().fg(Color::Cyan),
        ));
    }

    let line = Line::from(spans);
    let mut style = Style::default();
    if is_hover {
        style = style.bg(HOVER_BG);
    }
    if let Some(bg) = line_bg {
        style = style.bg(bg);
    }
    if let Some(fg) = line_fg {
        style = style.fg(fg);
    }
    ListItem::new(line).style(style)
}

fn render_instruction_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    let style = if app.run.hover_imem_bar {
        Style::default().fg(theme::HOVER_BG)
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
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Trace (last executed)",
            Style::default().fg(theme::ACCENT),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height as usize;
    let total = app.run.exec_trace.len();
    let skip = total.saturating_sub(visible);

    let items: Vec<ListItem<'static>> = app
        .run
        .exec_trace
        .iter()
        .skip(skip)
        .enumerate()
        .map(|(i, (addr, disasm))| {
            let style = if i + 1 == visible.min(total) {
                // Most recent entry
                Style::default().fg(theme::LABEL_Y)
            } else {
                Style::default().fg(theme::LABEL)
            };
            let lbl = app
                .run
                .labels
                .get(addr)
                .and_then(|v| v.first())
                .map(|s| format!(" <{s}>"))
                .unwrap_or_default();
            ListItem::new(format!("0x{addr:08x}{lbl}  {disasm}")).style(style)
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
