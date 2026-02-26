// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::ui::app::{App, CacheSubtab};

mod config;
mod stats;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // Split: subtab header (3 rows) + content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_subtab_header(f, layout[0], app);

    match app.cache.subtab {
        CacheSubtab::Stats => stats::render_stats(f, layout[1], app),
        CacheSubtab::Config => config::render_config(f, layout[1], app),
    }
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style = subtab_style(
        matches!(app.cache.subtab, CacheSubtab::Stats),
        app.cache.hover_subtab_stats,
    );
    let config_style = subtab_style(
        matches!(app.cache.subtab, CacheSubtab::Config),
        app.cache.hover_subtab_config,
    );

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(" Stats ", stats_style),
        Span::raw("  "),
        Span::styled(" Config ", config_style),
        Span::styled(
            "   Tab to switch",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("Cache Simulation", Style::default().fg(Color::Cyan).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn subtab_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if hovered {
        Style::default().fg(Color::Black).bg(Color::Gray)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
