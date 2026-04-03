mod config_view;
mod main_view;

use crate::ui::app::App;
use crate::ui::pipeline::PipelineSubtab;
use crate::ui::theme;
use crate::ui::view::components::{dense_action, push_dense_pair};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
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

    if !app.pipeline.enabled {
        let p = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  Pipeline disabled.",
                Style::default().fg(theme::PAUSED).bold(),
            )),
            Line::from(Span::styled(
                "  Enable it in the Config tab (Config -> Pipeline Enabled).",
                Style::default().fg(theme::LABEL),
            )),
        ]);
        f.render_widget(p, layout[2]);
        return;
    }
    if app.editor.last_ok_text.is_none() {
        let p = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  No program loaded.",
                Style::default().fg(theme::PAUSED).bold(),
            )),
            Line::from(Span::styled(
                "  Compile in the Editor tab to load one.",
                Style::default().fg(theme::LABEL),
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

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
    let single_core = app.max_cores <= 1;

    let main_style = subtab_style(p.subtab == PipelineSubtab::Main, p.hover_subtab_main);
    let config_style = subtab_style(p.subtab == PipelineSubtab::Config, p.hover_subtab_config);
    let core_text = format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1));
    let core_style = if single_core {
        Style::default().fg(theme::LABEL)
    } else if p.hover_core {
        Style::default().fg(theme::ACTIVE).bold()
    } else {
        Style::default().fg(theme::TEXT).bold()
    };

    let line1 = Line::from(vec![
        Span::raw(" "),
        Span::styled("main", main_style),
        Span::raw("   "),
        Span::styled("config", config_style),
        Span::styled("   core ", Style::default().fg(theme::LABEL)),
        Span::styled(core_text.clone(), core_style),
        Span::styled(
            format!(
                " / Hart {} / {}",
                app.core_hart_id(app.selected_core)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                app.core_status(app.selected_core).label()
            ),
            Style::default().fg(theme::LABEL),
        ),
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
            " Pipeline Simulator ",
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);

    // Record button geometry for mouse: y=inner.y, x ranges
    // "main" starts at x = inner.x + 1, "config" starts at +8
    app.pipeline
        .btn_subtab_main_rect
        .set((inner.y, inner.x + 1, inner.x + 5));
    app.pipeline
        .btn_subtab_config_rect
        .set((inner.y, inner.x + 8, inner.x + 14));
    if single_core {
        app.pipeline.btn_core_rect.set((0, 0, 0));
    } else {
        let core_x = inner.x + 17;
        let core_w = ("core ".len() + core_text.len()) as u16;
        app.pipeline
            .btn_core_rect
            .set((inner.y, core_x, core_x + core_w));
    }
}

// ── Exec controls ─────────────────────────────────────────────────────────────

fn render_exec_controls(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
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

    let mut spans = Vec::new();
    push_dense_pair(
        &mut spans,
        "speed",
        p.speed.label(),
        p.hover_speed,
        true,
        theme::TEXT,
    );
    push_dense_pair(
        &mut spans,
        "state",
        state_label,
        p.hover_state && state_clickable,
        state_clickable,
        state_color,
    );
    spans.push(Span::raw("   "));
    spans.push(dense_action("reset", theme::DANGER, p.hover_reset));
    spans.push(Span::styled(
        "   r=reset  f=speed  s=step  p/Space=run",
        Style::default().fg(theme::LABEL),
    ));
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

    let line2 = Line::from(Span::styled(cpi_str, Style::default().fg(theme::LABEL)));
    let line3 = Line::from(Span::styled(stall_str, Style::default().fg(theme::LABEL)));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Execution", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2, line3]), inner);

    // Record button geometry for mouse
    // Layout: "speed <speed_label>   state <state_label>   reset"
    // Offsets: speed_label at +6, state_label at +15+speed_w, reset at +18+speed_w+state_w
    let speed_x = inner.x + 1;
    let speed_label_w = ("speed ".len() + p.speed.label().len()) as u16;
    app.pipeline
        .btn_speed_rect
        .set((inner.y, speed_x, speed_x + speed_label_w));
    let state_x = inner.x + 4 + speed_label_w;
    let state_label_w = ("state ".len() + state_label.len()) as u16;
    app.pipeline
        .btn_state_rect
        .set((inner.y, state_x, state_x + state_label_w));
    let reset_x = state_x + state_label_w + 3;
    app.pipeline
        .btn_reset_rect
        .set((inner.y, reset_x, reset_x + 5));
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

fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
    let mut spans = vec![
        Span::raw(" "),
        dense_action("results", theme::ACCENT, p.hover_export_results),
    ];

    if matches!(p.subtab, PipelineSubtab::Config) {
        spans.push(Span::raw("   "));
        spans.push(dense_action(
            "import cfg",
            theme::METRIC_CYC,
            p.hover_import_cfg,
        ));
        spans.push(Span::raw("   "));
        spans.push(dense_action(
            "export cfg",
            theme::METRIC_CYC,
            p.hover_export_cfg,
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);

    let y = inner.y;
    let x = inner.x + 1;
    app.pipeline
        .btn_export_results_rect
        .set((y, x, x + "results".len() as u16));

    if matches!(p.subtab, PipelineSubtab::Config) {
        let import_x = x + "results".len() as u16 + 3;
        let export_x = import_x + "import cfg".len() as u16 + 3;
        app.pipeline
            .btn_import_cfg_rect
            .set((y, import_x, import_x + "import cfg".len() as u16));
        app.pipeline
            .btn_export_cfg_rect
            .set((y, export_x, export_x + "export cfg".len() as u16));
    } else {
        app.pipeline.btn_import_cfg_rect.set((0, 0, 0));
        app.pipeline.btn_export_cfg_rect.set((0, 0, 0));
    }
}
