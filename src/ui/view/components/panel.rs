//! Panel chrome — the rounded, titled `Block` that wraps almost every region of
//! the UI, plus the `inner() + render_widget` dance that follows it ~40×.
//!
//! Use [`panel`] for a titled content region, [`panel_state`] when the panel has
//! a state to advertise, [`panel_frame`] when you draw the title yourself,
//! [`panel_square`] for the non-rounded boxes (pipeline/gantt), and
//! [`handle_bar`] for the top-rule drag/preset/apply strips. [`render_panel`]
//! collapses "compute inner, render the block, return inner" into one call.
//!
//! ## The title bar
//!
//! ```text
//! ╭─ Registers ──────────────── int ─╮
//! ```
//!
//! The title is set in from the corner by a dash and a space, so it reads as a
//! label on the rule rather than as something jammed against the corner. The
//! right-hand slot carries **state** — which bank, which region, which address.
//!
//! It never carries a keyboard hint. `[Tab]=Float` in a title made the bracket
//! mean three different things across the UI (instruction class, key, button),
//! which is what made the `[?]` help button impossible to pick out. Keys live in
//! the footer and the help overlay; see [`style::key_hint`].

use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, BorderType, Borders};

use crate::ui::theme;
use crate::ui::view::style;

/// What a panel *is*, semantically. Encodes both the border color and the title
/// style so each call site states its intent explicitly.
///
/// The three neutral kinds form a deliberate ladder — secondary, primary, and
/// "the keyboard is here":
///
/// | kind      | border | title      |
/// |-----------|--------|------------|
/// | `Plain`   | dim    | dim        |
/// | `Accent`  | dim    | bright bold|
/// | `Focused` | violet | bright bold|
#[derive(Clone, Copy)]
pub(crate) enum PanelKind {
    /// Secondary content panel — neutral border, muted (LABEL) title.
    Plain,
    /// Primary panel — neutral border, bright bold title.
    Accent,
    /// The pane that currently owns the keyboard — violet border, bright bold
    /// title. This is the only place violet appears on a panel frame, so "where
    /// am I typing?" is answerable at a glance.
    Focused,
    /// Warning — amber border + bold title.
    Warning,
    /// Danger — red border + bold title.
    Danger,
    /// Arbitrary color for both border and (bold) title.
    Custom(Color),
}

impl PanelKind {
    fn border_color(self) -> Color {
        match self {
            PanelKind::Plain | PanelKind::Accent => theme::BORDER,
            PanelKind::Focused => theme::ACCENT,
            PanelKind::Warning => theme::PAUSED,
            PanelKind::Danger => theme::DANGER,
            PanelKind::Custom(c) => c,
        }
    }

    fn title_style(self) -> Style {
        match self {
            PanelKind::Plain => style::label(),
            PanelKind::Accent | PanelKind::Focused => Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
            PanelKind::Warning => Style::default()
                .fg(theme::PAUSED)
                .add_modifier(Modifier::BOLD),
            PanelKind::Danger => Style::default()
                .fg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
            PanelKind::Custom(c) => Style::default().fg(c).add_modifier(Modifier::BOLD),
        }
    }

    fn rule(self) -> Style {
        Style::default().fg(self.border_color())
    }
}

/// Rounded, all-borders panel titled `╭─ Title ─…─╮`.
pub(crate) fn panel(title: impl Into<String>, kind: PanelKind) -> Block<'static> {
    panel_frame(kind).title(Line::from(vec![
        Span::styled("─ ", kind.rule()),
        Span::styled(title.into(), kind.title_style()),
        Span::styled(" ", kind.rule()),
    ]))
}

/// [`panel`] plus the right-hand state slot: `╭─ Title ─…─ state ─╮`.
///
/// `state` is what the panel is currently showing — the register bank, the
/// memory region, the address under the cursor. An empty `state` degrades to a
/// plain [`panel`], so a caller can pass a conditional string without branching.
pub(crate) fn panel_state(
    title: impl Into<String>,
    state: impl Into<String>,
    kind: PanelKind,
) -> Block<'static> {
    let state = state.into();
    if state.is_empty() {
        return panel(title, kind);
    }
    panel_state_spans(title, vec![Span::styled(state, style::label())], kind)
}

/// [`panel_state`] where the state is styled per-span — for slots that show a
/// choice rather than a single fact, e.g. a register bank selector where the
/// visible bank is lit and the others sit dim beside it:
///
/// ```text
/// ╭─ Registers ──────────── int · flags ─╮
/// ```
///
/// This is how an alternative gets advertised without a key hint: the options
/// are *data*, so they can live in the title. Which key switches them belongs in
/// the footer.
pub(crate) fn panel_state_spans(
    title: impl Into<String>,
    state: Vec<Span<'static>>,
    kind: PanelKind,
) -> Block<'static> {
    let block = panel(title, kind);
    if state.is_empty() {
        return block;
    }
    let mut spans = vec![Span::styled(" ", kind.rule())];
    spans.extend(state);
    spans.push(Span::styled(" ─", kind.rule()));
    block.title(Title::from(Line::from(spans)).alignment(Alignment::Right))
}

/// Rounded, all-borders panel without a title (caller draws its own header, or
/// only needs the frame for `.inner()` geometry).
pub(crate) fn panel_frame(kind: PanelKind) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(kind.border_color()))
}

/// A square (non-rounded) all-borders box with a pre-styled title line and an
/// explicit border `Style` — for the pipeline stage / gantt cells whose border
/// is colored (and sometimes bold) per state.
///
/// The title is inset by a dash and a space like every other panel's, so a
/// pipeline stage does not read as a different species of box from the rest of
/// the app. Callers pass the title already styled, since its colour tracks the
/// stage's state.
pub(crate) fn panel_square(title: impl Into<Line<'static>>, border: Style) -> Block<'static> {
    let mut spans = vec![Span::styled("─ ", border)];
    spans.extend(title.into().spans);
    spans.push(Span::styled(" ", border));
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Line::from(spans))
}

/// A labelled hairline that divides one panel into sections:
///
/// ```text
///  ENCODING ───────────────────────────
/// ```
///
/// Use this instead of nesting a second bordered panel inside the first. A
/// nested box spends two rows on chrome to say what one rule says, and stacking
/// three of them — as the Run tab's details pane did — spends six rows of an
/// already-cramped column drawing lines around three views of the same
/// instruction.
pub(crate) fn section_rule(name: &str, width: u16) -> Line<'static> {
    let dashes = (width as usize).saturating_sub(name.chars().count() + 1);
    Line::from(vec![
        Span::styled(name.to_string(), style::label()),
        Span::raw(" "),
        Span::styled("─".repeat(dashes), Style::default().fg(theme::BORDER)),
    ])
}

/// A single top rule (`Borders::TOP`) — the collapsed/preset/apply/console
/// handle strips.
pub(crate) fn handle_bar(border: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border))
}

/// Render `block` into `area` and return its inner rect. Replaces the
/// `let inner = block.inner(area); f.render_widget(block, area);` pair.
pub(crate) fn render_panel(f: &mut Frame, area: Rect, block: Block<'static>) -> Rect {
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    /// Render `block` at `w × 3` and return its top border line.
    fn top_rule(block: Block<'static>, w: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, 3)).expect("terminal");
        terminal
            .draw(|f| f.render_widget(block, f.area()))
            .expect("render");
        let buffer = terminal.backend().buffer().clone();
        (0..w).map(|x| buffer[(x, 0)].symbol()).collect()
    }

    /// The title bar is the most-repeated shape in the app — ~40 panels wear it.
    /// Pin the exact spacing so a change has to be deliberate.
    #[test]
    fn the_title_sits_on_the_rule_with_a_dash_and_a_space() {
        assert_eq!(
            top_rule(panel("Registers", PanelKind::Plain), 24),
            "╭─ Registers ──────────╮"
        );
    }

    /// The right slot carries state, and it keeps a dash between itself and the
    /// corner so the rule reads as continuous.
    #[test]
    fn the_state_slot_is_right_aligned_against_the_corner() {
        assert_eq!(
            top_rule(panel_state("Registers", "int", PanelKind::Focused), 28),
            "╭─ Registers ──────── int ─╮"
        );
    }

    /// An absent state must not leave a hole where the slot would be, so callers
    /// can pass `""` instead of branching.
    #[test]
    fn an_empty_state_degrades_to_a_plain_panel() {
        assert_eq!(
            top_rule(panel_state("Registers", "", PanelKind::Plain), 24),
            top_rule(panel("Registers", PanelKind::Plain), 24)
        );
    }

    /// A title longer than the panel must not panic or spill past the corner.
    #[test]
    fn a_title_wider_than_the_panel_stays_inside_it() {
        for w in 1..=12u16 {
            let rule = top_rule(panel_state("Instruction Memory", "pc", PanelKind::Accent), w);
            assert_eq!(rule.chars().count(), w as usize);
        }
    }
}
