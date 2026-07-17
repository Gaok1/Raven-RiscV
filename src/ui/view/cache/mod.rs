// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{Frame, prelude::*, widgets::Paragraph};

use crate::falcon::cache::CacheController;
use crate::ui::app::{App, CacheHoverTarget, CacheScope, CacheSubtab, RunButton};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style;

pub(crate) mod config;
mod stats;
mod view;

// The session-snapshot popup is shared with the Virtual Memory tab.
pub(in crate::ui::view) use stats::render_snapshot_popup;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // When cache is disabled, show a notice and skip all cache-specific content.
    if !app.run.cache_enabled {
        let inner = render_panel(f, area, panel::panel_frame(PanelKind::Plain));
        let lines = vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  Cache simulation is disabled.",
                style::warning().bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  Enable it in the Settings tab to run cache statistics.",
                style::label(),
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

/// A button in the cache level selector bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheLevelBtn {
    Level(usize),
    Add,
    Remove,
}

/// A button in the cache shared-controls action group.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheCtrlBtn {
    Results,
    ImportCfg,
    ExportCfg,
}

/// A button in the cache scope selector.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheScopeBtn {
    I,
    D,
    Both,
}

/// The cache run-controls bar — `speed <s>  state <s>  reset` — as a [`Toolbar`]
/// keyed by the shared [`RunButton`] ids (mouse goes through `cache_exec_hit`).
pub(crate) fn build_cache_exec_bar(app: &App) -> Toolbar<RunButton> {
    let hov = |b: RunButton| app.hover_run_button == Some(b);
    let (state_text, state_color) = if app.run.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };
    let mut bar = Toolbar::new();
    bar.toggle(
        RunButton::Speed,
        "speed",
        app.run.speed.label(),
        ControlState::chip(true, hov(RunButton::Speed)),
        theme::TEXT,
    )
    .toggle(
        RunButton::State,
        "state",
        state_text,
        ControlState::chip(true, hov(RunButton::State)),
        state_color,
    )
    .action(
        RunButton::Reset,
        "reset",
        ControlState::chip(false, hov(RunButton::Reset)),
        theme::DANGER,
    );
    bar
}

/// The cache level selector — `l1 l2 …  add  remove` (rendered after a dim
/// `level ` label). Keyed by [`CacheLevelBtn`].
pub(crate) fn build_cache_level_bar(app: &App) -> Toolbar<CacheLevelBtn> {
    let selected = app.cache.selected_level;
    let num_extra = app.cache.extra_pending.len();
    let hov = |t: CacheHoverTarget| app.cache.hover == Some(t);
    let mut bar = Toolbar::new();
    bar.value(
        CacheLevelBtn::Level(0),
        "l1",
        ControlState::chip(selected == 0, hov(CacheHoverTarget::Level(0))),
        theme::ACCENT,
    );
    for i in 0..num_extra {
        let level = i + 1;
        let label = CacheController::extra_level_name(i).to_lowercase();
        bar.value(
            CacheLevelBtn::Level(level),
            &label,
            ControlState::chip(selected == level, hov(CacheHoverTarget::Level(level))),
            theme::ACCENT,
        );
    }
    bar.action(
        CacheLevelBtn::Add,
        "add",
        ControlState::chip(false, hov(CacheHoverTarget::AddLevel)),
        theme::ACCENT,
    );
    if num_extra > 0 {
        bar.action(
            CacheLevelBtn::Remove,
            "remove",
            ControlState::chip(false, hov(CacheHoverTarget::RemoveLevel)),
            theme::DANGER,
        );
    }
    bar
}

/// The shared controls bar's action group — `results` (+ import/export cfg in
/// the Config subtab). Keyed by [`CacheCtrlBtn`].
pub(crate) fn build_cache_ctrl_bar(app: &App) -> Toolbar<CacheCtrlBtn> {
    let hov = |t: CacheHoverTarget| app.cache.hover == Some(t);
    let mut bar = Toolbar::new();
    bar.action(
        CacheCtrlBtn::Results,
        "results",
        ControlState::chip(false, hov(CacheHoverTarget::ExportResults)),
        theme::ACCENT,
    );
    if matches!(app.cache.subtab, CacheSubtab::Config) {
        bar.action(
            CacheCtrlBtn::ImportCfg,
            "import cfg",
            ControlState::chip(false, hov(CacheHoverTarget::ImportCfg)),
            theme::METRIC_CYC,
        )
        .action(
            CacheCtrlBtn::ExportCfg,
            "export cfg",
            ControlState::chip(false, hov(CacheHoverTarget::ExportCfg)),
            theme::METRIC_CYC,
        );
    }
    bar
}

/// The scope selector — `i-cache d-cache both` (rendered after a dim `view `
/// label; L1 only). Keyed by [`CacheScopeBtn`].
pub(crate) fn build_cache_scope_bar(app: &App) -> Toolbar<CacheScopeBtn> {
    let hov = |t: CacheHoverTarget| app.cache.hover == Some(t);
    let mut bar = Toolbar::new();
    bar.value(
        CacheScopeBtn::I,
        "i-cache",
        ControlState::chip(matches!(app.cache.scope, CacheScope::ICache), hov(CacheHoverTarget::ScopeI)),
        theme::ACCENT,
    )
    .value(
        CacheScopeBtn::D,
        "d-cache",
        ControlState::chip(matches!(app.cache.scope, CacheScope::DCache), hov(CacheHoverTarget::ScopeD)),
        theme::ACCENT,
    )
    .value(
        CacheScopeBtn::Both,
        "both",
        ControlState::chip(matches!(app.cache.scope, CacheScope::Both), hov(CacheHoverTarget::ScopeBoth)),
        theme::ACCENT,
    );
    bar
}

fn render_cache_exec_controls(f: &mut Frame, area: Rect, app: &App) {
    let (total, cpi, instr) = if let Some(pipeline) = app.aggregate_pipeline_snapshot() {
        let cycles = pipeline.cycles;
        let committed = pipeline.committed;
        let cpi = if committed > 0 {
            cycles as f64 / committed as f64
        } else {
            0.0
        };
        (cycles, cpi, committed)
    } else {
        (
            app.run.mem().total_program_cycles(),
            app.run.mem().overall_cpi(),
            app.run.mem().instruction_count,
        )
    };

    let mut spans = build_cache_exec_bar(app).spans();
    spans.push(Span::styled(
        if matches!(app.cache.subtab, crate::ui::app::CacheSubtab::Stats) {
            "   r=reset  f=speed  p=pause  s=capture  ↑↓=history  D=del"
        } else {
            "   r=reset  f=speed  p=pause  s=step"
        },
        style::label(),
    ));
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::styled(
            format!(" Cycles:{total}"),
            style::metric(style::Metric::Cycles),
        ),
        Span::raw("  "),
        Span::styled(
            format!("CPI:{cpi:.2}"),
            style::metric(style::Metric::Cpi),
        ),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), style::label()),
    ]);

    let inner = render_panel(f, area, panel::panel("Execution", PanelKind::Plain));
    app.cache.exec_origin.set((inner.y, inner.x));
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

fn render_level_selector(f: &mut Frame, area: Rect, app: &App) {
    // The bar starts right after the dim `level ` label.
    app.cache
        .level_origin
        .set((area.y, area.x + "level ".len() as u16));

    let mut spans = vec![Span::styled("level", style::idle()), Span::raw(" ")];
    spans.extend(build_cache_level_bar(app).spans());
    spans.push(Span::styled("   +/= add level  -/_ remove", style::label()));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The Cache subtab bar — `[stats] [view] [settings]` — as a [`Toolbar`] keyed by
/// [`CacheSubtab`]. Shared by the renderer and `mouse::update_cache_hover` /
/// `handle_cache_click`, so the click targets cannot drift from the labels.
pub(crate) fn build_cache_subtab_bar(app: &App) -> Toolbar<CacheSubtab> {
    let st = |sub: CacheSubtab, t: CacheHoverTarget| {
        ControlState::chip(app.cache.subtab == sub, app.cache.hover == Some(t))
    };
    let mut bar = Toolbar::new();
    bar.value(CacheSubtab::Stats, "stats", st(CacheSubtab::Stats, CacheHoverTarget::SubtabStats), theme::ACCENT)
        .value(CacheSubtab::View, "view", st(CacheSubtab::View, CacheHoverTarget::SubtabView), theme::ACCENT)
        .value(CacheSubtab::Config, "settings", st(CacheSubtab::Config, CacheHoverTarget::SubtabConfig), theme::ACCENT);
    bar
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let level_label = if app.cache.selected_level == 0 {
        "L1 Split I/D".to_string()
    } else {
        format!(
            "{} Unified",
            CacheController::extra_level_name(app.cache.selected_level - 1)
        )
    };

    let block = panel::panel(
        format!("Cache Simulation — {level_label}"),
        PanelKind::Accent,
    );
    let inner = block.inner(area);
    app.cache.subtab_header_origin.set((inner.y, inner.x + 1));

    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_cache_subtab_bar(app).spans());
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", style::label()),
    ]);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

/// Shared controls bar — visible on every Cache subtab.
pub(super) fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let show_scope = app.cache.selected_level == 0;
    let block = panel::panel_frame(PanelKind::Plain);
    let inner = block.inner(area);

    let actions = build_cache_ctrl_bar(app);
    app.cache.ctrl_origin.set((inner.y, inner.x + 1));

    let mut spans = vec![Span::raw(" ")];
    spans.extend(actions.spans());

    if show_scope {
        // `view ` label + the scope bar, placed right after the action group.
        let scope_x = inner.x + 1 + actions.width() + 3 + "view ".len() as u16;
        app.cache.ctrl_scope_origin.set((inner.y, scope_x));
        spans.push(Span::raw("   "));
        spans.push(Span::styled("view", style::idle()));
        spans.push(Span::raw(" "));
        spans.extend(build_cache_scope_bar(app).spans());
    } else {
        app.cache.ctrl_scope_origin.set((0, 0));
    }

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

