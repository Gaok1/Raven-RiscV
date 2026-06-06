//! A dense horizontal toolbar: a row of small labeled controls that is the
//! **single source of truth** for both rendering and mouse hit-testing.
//!
//! Status/subtab bars used to compute their layout twice — once to emit the
//! spans in the view, and again, by hand with parallel column arithmetic, to map
//! a click column back to a button. The two drifted apart every time a control
//! was added or reordered. [`Toolbar`] closes that gap: build the row once from
//! your button-id type, then ask it for [`spans`](Toolbar::spans) *or* for the
//! control under a column ([`hit`](Toolbar::hit)). Both read the same per-cell
//! geometry, so a new button appears in the view and becomes clickable from one
//! edit.
//!
//! Every value is styled through [`controls::control_style`], so a hovered,
//! selected or disabled control looks identical across every toolbar.
//!
//! ```ignore
//! let mut bar = Toolbar::new();
//! bar.toggle(Btn::Fmt, "fmt", "hex", ControlState::chip(active, hovered), theme::ACCENT)
//!    .action(Btn::Reset, "reset", ControlState::chip(false, reset_hov), theme::DANGER);
//! let spans = bar.spans();            // view
//! let hit   = bar.hit(col, origin);   // input
//! ```

use ratatui::prelude::*;

use crate::ui::theme;
use crate::ui::view::components::controls::{control_style, ControlState};

/// Default columns of blank space rendered between adjacent cells.
const GAP: u16 = 3;

/// One control in a [`Toolbar`]: an optional dim `label` plus a pre-styled
/// `value` span. `start..end` are columns relative to the toolbar origin, filled
/// in as the cell is pushed.
struct Cell<Id> {
    id: Id,
    /// `Some` for a `label value` pair, `None` for a bare value/action word.
    label: Option<String>,
    value: Span<'static>,
    /// When `false` the cell is transparent to clicks (a `Disabled` control).
    enabled: bool,
    start: u16,
    end: u16,
}

/// A row of [`Cell`]s laid out left-to-right. Generic over the caller's button
/// id (a small `Copy` enum).
pub(crate) struct Toolbar<Id> {
    cells: Vec<Cell<Id>>,
    /// Running column where the next cell starts (relative to the origin).
    cursor: u16,
    /// Blank columns rendered between adjacent cells.
    gap: u16,
}

impl<Id: Copy> Default for Toolbar<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: Copy> Toolbar<Id> {
    pub(crate) fn new() -> Self {
        Self::with_gap(GAP)
    }

    /// A toolbar with a custom inter-cell gap (the main tab bar uses 2).
    pub(crate) fn with_gap(gap: u16) -> Self {
        Self {
            cells: Vec::new(),
            cursor: 0,
            gap,
        }
    }

    /// A `label value` control (e.g. `fmt hex`): a dim `label`, a space, then the
    /// `value` lit per `state`/`color`. The whole span — label included — is
    /// hit-testable, matching how a user aims at the word they read.
    pub(crate) fn toggle(
        &mut self,
        id: Id,
        label: &str,
        value: &str,
        state: ControlState,
        color: Color,
    ) -> &mut Self {
        let span = Span::styled(value.to_string(), control_style(state, color, theme::IDLE));
        self.span(id, Some(label), span, state != ControlState::Disabled)
    }

    /// A value-only control with no label (subtab / scope word): dim when
    /// `Normal`, `color` when `Selected`, bright `TEXT` when `Hovered`.
    pub(crate) fn value(
        &mut self,
        id: Id,
        text: &str,
        state: ControlState,
        color: Color,
    ) -> &mut Self {
        let span = Span::styled(text.to_string(), control_style(state, color, theme::IDLE));
        self.span(id, None, span, state != ControlState::Disabled)
    }

    /// A standalone action word (e.g. `reset`, `apply`): always lit in `color`,
    /// bright when hovered. There is no dim/inactive rendering — a `Normal` state
    /// is treated as lit. A `Disabled` action renders dimmed and is not clickable.
    pub(crate) fn action(
        &mut self,
        id: Id,
        text: &str,
        state: ControlState,
        color: Color,
    ) -> &mut Self {
        // Actions are lit whenever enabled, so collapse Normal → Selected.
        let lit = match state {
            ControlState::Disabled => ControlState::Disabled,
            ControlState::Hovered => ControlState::Hovered,
            _ => ControlState::Selected,
        };
        let span = Span::styled(text.to_string(), control_style(lit, color, theme::IDLE));
        self.span(id, None, span, state != ControlState::Disabled)
    }

    /// The low-level escape hatch: push a pre-styled `value` span (e.g. an
    /// editable field with its `█` cursor, or a `< enum >` selector built by
    /// [`controls`](super::controls)). The toolbar only owns geometry and the
    /// dim `label`; the caller owns the value's appearance.
    pub(crate) fn span(
        &mut self,
        id: Id,
        label: Option<&str>,
        value: Span<'static>,
        enabled: bool,
    ) -> &mut Self {
        if !self.cells.is_empty() {
            self.cursor += self.gap;
        }
        // label + one space + value, or just the value.
        let label = label.map(str::to_string);
        let width = match &label {
            Some(l) => l.chars().count() as u16 + 1 + value.width() as u16,
            None => value.width() as u16,
        };
        let start = self.cursor;
        self.cursor += width;
        self.cells.push(Cell {
            id,
            label,
            value,
            enabled,
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
                spans.push(Span::raw(" ".repeat(self.gap as usize)));
            }
            if let Some(label) = &cell.label {
                spans.push(Span::styled(label.clone(), Style::default().fg(theme::IDLE)));
                spans.push(Span::raw(" "));
            }
            spans.push(cell.value.clone());
        }
        spans
    }

    /// Total rendered width in columns (handy to place a second bar or label
    /// after this one on the same line).
    pub(crate) fn width(&self) -> u16 {
        self.cursor
    }

    /// Per-cell `(id, start, end)` columns relative to the origin — used to align
    /// a second row under each cell (the tab bar's underline).
    pub(crate) fn cells(&self) -> impl Iterator<Item = (Id, u16, u16)> + '_ {
        self.cells.iter().map(|c| (c.id, c.start, c.end))
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
