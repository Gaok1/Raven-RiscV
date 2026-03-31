use crate::ui::app::DocsPage;
use crate::ui::theme;
use crate::ui::view::App;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

const TY_R: u16 = 1 << 0;
const TY_M: u16 = 1 << 1;
const TY_I: u16 = 1 << 2;
const TY_LOAD: u16 = 1 << 3;
const TY_STORE: u16 = 1 << 4;
const TY_BRANCH: u16 = 1 << 5;
const TY_U: u16 = 1 << 6;
const TY_JUMP: u16 = 1 << 7;
const TY_SYS: u16 = 1 << 8;
const TY_PSEUDO: u16 = 1 << 9;
const TY_F: u16 = 1 << 10;
const TY_DIR: u16 = 1 << 11;

pub(crate) const ALL_MASK: u16 = 0x0FFF;

pub(crate) const FILTER_ITEMS: &[(&str, u16, Color)] = &[
    ("All", 0, Color::White),
    ("R", TY_R, Color::Yellow),
    ("M", TY_M, Color::LightRed),
    ("I", TY_I, Color::Green),
    ("Load", TY_LOAD, Color::Cyan),
    ("Store", TY_STORE, Color::LightBlue),
    ("Branch", TY_BRANCH, Color::Magenta),
    ("U", TY_U, Color::LightYellow),
    ("Jump", TY_JUMP, Color::LightCyan),
    ("SYS", TY_SYS, Color::Red),
    ("Pseudo", TY_PSEUDO, Color::LightMagenta),
    ("F", TY_F, Color::LightGreen),
    ("Dir", TY_DIR, Color::Gray),
];

pub(super) fn render_filter_bar(f: &mut Frame, area: Rect, app: &App) {
    app.docs.filter_bar_y.set(area.y);

    let type_filter = app.docs.type_filter;
    let cursor = app.docs.filter_cursor;
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (idx, &(label, bit, color)) in FILTER_ITEMS.iter().enumerate() {
        let is_cursor = idx == cursor;
        let is_active = if idx == 0 {
            type_filter == ALL_MASK
        } else {
            (type_filter & bit) != 0
        };

        let bullet = if is_active { "●" } else { "○" };
        let text = format!(" {bullet}{label} ");

        let fg = if is_active { color } else { theme::LABEL };
        let mut style = Style::default().fg(fg);
        if is_cursor {
            style = style
                .bg(Color::Rgb(50, 50, 80))
                .add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(text, style));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn render_page_tabs(f: &mut Frame, area: Rect, app: &App) {
    app.docs.tab_bar_y.set(area.y);

    let pages = [
        DocsPage::InstrRef,
        DocsPage::Syscalls,
        DocsPage::MemoryMap,
        DocsPage::FcacheRef,
    ];
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
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::LABEL)
        };
        xs[i] = (cursor_x, cursor_x + label_w);
        cursor_x += label_w;
        spans.push(Span::styled(label, style));
        underline_spans.push(Span::styled(
            if active {
                let left = (label_w.saturating_sub(text_w) / 2) as usize;
                let right = label_w as usize - left - text_w as usize;
                format!(
                    "{}{}{}",
                    " ".repeat(left),
                    "─".repeat(text_w as usize),
                    " ".repeat(right)
                )
            } else {
                " ".repeat(label_w as usize)
            },
            Style::default().fg(theme::ACCENT),
        ));

        if i + 1 < pages.len() {
            spans.push(Span::raw("  "));
            underline_spans.push(Span::raw("  "));
            cursor_x += 2;
        }
    }
    app.docs.tab_bar_xs.set(xs);

    let mut lines = vec![Line::from(spans)];
    if area.height > 1 {
        lines.push(Line::from(underline_spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

pub(super) fn render_tab_hint(
    f: &mut Frame,
    tab_area: Rect,
    app: &App,
    after_tab_idx: usize,
    hint: impl Into<String>,
) {
    let xs = app.docs.tab_bar_xs.get();
    let hint_x = xs[after_tab_idx].1 + 2;
    let hint_area = Rect::new(
        hint_x.min(tab_area.x + tab_area.width),
        tab_area.y,
        tab_area
            .width
            .saturating_sub(hint_x.saturating_sub(tab_area.x)),
        1,
    );
    f.render_widget(
        Paragraph::new(Span::styled(hint.into(), Style::default().fg(theme::LABEL))),
        hint_area,
    );
}

pub(super) fn separator_line(width: u16) -> Line<'static> {
    Line::styled(
        "─".repeat(width.min(300) as usize),
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )
}
