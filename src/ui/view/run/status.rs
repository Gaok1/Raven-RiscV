use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::{App, FormatMode, MemRegion, RunButton};
use crate::ui::theme;
use crate::ui::view::components::Toolbar;

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
    vec![
        Line::from(status_spans(app)),
        cycle_line(app),
        edit_status_line(app),
    ]
}

/// Third status row: while an inline edit is open it shows the commit/cancel
/// prompt, or the rejection message when the last commit was out of range.
/// Otherwise blank (the old hints moved to the `[?]` help button).
fn edit_status_line(app: &App) -> Line<'static> {
    if let Some(error) = &app.run.run_edit_error {
        Line::from(Span::styled(
            format!("  ✗ {error}"),
            Style::default().fg(Color::Red).bold(),
        ))
    } else if app.run.run_edit.is_some() {
        Line::from(Span::styled(
            "  editing — Enter=commit  Esc=cancel  ⌫=delete",
            Style::default().fg(theme::ACCENT),
        ))
    } else {
        Line::from("")
    }
}

fn cycle_line(app: &App) -> Line<'static> {
    let (total, cpi, instr) = if app.run.pipeline().enabled || app.run.pipeline().sequential_mode {
        let cycles = app.run.pipeline().cycle_count;
        let cpi = if app.run.pipeline().instr_committed > 0 {
            cycles as f64 / app.run.pipeline().instr_committed as f64
        } else {
            0.0
        };
        (cycles, cpi, app.run.pipeline().instr_committed)
    } else {
        (
            app.run.mem().total_program_cycles(),
            app.run.mem().overall_cpi(),
            app.run.mem().instruction_count,
        )
    };
    let scope_label = if app.run.pipeline().enabled || app.run.pipeline().sequential_mode {
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
    build_run_toolbar(app).spans()
}

/// The run-controls bar as a [`Toolbar`] — the single source of truth shared by
/// the renderer ([`status_spans`]) and the mouse hit-test
/// (`mouse::run_status_hit`). Add a control here and it shows up in the view and
/// becomes clickable in one edit.
pub(crate) fn build_run_toolbar(app: &App) -> Toolbar<RunButton> {
    let hov = |b: RunButton| app.hover_run_button == Some(b);
    let multi_core = app.max_cores > 1;
    let signed = sign_enabled(app);

    let mut bar = Toolbar::new();
    bar.pair(
        RunButton::Core,
        "core",
        &format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1)),
        multi_core && hov(RunButton::Core),
        multi_core,
        multi_core,
        theme::TEXT,
    )
    .pair(
        RunButton::View,
        "view",
        view_text(app),
        hov(RunButton::View),
        view_active(app),
        true,
        theme::TEXT,
    );

    if app.run_sidebar_shows_memory() {
        bar.pair(
            RunButton::Region,
            "region",
            region_text(app),
            hov(RunButton::Region),
            true,
            true,
            theme::TEXT,
        );
    }

    bar.pair(
        RunButton::Format,
        "fmt",
        format_text(app),
        hov(RunButton::Format),
        true,
        true,
        theme::TEXT,
    )
    .pair(
        RunButton::Sign,
        "sign",
        sign_text(app),
        signed && hov(RunButton::Sign),
        signed,
        signed,
        theme::TEXT,
    );

    if app.run_sidebar_shows_memory() {
        bar.pair(
            RunButton::Bytes,
            "bytes",
            bytes_text(app),
            hov(RunButton::Bytes),
            true,
            true,
            theme::TEXT,
        );
    }

    bar.pair(
        RunButton::Speed,
        "speed",
        speed_text(app),
        hov(RunButton::Speed),
        true,
        true,
        theme::TEXT,
    )
    .pair(
        RunButton::State,
        "state",
        &state_text(app),
        hov(RunButton::State),
        true,
        true,
        state_color(app),
    )
    .pair(
        RunButton::ExecCount,
        "count",
        if app.run.show_exec_count { "on" } else { "off" },
        hov(RunButton::ExecCount),
        app.run.show_exec_count,
        true,
        theme::TEXT,
    )
    .pair(
        RunButton::InstrType,
        "type",
        if app.run.show_instr_type { "on" } else { "off" },
        hov(RunButton::InstrType),
        app.run.show_instr_type,
        true,
        theme::TEXT,
    );

    let can_stepback = app.can_stepback_now();
    bar.action(
        RunButton::Stepback,
        "step-back",
        can_stepback && hov(RunButton::Stepback),
        can_stepback,
        theme::ACCENT,
    )
    .action(
        RunButton::Reset,
        "reset",
        hov(RunButton::Reset),
        true,
        theme::DANGER,
    );

    bar
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
        FormatMode::Bin => "bin",
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
            if app.run.cpu().ebreak_hit {
                "ebrk".to_string()
            } else {
                "pause".to_string()
            }
        }
        crate::ui::app::HartLifecycle::Exited => {
            if app.run.cpu().local_exit {
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
            if app.run.cpu().local_exit {
                theme::DANGER
            } else {
                theme::LABEL
            }
        }
        crate::ui::app::HartLifecycle::Faulted => theme::DANGER,
    }
}

