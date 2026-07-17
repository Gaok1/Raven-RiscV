//! Popup / overlay chrome — `Clear` + a rounded titled `Block`, the shape every
//! modal (exit, help, ELF, path-input, snapshot, tutorial) shares.
//!
//! Per the Raven convention all popups are `BorderType::Rounded` (older square
//! ones — help, splash — converge here). [`overlay`] clears the rect, draws the
//! block, and returns the inner rect for the caller to fill.

use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear};

/// Chrome for a popup: border color, a pre-styled title span, and an optional
/// bottom-edge hint line (e.g. `Esc close`).
pub(crate) struct OverlayStyle {
    pub border: Color,
    pub title: Span<'static>,
    pub bottom: Option<Line<'static>>,
}

impl OverlayStyle {
    /// Convenience: a popup with just a border color and title, no bottom hint.
    pub(crate) fn new(border: Color, title: Span<'static>) -> Self {
        Self {
            border,
            title,
            bottom: None,
        }
    }
}

/// Clear `rect`, draw a rounded popup block, and return the inner rect.
pub(crate) fn overlay(f: &mut Frame, rect: Rect, style: OverlayStyle) -> Rect {
    f.render_widget(Clear, rect);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(style.border))
        .title(style.title);
    if let Some(bottom) = style.bottom {
        block = block.title_bottom(bottom);
    }
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    inner
}
