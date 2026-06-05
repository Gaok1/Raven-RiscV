// ui/view/tlb/entries.rs — Table of installed TLB entries.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Row, Table},
};

use crate::ui::app::App;
use crate::ui::theme;

pub(super) fn render_entries(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "TLB Entries",
            Style::default().fg(theme::LABEL),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem().mmu();
    let total = mmu.tlb.entries.len();
    // Reserve one row for the header.
    let visible = (inner.height as usize).saturating_sub(1).max(1);
    let max_scroll = total.saturating_sub(visible);
    let scroll = app.tlb.entries_scroll.min(max_scroll);
    let rows: Vec<Row> = mmu
        .tlb
        .entries
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, e)| {
            let mark = |on: bool, c: &str| {
                if on {
                    Span::styled(c.to_string(), Style::default().fg(theme::RUNNING))
                } else {
                    Span::styled("-".to_string(), Style::default().fg(theme::IDLE))
                }
            };
            let perms = Line::from(vec![
                mark(e.perms.r, "R"),
                mark(e.perms.w, "W"),
                mark(e.perms.x, "X"),
                mark(e.perms.u, "U"),
            ]);
            if e.valid {
                Row::new(vec![
                    Line::from(Span::styled(
                        format!("{i:>3}"),
                        Style::default().fg(theme::BORDER),
                    )),
                    Line::from(Span::styled(
                        format!("0x{:05x}", e.vpn),
                        Style::default().fg(theme::TEXT),
                    )),
                    Line::from(Span::styled(
                        format!("0x{:06x}", e.ppn),
                        Style::default().fg(theme::TEXT),
                    )),
                    perms,
                    Line::from(Span::styled(
                        format!("{}", e.asid),
                        Style::default().fg(theme::LABEL),
                    )),
                    Line::from(mark(e.global, "G")),
                    Line::from(mark(e.accessed, "A")),
                    Line::from(mark(e.dirty, "D")),
                    Line::from(mark(e.mask_bits > 0, "M")),
                ])
            } else {
                Row::new(vec![
                    Line::from(Span::styled(
                        format!("{i:>3}"),
                        Style::default().fg(theme::BORDER),
                    )),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                    Line::from(Span::styled("—", Style::default().fg(theme::IDLE))),
                ])
            }
        })
        .collect();

    let widths = [
        Constraint::Length(4),  // #
        Constraint::Length(9),  // VPN
        Constraint::Length(9),  // PPN
        Constraint::Length(5),  // RWXU
        Constraint::Length(5),  // ASID
        Constraint::Length(2),  // G
        Constraint::Length(2),  // A
        Constraint::Length(2),  // D
        Constraint::Length(3),  // mega
    ];
    let header = Row::new(vec![
        Span::styled(" # ", Style::default().fg(theme::LABEL)),
        Span::styled("VPN", Style::default().fg(theme::LABEL)),
        Span::styled("PPN", Style::default().fg(theme::LABEL)),
        Span::styled("RWXU", Style::default().fg(theme::LABEL)),
        Span::styled("ASID", Style::default().fg(theme::LABEL)),
        Span::styled(" G", Style::default().fg(theme::LABEL)),
        Span::styled(" A", Style::default().fg(theme::LABEL)),
        Span::styled(" D", Style::default().fg(theme::LABEL)),
        Span::styled("Mp", Style::default().fg(theme::LABEL)),
    ])
    .style(Style::default().fg(theme::LABEL));

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);
}
