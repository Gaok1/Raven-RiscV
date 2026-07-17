mod config_view;
mod main_view;

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

pub fn render_pipeline(f: &mut Frame, area: Rect, app: &App) {
    app.run.pipeline().gantt_area_rect.set((0, 0, 0, 0));
    if !matches!(app.run.pipeline().subtab, PipelineSubtab::Config) {
        app.run.pipeline()
            .config_row_rects
            .set([(0, 0, 0); crate::ui::pipeline::PipelineBypassConfig::CONFIG_ROWS]);
    }

    // Layout: merged header (2) | content (min)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(f, layout[0], app);

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

    match app.run.pipeline().subtab {
        PipelineSubtab::Main => main_view::render_pipeline_main(f, layout[1], app),
        PipelineSubtab::Config => config_view::render_pipeline_config(f, layout[1], app),
    }
}

// ── Merged header ─────────────────────────────────────────────────────────────
//
// Two borderless lines replacing the old subtab / Execution / bottom-bar boxes:
//   L1: title, subtab buttons, core/hart/status, speed/state/reset, file actions
//   L2: cycle metrics + stall breakdown (+ sequential note / key hints)

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();
    let single_core = app.max_cores <= 1;
    let state_clickable = !p.faulted;

    let (state_label, state_color) = if p.faulted {
        ("fault", theme::DANGER)
    } else if p.halted {
        ("halt", theme::PAUSED)
    } else if app.run.is_running {
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
    row.push(dense_value(p.speed.label(), p.hover_speed, true, theme::TEXT));
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
    row.push(dense_action("results", theme::ACCENT, p.hover_export_results));
    row.record_hitbox(start, &p.btn_export_results_rect);

    if matches!(p.subtab, PipelineSubtab::Config) {
        row.gap(3);
        let start = row.cursor();
        row.push(dense_action("import cfg", theme::METRIC_CYC, p.hover_import_cfg));
        row.record_hitbox(start, &p.btn_import_cfg_rect);
        row.gap(3);
        let start = row.cursor();
        row.push(dense_action("export cfg", theme::METRIC_CYC, p.hover_export_cfg));
        row.record_hitbox(start, &p.btn_export_cfg_rect);
    } else {
        p.btn_import_cfg_rect.set((0, 0, 0));
        p.btn_export_cfg_rect.set((0, 0, 0));
    }
    let line1 = row.into_line();

    // ── Line 2: metrics ──
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        format!(" cyc {}", p.cycle_count),
        Style::default().fg(theme::METRIC_CYC),
    )];
    if p.instr_committed > 0 {
        let cpi = p.cycle_count as f64 / p.instr_committed as f64;
        spans.push(Span::styled(
            format!("  CPI {cpi:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        ));
        let stalls = if header_drops_stall_breakdown(area.width) {
            format!("  instr {}  stalls {}", p.instr_committed, p.stall_count)
        } else {
            let [raw, lu, br, fu, mem] = p.stall_by_type;
            format!(
                "  instr {}  stalls {} (RAW {raw} · LD {lu} · BR {br} · FU {fu} · MEM {mem})",
                p.instr_committed, p.stall_count
            )
        };
        spans.push(Span::styled(stalls, Style::default().fg(theme::LABEL)));
        if p.branches_executed > 0 {
            let mispredict_pct = p.flush_count as f64 / p.branches_executed as f64 * 100.0;
            spans.push(Span::styled(
                format!(
                    "  br {} · mispred {} ({mispredict_pct:.0}%)",
                    p.branches_executed, p.flush_count
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
    if p.sequential_mode {
        spans.push(Span::styled(
            "  ·  Sequential (pipeline off)",
            Style::default().fg(theme::PAUSED),
        ));
    } else {
        spans.push(Span::styled(
            "   s=step · p=run · r=reset · f=speed",
            Style::default().fg(theme::LABEL).add_modifier(Modifier::DIM),
        ));
    }
    let line2 = Line::from(spans);

    f.render_widget(Paragraph::new(vec![line1, line2]), area);
}

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
