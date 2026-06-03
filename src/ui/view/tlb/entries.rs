// ui/view/tlb/entries.rs — Table of installed TLB entries.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Row, Table},
};

use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

pub(super) fn render_entries(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("TLB Entries", PanelKind::Plain));
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem.mmu();
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
                    Span::styled(c.to_string(), style::success())
                } else {
                    Span::styled("-".to_string(), style::idle())
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
                    Line::from(Span::styled(format!("0x{:05x}", e.vpn), style::value())),
                    Line::from(Span::styled(format!("0x{:06x}", e.ppn), style::value())),
                    perms,
                    Line::from(Span::styled(format!("{}", e.asid), style::label())),
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
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                    Line::from(Span::styled("—", style::idle())),
                ])
            }
        })
        .collect();

    let widths = [
        Constraint::Length(4), // #
        Constraint::Length(9), // VPN
        Constraint::Length(9), // PPN
        Constraint::Length(5), // RWXU
        Constraint::Length(5), // ASID
        Constraint::Length(2), // G
        Constraint::Length(2), // A
        Constraint::Length(2), // D
        Constraint::Length(3), // mega
    ];
    let header = Row::new(vec![
        Span::styled(" # ", style::label()),
        Span::styled("VPN", style::label()),
        Span::styled("PPN", style::label()),
        Span::styled("RWXU", style::label()),
        Span::styled("ASID", style::label()),
        Span::styled(" G", style::label()),
        Span::styled(" A", style::label()),
        Span::styled(" D", style::label()),
        Span::styled("Mp", style::label()),
    ])
    .style(style::label());

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);
}
