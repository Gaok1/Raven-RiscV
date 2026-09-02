use crate::ui::app::DocsPage;
use crate::ui::theme;
use crate::ui::view::App;
use crate::ui::view::components::toolbar::pill;
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// One hue per filter position, so the badge colours stay put as a reader
/// scans the table. Any architecture draws from the same sequence, which is
/// why nothing here mentions a type name.
const KIND_COLORS: &[Color] = &[
    Color::Yellow,
    Color::LightRed,
    Color::Green,
    Color::Cyan,
    Color::LightBlue,
    Color::Magenta,
    Color::LightYellow,
    Color::LightCyan,
    Color::Red,
    Color::LightRed,
    Color::LightMagenta,
    Color::LightGreen,
    Color::Gray,
    Color::White,
    Color::LightGreen,
];

/// The instruction types the active architecture documents, in the order it
/// lists them, preceded by the "All" entry.
///
/// The types used to be a fixed list of RISC-V's — which is why the reference
/// pane could only ever describe RV32. They come from the ISA's own rows now,
/// so an architecture that groups its instructions differently filters by its
/// own groups.
pub(crate) fn filter_items(app: &App) -> Vec<(&'static str, u16, Color)> {
    let mut items = vec![("All", 0u16, Color::White)];
    for row in app.architecture.assembler().documented_instructions() {
        if items.iter().any(|(label, ..)| *label == row.kind) {
            continue;
        }
        // Bit 0 belongs to the first type; "All" carries no bit of its own.
        let Some(bit) = 1u16.checked_shl(items.len() as u32 - 1) else {
            break;
        };
        let color = KIND_COLORS[(items.len() - 1) % KIND_COLORS.len()];
        items.push((row.kind, bit, color));
    }
    items
}

/// Every type bit the active architecture uses — what "All" selects.
pub(crate) fn all_mask(app: &App) -> u16 {
    filter_items(app).iter().map(|(_, bit, _)| bit).sum()
}

/// The instruction-type filter row as a [`Toolbar`] keyed by item index — the
/// single source of truth for rendering it and for mapping a click column back
/// to a filter.
///
/// The two used to be separate: the renderer emitted `" ●{label} "` while the
/// mouse router guessed the same width back with `label.len() + 3`. Any change
/// to the chip — including giving it the surface that says it is clickable —
/// slid the click targets out from under the words.
pub(crate) fn build_docs_filter_bar(app: &App) -> Toolbar<usize> {
    let all_mask = all_mask(app);
    let mut bar = Toolbar::with_gap(1);
    for (idx, &(label, bit, color)) in filter_items(app).iter().enumerate() {
        let active = if idx == 0 {
            app.docs.type_filter == all_mask
        } else {
            (app.docs.type_filter & bit) != 0
        };
        // The bullet keeps carrying the checked state — a filter row is a set of
        // checkboxes, and the surface only says "clickable".
        let text = format!("{} {label}", if active { "●" } else { "○" });
        let state = ControlState::chip(active, idx == app.docs.filter_cursor);
        bar.span(idx, None, pill(&text, state, color), true);
    }
    bar
}

pub(super) fn render_filter_bar(f: &mut Frame, area: Rect, app: &App) {
    app.docs.filter_bar_y.set(area.y);
    app.docs.filter_bar_x.set(area.x);
    f.render_widget(
        Paragraph::new(Line::from(build_docs_filter_bar(app).spans())),
        area,
    );
}

/// The Docs pages the active architecture has something to say on.
///
/// Every page used to be shown for every backend, so SAP and Toy16 got RV32's
/// syscall table and RV32's memory map. A page appears only when the
/// architecture declares the thing it documents.
pub(crate) fn visible_pages(app: &App) -> Vec<DocsPage> {
    let capabilities = app.architecture.descriptor().capabilities;
    [
        (
            DocsPage::InstrRef,
            !app.architecture
                .assembler()
                .documented_instructions()
                .is_empty(),
        ),
        (DocsPage::Syscalls, capabilities.syscalls),
        // The page is hand-written RV32 prose — 16 MB of RAM, Sv32, `sp` at
        // RAM_SIZE — so it is gated on the paging scheme it describes rather
        // than on having a memory map, which every backend has. Once the page
        // is built from the descriptor and `MemoryInspect::regions`, this
        // becomes unconditional.
        (DocsPage::MemoryMap, capabilities.virtual_memory),
        // The page documents retuning and sharing a *configurable* hierarchy —
        // `.fcache` files, extra levels, CPI keys, a TLB. A fixed teaching
        // cache declares `cache` but answers `is_configurable() == false`, so
        // the page must not appear for it — the same predicate that hides the
        // Cache tab's add/remove controls.
        (DocsPage::FcacheRef, app.cache_is_configurable()),
    ]
    .into_iter()
    .filter_map(|(page, shown)| shown.then_some(page))
    .collect()
}

pub(super) fn render_page_tabs(f: &mut Frame, area: Rect, app: &App) {
    app.docs.tab_bar_y.set(area.y);

    let pages = visible_pages(app);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut underline_spans: Vec<Span<'static>> = Vec::new();
    let mut xs = [(0u16, 0u16); 4];
    let mut cursor_x = area.x;

    for (i, page) in pages.iter().enumerate() {
        let active = *page == app.docs.page;
        let label = format!(" {} ", page.label());
        let label_w = label.chars().count() as u16;
        let text_w = page.label().chars().count() as u16;
        let style = if active {
            Style::default()
                .fg(theme::ACTIVE)
                .add_modifier(Modifier::BOLD)
        } else if app.docs.hover_page == Some(*page) {
            style::value().add_modifier(Modifier::BOLD)
        } else {
            style::label()
        };
        if let Some(slot) = xs.get_mut(i) {
            *slot = (cursor_x, cursor_x + label_w);
        }
        cursor_x += label_w;
        spans.push(Span::styled(label, style));
        // One continuous rule whose heavy segment marks the active page —
        // the same shape as the main tab bar. It used to be a short dash
        // floating under the active label with blank gaps either side, which
        // read as a different kind of control from the row above it.
        underline_spans.push(if active {
            let left = (label_w.saturating_sub(text_w) / 2) as usize;
            let right = label_w as usize - left - text_w as usize;
            Span::styled(
                format!(
                    "{}{}{}",
                    "─".repeat(left),
                    "━".repeat(text_w as usize),
                    "─".repeat(right)
                ),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "─".repeat(label_w as usize),
                Style::default().fg(theme::BORDER),
            )
        });

        if i + 1 < pages.len() {
            spans.push(Span::raw("  "));
            underline_spans.push(Span::styled(
                "──",
                Style::default().fg(theme::BORDER),
            ));
            cursor_x += 2;
        }
    }
    app.docs.tab_bar_xs.set(xs);

    let mut lines = vec![Line::from(spans)];
    if area.height > 1 {
        // Carry the rule to the right edge so it frames the page, not just the
        // tabs.
        let used = cursor_x.saturating_sub(area.x);
        underline_spans.push(Span::styled(
            "─".repeat(area.width.saturating_sub(used) as usize),
            Style::default().fg(theme::BORDER),
        ));
        lines.push(Line::from(underline_spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

/// Place the key hint just past the last tab that was drawn — which depends on
/// how many pages the architecture has, so it cannot be a fixed index.
pub(super) fn render_tab_hint(f: &mut Frame, tab_area: Rect, app: &App, hint: impl Into<String>) {
    let xs = app.docs.tab_bar_xs.get();
    let last = visible_pages(app).len().saturating_sub(1);
    let hint_x = xs[last.min(xs.len() - 1)].1 + 2;
    let hint_area = Rect::new(
        hint_x.min(tab_area.x + tab_area.width),
        tab_area.y,
        tab_area
            .width
            .saturating_sub(hint_x.saturating_sub(tab_area.x)),
        1,
    );
    f.render_widget(
        Paragraph::new(Span::styled(hint.into(), style::label())),
        hint_area,
    );
}

pub(super) fn separator_line(width: u16) -> Line<'static> {
    Line::styled(
        "─".repeat(width.min(300) as usize),
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )
}
