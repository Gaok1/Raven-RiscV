use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::{App, FormatMode, MemRegion, RunButton};
use crate::ui::theme;

pub(crate) fn render_run_status(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title("Run Controls");

    let para = Paragraph::new(status_lines(app)).block(block);
    f.render_widget(para, area);
}

pub(crate) fn run_controls_plain_text(app: &App) -> String {
    status_spans(app)
        .into_iter()
        .map(|span| span.content.to_string())
        .collect()
}

fn status_lines(app: &App) -> Vec<Line<'static>> {
    let hint = Line::from(""); // hints removed — use [?] help button
    vec![Line::from(status_spans(app)), cycle_line(app), hint]
}

fn cycle_line(app: &App) -> Line<'static> {
    let (total, cpi, instr) = if app.pipeline.enabled || app.pipeline.sequential_mode {
        let cycles = app.pipeline.cycle_count;
        let cpi = if app.pipeline.instr_committed > 0 {
            cycles as f64 / app.pipeline.instr_committed as f64
        } else {
            0.0
        };
        (cycles, cpi, app.pipeline.instr_committed)
    } else {
        (
            app.run.mem.total_program_cycles(),
            app.run.mem.overall_cpi(),
            app.run.mem.instruction_count,
        )
    };
    let scope_label = if app.pipeline.enabled || app.pipeline.sequential_mode {
        "Scope:selected"
    } else {
        "Scope:program"
    };
    Line::from(vec![
        Span::styled(
            format!(
                "Core:{}  Hart:{}  {}",
                app.selected_core,
                app.core_hart_id(app.selected_core)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                app.core_status(app.selected_core).label()
            ),
            Style::default().fg(theme::ACCENT),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Cycles:{total}"),
            Style::default().fg(theme::METRIC_CYC),
        ),
        Span::raw("  "),
        Span::styled(
            format!("CPI:{cpi:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        ),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(theme::LABEL)),
        Span::raw("  "),
        Span::styled(scope_label, Style::default().fg(theme::LABEL)),
    ])
}

fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    push_dense_pair(
        &mut spans,
        "core",
        &format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1)),
        app.max_cores > 1 && app.hover_run_button == Some(RunButton::Core),
        app.max_cores > 1,
        if app.max_cores > 1 {
            theme::TEXT
        } else {
            theme::IDLE
        },
    );

    push_dense_pair(
        &mut spans,
        "view",
        view_text(app),
        app.hover_run_button == Some(RunButton::View),
        view_active(app),
        theme::TEXT,
    );

    if app.run_sidebar_shows_memory() {
        push_dense_pair(
            &mut spans,
            "region",
            region_text(app),
            app.hover_run_button == Some(RunButton::Region),
            true,
            theme::TEXT,
        );
    }

    push_dense_pair(
        &mut spans,
        "fmt",
        format_text(app),
        app.hover_run_button == Some(RunButton::Format),
        true,
        theme::TEXT,
    );

    push_dense_pair(
        &mut spans,
        "sign",
        sign_text(app),
        sign_enabled(app) && app.hover_run_button == Some(RunButton::Sign),
        sign_enabled(app),
        if sign_enabled(app) {
            theme::TEXT
        } else {
            theme::IDLE
        },
    );

    if app.run_sidebar_shows_memory() {
        push_dense_pair(
            &mut spans,
            "bytes",
            bytes_text(app),
            app.hover_run_button == Some(RunButton::Bytes),
            true,
            theme::TEXT,
        );
    }

    push_dense_pair(
        &mut spans,
        "speed",
        speed_text(app),
        app.hover_run_button == Some(RunButton::Speed),
        true,
        theme::TEXT,
    );

    push_dense_pair(
        &mut spans,
        "state",
        &state_text(app),
        app.hover_run_button == Some(RunButton::State),
        true,
        state_color(app),
    );

    push_dense_pair(
        &mut spans,
        "count",
        if app.run.show_exec_count { "on" } else { "off" },
        app.hover_run_button == Some(RunButton::ExecCount),
        app.run.show_exec_count,
        if app.run.show_exec_count {
            theme::TEXT
        } else {
            theme::IDLE
        },
    );

    push_dense_pair(
        &mut spans,
        "type",
        if app.run.show_instr_type { "on" } else { "off" },
        app.hover_run_button == Some(RunButton::InstrType),
        app.run.show_instr_type,
        if app.run.show_instr_type {
            theme::TEXT
        } else {
            theme::IDLE
        },
    );

    if !spans.is_empty() {
        spans.push(Span::raw("   "));
    }
    spans.push(action_btn(
        "reset",
        theme::DANGER,
        app.hover_run_button == Some(RunButton::Reset),
    ));

    spans
}

// ── Text helpers ────────────────────────────────────────────────────────────

fn view_text(app: &App) -> &'static str {
    if app.run.show_dyn {
        "dyn"
    } else if app.run.show_registers {
        "regs"
    } else {
        "ram"
    }
}

fn view_active(_app: &App) -> bool {
    // always considered "active" — it cycles between states
    true
}

fn region_text(app: &App) -> &'static str {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => "data",
        MemRegion::Stack => "stack",
        MemRegion::Access => "r/w",
        MemRegion::Heap => "heap",
    }
}

fn format_text(app: &App) -> &'static str {
    match app.run.fmt_mode {
        FormatMode::Hex => "hex",
        FormatMode::Dec => "dec",
        FormatMode::Str => "str",
    }
}

fn sign_enabled(app: &App) -> bool {
    matches!(app.run.fmt_mode, FormatMode::Dec)
}

fn sign_text(app: &App) -> &'static str {
    if app.run.show_signed { "sgn" } else { "uns" }
}

fn bytes_text(app: &App) -> &'static str {
    match app.run.mem_view_bytes {
        4 => "4b",
        2 => "2b",
        _ => "1b",
    }
}

fn speed_text(app: &App) -> &'static str {
    match app.run.speed.label() {
        "1x" => "1x",
        "2x" => "2x",
        "4x" => "4x",
        "8x" => "8x",
        "GO" => "go",
        other => other,
    }
}

pub(crate) fn state_text(app: &App) -> String {
    match app.core_status(app.selected_core) {
        crate::ui::app::HartLifecycle::Free => "free".to_string(),
        crate::ui::app::HartLifecycle::Running => "run".to_string(),
        crate::ui::app::HartLifecycle::Paused => {
            if app.run.cpu.ebreak_hit {
                "ebrk".to_string()
            } else {
                "pause".to_string()
            }
        }
        crate::ui::app::HartLifecycle::Exited => {
            if app.run.cpu.local_exit {
                "halt".to_string()
            } else {
                "exit".to_string()
            }
        }
        crate::ui::app::HartLifecycle::Faulted => "fault".to_string(),
    }
}

fn state_color(app: &App) -> Color {
    match app.core_status(app.selected_core) {
        crate::ui::app::HartLifecycle::Free => theme::IDLE,
        crate::ui::app::HartLifecycle::Running => theme::RUNNING,
        crate::ui::app::HartLifecycle::Paused => theme::PAUSED,
        crate::ui::app::HartLifecycle::Exited => {
            if app.run.cpu.local_exit {
                theme::DANGER
            } else {
                theme::LABEL
            }
        }
        crate::ui::app::HartLifecycle::Faulted => theme::DANGER,
    }
}

fn push_dense_pair(
    spans: &mut Vec<Span<'static>>,
    label: &str,
    value: &str,
    hovered: bool,
    active: bool,
    active_color: Color,
) {
    if !spans.is_empty() {
        spans.push(Span::raw("   "));
    }
    spans.push(Span::styled(
        label.to_string(),
        Style::default().fg(theme::IDLE),
    ));
    spans.push(Span::raw(" "));
    spans.push(value_btn(value, hovered, active, active_color));
}

fn value_btn(text: &str, hovered: bool, active: bool, color: Color) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    };
    Span::styled(text.to_string(), style)
}

fn action_btn(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };
    Span::styled(text.to_string(), style)
}
