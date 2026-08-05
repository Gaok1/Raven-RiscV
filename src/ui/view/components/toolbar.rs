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
use crate::ui::view::components::controls::{
    ControlState, control_style, control_surface_style,
};

/// Default columns of blank space rendered between adjacent cells.
const GAP: u16 = 3;

/// A clickable control, drawn as a filled pill: one padding column each side so
/// the surface reads as a shape rather than as a highlight on the word.
///
/// The surface is what separates a control from a readout. `fmt hex` and
/// `word 0x293` used to be typographically identical, which is the same
/// ambiguity that lost the help button in the chrome. A disabled control keeps
/// its columns but drops the surface — see
/// [`control_surface_style`](super::controls::control_surface_style).
pub(crate) fn pill(text: &str, state: ControlState, color: Color) -> Span<'static> {
    if state == ControlState::Disabled {
        // No surface, but the same width, so the row does not reflow when a
        // control goes inert under the cursor.
        return Span::styled(
            format!(" {text} "),
            control_style(state, color, theme::IDLE),
        );
    }
    Span::styled(format!(" {text} "), control_surface_style(state, color))
}

/// One cell in a [`Toolbar`]: an optional dim `label` plus pre-styled `value`
/// spans. `start..end` are columns relative to the toolbar origin, filled in as
/// the cell is pushed.
struct Cell<Id> {
    /// `None` for a decorative cell — a status badge, a group separator, a
    /// readout. Those occupy columns (so the clickable cells after them land in
    /// the right place) but can never be the answer to [`Toolbar::hit`].
    id: Option<Id>,
    /// `Some` for a `label value` pair, `None` for a bare value/action word.
    label: Option<String>,
    value: Vec<Span<'static>>,
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
        // Label and pill go in as one cell rather than using the `label` slot,
        // because that slot inserts its own separating space — and the pill
        // already carries a padding column. Two spaces between the word and the
        // control it names reads as a gap, not as a pair.
        self.push(
            Some(id),
            None,
            vec![
                Span::styled(label.to_string(), Style::default().fg(theme::IDLE)),
                pill(value, state, color),
            ],
            state != ControlState::Disabled,
        )
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
        self.span(id, None, pill(text, lit, color), state != ControlState::Disabled)
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
        self.push(Some(id), label, vec![value], enabled)
    }

    /// A cell that occupies columns but can never be clicked — a status badge, a
    /// readout, a group separator.
    ///
    /// Decorative cells still go through the toolbar rather than being spliced
    /// into the rendered line by hand, because the clickable cells *after* them
    /// need their columns counted. Splicing is what makes a hit-test drift one
    /// control to the left the day someone widens a badge.
    pub(crate) fn text(&mut self, spans: Vec<Span<'static>>) -> &mut Self {
        self.push(None, None, spans, false)
    }

    /// A dim `│` marking a group boundary, with the usual gap either side.
    /// Grouping is what turns a queue of same-weight controls into something
    /// scannable.
    pub(crate) fn separator(&mut self) -> &mut Self {
        self.text(vec![Span::styled(
            "│",
            Style::default().fg(theme::BORDER),
        )])
    }

    /// Advance the cursor so the next cell starts at `col` (relative to the
    /// toolbar origin). No-op if the toolbar is already past `col`.
    ///
    /// Lets a group be parked at a fixed column so it stops sliding as the text
    /// to its left changes width.
    pub(crate) fn pad_to(&mut self, col: u16) -> &mut Self {
        if col > self.cursor {
            let gap = col - self.cursor - self.gap.min(col - self.cursor);
            self.text(vec![Span::raw(" ".repeat(gap as usize))]);
        }
        self
    }

    fn push(
        &mut self,
        id: Option<Id>,
        label: Option<&str>,
        value: Vec<Span<'static>>,
        enabled: bool,
    ) -> &mut Self {
        if !self.cells.is_empty() {
            self.cursor += self.gap;
        }
        // label + one space + value, or just the value.
        let label = label.map(str::to_string);
        let value_w: u16 = value.iter().map(|s| s.width() as u16).sum();
        let width = match &label {
            Some(l) => l.chars().count() as u16 + 1 + value_w,
            None => value_w,
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
                spans.push(Span::styled(
                    label.clone(),
                    Style::default().fg(theme::IDLE),
                ));
                spans.push(Span::raw(" "));
            }
            spans.extend(cell.value.iter().cloned());
        }
        spans
    }

    /// Total rendered width in columns (handy to place a second bar or label
    /// after this one on the same line).
    pub(crate) fn width(&self) -> u16 {
        self.cursor
    }

    /// Per-cell `(id, start, end)` columns relative to the origin — used to align
    /// a second row under each cell (the tab bar's underline). Decorative cells
    /// have nothing to align to, so they are skipped.
    pub(crate) fn cells(&self) -> impl Iterator<Item = (Id, u16, u16)> + '_ {
        self.cells
            .iter()
            .filter_map(|c| c.id.map(|id| (id, c.start, c.end)))
    }

    /// The control under `col`, where `origin` is the toolbar's first rendered
    /// column. Disabled and decorative cells are transparent to clicks.
    pub(crate) fn hit(&self, col: u16, origin: u16) -> Option<Id> {
        self.cells
            .iter()
            .find(|c| c.enabled && col >= origin + c.start && col < origin + c.end)
            .and_then(|c| c.id)
    }
}
