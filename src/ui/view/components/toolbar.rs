//! A dense horizontal toolbar: a row of small labeled controls that is the
//! **single source of truth** for both rendering and mouse hit-testing.
//!
//! The status bars used to compute their layout twice — once to emit the spans
//! in the view, and again, by hand with parallel column arithmetic, to map a
//! click column back to a button. The two drifted apart every time a control
//! was added or reordered. [`Toolbar`] closes that gap: build the row once from
//! your button-id type, then ask it for [`spans`](Toolbar::spans) *or* for the
//! control under a column ([`hit`](Toolbar::hit)). Both read the same per-cell
//! geometry, so a new button appears in the view and becomes clickable from one
//! edit.
//!
//! ```ignore
//! let mut bar = Toolbar::new();
//! bar.pair(Btn::Fmt, "fmt", "hex", hovered, /*active*/ true, /*enabled*/ true, theme::TEXT)
//!    .action(Btn::Reset, "reset", reset_hovered, /*enabled*/ true, theme::DANGER);
//! let spans = bar.spans();            // view
//! let hit   = bar.hit(col, origin);   // input
//! ```

use ratatui::prelude::*;

use crate::ui::theme;

/// Columns of blank space rendered between adjacent cells.
const GAP: u16 = 3;

/// One control in a [`Toolbar`]: either a `label value` pair or a standalone
/// action word. `start..end` are columns relative to the toolbar origin, filled
/// in as the cell is pushed.
struct Cell<Id> {
    id: Id,
    /// `Some` for a `label value` pair, `None` for a bare action word.
    label: Option<String>,
    value: String,
    hovered: bool,
    /// Drives the lit (vs dimmed) style when not hovered.
    active: bool,
    /// When `false` the cell renders dimmed and is transparent to clicks.
    enabled: bool,
    color: Color,
    start: u16,
    end: u16,
}

/// A row of [`Cell`]s laid out left-to-right. Generic over the caller's button
/// id (a small `Copy` enum).
pub(crate) struct Toolbar<Id> {
    cells: Vec<Cell<Id>>,
    /// Running column where the next cell starts (relative to the origin).
    cursor: u16,
}

impl<Id: Copy> Default for Toolbar<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: Copy> Toolbar<Id> {
    pub(crate) fn new() -> Self {
        Self {
            cells: Vec::new(),
            cursor: 0,
        }
    }

    /// Append a `label value` control (e.g. `fmt hex`). The whole `label value`
    /// span — label included — is hit-testable, matching how a user aims at the
    /// word they read.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pair(
        &mut self,
        id: Id,
        label: &str,
        value: &str,
        hovered: bool,
        active: bool,
        enabled: bool,
        color: Color,
    ) -> &mut Self {
        self.push(
            id,
            Some(label.to_string()),
            value.to_string(),
            hovered,
            active,
            enabled,
            color,
        )
    }

    /// Append a standalone action word (e.g. `reset`). A disabled action renders
    /// dimmed and is not clickable.
    pub(crate) fn action(
        &mut self,
        id: Id,
        text: &str,
        hovered: bool,
        enabled: bool,
        color: Color,
    ) -> &mut Self {
        // An action is lit whenever it is enabled (there is no separate on/off).
        self.push(id, None, text.to_string(), hovered, enabled, enabled, color)
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        id: Id,
        label: Option<String>,
        value: String,
        hovered: bool,
        active: bool,
        enabled: bool,
        color: Color,
    ) -> &mut Self {
        if !self.cells.is_empty() {
            self.cursor += GAP;
        }
        let width = match &label {
            // label + one space + value
            Some(l) => l.chars().count() + 1 + value.chars().count(),
            None => value.chars().count(),
        } as u16;
        let start = self.cursor;
        self.cursor += width;
        self.cells.push(Cell {
            id,
            label,
            value,
            hovered,
            active,
            enabled,
            color,
            start,
            end: self.cursor,
        });
        self
    }

    /// Render every cell, in order, to styled spans for a single `Line`.
    pub(crate) fn spans(&self) -> Vec<Span<'static>> {
        let mut spans = Vec::with_capacity(self.cells.len() * 4);
        for (i, cell) in self.cells.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" ".repeat(GAP as usize)));
            }
            if let Some(label) = &cell.label {
                spans.push(Span::styled(
                    label.clone(),
                    Style::default().fg(theme::IDLE),
                ));
                spans.push(Span::raw(" "));
            }
            spans.push(value_span(&cell.value, cell.hovered, cell.active, cell.color));
        }
        spans
    }

    /// The control under `col`, where `origin` is the toolbar's first rendered
    /// column. Disabled cells are transparent to clicks.
    pub(crate) fn hit(&self, col: u16, origin: u16) -> Option<Id> {
        self.cells
            .iter()
            .find(|c| c.enabled && col >= origin + c.start && col < origin + c.end)
            .map(|c| c.id)
    }
}

/// Style a control value: bold-bright when hovered, lit in its colour when
/// active, dim otherwise. Shared so a hovered/active control looks identical
/// across every toolbar.
fn value_span(text: &str, hovered: bool, active: bool, color: Color) -> Span<'static> {
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
