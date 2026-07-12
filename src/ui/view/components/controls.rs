use std::cell::Cell;

use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::ui::theme;

/// Span row that tracks the terminal x-cursor as spans are pushed, so button
/// hitboxes can be recorded from real rendered widths instead of hand-counted
/// character offsets.
pub(crate) struct SpanRow {
    spans: Vec<Span<'static>>,
    x: u16,
    y: u16,
}

impl SpanRow {
    pub(crate) fn new(x: u16, y: u16) -> Self {
        Self {
            spans: Vec::new(),
            x,
            y,
        }
    }

    pub(crate) fn push(&mut self, span: Span<'static>) {
        self.x = self
            .x
            .saturating_add(UnicodeWidthStr::width(span.content.as_ref()) as u16);
        self.spans.push(span);
    }

    pub(crate) fn gap(&mut self, n: u16) {
        self.push(Span::raw(" ".repeat(n as usize)));
    }

    /// Current x-cursor; capture before pushing a button's spans and pass to
    /// [`SpanRow::record_hitbox`] afterwards.
    pub(crate) fn cursor(&self) -> u16 {
        self.x
    }

    pub(crate) fn record_hitbox(&self, start: u16, rect: &Cell<(u16, u16, u16)>) {
        rect.set((self.y, start, self.x));
    }

    pub(crate) fn into_line(self) -> Line<'static> {
        Line::from(self.spans)
    }
}

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
