use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::ui::app::App;
use crate::ui::theme;

const CLEAR_LABEL: &str = "x clear";
const CLEAR_WIDTH: u16 = 7;

fn clear_style(hovered: bool) -> Style {
    if hovered {
        Style::default()
            .fg(theme::DANGER)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC)
    } else {
        Style::default().fg(theme::LABEL)
    }
}

pub(crate) fn render_console(f: &mut Frame, area: Rect, app: &App) {
    // Collapsed bar: keep a visible one-line bar with an upward arrow and [CLR]
    if area.height <= 1 {
        // Draw a top border line as a handle bar
        let bar =
            Block::default()
                .borders(Borders::TOP)
                .border_style(if app.run.hover_console_bar {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
        f.render_widget(bar, area);

        // Up arrow in the middle to suggest expanding upwards
        let arrow_style = if app.run.hover_console_bar {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let arrow_x = area.x + area.width / 2;
        let arrow_area = Rect::new(arrow_x, area.y, 1, 1);
        let arrow = Paragraph::new("▲").style(arrow_style);
        f.render_widget(arrow, arrow_area);

        let clear_x = area.x + area.width.saturating_sub(CLEAR_WIDTH + 2);
        let clear_area = Rect::new(clear_x, area.y, CLEAR_WIDTH, 1);
        let clear = Paragraph::new(CLEAR_LABEL).style(clear_style(app.run.hover_console_clear));
        f.render_widget(clear, clear_area);
        return;
    }

    let border_style = if app.run.hover_console_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    // Title with optional waiting flag
    let mut title_spans: Vec<Span> = vec![Span::raw("Console - Ctrl+Up/Down scroll")];
    if app.console.reading {
        title_spans.push(Span::raw("  "));
        title_spans.push(Span::styled(
            "waiting input",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(title_spans))
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
            use crate::ui::console::ConsoleColor;
            let style = match l.color {
                ConsoleColor::Normal => Style::default(),
                ConsoleColor::Error => Style::default().fg(Color::Red),
                ConsoleColor::Warning => Style::default().fg(Color::Yellow),
                ConsoleColor::Success => Style::default().fg(Color::Green),
                ConsoleColor::Info => Style::default().fg(Color::Cyan),
            };
            Line::styled(l.text.as_str(), style)
        })
        .collect();
    if app.console.reading {
        lines.push(Line::from(format!("\u{0010} {}", app.console.current)));
    }
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);

    let arrow_style = if app.run.hover_console_bar {
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

    let clear_x = area.x + area.width.saturating_sub(CLEAR_WIDTH + 2);
    let clear_area = Rect::new(clear_x, area.y, CLEAR_WIDTH, 1);
    let clear = Paragraph::new(CLEAR_LABEL).style(clear_style(app.run.hover_console_clear));
    f.render_widget(clear, clear_area);
}
