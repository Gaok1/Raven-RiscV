//! # The Raven UI toolkit — how to build a view
//!
//! Views under `src/ui/view` should compose these shared building blocks rather
//! than hand-rolling ratatui primitives. The layering is deliberate:
//!
//! - [`crate::ui::theme`] — **palette only** (raw `Color` constants). Nothing else.
//! - [`crate::ui::view::style`] — **semantic `Style`/`Span` builders** (`label()`,
//!   `value()`, `title_span()`, `metric_span()`, `badge()`, `toggle()`,
//!   `hint_bar()`). Reach here instead of writing `Style::default().fg(...)`.
//! - [`layout`] — **pure geometry** (`Rect` math, no `Frame`). The single source
//!   of truth for nested rectangles, shared by renderers *and* `input::mouse`
//!   so hitboxes can't drift from what is drawn.
//! - `components/*` (this module) — **widgets**: panels, overlays, controls,
//!   tables, lists, console, build status.
//!
//! Rules of thumb:
//! - Need a color? It lives in `theme`. Need a *styled* thing? It lives in `style`.
//! - Need a rectangle that the mouse code also computes? Put it in [`layout`] and
//!   call the same function from both sides — never recompute a split inline.
//! - A control's look/behavior should be editable in exactly one place.
//!
//! ## Before / after
//!
//! A titled, rounded content panel — *before* (hand-rolled in every view):
//!
//! ```ignore
//! let block = Block::default()
//!     .borders(Borders::ALL)
//!     .border_type(BorderType::Rounded)
//!     .border_style(Style::default().fg(theme::BORDER))
//!     .title(Span::styled("TLB Entries", Style::default().fg(theme::ACCENT).bold()));
//! let inner = block.inner(area);
//! f.render_widget(block, area);
//! ```
//!
//! *after* (one call, and the mouse side computes `inner` the same way):
//!
//! ```ignore
//! let inner = panel::render_panel(f, area, panel::panel("TLB Entries", PanelKind::Accent));
//! ```
//!
//! The root frame, *before* (duplicated in the renderer and ~10 mouse handlers):
//!
//! ```ignore
//! let chunks = Layout::default()
//!     .direction(Direction::Vertical)
//!     .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
//!     .split(area);
//! let body = chunks[1];
//! ```
//!
//! *after* — one definition in [`layout`], called from both sides:
//!
//! ```ignore
//! let (_tabs, body, _status) = layout::app_frame(area);
//! ```
//!
//! ## Creating a button / control bar
//!
//! A horizontal row of clickable controls (subtabs, toggles, action words) is a
//! [`Toolbar`]. Build it **once** from app state in a `build_*_bar(app)` function,
//! keyed by a small `Copy` id enum. The renderer asks it for [`spans`] and the
//! mouse code asks the *same* builder for [`hit`] — so a control appears in the
//! view and becomes clickable from one edit, and the click target can never drift
//! from the label. Selected controls light up in `ACCENT`; hover is bright `TEXT`.
//!
//! ```ignore
//! // shared builder — the single source of geometry + style
//! pub(crate) fn build_foo_bar(app: &App) -> Toolbar<FooBtn> {
//!     let mut bar = Toolbar::new();
//!     bar.value(FooBtn::Stats, "stats", ControlState::chip(active, hovered), theme::ACCENT)
//!        .action(FooBtn::Reset, "reset", ControlState::chip(false, reset_hov), theme::DANGER);
//!     bar
//! }
//! // renderer:  spans.extend(build_foo_bar(app).spans());  + store (row, first_col)
//! // mouse:     build_foo_bar(app).hit(col, origin_col)  -> Option<FooBtn>
//! ```
//!
//! Use `.toggle` for a `label value` pair, `.value` for a bare word, `.action`
//! for an always-lit verb, and `.span` to push a pre-styled value (e.g. an
//! editable field's `█` cursor). Vertical field forms are not toolbars — they use
//! [`controls::field_row`] / [`controls::control_style`] instead.
//!
//! [`spans`]: Toolbar::spans
//! [`hit`]: Toolbar::hit

pub(super) mod build;
pub(super) mod console;
pub(super) mod controls;
pub(crate) mod layout;
pub(super) mod lists;
pub(crate) mod overlay;
pub(crate) mod panel;
pub(super) mod tables;
pub(crate) mod toolbar;

// Re-export selected widgets for use by sibling modules under `view`
pub(super) use console::render_console;
pub(crate) use controls::{
    ControlState, bool_value, dense_action, dense_value, edit_value, field_row, label_span,
    push_dense_pair,
};
pub(crate) use toolbar::Toolbar;
pub(crate) use lists::{
    horizontal_scrollbar, scroll_offset_from_pos, vertical_scrollbar, visible_window,
};
pub(crate) use tables::{Align, Col, DataTable};
