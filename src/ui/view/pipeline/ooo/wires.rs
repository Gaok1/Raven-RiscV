//! Dependency wires drawn in the gutter between the two structure tables.
//!
//! A station row and a reorder-buffer row are the *same instruction* seen from
//! two structures, so the chain the hover resolved can be drawn literally: one
//! vertical spine down the gutter with a stub out to every row that belongs to
//! the focused chain. Seeing the primary, the entries it waits on and the
//! entries waiting on it strung onto one line is the whole point — it turns
//! "these rows share a color" into "these rows are one dependency".
//!
//! Wires are painted straight into the frame buffer after the tables, so they
//! sit on top of the panel borders they cross.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
};

use super::focus::Role;
use crate::ui::theme;

/// Which neighbours a spine cell connects to, used to pick its box-drawing
/// glyph.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(super) struct Junction {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Junction {
    /// The box-drawing character for this set of connections. Rounded corners
    /// match the panel chrome elsewhere in the UI.
    pub(super) fn glyph(self) -> char {
        match (self.up, self.down, self.left, self.right) {
            (true, true, true, true) => '\u{253c}',   // ┼
            (true, true, true, false) => '\u{2524}',  // ┤
            (true, true, false, true) => '\u{251c}',  // ├
            (true, true, false, false) => '\u{2502}', // │
            (false, true, true, false) => '\u{256e}', // ╮
            (false, true, false, true) => '\u{256d}', // ╭
            (true, false, true, false) => '\u{256f}', // ╯
            (true, false, false, true) => '\u{2570}', // ╰
            (false, false, true, true) => '\u{2500}', // ─
            (false, true, true, true) => '\u{252c}',  // ┬
            (true, false, true, true) => '\u{2534}',  // ┴
            _ => '\u{2502}',
        }
    }
}

/// Collects the rows to wire together, then paints them in one pass.
pub(super) struct WireLayer {
    gutter: Rect,
    /// Rows on the stations side, with the role that colors their stub.
    left: Vec<(u16, Role)>,
    /// Rows on the reorder-buffer side.
    right: Vec<(u16, Role)>,
}

impl WireLayer {
    pub(super) fn new(gutter: Rect) -> Self {
        Self {
            gutter,
            left: Vec::new(),
            right: Vec::new(),
        }
    }

    /// Whether wires will be drawn at all — callers skip the bookkeeping when
    /// the gutter was dropped for width.
    pub(super) fn enabled(&self) -> bool {
        self.gutter.width >= 3 && self.gutter.height > 0
    }

    pub(super) fn connect_left(&mut self, y: u16, role: Role) {
        if self.enabled() && self.in_gutter(y) {
            self.left.push((y, role));
        }
    }

    pub(super) fn connect_right(&mut self, y: u16, role: Role) {
        if self.enabled() && self.in_gutter(y) {
            self.right.push((y, role));
        }
    }

    fn in_gutter(&self, y: u16) -> bool {
        y >= self.gutter.y && y < self.gutter.y + self.gutter.height
    }

    /// The spine's column — the middle of the gutter, so both stubs get room.
    fn spine_x(&self) -> u16 {
        self.gutter.x + self.gutter.width / 2
    }

    pub(super) fn paint(&self, f: &mut Frame) {
        // A spine needs at least two rows to be telling anything.
        if !self.enabled() || self.left.len() + self.right.len() < 2 {
            return;
        }
        let ys: Vec<u16> = self
            .left
            .iter()
            .chain(self.right.iter())
            .map(|(y, _)| *y)
            .collect();
        let (top, bottom) = (
            *ys.iter().min().expect("non-empty"),
            *ys.iter().max().expect("non-empty"),
        );
        let spine_x = self.spine_x();
        let spine_style = Style::default().fg(theme::ACCENT);

        for y in top..=bottom {
            let j = Junction {
                up: y > top,
                down: y < bottom,
                left: self.left.iter().any(|(ry, _)| *ry == y),
                right: self.right.iter().any(|(ry, _)| *ry == y),
            };
            put(f, spine_x, y, j.glyph(), spine_style);
        }

        for (y, role) in &self.left {
            let style = stub_style(*role);
            // Stub runs from the panel edge to the spine; the arrow points at
            // the station row, which is where the eye starts.
            for x in (self.gutter.x + 1)..spine_x {
                put(f, x, *y, '\u{2500}', style);
            }
            put(f, self.gutter.x, *y, '\u{25c0}', style);
        }
        for (y, role) in &self.right {
            let style = stub_style(*role);
            for x in (spine_x + 1)..(self.gutter.x + self.gutter.width - 1) {
                put(f, x, *y, '\u{2500}', style);
            }
            put(
                f,
                self.gutter.x + self.gutter.width - 1,
                *y,
                '\u{25b6}',
                style,
            );
        }
    }
}

fn stub_style(role: Role) -> Style {
    match role {
        Role::Primary => Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
        Role::Producer => Style::default().fg(theme::WIRE_PRODUCER),
        Role::Consumer => Style::default().fg(theme::WIRE_CONSUMER),
        Role::Neutral | Role::Dim => Style::default().fg(theme::BORDER),
    }
}

/// Write one cell, ignoring anything outside the frame.
fn put(f: &mut Frame, x: u16, y: u16, c: char, style: Style) {
    if let Some(cell) = f.buffer_mut().cell_mut((x, y)) {
        cell.set_char(c);
        cell.set_style(style);
    }
}
