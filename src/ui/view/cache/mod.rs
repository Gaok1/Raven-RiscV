// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::falcon::cache::CacheController;
use crate::ui::app::{App, CacheScope, CacheSubtab};
use crate::ui::view::run::render_run_status;

mod config;
mod stats;
mod view;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // Layout: level selector (1) | subtab header (3) | run controls (5) | content (min) | shared controls bar (3)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // level selector bar
            Constraint::Length(3), // subtab header
            Constraint::Length(5), // run controls (always visible)
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar (Reset / Pause / Scope)
        ])
        .split(area);

    render_level_selector(f, layout[0], app);
    render_subtab_header(f, layout[1], app);
    render_run_status(f, layout[2], app);

    match app.cache.subtab {
        CacheSubtab::Stats  => stats::render_stats(f, layout[3], app),
        CacheSubtab::View   => view::render_view(f, layout[3], app),
        CacheSubtab::Config => config::render_config(f, layout[3], app),
    }

    render_controls_bar(f, layout[4], app);
}

fn render_level_selector(f: &mut Frame, area: Rect, app: &App) {
    let num_extra = app.cache.extra_pending.len();
    let selected = app.cache.selected_level;

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));

    // L1 button
    let l1_active = selected == 0;
    let l1_hover = app.cache.hover_level.first().copied().unwrap_or(false);
    spans.push(Span::styled("[ L1 ]", level_btn_style(l1_active, l1_hover)));

    // L2, L3, … buttons
    for i in 0..num_extra {
        let level = i + 1;
        let active = selected == level;
        let hovered = app.cache.hover_level.get(level).copied().unwrap_or(false);
        spans.push(Span::raw(" "));
        let label = format!("[ {} ]", CacheController::extra_level_name(i));
        spans.push(Span::styled(label, level_btn_style(active, hovered)));
    }

    spans.push(Span::raw("  "));

    // Add button
    let add_style = if app.cache.hover_add_level {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Yellow)
    };
    spans.push(Span::styled("[+ Add]", add_style));

    // Remove button (only when extra levels exist)
    if num_extra > 0 {
        spans.push(Span::raw(" "));
        let rem_style = if app.cache.hover_remove_level {
            Style::default().fg(Color::Black).bg(Color::Red)
        } else {
            Style::default().fg(Color::Red)
        };
        spans.push(Span::styled("[- Remove]", rem_style));
    }

    spans.push(Span::styled(
        "   +/= add level  -/_ remove",
        Style::default().fg(Color::DarkGray),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style  = subtab_style(matches!(app.cache.subtab, CacheSubtab::Stats),  app.cache.hover_subtab_stats);
    let view_style   = subtab_style(matches!(app.cache.subtab, CacheSubtab::View),   app.cache.hover_subtab_view);
    let config_style = subtab_style(matches!(app.cache.subtab, CacheSubtab::Config), app.cache.hover_subtab_config);

    let level_label = if app.cache.selected_level == 0 {
        "L1 Split I/D".to_string()
    } else {
        format!("{} Unified", CacheController::extra_level_name(app.cache.selected_level - 1))
    };

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
        .title(Span::styled(
            format!("Cache Simulation — {level_label}"),
            Style::default().fg(Color::Cyan).bold(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

/// Shared controls bar — visible on every Cache subtab.
pub(super) fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
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
    let export_style = if app.cache.hover_export_results {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };
    let compare_style = if app.cache.hover_compare {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else if app.cache.loaded_snapshot.is_some() {
        Style::default().fg(Color::LightBlue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let pause_label = if app.run.is_running { "[Pause]" } else { "[Resume]" };

    // Scope buttons: only shown when L1 is selected
    let show_scope = app.cache.selected_level == 0;
    let scope_i_style    = scope_btn_style(matches!(app.cache.scope, CacheScope::ICache), app.cache.hover_scope_i);
    let scope_d_style    = scope_btn_style(matches!(app.cache.scope, CacheScope::DCache), app.cache.hover_scope_d);
    let scope_both_style = scope_btn_style(matches!(app.cache.scope, CacheScope::Both),   app.cache.hover_scope_both);

    // Layout: " [Reset]  [Pause]  [\u{2b06} Export]  [\u{2b07} Compare]    View: [I-Cache] [D-Cache] [Both]  hint"
    // x=1..8   x=10..17  x=19..29         x=31..42
    let mut line_spans = vec![
        Span::raw(" "),
        Span::styled("[Reset]",      reset_style),
        Span::raw("  "),
        Span::styled(pause_label,    pause_style),
        Span::raw("  "),
        Span::styled("[\u{2b06} Export]",  export_style),
        Span::raw("  "),
        Span::styled("[\u{2b07} Compare]", compare_style),
    ];

    if show_scope {
        line_spans.push(Span::raw("    View: "));
        line_spans.push(Span::styled("[I-Cache]", scope_i_style));
        line_spans.push(Span::raw(" "));
        line_spans.push(Span::styled("[D-Cache]", scope_d_style));
        line_spans.push(Span::raw(" "));
        line_spans.push(Span::styled("[Both]",    scope_both_style));
        line_spans.push(Span::styled(
            "   r=reset  p=pause  Ctrl+R=export  Ctrl+M=compare",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        line_spans.push(Span::styled(
            "   r=reset  p=pause  Ctrl+R=export  Ctrl+M=compare",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let line = Line::from(line_spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn level_btn_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
    } else if hovered {
        Style::default().fg(Color::Black).bg(Color::Gray)
    } else {
        Style::default().fg(Color::DarkGray)
    }
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

pub(super) fn scope_btn_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else if hovered {
        Style::default().fg(Color::Black).bg(Color::Gray)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
