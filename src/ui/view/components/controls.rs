use ratatui::prelude::*;
use ratatui::widgets::ListItem;

use crate::ui::theme;

// ── Interaction state & the label "triangle" ──────────────────────────────────

/// Interaction state of a labelled control. Drives the label "triangle" that was
/// open-coded ~14× across settings + the config panels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlState {
    Normal,
    Hovered,
    Selected,
    Disabled,
}

impl ControlState {
    /// Selected wins over Hovered (matches the `if sel … else if hov …` order
    /// every call site used).
    pub(crate) fn from(selected: bool, hovered: bool) -> Self {
        if selected {
            Self::Selected
        } else if hovered {
            Self::Hovered
        } else {
            Self::Normal
        }
    }

    /// Collapse to `Disabled` when a control is inert (e.g. TLB row while VM off).
    pub(crate) fn disabled_if(self, off: bool) -> Self {
        if off { Self::Disabled } else { self }
    }
}

/// The single style core shared by every control (labels, value chips, toggles,
/// subtabs, actions). Maps the four interaction states to their leaf styles:
/// Hovered → text bold, Selected → `active_color` bold, Disabled → dim border,
/// Normal → `normal_color`.
///
/// Callers decide the *precedence* between Selected and Hovered themselves (the
/// label triangle is selected-first via [`ControlState::from`]; value chips are
/// hover-first, see [`dense_value`]) and pass the two colours that vary by role:
/// a value chip selects in its semantic colour over a dim `IDLE` normal, while a
/// label selects in `ACCENT` over a visible `base` normal.
pub(crate) fn control_style(
    state: ControlState,
    active_color: Color,
    normal_color: Color,
) -> Style {
    match state {
        ControlState::Hovered => Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ControlState::Selected => Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD),
        ControlState::Disabled => Style::default().fg(theme::BORDER),
        ControlState::Normal => Style::default().fg(normal_color),
    }
}

/// The unified label triangle: Selected → accent bold, Hovered → text bold,
/// Disabled → dim border, Normal → the caller's `base` color (LABEL for most
/// settings, IDLE for pipeline, CPI_PANEL for the CPI section).
pub(crate) fn label_style(state: ControlState, base: Color) -> Style {
    control_style(state, theme::ACCENT, base)
}

/// `label_style` applied to text, ready to drop into a `Line`.
pub(crate) fn label_span(
    text: impl Into<String>,
    state: ControlState,
    base: Color,
) -> Span<'static> {
    Span::styled(text.into(), label_style(state, base))
}

// ── Value spans (the unified boolean / edit-cursor / enum-cycle renderings) ────

/// The unified boolean value: `true`/`false` in green/red, bold; hovered → bold
/// text. Absorbs `settings::bool_button` and the vm/pipeline `[on]/[off]` chips.
pub(crate) fn bool_value(value: bool, hovered: bool) -> Span<'static> {
    let (label, color) = if value {
        ("true", theme::RUNNING)
    } else {
        ("false", theme::DANGER)
    };
    dense_value(label, hovered, true, color)
}

/// A numeric field's value and the single source of the `█` edit cursor: shows
/// `{buf}█` (accent, bold) while editing, otherwise the plain value in `color`.
pub(crate) fn edit_value(
    display: &str,
    editing: Option<&str>,
    hovered: bool,
    color: Color,
) -> Span<'static> {
    match editing {
        Some(buf) => Span::styled(
            format!("{buf}█"),
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
        None => dense_value(display, hovered, true, color),
    }
}

/// The value span for an editable cache/TLB config field. While the field is
/// active it shows the edit cursor (numeric) or `< value >` (cyclable enum);
/// otherwise the plain value, coloured TEXT when unchanged and LABEL_Y once it
/// differs from the applied config.
pub(crate) fn field_value(
    value: &str,
    active: bool,
    numeric: bool,
    edit_buf: &str,
    hovered: bool,
    changed: bool,
) -> Span<'static> {
    if active {
        if numeric {
            edit_value(value, Some(edit_buf), false, theme::ACCENT)
        } else {
            dense_value(&format!("< {value} >"), false, true, theme::ACCENT)
        }
    } else {
        let color = if changed { theme::LABEL_Y } else { theme::TEXT };
        dense_value(value, hovered, true, color)
    }
}

/// One editable config-field row (cache / TLB settings): styled label + value,
/// label-state and value-context derived from the same field. The per-file
/// `field_item` closures collapse onto this.
pub(crate) fn field_row(
    label: &'static str,
    value: &str,
    active: bool,
    numeric: bool,
    edit_buf: &str,
    hovered: bool,
    changed: bool,
) -> ListItem<'static> {
    let state = ControlState::from(active, hovered);
    ListItem::new(Line::from(vec![
        label_span(label, state, theme::LABEL),
        field_value(value, active, numeric, edit_buf, hovered, changed),
    ]))
}

pub(crate) fn push_dense_pair(
    spans: &mut Vec<Span<'static>>,
    label: &str,
    value: &str,
    hovered: bool,
    active: bool,
    active_color: Color,
) {
    if !spans.is_empty() {
        spans.push(Span::raw("   "));
    }
    spans.push(Span::styled(
        label.to_string(),
        Style::default().fg(theme::IDLE),
    ));
    spans.push(Span::raw(" "));
    spans.push(dense_value(value, hovered, active, active_color));
}

/// A value chip: bold-bright when hovered, lit in `color` when active, dim
/// otherwise. Hover-first precedence (hover feedback wins over the active
/// colour), unlike the selected-first label triangle.
pub(crate) fn dense_value(text: &str, hovered: bool, active: bool, color: Color) -> Span<'static> {
    let state = if hovered {
        ControlState::Hovered
    } else if active {
        ControlState::Selected
    } else {
        ControlState::Normal
    };
    Span::styled(text.to_string(), control_style(state, color, theme::IDLE))
}

/// A standalone action word (e.g. `reset`): always lit in `color`, bold-bright
/// when hovered. There is no inactive state.
pub(crate) fn dense_action(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let state = if hovered {
        ControlState::Hovered
    } else {
        ControlState::Selected
    };
    Span::styled(text.to_string(), control_style(state, color, theme::IDLE))
}
