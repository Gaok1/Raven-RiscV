//! Semantic style & span builders for the Raven UI.
//!
//! `theme.rs` owns the *palette* (raw `Color` constants); this module turns
//! those colors into the handful of **semantic** `Style`/`Span` builders that
//! every view should use instead of open-coding `Style::default().fg(...)`.
//! If a control's color or weight needs to change, edit it here once.
//!
//! See `view/components/mod.rs` for the overall "Raven way" of writing UI and
//! which helper to reach for in each situation.

// Phase 0 scaffolding: these builders are consumed incrementally by Phases 2-5.
// The module-level allow is removed in Phase 6 once every site is migrated.
#![allow(dead_code)]

use ratatui::prelude::*;

use crate::ui::theme;

// ── Semantic text styles ──────────────────────────────────────────────────────

/// Auxiliary / hint text — muted neutral.
pub(crate) fn label() -> Style {
    Style::default().fg(theme::LABEL)
}

/// Normal body / value text — neutral off-white.
pub(crate) fn value() -> Style {
    Style::default().fg(theme::TEXT)
}

/// Idle / inactive control text.
pub(crate) fn idle() -> Style {
    Style::default().fg(theme::IDLE)
}

/// Danger / error / destructive.
pub(crate) fn danger() -> Style {
    Style::default().fg(theme::DANGER)
}

/// Success / running / "true".
pub(crate) fn success() -> Style {
    Style::default().fg(theme::RUNNING)
}

/// Warning / paused.
pub(crate) fn warning() -> Style {
    Style::default().fg(theme::PAUSED)
}

/// Panel / popup title — accent, bold.
pub(crate) fn title() -> Style {
    Style::default()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD)
}

/// Key hint glyph — accent, bold.
pub(crate) fn key() -> Style {
    Style::default()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD)
}

/// A titled span ready to drop into a `Block::title(...)`.
pub(crate) fn title_span(s: impl Into<String>) -> Span<'static> {
    Span::styled(s.into(), title())
}

// ── Metrics (Cycles / CPI / IPC) ──────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Metric {
    Cycles,
    Cpi,
    Ipc,
}

pub(crate) fn metric(k: Metric) -> Style {
    let c = match k {
        Metric::Cycles => theme::METRIC_CYC,
        Metric::Cpi => theme::METRIC_CPI,
        Metric::Ipc => theme::METRIC_IPC,
    };
    Style::default().fg(c)
}

/// `"<label><val>"` styled in the metric's color. The caller supplies any
/// separator inside `label` (e.g. `"Cycles:"` or `"CPI: "`).
pub(crate) fn metric_span(
    label: impl Into<String>,
    val: impl std::fmt::Display,
    k: Metric,
) -> Span<'static> {
    Span::styled(format!("{}{}", label.into(), val), metric(k))
}

// ── Badges / pills (filled chip, dark text) ───────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Badge {
    Accent,
    Danger,
    Idle,
    Success,
}

pub(crate) fn badge(text: impl Into<String>, kind: Badge) -> Span<'static> {
    let bg = match kind {
        Badge::Accent => theme::ACCENT,
        Badge::Danger => theme::DANGER,
        Badge::Idle => theme::IDLE,
        Badge::Success => theme::RUNNING,
    };
    Span::styled(
        text.into(),
        Style::default()
            .fg(Color::Black)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

// ── Toggle chip (the unified 3-state control style) ───────────────────────────

/// The one "toggle chip" style triangle: hovered → bold text, active → bold
/// `active_color`, otherwise idle. Collapses the ~6 per-file `*_btn_style`
/// duplicates and matches `controls::dense_value`.
pub(crate) fn toggle(active: bool, hovered: bool, active_color: Color) -> Style {
    if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    }
}

// ── Key hints / legend bars ───────────────────────────────────────────────────

/// `[key] desc` as two spans: accent key + muted description.
pub(crate) fn key_hint(key_label: &str, desc: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(key_label.to_string(), key()),
        Span::styled(format!(" {desc}"), label()),
    ]
}

/// A footer/legend line built from `(key, desc)` pairs, separated by spacing.
pub(crate) fn hint_bar(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, d)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("   ", label()));
        }
        spans.extend(key_hint(k, d));
    }
    Line::from(spans)
}
