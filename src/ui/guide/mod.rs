// ui/guide — the help guide: one document per tab, read in an overlay with
// its sections down the left and the text scrolling on the right.
//
// The text is plain prose with two marks: `**term**` for a term being
// introduced, and `` `key` `` for a key, a button or a value as the screen
// spells it. Each language's text spells an acronym out the first time it uses
// it and then lets it stand; the glossary at the end spells them all out again
// for a reader who jumps straight to a section.

mod vm;

use std::cell::{Cell, RefCell};

use ratatui::layout::Rect;

use crate::ui::app::{DocsLang, Tab};

/// One piece of a section.
pub(crate) enum Block {
    /// A sub-heading.
    H(&'static str),
    /// A paragraph, wrapped to the width.
    P(&'static str),
    /// A bulleted list; each item wraps under its own text.
    List(&'static [&'static str]),
    /// Two columns: a key, a label or a term, then what it means.
    Keys(&'static [(&'static str, &'static str)]),
    /// Preformatted lines, drawn as written — a diagram.
    Pre(&'static str),
}

pub(crate) struct Section {
    pub(crate) title: &'static str,
    pub(crate) blocks: &'static [Block],
}

/// The guide for `tab` in `lang`, if the tab has one.
pub(crate) fn guide(tab: Tab, lang: DocsLang) -> Option<&'static [Section]> {
    match (tab, lang) {
        (Tab::Tlb, DocsLang::En) => Some(vm::EN),
        (Tab::Tlb, DocsLang::PtBr) => Some(vm::PT),
        _ => None,
    }
}

/// The guide overlay: where the reader is, and what the last frame laid out —
/// the input handlers read that back to scroll and to hit the contents.
#[derive(Default)]
pub(crate) struct GuideState {
    pub(crate) open: bool,
    /// First line of the text on screen.
    pub(crate) scroll: usize,
    /// The contents row under the pointer.
    pub(crate) hover: Option<usize>,
    /// Line where each section starts, from the last frame.
    pub(crate) anchors: RefCell<Vec<usize>>,
    /// Lines of text, and how many fit, in the last frame.
    pub(crate) lines: Cell<usize>,
    pub(crate) viewport: Cell<usize>,
    /// Columns the text was wrapped to.
    pub(crate) text_width: Cell<usize>,
    pub(crate) popup: Cell<Rect>,
    /// The contents column (one row per section) and the text.
    pub(crate) contents: Cell<Rect>,
    pub(crate) body: Cell<Rect>,
}

impl GuideState {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.scroll = 0;
        self.hover = None;
    }

    pub(crate) fn max_scroll(&self) -> usize {
        self.lines.get().saturating_sub(self.viewport.get())
    }

    pub(crate) fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll());
    }

    pub(crate) fn page(&self) -> isize {
        self.viewport.get().saturating_sub(2).max(1) as isize
    }

    /// The section on screen.
    pub(crate) fn section(&self) -> usize {
        self.section_at(self.scroll)
    }

    /// The section the line `scroll` is in — or the last one once the text
    /// has scrolled as far as it goes, since a short last section never
    /// reaches the top of the screen.
    pub(crate) fn section_at(&self, scroll: usize) -> usize {
        let anchors = self.anchors.borrow();
        let max = self.max_scroll();
        if max > 0 && scroll >= max {
            return anchors.len().saturating_sub(1);
        }
        anchors
            .iter()
            .rposition(|&line| line <= scroll)
            .unwrap_or(0)
    }

    pub(crate) fn jump(&mut self, section: usize) {
        let line = self.anchors.borrow().get(section).copied();
        if let Some(line) = line {
            self.scroll = line.min(self.max_scroll());
        }
    }

    /// The previous or next section from the one on screen.
    pub(crate) fn step_section(&mut self, forward: bool) {
        let count = self.anchors.borrow().len();
        if count == 0 {
            return;
        }
        let current = self.section();
        let target = if forward {
            (current + 1).min(count - 1)
        } else if self.anchors.borrow()[current] < self.scroll {
            // Mid-section: back to its own start first.
            current
        } else {
            current.saturating_sub(1)
        };
        self.jump(target);
    }

    /// The contents row at `(column, row)`.
    pub(crate) fn contents_row_at(&self, column: u16, row: u16) -> Option<usize> {
        let rect = self.contents.get();
        let inside = column >= rect.x
            && column < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height;
        let index = usize::from(row.checked_sub(rect.y)?);
        (inside && index < self.anchors.borrow().len()).then_some(index)
    }
}
