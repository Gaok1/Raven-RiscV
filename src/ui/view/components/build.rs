use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::ui::app::App;

pub(crate) fn render_build_status(f: &mut Frame, area: Rect, app: &App) {
    let (msg, style, build_border) = if app.last_compile_ok == Some(false) {
        let line = app.diag_line.map(|n| n + 1).unwrap_or(0);
        let text = app.diag_line_text.as_deref().unwrap_or("");
        let err = app.diag_msg.as_deref().unwrap_or("");
        (
            format!("Error line {}: {} ({})", line, text, err),
            Style::default().bg(Color::Red).fg(Color::Black),
            Color::Black,
        )
    } else if app.last_compile_ok == Some(true) {
        (
            app.last_assemble_msg.clone().unwrap_or_default(),
            Style::default().bg(Color::Green).fg(Color::Black),
            Color::Black,
        )
    } else {
        ("Not compiled".to_string(), Style::default(), Color::DarkGray)
    };
    let status = Paragraph::new(msg)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Build")
                .border_style(Style::default().fg(build_border))
                .border_type(BorderType::Rounded),
        );
    f.render_widget(status, area);
}
