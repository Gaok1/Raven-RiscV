use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::ui::theme;
use super::{App, FormatMode, MemRegion, RunButton};

pub(crate) fn render_run_status(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title("Run Controls");

    let para = Paragraph::new(status_lines(app)).block(block);
    f.render_widget(para, area);
}

fn status_lines(app: &App) -> Vec<Line<'static>> {
    let hint = Line::from(""); // hints removed — use [?] help button
    vec![Line::from(status_spans(app)), cycle_line(app), hint]
}

fn cycle_line(app: &App) -> Line<'static> {
    let total = app.run.mem.total_program_cycles();
    let cpi   = app.run.mem.overall_cpi();
    let instr = app.run.mem.instruction_count;
    Line::from(vec![
        Span::styled(format!("Cycles:{total}"), Style::default().fg(theme::METRIC_CYC)),
        Span::raw("  "),
        Span::styled(format!("CPI:{cpi:.2}"), Style::default().fg(theme::METRIC_CPI)),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(theme::LABEL)),
    ])
}

fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    spans.push(Span::raw("View "));
    spans.push(toggle_btn(
        view_text(app),
        view_active(app),
        app.hover_run_button == Some(RunButton::View),
    ));

    if !app.run.show_registers && !app.run.show_bp_list {
        spans.push(Span::raw("  Region "));
        spans.push(toggle_btn(
            region_text(app),
            true,
            app.hover_run_button == Some(RunButton::Region),
        ));
    }

    spans.push(Span::raw("  Format "));
    spans.push(toggle_btn(
        format_text(app),
        true,
        app.hover_run_button == Some(RunButton::Format),
    ));

    spans.push(Span::raw("  Sign "));
    spans.push(toggle_btn(
        sign_text(app),
        sign_enabled(app),
        sign_enabled(app) && app.hover_run_button == Some(RunButton::Sign),
    ));

    if !app.run.show_registers && !app.run.show_bp_list {
        spans.push(Span::raw("  Bytes "));
        spans.push(toggle_btn(
            bytes_text(app),
            true,
            app.hover_run_button == Some(RunButton::Bytes),
        ));
    }

    spans.push(Span::raw("  Speed "));
    spans.push(toggle_btn(
        speed_text(app),
        true,
        app.hover_run_button == Some(RunButton::Speed),
    ));

    spans.push(Span::raw("  State "));
    spans.push(state_btn(app));

    spans.push(Span::raw("  Count "));
    spans.push(toggle_btn(
        if app.run.show_exec_count { "ON" } else { "OFF" },
        app.run.show_exec_count,
        app.hover_run_button == Some(RunButton::ExecCount),
    ));

    spans.push(Span::raw("  Type "));
    spans.push(toggle_btn(
        if app.run.show_instr_type { "ON" } else { "OFF" },
        app.run.show_instr_type,
        app.hover_run_button == Some(RunButton::InstrType),
    ));

    spans.push(Span::raw("  "));
    spans.push(semantic_btn(
        "Reset",
        theme::DANGER,
        app.hover_run_button == Some(RunButton::Reset),
    ));

    spans
}

// ── Text helpers ────────────────────────────────────────────────────────────

fn view_text(app: &App) -> &'static str {
    if app.run.show_bp_list { "BP" }
    else if app.run.show_registers { "REGS" }
    else { "RAM" }
}

fn view_active(_app: &App) -> bool {
    // always considered "active" — it cycles between states
    true
}

fn region_text(app: &App) -> &'static str {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => "DATA",
        MemRegion::Stack => "STACK",
    }
}

fn format_text(app: &App) -> &'static str {
    match app.run.fmt_mode {
        FormatMode::Hex => "HEX",
        FormatMode::Dec => "DEC",
        FormatMode::Str => "STR",
    }
}

fn sign_enabled(app: &App) -> bool {
    matches!(app.run.fmt_mode, FormatMode::Dec)
}

fn sign_text(app: &App) -> &'static str {
    if app.run.show_signed { "SGN" } else { "UNS" }
}

fn bytes_text(app: &App) -> &'static str {
    match app.run.mem_view_bytes {
        4 => "4B",
        2 => "2B",
        _ => "1B",
    }
}

fn speed_text(app: &App) -> &'static str {
    app.run.speed.label()
}

// ── Button span builders ────────────────────────────────────────────────────

/// Generic toggle button: active uses ACTIVE fg+bold, idle uses IDLE fg, hover uses HOVER colors.
fn toggle_btn(text: &str, active: bool, hovered: bool) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::HOVER_FG)
            .bg(theme::HOVER_BG)
            .add_modifier(Modifier::ITALIC)
    } else if active {
        Style::default()
            .fg(theme::ACTIVE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::IDLE)
            .add_modifier(Modifier::DIM)
    };
    Span::styled(format!("[{text}]"), style)
}

/// Semantic button (RUN/PAUSE/RESET) with a dedicated background color.
fn semantic_btn(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::HOVER_FG)
            .bg(theme::HOVER_BG)
            .add_modifier(Modifier::ITALIC)
    } else {
        Style::default()
            .fg(Color::Rgb(0, 0, 0))
            .bg(color)
            .add_modifier(Modifier::BOLD)
    };
    Span::styled(format!("[{text}]"), style)
}

/// State button (RUN/PAUSE) — semantic color depends on running state.
fn state_btn(app: &App) -> Span<'static> {
    let hovered = app.hover_run_button == Some(RunButton::State);
    let (text, color) = if app.run.is_running {
        ("RUN", theme::RUNNING)
    } else {
        ("PAUSE", theme::PAUSED)
    };
    semantic_btn(text, color, hovered)
}
