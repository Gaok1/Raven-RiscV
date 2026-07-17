//! Panel chrome — the rounded, titled `Block` that wraps almost every region of
//! the UI, plus the `inner() + render_widget` dance that follows it ~40×.
//!
//! Use [`panel`] for a titled content region, [`panel_frame`] when you draw the
//! title yourself, [`panel_square`] for the non-rounded boxes (pipeline/gantt),
//! and [`handle_bar`] for the top-rule drag/preset/apply strips. [`render_panel`]
//! collapses "compute inner, render the block, return inner" into one call.

use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders};

use crate::ui::theme;
use crate::ui::view::style;

/// What a panel *is*, semantically. Encodes both the border color and the title
/// style so each call site states its intent explicitly (per the Raven
/// convention: accent-bold titles for primary/active panels, muted for
/// secondary content).
#[derive(Clone, Copy)]
pub(crate) enum PanelKind {
    /// Secondary content panel — neutral border, muted (LABEL) title.
    Plain,
    /// Primary / active panel — neutral border, accent-bold title.
    Accent,
    /// Warning — amber border + bold title.
    Warning,
    /// Danger — red border + bold title.
    Danger,
    /// Arbitrary color for both border and (bold) title.
    Custom(Color),
}

impl PanelKind {
    fn border_color(self) -> Color {
        match self {
            PanelKind::Plain | PanelKind::Accent => theme::BORDER,
            PanelKind::Warning => theme::PAUSED,
            PanelKind::Danger => theme::DANGER,
            PanelKind::Custom(c) => c,
        }
    }

    fn title_style(self) -> Style {
        match self {
            PanelKind::Plain => style::label(),
            PanelKind::Accent => style::title(),
            PanelKind::Warning => Style::default()
                .fg(theme::PAUSED)
                .add_modifier(Modifier::BOLD),
            PanelKind::Danger => Style::default()
                .fg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
            PanelKind::Custom(c) => Style::default().fg(c).add_modifier(Modifier::BOLD),
        }
    }
}

/// Rounded, all-borders panel with a styled title.
pub(crate) fn panel(title: impl Into<String>, kind: PanelKind) -> Block<'static> {
    panel_frame(kind).title(Span::styled(title.into(), kind.title_style()))
}

/// Rounded, all-borders panel without a title (caller draws its own header, or
/// only needs the frame for `.inner()` geometry).
pub(crate) fn panel_frame(kind: PanelKind) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(kind.border_color()))
}

/// A square (non-rounded) all-borders box with a pre-styled title line and an
/// explicit border `Style` — for the pipeline stage / gantt cells whose border
/// is colored (and sometimes bold) per state.
pub(crate) fn panel_square(title: impl Into<Line<'static>>, border: Style) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title.into())
}

/// A single top rule (`Borders::TOP`) — the collapsed/preset/apply/console
/// handle strips.
pub(crate) fn handle_bar(border: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border))
}

/// Render `block` into `area` and return its inner rect. Replaces the
/// `let inner = block.inner(area); f.render_widget(block, area);` pair.
pub(crate) fn render_panel(f: &mut Frame, area: Rect, block: Block<'static>) -> Rect {
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}
