//! Scrollable lists: the shared scroll-window math and a selection-aware list.
//!
//! Two recurring jobs across the run/docs/tlb views are centralised here:
//!
//! - [`visible_window`] — the "clamp the scroll offset and slice a viewport out
//!   of `total` rows" arithmetic that was open-coded in `docs/free_page`,
//!   `docs/instr_ref`, `tlb/entries`, `tlb/page_tree`, `tlb/vm_settings`, … .
//! - [`selectable_list`] — a `List` whose selected / hovered rows pick up the
//!   centralised selection backgrounds from [`theme`] instead of each call site
//!   hand-coding `Rgb(50,50,80)` / `Rgb(40,60,40)`.

// `selectable_list`/`ListRow` are offered ahead of the bespoke run-view lists
// that will adopt them; `visible_window` is wired in now. Mirrors `style.rs`.
#![allow(dead_code)]

use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::ui::theme;

/// Clamp `scroll` to a valid offset for `total` rows in a `view_h`-tall
/// viewport and return the visible half-open range `[start, end)`.
///
/// `start` never exceeds `total - view_h` (so the last page stays full) and
/// `end` is clamped to `total`. Callers that slice with `.skip(start)` /
/// `.take(view_h)` can use just `start`; those that index `rows[start..end]`
/// use both. A `view_h` of 0 yields an empty `(start, start)` window.
pub(crate) fn visible_window(total: usize, view_h: usize, scroll: usize) -> (usize, usize) {
    let max_start = total.saturating_sub(view_h);
    let start = scroll.min(max_start);
    let end = (start + view_h).min(total);
    (start, end)
}

/// One row of a [`selectable_list`]: its rendered content plus interaction flags.
pub(crate) struct ListRow {
    pub line: Line<'static>,
    pub selected: bool,
    pub hover: bool,
}

impl ListRow {
    pub(crate) fn new(line: impl Into<Line<'static>>) -> Self {
        Self {
            line: line.into(),
            selected: false,
            hover: false,
        }
    }

    pub(crate) fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub(crate) fn hover(mut self, on: bool) -> Self {
        self.hover = on;
        self
    }
}

/// Build a `List` whose rows carry the centralised selection / hover row
/// backgrounds ([`theme::SEL_ROW_BG`] / [`theme::HOVER_ROW_BG`]). Selected wins
/// over hovered, matching every call site's precedence.
pub(crate) fn selectable_list(rows: impl IntoIterator<Item = ListRow>) -> List<'static> {
    let items: Vec<ListItem<'static>> = rows
        .into_iter()
        .map(|r| {
            let item = ListItem::new(r.line);
            if r.selected {
                item.style(Style::default().bg(theme::SEL_ROW_BG))
            } else if r.hover {
                item.style(Style::default().bg(theme::HOVER_ROW_BG))
            } else {
                item
            }
        })
        .collect();
    List::new(items)
}

/// Draw a vertical scrollbar on the right edge of `area` when `content_len`
/// overflows the `viewport` height. `offset` is the index of the first visible
/// row. A no-op when everything fits, so callers can invoke it unconditionally
/// — but reserve one column for it (e.g. lay content out in `area.width - 1`)
/// so the bar never paints over text.
pub(crate) fn vertical_scrollbar(
    f: &mut Frame,
    area: Rect,
    content_len: usize,
    viewport: usize,
    offset: usize,
) {
    if content_len <= viewport || area.height == 0 {
        return;
    }
    // Ratatui puts the thumb at the track's end only at position content-1
    // (last row at the *top* of the viewport). Our offsets stop at
    // content-viewport (last page fully visible), so hand it the number of
    // scroll positions instead — the thumb then spans the full track.
    let mut state = ScrollbarState::new(content_len - viewport + 1)
        .position(offset)
        .viewport_content_length(viewport);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("\u{2191}"))
            .end_symbol(Some("\u{2193}")),
        area,
        &mut state,
    );
}

/// Draw a horizontal scrollbar along the bottom edge of `area` when
/// `content_len` overflows the `viewport` width. `offset` is the leftmost
/// visible column. No-op when everything fits — reserve one row for it.
pub(crate) fn horizontal_scrollbar(
    f: &mut Frame,
    area: Rect,
    content_len: usize,
    viewport: usize,
    offset: usize,
) {
    if content_len <= viewport || area.width == 0 {
        return;
    }
    // See vertical_scrollbar: scroll positions, not total rows.
    let mut state = ScrollbarState::new(content_len - viewport + 1)
        .position(offset)
        .viewport_content_length(viewport);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .begin_symbol(Some("\u{25c4}"))
            .end_symbol(Some("\u{25ba}")),
        area,
        &mut state,
    );
}

/// Geometry + render parameters of a scrollbar, registered by render each
/// frame so the mouse side can hit-test the bar and drag its thumb glued to
/// the cursor. `content`/`viewport`/`offset` are the exact values the bar was
/// rendered with, letting [`SbGeom::thumb`] mirror ratatui's thumb placement.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SbGeom {
    /// First cell of the bar along its axis (the begin-arrow cell).
    pub start: u16,
    /// Bar length in cells, the two arrow cells included.
    pub len: u16,
    /// The fixed cross-axis cell the bar lives on (column for a vertical bar,
    /// row for a horizontal one).
    pub cross: u16,
    /// `ScrollbarState` content length.
    pub content: usize,
    /// `ScrollbarState` viewport length; 0 means ratatui's fall-back to the
    /// full bar length.
    pub viewport: usize,
    /// Scroll offset the bar was rendered at.
    pub offset: usize,
    /// Upper clamp for offsets produced by clicking/dragging the bar.
    pub max: usize,
}

impl SbGeom {
    /// Whether the cursor — split into its along-axis and cross-axis cells —
    /// lands on the bar (arrow cells included).
    pub(crate) fn hit(&self, along: u16, cross: u16) -> bool {
        cross == self.cross && along >= self.start && along < self.start + self.len
    }

    /// Track length in cells with the two arrow cells excluded.
    fn track_len(&self) -> f64 {
        f64::from(self.len.saturating_sub(2))
    }

    /// Viewport length as ratatui resolves it: the stored value, or the full
    /// bar length when `viewport_content_length` was 0.
    fn effective_viewport(&self) -> usize {
        if self.viewport == 0 {
            self.len as usize
        } else {
            self.viewport
        }
    }

    /// The `ScrollbarState` content length the bar was rendered with. Bars
    /// drawn by [`vertical_scrollbar`]/[`horizontal_scrollbar`] (viewport != 0)
    /// hand ratatui the number of scroll positions `content - viewport + 1`, so
    /// the thumb reaches the track's end on the last page; hand-rolled bars
    /// (viewport == 0) pass `content` directly.
    fn positions(&self) -> usize {
        if self.viewport == 0 {
            self.content
        } else {
            self.content.saturating_sub(self.viewport) + 1
        }
    }

    /// Highest content position plus the viewport length — the denominator of
    /// ratatui's thumb map (`Scrollbar::part_lengths`).
    fn scale(&self) -> f64 {
        (self.positions().saturating_sub(1) + self.effective_viewport()) as f64
    }

    /// First cell (absolute) and length of the thumb exactly as ratatui draws
    /// it for this bar's render parameters.
    pub(crate) fn thumb(&self) -> (u16, u16) {
        let track = self.track_len();
        if track < 1.0 || self.content == 0 {
            return (self.start.saturating_add(1), 0);
        }
        let viewport = self.effective_viewport() as f64;
        let pos = (self.offset as f64).min(self.positions().saturating_sub(1) as f64);
        let t_start = (pos * track / self.scale()).round().clamp(0.0, track - 1.0);
        let t_end = ((pos + viewport) * track / self.scale()).round().clamp(0.0, track);
        let t_len = (t_end - t_start).max(1.0);
        (self.start + 1 + t_start as u16, t_len as u16)
    }

    /// Offset that puts the thumb's first cell at the absolute cell
    /// `thumb_start` — the inverse of [`SbGeom::thumb`], clamped to
    /// `[0, max]`.
    fn offset_for_thumb_start(&self, thumb_start: f64) -> usize {
        let track = self.track_len();
        if track < 1.0 {
            return 0;
        }
        let rel = (thumb_start - f64::from(self.start) - 1.0).max(0.0);
        (((rel * self.scale()) / track).round() as usize).min(self.max)
    }

    /// Begin a drag at cursor cell `pos`: returns `(grab, offset)` where
    /// `grab` is the cell of the thumb the cursor holds. Pressing on the
    /// thumb keeps the current offset (no jump); pressing on the track jumps
    /// so the thumb centres under the cursor. Feed `grab` back into
    /// [`SbGeom::drag`] on every subsequent drag event.
    pub(crate) fn begin_drag(&self, pos: u16) -> (u16, usize) {
        let (t_start, t_len) = self.thumb();
        if pos >= t_start && pos < t_start + t_len {
            (pos - t_start, self.offset.min(self.max))
        } else {
            let grab = t_len / 2;
            (
                grab,
                self.offset_for_thumb_start(f64::from(pos) - f64::from(grab)),
            )
        }
    }

    /// Offset for a drag at cursor cell `pos` holding the thumb at `grab`,
    /// keeping the grabbed thumb cell glued to the cursor.
    pub(crate) fn drag(&self, pos: u16, grab: u16) -> usize {
        self.offset_for_thumb_start(f64::from(pos) - f64::from(grab))
    }
}

#[cfg(test)]
mod tests {
    use super::{SbGeom, visible_window};

    // 100 rows, 20 visible, bar drawn over 22 cells (20-cell track + arrows).
    fn geom(offset: usize) -> SbGeom {
        SbGeom {
            start: 10,
            len: 22,
            cross: 5,
            content: 100,
            viewport: 20,
            offset,
            max: 80,
        }
    }

    #[test]
    fn thumb_matches_ratatui_placement() {
        // positions = 81, scale = 80 + 20 = 100, track = 20:
        // offset 0  → start round(0)=0,  end round(20·20/100)=4  → cells 11..15
        assert_eq!(geom(0).thumb(), (11, 4));
        // offset 80 → start round(80·20/100)=16, end round(100·20/100)=20
        assert_eq!(geom(80).thumb(), (27, 4));
    }

    #[test]
    fn thumb_reaches_track_end_at_max_offset() {
        // Regression: with the last page visible the thumb must sit flush with
        // the bottom of the track (it used to stop at ~75%).
        let g = geom(80);
        let (t_start, t_len) = g.thumb();
        let track_end = g.start + g.len - 2; // last track cell before the end arrow
        assert_eq!(t_start + t_len - 1, track_end);
        // And at offset 0 it starts at the top of the track.
        assert_eq!(geom(0).thumb().0, geom(0).start + 1);
    }

    #[test]
    fn press_on_thumb_keeps_offset_and_drag_stays_glued() {
        let g = geom(30);
        let (t_start, _) = g.thumb();
        let (grab, offset) = g.begin_drag(t_start + 1);
        assert_eq!((grab, offset), (1, 30)); // no jump on grab

        // Moving the cursor N cells moves the thumb exactly N cells (while the
        // offset stays inside `[0, max]` — the thumb clamps at the ends).
        for delta in 1..=8u16 {
            let dragged = g.drag(t_start + 1 + delta, grab);
            let moved = SbGeom { offset: dragged, ..g };
            assert_eq!(moved.thumb().0, t_start + delta, "delta {delta}");
        }
    }

    #[test]
    fn press_on_track_centres_thumb_under_cursor() {
        let g = geom(0);
        let (grab, offset) = g.begin_drag(20); // well below the 3-cell thumb
        let jumped = SbGeom { offset, ..g };
        let (t_start, t_len) = jumped.thumb();
        assert_eq!(grab, t_len / 2);
        assert!((t_start..t_start + t_len).contains(&20));
    }

    #[test]
    fn degenerate_bars_map_to_zero() {
        let tiny = SbGeom { len: 2, ..geom(0) }; // no track cells
        assert_eq!(tiny.begin_drag(11).1, 0);
        assert_eq!(tiny.drag(11, 0), 0);
    }


    #[test]
    fn window_clamps_scroll_to_last_full_page() {
        // 100 rows, 10 tall, scrolled way past the end → last page [90, 100).
        assert_eq!(visible_window(100, 10, 999), (90, 100));
    }

    #[test]
    fn window_within_bounds_is_untouched() {
        assert_eq!(visible_window(100, 10, 5), (5, 15));
    }

    #[test]
    fn window_shorter_than_viewport_shows_all() {
        assert_eq!(visible_window(3, 10, 0), (0, 3));
        assert_eq!(visible_window(3, 10, 7), (0, 3));
    }

    #[test]
    fn zero_viewport_is_empty() {
        assert_eq!(visible_window(50, 0, 4), (4, 4));
    }
}
