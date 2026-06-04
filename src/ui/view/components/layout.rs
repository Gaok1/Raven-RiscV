//! Pure layout geometry — **no `Frame`**, so it can be shared by both the view
//! renderers and `input::mouse` hit-testing. This is the single source of truth
//! for the app's nested rectangles; computing a `Rect` the same way on the
//! render side and the click side is what keeps hitboxes from drifting.
//!
//! Mirrors the good existing pattern `run::run_panel_constraints`.

// Phase 0 scaffolding: consumed by Phase 5 (view↔mouse dedup). The
// module-level allow is removed in Phase 6 once every site is migrated.
#![allow(dead_code)]

use ratatui::prelude::*;

/// A `w × h` rect centered inside `area`.
pub(crate) fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w,
        h,
    )
}

/// A rect sized as a percentage of `area`, centered.
pub(crate) fn centered_pct(pw: u16, ph: u16, area: Rect) -> Rect {
    let w = area.width.saturating_mul(pw) / 100;
    let h = area.height.saturating_mul(ph) / 100;
    centered_rect(w, h, area)
}

/// A full-height rect whose width is `pref` clamped to `[min, area.width - 2*margin]`,
/// centered horizontally.
pub(crate) fn centered_width(area: Rect, pref: u16, min: u16, margin: u16) -> Rect {
    let max = area.width.saturating_sub(margin.saturating_mul(2));
    let w = pref.clamp(min.min(max), max);
    Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y,
        w,
        area.height,
    )
}

/// A full-height column of width `content_w` centered via `Fill | Length | Fill`.
pub(crate) fn centered_column(area: Rect, content_w: u16) -> Rect {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(content_w),
            Constraint::Fill(1),
        ])
        .split(area)[1]
}

/// Split `area` into `(body, footer)` where the footer is `footer_h` rows tall.
pub(crate) fn body_footer(area: Rect, footer_h: u16) -> (Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_h)])
        .split(area);
    (rows[0], rows[1])
}

/// Split `area` into fixed-height header rows (given by `header_heights`) plus a
/// flexible body that fills the remainder. Returns `(headers, body)`.
pub(crate) fn header_body(area: Rect, header_heights: &[u16]) -> (Vec<Rect>, Rect) {
    let mut constraints: Vec<Constraint> = header_heights
        .iter()
        .map(|&h| Constraint::Length(h))
        .collect();
    constraints.push(Constraint::Min(0));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let body = chunks[chunks.len() - 1];
    let headers = chunks[..chunks.len() - 1].to_vec();
    (headers, body)
}

/// The root app frame: `(tabs, body, status)` = `[Length(3), Min(5), Length(1)]`.
/// The single definition of this split, shared by `view::ui` and the mouse
/// router so the tab bar / body / status line agree on their bounds.
pub(crate) fn app_frame(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

/// The root app frame as an indexable `[tabs, body, status]` array — the same
/// split as [`app_frame`], for the `input::mouse` sites that index by position.
pub(crate) fn app_frame_chunks(area: Rect) -> [Rect; 3] {
    let (tabs, body, status) = app_frame(area);
    [tabs, body, status]
}

/// Split `area` into `n` equal-width columns.
pub(crate) fn even_columns(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let constraints = vec![Constraint::Ratio(1, n as u32); n];
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area)
        .to_vec()
}

/// Place a `pw × ph` popup relative to `anchor` (below → above → right → left →
/// centered fallback). With `anchor == None` it is simply centered in `term`.
/// Unifies `view::best_popup_rect` and `tutorial::tutorial_popup_rect`.
pub(crate) fn anchored_popup(anchor: Option<Rect>, pw: u16, ph: u16, term: Rect) -> Rect {
    let Some(t) = anchor else {
        return centered_rect(pw, ph, term);
    };
    let gap = 1;
    // If the anchor is much wider than the popup, center over it; otherwise
    // left-align to the anchor's left edge (clamped onto the terminal).
    let align_x = if t.width >= pw.saturating_add(16) {
        centered_rect(pw, ph, term).x
    } else {
        clamp_x(t.x, pw, term)
    };

    // Below
    let below_y = t.y + t.height + gap;
    if below_y + ph <= term.y + term.height {
        return Rect::new(align_x, below_y, pw, ph);
    }
    // Above
    if t.y >= term.y + ph + gap {
        return Rect::new(align_x, t.y - ph - gap, pw, ph);
    }
    // Right
    let right_x = t.x + t.width + gap;
    if right_x + pw <= term.x + term.width {
        return Rect::new(right_x, clamp_y(t.y, ph, term), pw, ph);
    }
    // Left
    if t.x >= term.x + pw + gap {
        return Rect::new(t.x - pw - gap, clamp_y(t.y, ph, term), pw, ph);
    }
    // Centered fallback
    centered_rect(pw, ph, term)
}

fn clamp_x(preferred: u16, pw: u16, term: Rect) -> u16 {
    let max_x = (term.x + term.width).saturating_sub(pw);
    preferred.min(max_x).max(term.x)
}

fn clamp_y(preferred: u16, ph: u16, term: Rect) -> u16 {
    let max_y = (term.y + term.height).saturating_sub(ph);
    preferred.min(max_y).max(term.y)
}
