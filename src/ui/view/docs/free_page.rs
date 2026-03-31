use super::chrome::{render_page_tabs, render_tab_hint, separator_line};
use super::content::{fcache_ref, memory_map, syscalls};
use crate::ui::app::{DocsLang, DocsPage};
use crate::ui::view::App;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

pub(super) fn render(f: &mut Frame, area: Rect, app: &App, lines: Vec<Line<'static>>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let tab_area = chunks[0];
    render_page_tabs(f, tab_area, app);
    render_tab_hint(
        f,
        tab_area,
        app,
        2,
        format!("  [{}] L=lang  ↑/↓=scroll", app.docs.lang.label()),
    );

    let sep_area = Rect::new(chunks[0].x, chunks[0].y + 1, chunks[0].width, 1);
    f.render_widget(Paragraph::new(separator_line(area.width)), sep_area);

    let content_area = chunks[1];
    if content_area.height == 0 {
        return;
    }

    let viewport_h = content_area.height as usize;
    let max_start = lines.len().saturating_sub(viewport_h);
    let start = app.docs.scroll.min(max_start);
    let end = (start + viewport_h).min(lines.len());
    f.render_widget(
        Paragraph::new(lines[start..end].to_vec()).wrap(Wrap { trim: false }),
        content_area,
    );
}

pub(crate) fn free_page_line_count(page: DocsPage, lang: DocsLang) -> usize {
    match page {
        DocsPage::InstrRef => 0,
        DocsPage::Syscalls => syscalls::syscall_lines(lang).len(),
        DocsPage::MemoryMap => memory_map::memory_map_lines(lang).len(),
        DocsPage::FcacheRef => fcache_ref::fcache_ref_lines(lang).len(),
    }
}
