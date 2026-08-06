mod config_view;
mod main_view;
mod ooo;

pub(crate) use main_view::{MainLayoutPlan, plan_main_layout};

use crate::ui::app::App;
use crate::ui::pipeline::PipelineSubtab;
use crate::ui::theme;
use crate::ui::view::components::{SpanRow, dense_action, dense_value};
use crate::ui::view::style;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::Paragraph,
};
use raven_engine::capability::{PipelineInspect, PipelineStats};

pub fn render_pipeline(f: &mut Frame, area: Rect, app: &App) {
    app.run.pipeline_view().gantt_area_rect.set((0, 0, 0, 0));
    if !matches!(app.run.pipeline_view().subtab, PipelineSubtab::Config) {
        app.run
            .pipeline_view()
            .config_row_rects
            .set([(0, 0, 0); crate::ui::pipeline::PIPELINE_CONFIG_ROWS]);
    }

    // Layout: merged header | content. Three rows, not two: the controls and the
    // metrics under them are separate groups, and the blank between them is what
    // says so — the same rhythm every other tab's header now keeps.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(PIPELINE_HEADER_H), Constraint::Min(0)])
        .split(area);

    // The tab is only offered to a backend that declares a pipeline, so `None`
    // here means the capability went away under us — draw nothing rather than
    // an empty datapath.
    let Some(pipeline) = app.pipeline() else {
        return;
    };
    render_header(f, layout[0], app, pipeline);

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
        f.render_widget(p, layout[1]);
        return;
    }

    match app.run.pipeline_view().subtab {
        // A model that has left program order has no stages to draw, so it gets
        // the workbench of structures that replaced them. Asking the backend
        // each frame rather than once: the model is a setting, and a user can
        // change it between two frames.
        PipelineSubtab::Main => match app.pipeline_dynamic() {
            Some(model) => ooo::render_pipeline_ooo(f, layout[1], app, pipeline, model),
            None => main_view::render_pipeline_main(f, layout[1], app, pipeline),
        },
        PipelineSubtab::Config => config_view::render_pipeline_config(f, layout[1], app),
    }
}

// ── Merged header ─────────────────────────────────────────────────────────────
//
// Two borderless lines replacing the old subtab / Execution / bottom-bar boxes:
//   L1: title, subtab buttons, core/hart/status, speed/state/reset, file actions
//   L2: cycle metrics + stall breakdown (+ sequential note / key hints)

fn render_header(f: &mut Frame, area: Rect, app: &App, inspect: &dyn PipelineInspect) {
    let p = app.run.pipeline_view();
    let status = inspect.status();
    let stats = inspect.stats();
    let single_core = app.max_cores <= 1;
    let state_clickable = !status.faulted;

    let (state_label, state_color) = if status.faulted {
        ("fault", theme::DANGER)
    } else if status.halted {
        ("halt", theme::PAUSED)
    } else if app.session.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };

    // ── Line 1: buttons ──
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::styled(
        " Pipeline ",
        Style::default().fg(theme::ACCENT).bold(),
    ));
    row.gap(1);

    let start = row.cursor();
    row.push(Span::styled(
        "main",
        subtab_style(p.subtab == PipelineSubtab::Main, p.hover_subtab_main),
    ));
    row.record_hitbox(start, &p.btn_subtab_main_rect);
    row.gap(2);
    let start = row.cursor();
    row.push(Span::styled(
        "settings",
        subtab_style(p.subtab == PipelineSubtab::Config, p.hover_subtab_config),
    ));
    row.record_hitbox(start, &p.btn_subtab_config_rect);

    row.gap(3);
    let core_style = if single_core {
        Style::default().fg(theme::LABEL)
    } else if p.hover_core {
        Style::default().fg(theme::ACTIVE).bold()
    } else {
        Style::default().fg(theme::TEXT).bold()
    };
    let start = row.cursor();
    row.push(Span::styled("core ", Style::default().fg(theme::LABEL)));
    row.push(Span::styled(
        format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1)),
        core_style,
    ));
    if single_core {
        p.btn_core_rect.set((0, 0, 0));
    } else {
        row.record_hitbox(start, &p.btn_core_rect);
    }
    row.push(Span::styled(
        format!(
            " · hart {} · {}",
            app.core_hart_id(app.selected_core)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            app.core_status(app.selected_core).label()
        ),
        Style::default().fg(theme::LABEL),
    ));

    row.gap(3);
    let start = row.cursor();
    row.push(Span::styled("speed ", Style::default().fg(theme::IDLE)));
    row.push(dense_value(
        p.speed.label(),
        p.hover_speed,
        true,
        theme::TEXT,
    ));
    row.record_hitbox(start, &p.btn_speed_rect);

    row.gap(3);
    let start = row.cursor();
    row.push(Span::styled("state ", Style::default().fg(theme::IDLE)));
    row.push(dense_value(
        state_label,
        p.hover_state && state_clickable,
        state_clickable,
        state_color,
    ));
    row.record_hitbox(start, &p.btn_state_rect);

    row.gap(3);
    let start = row.cursor();
    row.push(dense_action("reset", theme::DANGER, p.hover_reset));
    row.record_hitbox(start, &p.btn_reset_rect);

    row.gap(3);
    let start = row.cursor();
    row.push(dense_action(
        "results",
        theme::ACCENT,
        p.hover_export_results,
    ));
    row.record_hitbox(start, &p.btn_export_results_rect);

    if matches!(p.subtab, PipelineSubtab::Config) {
        row.gap(3);
        let start = row.cursor();
        row.push(dense_action(
            "import cfg",
            theme::METRIC_CYC,
            p.hover_import_cfg,
        ));
        row.record_hitbox(start, &p.btn_import_cfg_rect);
        row.gap(3);
        let start = row.cursor();
        row.push(dense_action(
            "export cfg",
            theme::METRIC_CYC,
            p.hover_export_cfg,
        ));
        row.record_hitbox(start, &p.btn_export_cfg_rect);
    } else {
        p.btn_import_cfg_rect.set((0, 0, 0));
        p.btn_export_cfg_rect.set((0, 0, 0));
    }
    let line1 = row.into_line();

    // ── Line 2: metrics ──
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        format!(" cyc {}", stats.cycles),
        Style::default().fg(theme::METRIC_CYC),
    )];
    if stats.committed > 0 {
        let cpi = stats.cpi();
        spans.push(Span::styled(
            format!("  CPI {cpi:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        ));
        spans.push(Span::styled(
            stall_breakdown(&stats, area.width),
            Style::default().fg(theme::LABEL),
        ));
        if stats.branches > 0 {
            let mispredict_pct = stats.flushes as f64 / stats.branches as f64 * 100.0;
            spans.push(Span::styled(
                format!(
                    "  br {} · mispred {} ({mispredict_pct:.0}%)",
                    stats.branches, stats.flushes
                ),
                Style::default().fg(theme::LABEL),
            ));
        }
    } else {
        spans.push(Span::styled(
            "  (no instructions committed)",
            Style::default().fg(theme::LABEL),
        ));
    }
    if status.sequential {
        spans.push(Span::styled(
            "  ·  Sequential (pipeline off)",
            Style::default().fg(theme::PAUSED),
        ));
    } else {
        // The footer already lists these keys; a second copy here spent a row
        // repeating itself and put keys somewhere the design system reserves for
        // state.
    }
    let line2 = Line::from(spans);

    f.render_widget(Paragraph::new(vec![line1, Line::raw(""), line2]), area);
}

/// What the cycles went to, for the metrics line.
///
/// The two name hazards are listed only when something actually paid for them,
/// which is the same as saying: only under a model that has no spare names. In
/// an in-order or renaming machine they are permanently zero, and a pair of
/// zeroes on every screen would be five words of noise — but where they are not
/// zero, they are the number the model is being watched for.
fn stall_breakdown(stats: &PipelineStats, width: u16) -> String {
    if header_drops_stall_breakdown(width) {
        return format!("  instr {}  stalls {}", stats.committed, stats.stalls);
    }
    let raw = stats.raw_stalls;
    let lu = stats.load_use_stalls;
    let br = stats.branch_stalls;
    let fu = stats.functional_unit_stalls;
    let mem = stats.memory_stalls;
    let names = match (stats.waw_stalls, stats.war_stalls) {
        (0, 0) => String::new(),
        (waw, war) => format!(" · WAW {waw} · WAR {war}"),
    };
    format!(
        "  instr {}  stalls {} (RAW {raw} · LD {lu} · BR {br} · FU {fu} · MEM {mem}{names})",
        stats.committed, stats.stalls
    )
}

/// Rows the Pipeline header occupies: controls, gap, metrics.
pub(crate) const PIPELINE_HEADER_H: u16 = 3;

/// Below this width header line 2 shows only the stall total, without the
/// per-type breakdown.
fn header_drops_stall_breakdown(w: u16) -> bool {
    w < 90
}

fn subtab_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(theme::ACTIVE).bold()
    } else if hovered {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::IDLE)
    }
}

#[cfg(test)]
mod tests {
    use super::stall_breakdown;
    use raven_engine::capability::PipelineStats;

    #[test]
    fn the_name_hazards_are_listed_only_by_a_model_that_pays_for_them() {
        let in_order = PipelineStats {
            committed: 4,
            stalls: 2,
            raw_stalls: 2,
            ..PipelineStats::default()
        };
        let line = stall_breakdown(&in_order, 120);
        assert!(line.contains("RAW 2"));
        assert!(
            !line.contains("WAW"),
            "two permanent zeroes are noise: {line}"
        );

        let scoreboard = PipelineStats {
            waw_stalls: 3,
            war_stalls: 1,
            ..in_order
        };
        let line = stall_breakdown(&scoreboard, 120);
        assert!(line.contains("WAW 3") && line.contains("WAR 1"), "{line}");

        // A narrow header keeps only the total, whichever model is running.
        assert!(!stall_breakdown(&scoreboard, 80).contains("WAW"));
    }
}

