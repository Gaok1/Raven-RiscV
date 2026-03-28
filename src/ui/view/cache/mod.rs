// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::falcon::cache::CacheController;
use crate::ui::app::{App, CacheScope, CacheSubtab, RunButton};
use crate::ui::theme;
use crate::ui::view::components::{dense_action, dense_value, push_dense_pair};

mod config;
mod stats;
mod view;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // When cache is disabled, show a notice and skip all cache-specific content.
    if !app.run.cache_enabled {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme::BORDER));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  Cache simulation is disabled.",
                Style::default().fg(theme::PAUSED).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  Enable it in the Config tab to run cache statistics.",
                Style::default().fg(theme::LABEL),
            )),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    // Layout: level selector (1) | subtab header (3) | exec controls (4) | content (min) | shared controls bar (3)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // level selector bar
            Constraint::Length(4), // subtab header
            Constraint::Length(4), // exec controls (Speed / State / Cycles)
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar (Reset / Export / Compare / Scope)
        ])
        .split(area);

    render_level_selector(f, layout[0], app);
    render_subtab_header(f, layout[1], app);
    render_cache_exec_controls(f, layout[2], app);

    match app.cache.subtab {
        CacheSubtab::Stats => stats::render_stats(f, layout[3], app),
        CacheSubtab::View => view::render_view(f, layout[3], app),
        CacheSubtab::Config => config::render_config(f, layout[3], app),
    }

    render_controls_bar(f, layout[4], app);

    if app.cache.viewing_snapshot.is_some() {
        stats::render_snapshot_popup(f, area, app);
    }
}

fn render_cache_exec_controls(f: &mut Frame, area: Rect, app: &App) {
    let speed_text = app.run.speed.label();

    let hover_reset = app.hover_run_button == Some(RunButton::Reset);
    let hover_speed = app.hover_run_button == Some(RunButton::Speed);
    let hover_state = app.hover_run_button == Some(RunButton::State);

    let (state_text, state_color) = if app.run.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };

    let total = app.run.mem.total_program_cycles();
    let cpi = app.run.mem.overall_cpi();
    let instr = app.run.mem.instruction_count;

    let mut spans = Vec::new();
    push_dense_pair(
        &mut spans,
        "speed",
        speed_text,
        hover_speed,
        true,
        theme::TEXT,
    );
    push_dense_pair(
        &mut spans,
        "state",
        state_text,
        hover_state,
        true,
        state_color,
    );
    spans.push(Span::raw("   "));
    spans.push(dense_action("reset", theme::DANGER, hover_reset));
    spans.push(Span::styled(
        if matches!(app.cache.subtab, crate::ui::app::CacheSubtab::Stats) {
            "   r=reset  f=speed  p=pause  s=capture  ↑↓=history  D=del"
        } else {
            "   r=reset  f=speed  p=pause  s=step"
        },
        Style::default().fg(theme::LABEL),
    ));
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::styled(
            format!(" Cycles:{total}"),
            Style::default().fg(theme::METRIC_CYC),
        ),
        Span::raw("  "),
        Span::styled(
            format!("CPI:{cpi:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        ),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(theme::LABEL)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title(Span::styled("Execution", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

fn render_level_selector(f: &mut Frame, area: Rect, app: &App) {
    let num_extra = app.cache.extra_pending.len();
    let selected = app.cache.selected_level;

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("level", Style::default().fg(theme::IDLE)));
    spans.push(Span::raw(" "));
    let l1_active = selected == 0;
    let l1_hover = app.cache.hover_level.first().copied().unwrap_or(false);
    spans.push(Span::styled("l1", level_btn_style(l1_active, l1_hover)));

    for i in 0..num_extra {
        let level = i + 1;
        let active = selected == level;
        let hovered = app.cache.hover_level.get(level).copied().unwrap_or(false);
        spans.push(Span::raw("   "));
        let label = CacheController::extra_level_name(i).to_lowercase();
        spans.push(Span::styled(label, level_btn_style(active, hovered)));
    }

    spans.push(Span::raw("   "));
    spans.push(dense_action(
        "add",
        theme::ACCENT,
        app.cache.hover_add_level,
    ));

    if num_extra > 0 {
        spans.push(Span::raw("   "));
        spans.push(dense_action(
            "remove",
            theme::DANGER,
            app.cache.hover_remove_level,
        ));
    }

    spans.push(Span::styled(
        "   +/= add level  -/_ remove",
        Style::default().fg(theme::LABEL),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style = subtab_style(
        matches!(app.cache.subtab, CacheSubtab::Stats),
        app.cache.hover_subtab_stats,
    );
    let view_style = subtab_style(
        matches!(app.cache.subtab, CacheSubtab::View),
        app.cache.hover_subtab_view,
    );
    let config_style = subtab_style(
        matches!(app.cache.subtab, CacheSubtab::Config),
        app.cache.hover_subtab_config,
    );

    let level_label = if app.cache.selected_level == 0 {
        "L1 Split I/D".to_string()
    } else {
        format!(
            "{} Unified",
            CacheController::extra_level_name(app.cache.selected_level - 1)
        )
    };

    // x-offsets from inner left:
    //  " Stats "  = x 1..7   (x >= 1 && x < 8)
    //  "  "       = x 8..9
    //  " View "   = x 10..15 (x >= 10 && x < 16)
    //  "  "       = x 16..17
    //  " Config " = x 18..25 (x >= 18 && x < 26)
    let line1 = Line::from(vec![
        Span::raw(" "),
        Span::styled("stats", stats_style),
        Span::raw("   "),
        Span::styled("view", view_style),
        Span::raw("   "),
        Span::styled("config", config_style),
    ]);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", Style::default().fg(theme::LABEL)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            format!("Cache Simulation — {level_label}"),
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

/// Shared controls bar — visible on every Cache subtab.
pub(super) fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let show_scope = app.cache.selected_level == 0;
    let show_cfg_btns = matches!(app.cache.subtab, CacheSubtab::Config);
    let mut line_spans = vec![
        Span::raw(" "),
        dense_action("results", theme::ACCENT, app.cache.hover_export_results),
    ];

    if show_cfg_btns {
        line_spans.push(Span::raw("   "));
        line_spans.push(dense_action(
            "import cfg",
            theme::METRIC_CYC,
            app.cache.hover_import_cfg,
        ));
        line_spans.push(Span::raw("   "));
        line_spans.push(dense_action(
            "export cfg",
            theme::METRIC_CYC,
            app.cache.hover_export_cfg,
        ));
    }

    if show_scope {
        line_spans.push(Span::raw("   "));
        line_spans.push(Span::styled("view", Style::default().fg(theme::IDLE)));
        line_spans.push(Span::raw(" "));
        line_spans.push(dense_value(
            "i-cache",
            app.cache.hover_scope_i,
            matches!(app.cache.scope, CacheScope::ICache),
            theme::TEXT,
        ));
        line_spans.push(Span::raw(" "));
        line_spans.push(dense_value(
            "d-cache",
            app.cache.hover_scope_d,
            matches!(app.cache.scope, CacheScope::DCache),
            theme::TEXT,
        ));
        line_spans.push(Span::raw(" "));
        line_spans.push(dense_value(
            "both",
            app.cache.hover_scope_both,
            matches!(app.cache.scope, CacheScope::Both),
            theme::TEXT,
        ));
    }

    let line = Line::from(line_spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn level_btn_style(active: bool, hovered: bool) -> Style {
    dense_style(active, hovered, theme::TEXT)
}

fn subtab_style(active: bool, hovered: bool) -> Style {
    dense_style(active, hovered, theme::TEXT)
}

pub(super) fn scope_btn_style(active: bool, hovered: bool) -> Style {
    dense_style(active, hovered, theme::TEXT)
}

fn dense_style(active: bool, hovered: bool, color: Color) -> Style {
    if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    }
}
