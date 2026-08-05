// ui/view/cache/mod.rs — Cache tab top-level renderer
use ratatui::{Frame, prelude::*, widgets::Paragraph};

use crate::ui::app::{App, CacheHoverTarget, CacheScope, CacheSubtab, RunButton};
use crate::ui::theme;
use crate::ui::view::components::panel::{PanelKind, render_panel};
use crate::ui::view::components::{ControlState, SpanRow, Toolbar};
use crate::ui::view::style;

pub(crate) mod config;
mod stats;
mod view;

// The session-snapshot popup is shared with the Virtual Memory tab.
pub(in crate::ui::view) use stats::render_snapshot_popup;

pub(super) fn render_cache(f: &mut Frame, area: Rect, app: &App) {
    // Cleared every frame; the Stats history renderer re-registers its
    // scrollbar track when (and only when) the bar is actually drawn.
    app.cache.history_sb.set(None);

    // When cache is disabled, show a notice and skip all cache-specific content.
    if !app.session.cache_enabled {
        let inner = render_panel(
            f,
            area,
            crate::ui::view::components::panel::panel_frame(PanelKind::Plain),
        );
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

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(CACHE_HEADER_H), Constraint::Min(0)])
        .split(area);

    render_cache_header(f, layout[0], app);

    match app.cache.subtab {
        CacheSubtab::Stats => stats::render_stats(f, layout[1], app),
        CacheSubtab::View => view::render_view(f, layout[1], app),
        CacheSubtab::Config => config::render_config(f, layout[1], app),
    }

    if app.cache.viewing_snapshot.is_some() {
        stats::render_snapshot_popup(f, area, app);
    }
}

/// Rows the borderless Cache header occupies — two rows of controls with a
/// blank between them. Mouse hit-testing splits the tab with the same constant,
/// so the two cannot drift.
///
/// The gap earns its row: stacked directly, the two rows of pills read as one
/// dense block instead of as "what am I looking at" over "what is it doing".
pub(crate) const CACHE_HEADER_H: u16 = 3;

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
    let (state_text, state_color) = if app.session.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };
    let mut bar = Toolbar::new();
    bar.toggle(
        RunButton::Speed,
        "speed",
        app.session.speed.label(),
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
    let num_extra = if !app.cache_is_configurable() {
        app.cache_hierarchy()
            .map_or(0, |cache| cache.level_count().saturating_sub(1))
    } else {
        app.cache.extra_pending.len()
    };
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
        let label = format!("l{}", i + 2);
        bar.value(
            CacheLevelBtn::Level(level),
            &label,
            ControlState::chip(selected == level, hov(CacheHoverTarget::Level(level))),
            theme::ACCENT,
        );
    }
    if app.cache_is_configurable() {
        bar.action(
            CacheLevelBtn::Add,
            "add",
            ControlState::chip(false, hov(CacheHoverTarget::AddLevel)),
            theme::ACCENT,
        );
    }
    if app.cache_is_configurable() && num_extra > 0 {
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
    if app.cache_is_configurable() {
        bar.action(
            CacheCtrlBtn::Results,
            "results",
            ControlState::chip(false, hov(CacheHoverTarget::ExportResults)),
            theme::ACCENT,
        );
    }
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
        ControlState::chip(
            matches!(app.cache.scope, CacheScope::ICache),
            hov(CacheHoverTarget::ScopeI),
        ),
        theme::ACCENT,
    )
    .value(
        CacheScopeBtn::D,
        "d-cache",
        ControlState::chip(
            matches!(app.cache.scope, CacheScope::DCache),
            hov(CacheHoverTarget::ScopeD),
        ),
        theme::ACCENT,
    )
    .value(
        CacheScopeBtn::Both,
        "both",
        ControlState::chip(
            matches!(app.cache.scope, CacheScope::Both),
            hov(CacheHoverTarget::ScopeBoth),
        ),
        theme::ACCENT,
    );
    bar
}

/// The Cache tab's whole chrome, in two borderless lines.
///
/// It used to be four nested boxes — a level strip, a `Cache Simulation` panel
/// whose entire content was one row of subtabs, an `Execution` panel, and an
/// untitled panel at the bottom holding the action group — which spent **12 of
/// 44 rows** drawing borders around four single lines of controls, and filled
/// the slack with key legends (`r=reset  f=speed  …`, `+/= add level`, `Tab to
/// switch`) that the footer and the help overlay already carry.
///
/// ```text
///  Cache   stats  view  settings  │  level  l1  add   L1 Split I/D  │  scope  i-cache …
///  speed 1x   state pause   reset  │  results               cyc 0   CPI 0.00   instr 0
/// ```
///
/// Line 1 answers *what am I looking at*, line 2 *what is it doing* — with the
/// metrics parked against the right edge so they stop sliding as the controls to
/// their left change width. Every group is a [`Toolbar`], so the columns the
/// mouse tests are the columns that were drawn.
fn render_cache_header(f: &mut Frame, area: Rect, app: &App) {
    let sep = || Span::styled("│", Style::default().fg(theme::BORDER));

    // ── Line 1: identity · subtabs · level · scope ──
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::styled(" Cache ", style::title()));
    row.gap(2);

    app.cache.subtab_header_origin.set((area.y, row.cursor()));
    for span in build_cache_subtab_bar(app).spans() {
        row.push(span);
    }

    row.gap(3);
    row.push(sep());
    row.gap(3);
    row.push(Span::styled("level", style::idle()));
    row.push(Span::raw(" "));
    app.cache.level_origin.set((area.y, row.cursor()));
    for span in build_cache_level_bar(app).spans() {
        row.push(span);
    }
    row.gap(3);
    row.push(Span::styled(
        if app.cache.selected_level == 0 {
            "L1 Split I/D".to_string()
        } else {
            format!("L{} Unified", app.cache.selected_level + 1)
        },
        style::label(),
    ));

    // Scope only means something while the split L1 is selected; the unified
    // upper levels have no I/D to choose between.
    if app.cache.selected_level == 0 {
        row.gap(3);
        row.push(sep());
        row.gap(3);
        // `scope`, not `view` — the subtab beside it is already called *view*,
        // and one word cannot label both "which pane" and "which cache".
        row.push(Span::styled("scope", style::idle()));
        row.push(Span::raw(" "));
        app.cache.ctrl_scope_origin.set((area.y, row.cursor()));
        for span in build_cache_scope_bar(app).spans() {
            row.push(span);
        }
    } else {
        app.cache.ctrl_scope_origin.set((0, 0));
    }
    let line1 = row.into_line();

    // ── Line 2: transport · actions · metrics ──
    let y2 = area.y + 2;
    let mut row = SpanRow::new(area.x, y2);
    row.gap(1);
    app.cache.exec_origin.set((y2, row.cursor()));
    for span in build_cache_exec_bar(app).spans() {
        row.push(span);
    }

    let actions = build_cache_ctrl_bar(app);
    if actions.width() > 0 {
        row.gap(3);
        row.push(sep());
        row.gap(3);
        app.cache.ctrl_origin.set((y2, row.cursor()));
        for span in actions.spans() {
            row.push(span);
        }
    } else {
        app.cache.ctrl_origin.set((0, 0));
    }

    let totals = app.execution_totals();
    let metrics = vec![
        style::metric_span("cyc ", totals.cycles, style::Metric::Cycles),
        Span::raw("   "),
        style::metric_span("CPI ", format!("{:.2}", totals.cpi()), style::Metric::Cpi),
        Span::raw("   "),
        Span::styled("instr", style::idle()),
        Span::styled(format!(" {}", totals.instructions), style::value()),
        Span::raw(" "),
    ];
    let metrics_w: u16 = metrics.iter().map(|s| s.width() as u16).sum();
    let right_edge = area.x + area.width;
    row.gap(
        right_edge
            .saturating_sub(metrics_w)
            .saturating_sub(row.cursor())
            .max(3),
    );
    for span in metrics {
        row.push(span);
    }

    f.render_widget(
        Paragraph::new(vec![line1, Line::raw(""), row.into_line()]),
        area,
    );
}

/// The Cache subtab bar — `[stats] [view] [settings]` — as a [`Toolbar`] keyed by
/// [`CacheSubtab`]. Shared by the renderer and `mouse::update_cache_hover` /
/// `handle_cache_click`, so the click targets cannot drift from the labels.
pub(crate) fn build_cache_subtab_bar(app: &App) -> Toolbar<CacheSubtab> {
    let st = |sub: CacheSubtab, t: CacheHoverTarget| {
        ControlState::chip(app.cache.subtab == sub, app.cache.hover == Some(t))
    };
    let mut bar = Toolbar::new();
    bar.value(
        CacheSubtab::Stats,
        "stats",
        st(CacheSubtab::Stats, CacheHoverTarget::SubtabStats),
        theme::ACCENT,
    )
    .value(
        CacheSubtab::View,
        "view",
        st(CacheSubtab::View, CacheHoverTarget::SubtabView),
        theme::ACCENT,
    )
    .value(
        CacheSubtab::Config,
        "settings",
        st(CacheSubtab::Config, CacheHoverTarget::SubtabConfig),
        theme::ACCENT,
    );
    bar
}

#[cfg(test)]
mod tests {
    use crate::ui::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|f| super::render_cache(f, f.area(), app))
            .expect("render");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The tab short-circuits to a notice when cache simulation is off, so
    /// these turn it on — the point here is the panes, not the notice.
    fn app(id: &str) -> App {
        let mut app =
            App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, id).unwrap();
        app.session.cache_enabled = true;
        app
    }

    /// One Cache tab for every backend that declares a cache — the parallel
    /// "portable" stats view is gone, so a lesser rendering cannot come back.
    #[test]
    fn every_caching_backend_gets_the_same_cache_panes() {
        for id in crate::arch::registry().ids() {
            let app = app(id);
            if !app.architecture.descriptor().capabilities.cache {
                continue;
            }
            let screen = screen(&app, 160, 40);
            for text in ["I-Cache", "D-Cache", "program total"] {
                assert!(screen.contains(text), "{id} is missing {text}:\n{screen}");
            }
        }
    }

    /// Editing controls follow the hierarchy's own answer, not the backend's
    /// name: a fixed cache offers no add/remove.
    #[test]
    fn level_editing_is_offered_only_where_the_cache_is_configurable() {
        let riscv = app("riscv32");
        assert!(riscv.cache_is_configurable());
        assert!(screen(&riscv, 160, 40).contains("add"));

        for id in ["sap", "toy16"] {
            let app = app(id);
            if !app.architecture.descriptor().capabilities.cache {
                continue;
            }
            assert!(!app.cache_is_configurable(), "{id} claims a tunable cache");
            assert!(
                !screen(&app, 160, 40).contains("add level"),
                "{id} offers level editing it cannot apply"
            );
        }
    }

    #[test]
    fn the_cache_tab_survives_small_terminals_on_every_architecture() {
        for id in crate::arch::registry().ids() {
            let app = app(id);
            for (w, h) in [(160, 40), (80, 24), (40, 12), (20, 6)] {
                let _ = screen(&app, w, h);
            }
        }
    }
}
