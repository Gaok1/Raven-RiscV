use ratatui::prelude::*;

use crate::ui::theme;

pub(crate) fn push_dense_pair(
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
    spans.push(dense_value(value, hovered, active, active_color));
}

pub(crate) fn dense_value(text: &str, hovered: bool, active: bool, color: Color) -> Span<'static> {
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

pub(crate) fn dense_action(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };
    Span::styled(text.to_string(), style)
}
