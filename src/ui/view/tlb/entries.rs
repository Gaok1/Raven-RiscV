// ui/view/tlb/entries.rs — Table of installed TLB entries.

use ratatui::{Frame, prelude::*};

use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{
    Align, Col, DataTable, SbGeom, vertical_scrollbar, visible_window,
};
use crate::ui::view::style;

pub(super) fn render_entries(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("TLB Entries", PanelKind::Plain));
    if inner.height == 0 {
        return;
    }

    let Some(translation) = app.translation() else {
        return;
    };
    let total = translation.tlb_len();
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

    // Page-number columns are sized from the scheme's declared widths, so a
    // 64-bit paging mode widens them instead of truncating.
    let scheme = translation.scheme();
    let vpn_digits = scheme
        .virtual_hex_width()
        .saturating_sub(usize::from(scheme.offset_bits) / 4)
        .max(1);
    let ppn_digits = usize::from(scheme.physical_bits)
        .div_ceil(4)
        .saturating_sub(usize::from(scheme.offset_bits) / 4)
        .max(1);
    let page_col = |digits: usize| Constraint::Length(digits as u16 + 3);

    let cols = vec![
        Col::new(" # ", Constraint::Length(4), Align::Left),
        Col::new("VPN", page_col(vpn_digits), Align::Left),
        Col::new("PPN", page_col(ppn_digits), Align::Left),
        Col::new("RWXU", Constraint::Length(5), Align::Left),
        Col::new("ASID", Constraint::Length(5), Align::Left),
        Col::new(" G", Constraint::Length(2), Align::Left),
        Col::new(" A", Constraint::Length(2), Align::Left),
        Col::new(" D", Constraint::Length(2), Align::Left),
        Col::new("Mp", Constraint::Length(3), Align::Left),
    ];

    let mut table = DataTable::new(cols);
    for i in scroll..(scroll + visible).min(total) {
        let Some(e) = translation.tlb_entry(i) else {
            continue;
        };
        let idx = Line::from(Span::styled(
            format!("{i:>3}"),
            Style::default().fg(theme::BORDER),
        ));
        let row = if e.valid {
            let perms = Line::from(vec![
                mark(e.permissions.read, "R"),
                mark(e.permissions.write, "W"),
                mark(e.permissions.execute, "X"),
                mark(e.permissions.user, "U"),
            ]);
            vec![
                idx,
                Line::from(Span::styled(
                    format!("0x{:0vpn_digits$x}", e.virtual_page),
                    style::value(),
                )),
                Line::from(Span::styled(
                    format!("0x{:0ppn_digits$x}", e.physical_page),
                    style::value(),
                )),
                perms,
                Line::from(Span::styled(
                    e.address_space.map_or_else(|| "—".into(), |id| id.to_string()),
                    style::label(),
                )),
                Line::from(mark(e.global, "G")),
                Line::from(mark(e.accessed, "A")),
                Line::from(mark(e.dirty, "D")),
                Line::from(mark(e.superpage_bits > 0, "M")),
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

    // Register the bar's track for mouse click-to-jump + drag (cleared by the
    // tab root each frame, so it only hits while actually rendered).
    if needs_sb {
        let sb_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));
        vertical_scrollbar(f, sb_area, total, visible, scroll);
        app.tlb.entries_sb.set(Some(SbGeom {
            start: sb_area.y,
            len: sb_area.height,
            cross: inner.x + inner.width.saturating_sub(1),
            content: total,
            viewport: visible,
            offset: scroll,
            max: total - visible,
        }));
    }
}
