// ui/view/tlb/entries.rs — Table of installed TLB entries.

use ratatui::{Frame, prelude::*};

use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{Align, Col, DataTable, vertical_scrollbar, visible_window};
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
    let (scroll, _) = visible_window(total, visible, app.tlb.entries_scroll);

    // Reserve a right column for the scrollbar when the list overflows.
    let needs_sb = total > visible;
    let table_area = if needs_sb {
        Rect::new(inner.x, inner.y, inner.width.saturating_sub(1), inner.height)
    } else {
        inner
    };

    let mark = |on: bool, c: &str| {
        if on {
            Span::styled(c.to_string(), style::success())
        } else {
            Span::styled("-".to_string(), style::idle())
        }
    };

    let cols = vec![
        Col::new(" # ", Constraint::Length(4), Align::Left),
        Col::new("VPN", Constraint::Length(9), Align::Left),
        Col::new("PPN", Constraint::Length(9), Align::Left),
        Col::new("RWXU", Constraint::Length(5), Align::Left),
        Col::new("ASID", Constraint::Length(5), Align::Left),
        Col::new(" G", Constraint::Length(2), Align::Left),
        Col::new(" A", Constraint::Length(2), Align::Left),
        Col::new(" D", Constraint::Length(2), Align::Left),
        Col::new("Mp", Constraint::Length(3), Align::Left),
    ];

    let mut table = DataTable::new(cols);
    for (i, e) in mmu.tlb.entries.iter().enumerate().skip(scroll).take(visible) {
        let idx = Line::from(Span::styled(
            format!("{i:>3}"),
            Style::default().fg(theme::BORDER),
        ));
        let row = if e.valid {
            let perms = Line::from(vec![
                mark(e.perms.r, "R"),
                mark(e.perms.w, "W"),
                mark(e.perms.x, "X"),
                mark(e.perms.u, "U"),
            ]);
            vec![
                idx,
                Line::from(Span::styled(format!("0x{:05x}", e.vpn), style::value())),
                Line::from(Span::styled(format!("0x{:06x}", e.ppn), style::value())),
                perms,
                Line::from(Span::styled(format!("{}", e.asid), style::label())),
                Line::from(mark(e.global, "G")),
                Line::from(mark(e.accessed, "A")),
                Line::from(mark(e.dirty, "D")),
                Line::from(mark(e.mask_bits > 0, "M")),
            ]
        } else {
            let dash = || Line::from(Span::styled("\u{2014}", style::idle()));
            vec![
                idx,
                dash(),
                dash(),
                dash(),
                dash(),
                dash(),
                dash(),
                dash(),
                dash(),
            ]
        };
        table = table.row(row);
    }

    f.render_widget(table.build(), table_area);

    if needs_sb {
        let sb_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));
        vertical_scrollbar(f, sb_area, total, visible, scroll);
    }
}
