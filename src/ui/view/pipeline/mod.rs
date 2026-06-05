mod config_view;
mod main_view;

use crate::ui::app::App;
use crate::ui::pipeline::PipelineSubtab;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style;

/// A button in the pipeline header bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineHeaderBtn {
    Main,
    Config,
    Core,
}

/// A button in the pipeline exec-controls bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineExecBtn {
    Speed,
    State,
    Reset,
}

/// A button in the pipeline bottom controls bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineCtrlBtn {
    Results,
    ImportCfg,
    ExportCfg,
}

/// The live `(label, color)` of the exec `state` chip.
fn pipeline_state_chip(app: &App) -> (&'static str, ratatui::style::Color) {
    let p = &app.pipeline;
    if p.faulted {
        ("fault", theme::DANGER)
    } else if p.halted {
        ("halt", theme::PAUSED)
    } else if app.run.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    }
}

/// The exec-controls bar — `speed <s>  state <s>  reset` — as a [`Toolbar`].
pub(crate) fn build_pipeline_exec_bar(app: &App) -> Toolbar<PipelineExecBtn> {
    let p = &app.pipeline;
    let state_clickable = !p.faulted;
    let (state_label, state_color) = pipeline_state_chip(app);
    let mut bar = Toolbar::new();
    bar.toggle(
        PipelineExecBtn::Speed,
        "speed",
        p.speed.label(),
        ControlState::chip(true, p.hover_speed),
        theme::TEXT,
    );
    let state_ctrl = if state_clickable {
        ControlState::chip(true, p.hover_state)
    } else {
        ControlState::Disabled
    };
    bar.toggle(PipelineExecBtn::State, "state", state_label, state_ctrl, state_color);
    bar.action(
        PipelineExecBtn::Reset,
        "reset",
        ControlState::chip(false, p.hover_reset),
        theme::DANGER,
    );
    bar
}

/// The bottom controls bar — `results` (+ `import cfg` `export cfg` in Config) —
/// as a [`Toolbar`].
pub(crate) fn build_pipeline_ctrl_bar(app: &App) -> Toolbar<PipelineCtrlBtn> {
    let p = &app.pipeline;
    let mut bar = Toolbar::new();
    bar.action(
        PipelineCtrlBtn::Results,
        "results",
        ControlState::chip(false, p.hover_export_results),
        theme::ACCENT,
    );
    if matches!(p.subtab, PipelineSubtab::Config) {
        bar.action(
            PipelineCtrlBtn::ImportCfg,
            "import cfg",
            ControlState::chip(false, p.hover_import_cfg),
            theme::METRIC_CYC,
        )
        .action(
            PipelineCtrlBtn::ExportCfg,
            "export cfg",
            ControlState::chip(false, p.hover_export_cfg),
            theme::METRIC_CYC,
        );
    }
    bar
}
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::Paragraph,
};

pub fn render_pipeline(f: &mut Frame, area: Rect, app: &App) {
    app.pipeline.gantt_area_rect.set((0, 0, 0, 0));
    if !matches!(app.pipeline.subtab, PipelineSubtab::Config) {
        app.pipeline
            .config_row_rects
            .set([(0, 0, 0); crate::ui::pipeline::PipelineBypassConfig::CONFIG_ROWS]);
    }

    // Layout: subtab_header (3) | exec_controls (4) | content (min) | controls (3)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    render_subtab_header(f, layout[0], app);
    render_exec_controls(f, layout[1], app);
    render_controls_bar(f, layout[3], app);

    // When pipeline is disabled the sequential visualization is available;
    // fall through to the normal rendering path.
    if app.editor.last_ok_text.is_none() {
        let p = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  No program loaded.",
                style::warning().bold(),
            )),
            Line::from(Span::styled(
                "  Compile in the Editor tab to load one.",
                style::label(),
            )),
        ]);
        f.render_widget(p, layout[2]);
        return;
    }

    match app.pipeline.subtab {
        PipelineSubtab::Main => main_view::render_pipeline_main(f, layout[2], app),
        PipelineSubtab::Config => config_view::render_pipeline_config(f, layout[2], app),
    }
}

// ── Subtab header ─────────────────────────────────────────────────────────────

/// The pipeline header bar — `[main] [settings]  core N/M` — as a [`Toolbar`].
/// The two subtabs light up in ACCENT when selected; `core` is a stepper that
/// keeps its off-white value and is `Disabled` (inert) on a single-core machine.
/// Shared by the renderer and `mouse::update_pipeline_hover` / click.
pub(crate) fn build_pipeline_header_bar(app: &App) -> Toolbar<PipelineHeaderBtn> {
    let p = &app.pipeline;
    let single_core = app.max_cores <= 1;
    let core_text = format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1));
    let mut bar = Toolbar::new();
    bar.value(
        PipelineHeaderBtn::Main,
        "main",
        ControlState::chip(p.subtab == PipelineSubtab::Main, p.hover_subtab_main),
        theme::ACCENT,
    )
    .value(
        PipelineHeaderBtn::Config,
        "settings",
        ControlState::chip(p.subtab == PipelineSubtab::Config, p.hover_subtab_config),
        theme::ACCENT,
    );
    let core_state = if single_core {
        ControlState::Disabled
    } else {
        ControlState::chip(true, p.hover_core)
    };
    bar.toggle(PipelineHeaderBtn::Core, "core", &core_text, core_state, theme::TEXT);
    bar
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel(" Pipeline Simulator ", PanelKind::Accent),
    );
    app.pipeline.header_origin.set((inner.y, inner.x + 1));

    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_pipeline_header_bar(app).spans());
    spans.push(Span::styled(
        format!(
            " / Hart {} / {}",
            app.core_hart_id(app.selected_core)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            app.core_status(app.selected_core).label()
        ),
        style::label(),
    ));
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", style::label()),
    ]);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

// ── Exec controls ─────────────────────────────────────────────────────────────

fn render_exec_controls(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_pipeline_exec_bar(app).spans());
    if p.sequential_mode {
        spans.push(Span::styled(
            "   Sequential (pipeline off) — one instruction at a time",
            style::warning(),
        ));
    } else {
        spans.push(Span::styled(
            "   r=reset  f=speed  s=step  p/Space=run",
            style::label(),
        ));
    }
    let line1 = Line::from(spans);

    let (cpi_str, stall_str) = if p.instr_committed > 0 {
        let cpi = p.cycle_count as f64 / p.instr_committed as f64;
        let branch_str = if p.branches_executed > 0 {
            let mispredict_pct = p.flush_count as f64 / p.branches_executed as f64 * 100.0;
            format!(
                "  control:{}  mispred:{} ({:.0}%)",
                p.branches_executed, p.flush_count, mispredict_pct
            )
        } else {
            String::new()
        };
        let main = format!(
            " Cycle:{}  CPI:{cpi:.2}  instrs:{}  stalls:{}{}",
            p.cycle_count, p.instr_committed, p.stall_count, branch_str,
        );
        let [raw, lu, br, fu, mem] = p.stall_by_type;
        let detail =
            format!(" Stall tags — RAW:{raw}  Load-Use:{lu}  Branch:{br}  FU:{fu}  Mem:{mem}");
        (main, detail)
    } else {
        (
            format!(" Cycle:{}  (no instructions committed)", p.cycle_count),
            String::new(),
        )
    };

    let line2 = Line::from(Span::styled(cpi_str, style::label()));
    let line3 = Line::from(Span::styled(stall_str, style::label()));

    let inner = render_panel(f, area, panel::panel("Execution", PanelKind::Plain));
    app.pipeline.exec_origin.set((inner.y, inner.x + 1));
    f.render_widget(Paragraph::new(vec![line1, line2, line3]), inner);
}


fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_pipeline_ctrl_bar(app).spans());

    let inner = render_panel(f, area, panel::panel_frame(PanelKind::Plain));
    app.pipeline.ctrl_origin.set((inner.y, inner.x + 1));
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}
