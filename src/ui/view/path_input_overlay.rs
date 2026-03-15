use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};

use crate::ui::theme;
use crate::ui::app::{App, PathInputAction};

pub fn render_path_input(f: &mut Frame, area: Rect, app: &App) {
    if !app.path_input.open { return; }

    let title = match &app.path_input.action {
        PathInputAction::OpenFas | PathInputAction::OpenBin
        | PathInputAction::OpenFcache | PathInputAction::OpenSnapshot => " Open File ",
        _ => " Save File ",
    };

    let comp_show = app.path_input.completions.len().min(6) as u16;
    let popup_h = 3 + comp_show; // border(2) + input(1) + completions
    let popup_w = (area.width * 3 / 4).max(60).min(area.width.saturating_sub(4));
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(popup_w)) / 2,
        area.y + area.height.saturating_sub(popup_h + 1),
        popup_w,
        popup_h,
    );

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .border_type(BorderType::Rounded)
        .title(Span::styled(title, Style::default().fg(theme::ACCENT).bold()));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Split: 1 line for input, rest for completions
    let (input_area, comp_area) = if inner.height > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        (chunks[0], Some(chunks[1]))
    } else {
        (inner, None)
    };

    // Input line — scroll left if the query is wider than the available space
    let q = &app.path_input.query;
    let avail = input_area.width.saturating_sub(2) as usize; // subtract "> " prefix
    let q_chars: Vec<char> = q.chars().collect();
    let display_q = if q_chars.len() > avail && avail > 1 {
        // Show the tail of the path with a "…" prefix so the user sees what they typed
        let tail: String = q_chars[q_chars.len() - (avail - 1)..].iter().collect();
        format!("…{tail}")
    } else {
        q.clone()
    };
    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme::ACCENT).bold()),
        Span::styled(display_q, Style::default().fg(theme::TEXT)),
    ]);
    f.render_widget(Paragraph::new(input_line), input_area);

    // Cursor: always at the right edge when the query is scrolled
    let cx = if q_chars.len() >= avail {
        input_area.x + input_area.width.saturating_sub(1)
    } else {
        input_area.x + 2 + q_chars.len() as u16
    };
    if input_area.height > 0 {
        f.set_cursor_position((cx, input_area.y));
    }

    // Completions list
    if let Some(comp_area) = comp_area {
        let sel = app.path_input.completion_sel;
        let items: Vec<ListItem<'static>> = app.path_input.completions.iter()
            .take(6)
            .enumerate()
            .map(|(i, c)| {
                let style = if i == sel {
                    Style::default().fg(Color::Black).bg(theme::ACCENT)
                } else {
                    Style::default().fg(theme::LABEL)
                };
                // Show just the filename part, truncated to fit
                let display = std::path::Path::new(c)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| if c.ends_with('/') { format!("{n}/") } else { n.to_string() })
                    .unwrap_or_else(|| c.clone());
                let max_w = comp_area.width.saturating_sub(4) as usize;
                let truncated = if display.chars().count() > max_w && max_w > 3 {
                    format!("{}…", &display[..display.char_indices().nth(max_w - 1).map_or(0, |(i, _)| i)])
                } else {
                    display
                };
                ListItem::new(format!("  {truncated}")).style(style)
            })
            .collect();
        f.render_widget(List::new(items), comp_area);
    }
}
