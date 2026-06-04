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

use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem};

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

#[cfg(test)]
mod tests {
    use super::visible_window;

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
