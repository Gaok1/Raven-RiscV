use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::ui::app::App;

pub(crate) fn render_console(f: &mut Frame, area: Rect, app: &App) {
    // Collapsed bar: keep a visible one-line bar with an upward arrow and [CLR]
    if area.height <= 1 {
        // Draw a top border line as a handle bar
        let bar = Block::default()
            .borders(Borders::TOP)
            .border_style(if app.hover_console_bar { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) });
        f.render_widget(bar, area);

        // Up arrow in the middle to suggest expanding upwards
        let arrow_style = if app.hover_console_bar { Style::default().fg(Color::Yellow) } else { Style::default() };
        let arrow_x = area.x + area.width / 2;
        let arrow_area = Rect::new(arrow_x, area.y, 1, 1);
        let arrow = Paragraph::new("▲").style(arrow_style);
        f.render_widget(arrow, arrow_area);

        // [CLR] button still visible on collapsed bar
        let clear_style = if app.hover_console_clear {
            Style::default().fg(Color::Black).bg(Color::LightRed).add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(Color::Black).bg(Color::Red)
        };
        let clear_x = area.x + area.width.saturating_sub(6);
        let clear_area = Rect::new(clear_x, area.y, 5, 1);
        let clear = Paragraph::new("[CLR]").style(clear_style);
        f.render_widget(clear, clear_area);
        return;
    }

    let border_style = if app.hover_console_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Console - Ctrl+Up/Down scroll")
        .border_style(border_style);

    let inner = block.inner(area);
    let h = inner.height.saturating_sub(1) as usize;
    let total = app.console.lines.len();
    let max_scroll = total.saturating_sub(h);
    let scroll = app.console.scroll.min(max_scroll);
    let start = total.saturating_sub(h + scroll);
    let end = total.saturating_sub(scroll);
    let mut lines: Vec<Line> = app.console.lines[start..end]
        .iter()
        .map(|l| {
            if l.is_error {
                Line::styled(l.text.as_str(), Style::default().fg(Color::Red))
            } else {
                Line::from(l.text.as_str())
            }
        })
        .collect();
    if app.console.reading {
        lines.push(Line::from(format!("\u{0010} {}", app.console.current)));
    }
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);

    let arrow_style = if app.hover_console_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let arrow_x = area.x + area.width / 2;
    let arrow_y = area.y;
    let arrow_area = Rect::new(arrow_x, arrow_y, 1, 1);
    // Up arrow on the bar to indicate draggable upwards
    let arrow = Paragraph::new("▲").style(arrow_style);
    f.render_widget(arrow, arrow_area);

    let clear_style = if app.hover_console_clear {
        Style::default()
            .fg(Color::Black)
            .bg(Color::LightRed)
            .add_modifier(Modifier::ITALIC)
    } else {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
    };
    let clear_x = area.x + area.width.saturating_sub(6);
    let clear_area = Rect::new(clear_x, area.y, 5, 1);
    let clear = Paragraph::new("[CLR]").style(clear_style);
    f.render_widget(clear, clear_area);
}
