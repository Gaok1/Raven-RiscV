// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::ui::app::{App, CacheScope, CacheSubtab};

mod config;
mod stats;
mod view;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // Layout:  subtab header (3) | content (min) | shared controls bar (3)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // subtab header
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar (Reset / Pause / Scope)
        ])
        .split(area);

    render_subtab_header(f, layout[0], app);

    match app.cache.subtab {
        CacheSubtab::Stats  => stats::render_stats(f, layout[1], app),
        CacheSubtab::View   => view::render_view(f, layout[1], app),
        CacheSubtab::Config => config::render_config(f, layout[1], app),
    }

    render_controls_bar(f, layout[2], app);
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style  = subtab_style(matches!(app.cache.subtab, CacheSubtab::Stats),  app.cache.hover_subtab_stats);
    let view_style   = subtab_style(matches!(app.cache.subtab, CacheSubtab::View),   app.cache.hover_subtab_view);
    let config_style = subtab_style(matches!(app.cache.subtab, CacheSubtab::Config), app.cache.hover_subtab_config);

    // x-offsets from inner left:
    //  " Stats "  = x 1..7   (x >= 1 && x < 8)
    //  "  "       = x 8..9
    //  " View "   = x 10..15 (x >= 10 && x < 16)
    //  "  "       = x 16..17
    //  " Config " = x 18..25 (x >= 18 && x < 26)
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(" Stats ",  stats_style),
        Span::raw("  "),
        Span::styled(" View ",   view_style),
        Span::raw("  "),
        Span::styled(" Config ", config_style),
        Span::styled("   Tab to switch", Style::default().fg(Color::DarkGray)),
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

/// Shared controls bar — visible on every Cache subtab.
fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let reset_style = if app.cache.hover_reset {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Black).bg(Color::Red)
    };
    let pause_style = if app.cache.hover_pause {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else if !app.run.is_running {
        Style::default().fg(Color::Black).bg(Color::Green)
    } else {
        Style::default().fg(Color::Black).bg(Color::Blue)
    };
    let scope_i_style    = scope_btn_style(matches!(app.cache.scope, CacheScope::ICache), app.cache.hover_scope_i);
    let scope_d_style    = scope_btn_style(matches!(app.cache.scope, CacheScope::DCache), app.cache.hover_scope_d);
    let scope_both_style = scope_btn_style(matches!(app.cache.scope, CacheScope::Both),   app.cache.hover_scope_both);

    let pause_label = if app.run.is_running { "[Pause]" } else { "[Resume]" };

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("[Reset]",   reset_style),
        Span::raw("  "),
        Span::styled(pause_label, pause_style),
        Span::raw("    View: "),
        Span::styled("[I-Cache]", scope_i_style),
        Span::raw(" "),
        Span::styled("[D-Cache]", scope_d_style),
        Span::raw(" "),
        Span::styled("[Both]",    scope_both_style),
        Span::styled(
            "   r=reset  p=pause  i/d/b=scope",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
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

fn scope_btn_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else if hovered {
        Style::default().fg(Color::Black).bg(Color::Gray)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
