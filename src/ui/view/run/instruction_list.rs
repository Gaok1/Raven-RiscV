use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use super::App;
use super::formatting::address_hex_width;
use super::memory::{exec_address_in_range, imem_address_in_range};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{SbGeom, vertical_scrollbar};
use crate::ui::view::style;

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

    // One column reserved for the scrollbar, so the PC row's background — and
    // the hover highlight — stop before the track instead of showing through it
    // (see `sidebar::render_register_view`).
    let body = Rect::new(
        list_area.x,
        list_area.y,
        list_area.width.saturating_sub(1),
        list_area.height,
    );
    let items = instruction_items(body, app);

    f.render_widget(block, area);
    f.render_widget(List::new(items), body);

    // How far through the program this window sits, in the same visual rows
    // `imem_scroll` counts — labels and block comments included, so the thumb
    // tracks what is on screen rather than an address arithmetic of its own.
    let total = app.imem_total_visual_rows();
    let viewport = list_area.height as usize;
    let offset = app.run.imem_scroll.min(total.saturating_sub(viewport));
    vertical_scrollbar(f, list_area, total, viewport, offset);
    app.run.imem_sb.set((total > viewport && viewport > 0).then(|| SbGeom {
        start: list_area.y,
        len: list_area.height,
        cross: list_area.x + list_area.width.saturating_sub(1),
        content: total,
        viewport,
        offset,
        max: total - viewport,
    }));

    render_instruction_drag_arrow(f, area, app);

    if let Some(bar) = search_area {
        render_imem_search_bar(f, bar, app);
    }
}

fn render_imem_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(20, 22, 40);
    let q = &app.run.imem_search_query;

    let match_count = if q.is_empty() {
        0
    } else {
        app.run.imem_search_match_count
    };

    let result_span = if q.is_empty() {
        Span::styled("", Style::default().bg(bg))
    } else if match_count > 0 {
        Span::styled(
            format!(
                "  →  {match_count} match{}",
                if match_count == 1 { "" } else { "es" }
            ),
            style::success().bg(bg),
        )
    } else {
        Span::styled("  ✗ no match", Style::default().fg(Color::Red).bg(bg))
    };

    let line = Line::from(vec![
        Span::styled(" Label: ", Style::default().fg(theme::ACCENT).bg(bg).bold()),
        Span::styled(q.clone(), Style::default().fg(theme::LABEL_Y).bg(bg)),
        result_span,
        Span::styled("  [ctrl+v] paste", style::idle().bg(bg)),
        Span::styled("  [esc] close", style::idle().bg(bg)),
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
    let border = if app.run.hover_imem_bar {
        theme::HOVER_BG
    } else {
        theme::BORDER
    };
    panel::panel_state("Instruction Memory", "pc", PanelKind::Custom(border))
}

fn instruction_items(inner: Rect, app: &App) -> Vec<ListItem<'static>> {
    if let Some(region) = app.active_imem_exec_region() {
        return instruction_items_dynamic(inner, app, region.start, region.end);
    }

    // imem_scroll is now in visual rows; compute the starting address + how many
    // header rows (block_comment/labels) to skip at the first block.
    let (mut addr, mut skip) = app.imem_addr_skip_for_scroll();
    let lines = inner.height as u32;
    let mut items = Vec::new();
    let mut remaining = lines;

    while remaining > 0 && imem_address_in_range(app, addr) {
        // Block comment separator
        if let Some(bc) = app.session.block_comments.get(&addr) {
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
        if let Some(label_names) = app.session.labels.get(&addr) {
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
        addr = addr.wrapping_add(app.instruction_width_at(u64::from(addr)) as u32);
    }
    items
}

fn instruction_items_dynamic(
    inner: Rect,
    app: &App,
    start: u32,
    end: u32,
) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();
    let mut addr = start.saturating_add((app.run.imem_scroll as u32).saturating_mul(4));
    if addr >= end {
        addr = end.saturating_sub(4);
    }

    for _ in 0..inner.height {
        if !exec_address_in_range(app, addr) || addr >= end {
            break;
        }
        items.push(instruction_item(app, addr));
        addr = addr.wrapping_add(app.instruction_width_at(u64::from(addr)) as u32);
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

/// The class badge for an instruction, named by the backend's own decoder.
///
/// RV32 names encoding formats (`R`, `I`, `S`…) and toy16 names semantics
/// (`Load`, `ALU`…); both are just strings here, so the badge is right for
/// whichever architecture is loaded. Colour is keyed off the name so a given
/// class stays one colour down a listing.
fn type_badge(app: &App, addr: u32) -> (String, Color) {
    let Some(class) = instruction_class(app, addr) else {
        return ("? ".to_string(), Color::DarkGray);
    };
    // Short classes are shown whole; longer ones are initialled so the badge
    // never crowds out the disassembly.
    //
    // No brackets: the class is data, and the bracket is reserved for keyboard
    // keys. Colour already does the work brackets were doing — it separates the
    // badge from the disassembly *and* groups the classes by family, which the
    // brackets never did. Padded to two columns so the addresses stay aligned
    // whether the class is one letter or two.
    let text = if class.len() <= 2 {
        format!("{class:<2}")
    } else {
        format!("{:<2}", class.chars().next().unwrap_or('?'))
    };
    (text, class_color(class))
}

fn instruction_class(app: &App, addr: u32) -> Option<&'static str> {
    app.code()?
        .inspect(u64::from(addr), &app.instruction_bytes_at(u64::from(addr)))
        .map(|info| info.class)
}

fn class_color(class: &str) -> Color {
    match class {
        "R" | "A" | "ALU" => Color::LightRed,
        "I" | "F" => Color::LightBlue,
        "S" | "Store" => Color::LightYellow,
        "B" | "Load" => Color::LightGreen,
        "U" | "I/O" => Color::LightMagenta,
        "J" | "Control" => Color::LightCyan,
        _ => Color::Gray,
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
/// The row the details panel is pinned to (click-selected). Stronger than the
/// hover wash so the pin survives the mouse moving away.
const SELECTED_BG: Color = theme::BG_RAISED;

fn instruction_item(app: &App, addr: u32) -> ListItem<'static> {
    let width = address_hex_width(app);
    let word = app
        .memory()
        .map_or(0, |memory| memory.peek_word(u64::from(addr), 4)) as u32;
    let is_bp = app.session.breakpoints.contains(&addr);
    let is_pc = u64::from(addr) == app.program_counter();
    let is_selected = !is_pc && app.run.details_addr == Some(addr);
    let is_hover = !is_pc && app.run.hover_imem_addr == Some(addr);

    // Collect non-selected harts that are currently at this address.
    let peer_ids = app.peer_hart_ids_at(addr);

    // A gutter, not an inline glyph: the PC bar sits in its own two columns at
    // the far left, so the disassembly below it stays aligned instead of being
    // shoved one column right on whichever row happens to be current.
    let marker: Vec<Span<'static>> = vec![
        if is_pc {
            Span::styled("▌", Style::default().fg(theme::ACCENT).bold())
        } else {
            Span::raw(" ")
        },
        if is_bp {
            Span::styled("●", Style::default().fg(theme::DANGER).bold())
        } else {
            Span::raw(" ")
        },
    ];

    // Through the backend's disassembler; undecodable bytes show as raw hex
    // rather than an invented mnemonic.
    let disasm = app
        .disassemble_at(u64::from(addr))
        .unwrap_or_else(|| format!("0x{word:08x}"));

    let exec_count = app.session.exec_counts.get(&addr).copied().unwrap_or(0);

    // A tinted row rather than black-on-yellow. The old highlight was the
    // loudest thing on the screen and stole the amber that mnemonics and the
    // paused state use, so "which line is current" and "this is a warning" read
    // as the same signal.
    let (line_bg, line_fg) = if is_pc {
        (Some(theme::SEL_ROW_BG), None)
    } else if is_bp {
        (None, Some(theme::DANGER))
    } else {
        (None, None)
    };

    let addr_part = format!("0x{addr:0width$x}:  {disasm}");

    // Build span list
    let mut spans: Vec<Span<'static>> = marker;

    // Type badge (before main text) — shown only if enabled
    if app.run.show_instr_type {
        let (badge_text, badge_color) = type_badge(app, addr);
        spans.push(Span::styled(badge_text, Style::default().fg(badge_color)));
        spans.push(Span::raw(" "));
    }

    // Main instruction text
    if let Some(comment) = app.session.comments.get(&addr) {
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
        if let Some((taken, target)) = app
            .rv32()
            .and_then(|rv32| branch_outcome(word, addr, rv32.cpu()))
        {
            let label = app
                .session
                .labels
                .get(&target)
                .and_then(|v| v.first())
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let (arrow, color) = if taken {
                (
                    format!("  \u{2192} 0x{target:0width$x}{label}"),
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

    // Peer-hart PC markers: which other harts sit at this address. Bare and
    // coloured — it is data, and the bracket means a key.
    for id in &peer_ids {
        spans.push(Span::styled(
            format!(" h{id}"),
            Style::default().fg(Color::Cyan),
        ));
    }

    let line = Line::from(spans);
    let mut style = Style::default();
    if is_selected {
        style = style.bg(SELECTED_BG);
    }
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

/// The resize grip on this pane's right edge — only while hovered, since it is
/// drawn over the border itself. See `run::render_sidebar_drag_arrow`.
fn render_instruction_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    if !app.run.hover_imem_bar {
        return;
    }
    let grip = Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y + area.height / 2,
        1,
        1,
    );
    f.render_widget(
        Paragraph::new("┃").style(Style::default().fg(theme::ACCENT)),
        grip,
    );
}

/// Render the execution trace panel (last N executed instructions).
pub(super) fn render_exec_trace(f: &mut Frame, area: Rect, app: &App) {
    let block = panel::panel_state("Trace", "last executed", PanelKind::Plain);
    let inner = render_panel(f, area, block);
    let width = address_hex_width(app);

    let visible = inner.height as usize;
    let total = app.session.exec_trace.len();
    let skip = total.saturating_sub(visible);

    let items: Vec<ListItem<'static>> = app
        .session
        .exec_trace
        .iter()
        .skip(skip)
        .enumerate()
        .map(|(i, (addr, disasm))| {
            let style = if i + 1 == visible.min(total) {
                // Most recent entry
                Style::default().fg(theme::LABEL_Y)
            } else {
                style::label()
            };
            let lbl = app
                .session
                .labels
                .get(addr)
                .and_then(|v| v.first())
                .map(|s| format!(" <{s}>"))
                .unwrap_or_default();
            ListItem::new(format!("0x{addr:0width$x}{lbl}  {disasm}")).style(style)
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
