use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::{App, FormatMode, MemRegion, RunButton, RunSpeed};

pub(super) fn render_run_status(f: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.hover_run_button.is_some() {
        Color::LightCyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .border_type(BorderType::Rounded)
        .title("Run Controls");

    let para = Paragraph::new(status_lines(app)).block(block);
    f.render_widget(para, area);
}

fn status_lines(app: &App) -> Vec<Line<'static>> {
    vec![Line::from(status_spans(app)), cycle_line(app), command_line(app)]
}

fn cycle_line(app: &App) -> Line<'static> {
    let total = app.run.mem.total_program_cycles();
    let cpi   = app.run.mem.overall_cpi();
    let instr = app.run.mem.instruction_count;
    Line::from(vec![
        Span::styled("\u{23f1} ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Cycles:{total}"), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(format!("CPI:{cpi:.2}"), Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(Color::DarkGray)),
    ])
}

fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    spans.push(Span::raw("View "));
    spans.push(button_span(
        view_text(app),
        view_color(app),
        app.hover_run_button == Some(RunButton::View),
    ));

    if !app.run.show_registers {
        spans.push(Span::raw("  Region "));
        spans.push(button_span(
            region_text(app),
            region_color(app),
            app.hover_run_button == Some(RunButton::Region),
        ));
    }

    spans.push(Span::raw("  Format "));
    spans.push(button_span(
        format_text(app),
        format_color(app),
        app.hover_run_button == Some(RunButton::Format),
    ));

    spans.push(Span::raw("  Sign "));
    spans.push(button_span(
        sign_text(app),
        sign_color(app),
        sign_enabled(app) && app.hover_run_button == Some(RunButton::Sign),
    ));

    if !app.run.show_registers {
        spans.push(Span::raw("  Bytes "));
        spans.push(button_span(
            bytes_text(app),
            Color::Yellow,
            app.hover_run_button == Some(RunButton::Bytes),
        ));
    }

    spans.push(Span::raw("  Speed "));
    spans.push(button_span(
        speed_text(app),
        speed_color(app),
        app.hover_run_button == Some(RunButton::Speed),
    ));

    spans.push(Span::raw("  State "));
    spans.push(button_span(
        state_text(app),
        state_color(app),
        app.hover_run_button == Some(RunButton::State),
    ));

    spans
}

fn command_line(app: &App) -> Line<'static> {
    if app.run.faulted {
        if let Some(code) = app.run.cpu.exit_code {
            let code = code as i32;
            Line::from(Span::styled(
                format!("Program finished (exit {code}). Press R to restart."),
                Style::default().fg(Color::Yellow),
            ))
        } else {
            Line::from(Span::styled(
                "Program stopped. Press R to restart.",
                Style::default().fg(Color::Yellow),
            ))
        }
    } else if matches!(app.run.speed, RunSpeed::Instant) && app.run.is_running {
        Line::from(Span::styled(
            "Running at full speed... press p to pause (\u{2192} 1x) or R to restart",
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from("Commands: s=step  r=run  p=pause/resume  R=restart  f=speed  Up/Down scroll")
    }
}

fn view_text(app: &App) -> &'static str {
    if app.run.show_registers { "REGS" } else { "RAM" }
}

fn view_color(app: &App) -> Color {
    if app.run.show_registers {
        Color::Blue
    } else {
        Color::Green
    }
}

fn region_text(app: &App) -> &'static str {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => "DATA",
        MemRegion::Stack => "STACK",
    }
}

fn region_color(app: &App) -> Color {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => Color::Yellow,
        MemRegion::Stack => Color::LightBlue,
    }
}

fn format_text(app: &App) -> &'static str {
    match app.run.fmt_mode {
        FormatMode::Hex => "HEX",
        FormatMode::Dec => "DEC",
        FormatMode::Str => "STR",
    }
}

fn format_color(app: &App) -> Color {
    match app.run.fmt_mode {
        FormatMode::Hex => Color::Magenta,
        FormatMode::Dec => Color::Cyan,
        FormatMode::Str => Color::Yellow,
    }
}

fn sign_enabled(app: &App) -> bool {
    matches!(app.run.fmt_mode, FormatMode::Dec)
}

fn sign_text(app: &App) -> &'static str {
    if app.run.show_signed { "SGN" } else { "UNS" }
}

fn sign_color(app: &App) -> Color {
    match (app.run.show_signed, sign_enabled(app)) {
        (true, true) => Color::LightGreen,
        (false, true) => Color::LightBlue,
        _ => Color::DarkGray,
    }
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

fn speed_color(app: &App) -> Color {
    match app.run.speed {
        RunSpeed::X1 => Color::Blue,
        RunSpeed::X2 => Color::Cyan,
        RunSpeed::X4 => Color::Yellow,
        RunSpeed::Instant => Color::Magenta,
    }
}

fn state_text(app: &App) -> &'static str {
    if app.run.is_running { "RUN" } else { "PAUSE" }
}

fn state_color(app: &App) -> Color {
    if app.run.is_running {
        Color::Green
    } else {
        Color::Red
    }
}

fn button_span(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let base = Style::default().fg(Color::Black);
    let style = if hovered {
        base.bg(hover_button_color(color))
            .add_modifier(Modifier::ITALIC)
    } else {
        base.bg(color).add_modifier(Modifier::DIM)
    };
    Span::styled(format!("[{text}]"), style)
}

fn hover_button_color(color: Color) -> Color {
    match color {
        Color::Blue => Color::LightBlue,
        Color::Green => Color::LightGreen,
        Color::Magenta => Color::LightMagenta,
        Color::Cyan => Color::LightCyan,
        Color::Yellow => Color::LightYellow,
        Color::Red => Color::LightRed,
        Color::Gray => Color::White,
        Color::DarkGray => Color::Gray,
        Color::LightBlue => Color::White,
        Color::LightGreen => Color::White,
        Color::LightMagenta => Color::White,
        Color::LightCyan => Color::White,
        Color::LightYellow => Color::White,
        Color::White => Color::LightYellow,
        Color::Black => Color::DarkGray,
        other => other,
    }
}
