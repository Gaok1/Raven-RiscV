//! The Run tab's command bar — the three rows above the panes.
//!
//! ```text
//!   ● PAUSED   core 0/3   hart 0        ▶ run  ▌▌ pause  ▶▌ step  ▌◀ back  ⟲ reset       cycles 1 284   cpi 1.07
//!   view regs  │  fmt hex   sign uns  │  count on   type on  │  speed 1x                              instrs 1 203
//! ────────────────────────────────────────────────────────────────────────────────────────────────────────────────
//! ```
//!
//! This used to be a bordered `Run Controls` panel five rows tall, holding ten
//! controls of identical weight in one queue. Three things were wrong with it:
//! the border spent two rows to say nothing, `state run` looked clickable but
//! was a readout, and nothing separated a display toggle from a destructive
//! action.
//!
//! So: the border is gone (a toolbar is not a content region), the state became
//! a badge whose colour carries the meaning, the transport verbs became explicit
//! controls that say what they *do* rather than what the machine currently *is*,
//! and the toggles are grouped by what they affect. Five rows down to three, two
//! of them content.
//!
//! ## Hit-testing
//!
//! Both rows are [`Toolbar`]s, which is the single source of truth for layout
//! *and* for mapping a click column back to a control — see
//! [`build_run_toolbar`] and [`build_run_display_toolbar`]. Add a control to one
//! of those and it renders and becomes clickable in one edit.

use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::{App, FormatMode, MemRegion, RunButton};
use crate::ui::app::HartLifecycle;
use crate::ui::theme;
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style::{self, Metric};

/// Rows the command bar occupies: transport, gap, toggles, rule.
///
/// The blank row is not padding for its own sake — two rows of pills stacked
/// directly on each other read as one dense block, and the eye has to find the
/// boundary between "what the machine is doing" and "what I am looking at".
/// A row of air is the cheapest way to say they are two groups.
pub(crate) const RUN_COMMAND_BAR_H: u16 = 4;

/// Row the display-toggle bar renders on, relative to the command bar's top.
/// The mouse asks for this rather than assuming the row below the transport.
pub(crate) const RUN_DISPLAY_ROW: u16 = 2;

/// Column the transport controls start at, measured from the bar's left edge.
/// The status badge sits to its left; parking the transport at a fixed column
/// stops it sliding around as the badge text changes width between `run` and
/// `faulted`.
const TRANSPORT_COL: u16 = 30;

/// Left indent shared by both rows — also the origin the mouse hit-test uses.
pub(crate) const RUN_BAR_INDENT: u16 = 2;

pub(crate) fn render_run_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(Paragraph::new(command_bar_lines(app, area.width)), area);
}

pub(crate) fn run_controls_plain_text(app: &App) -> String {
    build_run_toolbar(app)
        .spans()
        .into_iter()
        .chain(build_run_display_toolbar(app).spans())
        .map(|span| span.content.to_string())
        .collect()
}

fn command_bar_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    vec![
        transport_line(app, width),
        Line::raw(""),
        display_line(app, width),
        Line::from(Span::styled(
            "─".repeat(width as usize),
            Style::default().fg(theme::BORDER),
        )),
    ]
}

// ── Row 1: state, transport, metrics ─────────────────────────────────────────

fn transport_line(app: &App, width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ".repeat(RUN_BAR_INDENT as usize))];
    spans.extend(build_run_toolbar(app).spans());

    // Metrics ride the right edge so they stay put while controls to their left
    // appear and disappear with the sidebar mode.
    let metrics = metric_spans(app);
    let used = RUN_BAR_INDENT + build_run_toolbar(app).width();
    let metrics_w: u16 = metrics.iter().map(|s| s.width() as u16).sum();
    if width > used + metrics_w {
        spans.push(Span::raw(" ".repeat((width - used - metrics_w) as usize)));
        spans.extend(metrics);
    }
    Line::from(spans)
}

/// The status badge: one shape, colour carries the meaning. It is *not* in the
/// toolbar's clickable set — it reports, and the transport controls beside it
/// act. Mixing the two in one queue is what made `state run` ambiguous.
fn status_badge(app: &App) -> Vec<Span<'static>> {
    let (text, color) = state_label(app);
    vec![
        Span::styled("●", Style::default().fg(color)),
        Span::styled(
            format!(" {}", text.to_uppercase()),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]
}

/// What the machine is doing, and the colour that says it.
///
/// [`HartLifecycle::Running`] means the hart is *bound and runnable*, not that
/// it is executing right now — whether the simulation is stepping is
/// `session.is_running`. Reading the lifecycle alone is what made the old bar
/// report `state run` while the machine sat still, and it is the same mistake
/// that would leave `▶ run` looking inert on a stopped machine.
fn state_label(app: &App) -> (&'static str, Color) {
    // A trait-driven backend has no per-hart lifecycle to read: the badge asks
    // the machine's own state — the same place `machine_step`/`machine_cycle`
    // turn off the run. Without this the bar reported `paused` after a toy16 or
    // x86-64 program had already `halt`ed or `exit`ed.
    if app.rv32().is_none() {
        return match app.machine_snapshot().state {
            raven_engine::MachineState::Halted => ("halted", theme::DANGER),
            raven_engine::MachineState::Exited(_) => ("exited", theme::LABEL),
            raven_engine::MachineState::Faulted => ("faulted", theme::DANGER),
            raven_engine::MachineState::AwaitingInput => ("awaiting input", theme::PAUSED),
            _ => {
                if app.session.is_running {
                    ("running", theme::RUNNING)
                } else {
                    ("paused", theme::PAUSED)
                }
            }
        };
    }

    match app.core_status(app.selected_core) {
        HartLifecycle::Free => ("free", theme::IDLE),
        HartLifecycle::Faulted => ("faulted", theme::DANGER),
        HartLifecycle::Exited => {
            if app.rv32().is_some_and(|rv32| rv32.cpu().local_exit) {
                ("halted", theme::DANGER)
            } else {
                ("exited", theme::LABEL)
            }
        }
        HartLifecycle::Paused | HartLifecycle::Running => {
            if app.rv32().is_some_and(|rv32| rv32.cpu().ebreak_hit) {
                ("ebreak", theme::PAUSED)
            } else if app.session.is_running {
                ("running", theme::RUNNING)
            } else {
                ("paused", theme::PAUSED)
            }
        }
    }
}

fn metric_spans(app: &App) -> Vec<Span<'static>> {
    let totals = app.execution_totals();
    vec![
        style::metric_span("cycles ", totals.cycles, Metric::Cycles),
        Span::raw("   "),
        style::metric_span("CPI ", format!("{:.2}", totals.cpi()), Metric::Cpi),
        Span::raw("   "),
        Span::styled(format!("instrs {}", totals.instructions), style::label()),
        Span::raw("  "),
    ]
}

/// The transport half of the bar as a [`Toolbar`] — the single source of truth
/// shared by the renderer and `mouse::run_status_hit`.
///
/// The status badge is pushed as a disabled cell so the toolbar owns the whole
/// row's geometry: the hit-test origin stays the row's left indent instead of
/// being the indent plus a separately-computed badge width, which is exactly the
/// kind of parallel arithmetic [`Toolbar`] exists to delete.
pub(crate) fn build_run_toolbar(app: &App) -> Toolbar<RunButton> {
    let hov = |b: RunButton| app.hover_run_button == Some(b);
    let multi_core = app.max_cores > 1;
    // Whether the simulation is *stepping*, which is what the transport acts on
    // — not the hart's lifecycle. See `state_label`.
    let running = app.session.is_running;

    // Gap 1: the pills carry their own padding column, so a wider gap pushes the
    // cluster apart instead of grouping it.
    let mut bar = Toolbar::with_gap(1);
    bar.text(status_badge(app));
    bar.toggle(
        RunButton::Core,
        "core",
        &format!("{}/{}", app.selected_core, app.max_cores.saturating_sub(1)),
        ControlState::chip(multi_core, hov(RunButton::Core)).disabled_if(!multi_core),
        theme::TEXT,
    );
    bar.text(vec![
        Span::styled("hart ", style::idle()),
        Span::styled(
            app.core_hart_id(app.selected_core)
                .map_or_else(|| "-".to_string(), |id| id.to_string()),
            style::value(),
        ),
    ]);

    bar.pad_to(TRANSPORT_COL);

    // Each verb says what it does and goes inert when it cannot do it, instead
    // of one `state` chip that reported what the machine already was. A control
    // you cannot use should look unusable, not absent — the row must not reflow
    // under the cursor as the machine changes state.
    bar.action(
        RunButton::Run,
        "▶ run",
        ControlState::chip(false, hov(RunButton::Run)).disabled_if(running),
        theme::RUNNING,
    )
    .action(
        RunButton::Pause,
        "▌▌ pause",
        ControlState::chip(false, hov(RunButton::Pause)).disabled_if(!running),
        theme::PAUSED,
    )
    .action(
        RunButton::Step,
        "▶▌ step",
        ControlState::chip(false, hov(RunButton::Step)).disabled_if(running),
        theme::TEXT,
    )
    .action(
        RunButton::Stepback,
        "▌◀ back",
        ControlState::chip(false, hov(RunButton::Stepback))
            .disabled_if(!app.can_stepback_now()),
        theme::TEXT,
    )
    .action(
        RunButton::Reset,
        "⟲ reset",
        ControlState::chip(false, hov(RunButton::Reset)),
        theme::DANGER,
    );

    bar
}

// ── Row 2: display toggles, grouped ──────────────────────────────────────────

/// The display half of the bar. Separate toolbar, separate row, so the hit-test
/// can tell a click on `fmt hex` from a click on `▶ run` by row alone.
pub(crate) fn build_run_display_toolbar(app: &App) -> Toolbar<RunButton> {
    let hov = |b: RunButton| app.hover_run_button == Some(b);
    let signed = sign_enabled(app);

    // Gap 1: the pills carry their own padding column, so a wider gap pushes the
    // cluster apart instead of grouping it.
    let mut bar = Toolbar::with_gap(1);
    bar.toggle(
        RunButton::View,
        "view",
        view_text(app),
        ControlState::chip(true, hov(RunButton::View)),
        theme::TEXT,
    );

    bar.separator();
    bar.toggle(
        RunButton::Format,
        "fmt",
        format_text(app),
        ControlState::chip(true, hov(RunButton::Format)),
        theme::TEXT,
    )
    .toggle(
        RunButton::Sign,
        "sign",
        sign_text(app),
        ControlState::chip(signed, hov(RunButton::Sign)).disabled_if(!signed),
        theme::TEXT,
    );

    if app.run_sidebar_shows_memory() {
        bar.separator();
        bar.toggle(
            RunButton::Region,
            "region",
            region_text(app),
            ControlState::chip(true, hov(RunButton::Region)),
            theme::TEXT,
        )
        .toggle(
            RunButton::Bytes,
            "bytes",
            bytes_text(app),
            ControlState::chip(true, hov(RunButton::Bytes)),
            theme::TEXT,
        );
    }

    bar.separator();
    bar.toggle(
        RunButton::ExecCount,
        "count",
        if app.run.show_exec_count { "on" } else { "off" },
        ControlState::chip(app.run.show_exec_count, hov(RunButton::ExecCount)),
        theme::TEXT,
    )
    .toggle(
        RunButton::InstrType,
        "type",
        if app.run.show_instr_type { "on" } else { "off" },
        ControlState::chip(app.run.show_instr_type, hov(RunButton::InstrType)),
        theme::TEXT,
    );

    if app.console.screen.is_some() {
        bar.toggle(
            RunButton::Screen,
            "screen",
            if app.run.show_screen { "on" } else { "off" },
            ControlState::chip(app.run.show_screen, hov(RunButton::Screen)),
            theme::TEXT,
        );
    }

    bar.separator();
    bar.toggle(
        RunButton::Speed,
        "speed",
        app.session.speed.label(),
        ControlState::chip(true, hov(RunButton::Speed)),
        theme::TEXT,
    );

    bar
}

fn display_line(app: &App, width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ".repeat(RUN_BAR_INDENT as usize))];
    spans.extend(build_run_display_toolbar(app).spans());

    // While an inline edit is open the row carries its prompt instead of the
    // trailing state — the message is transient and the row is already here.
    let tail = edit_status_spans(app);
    let used = RUN_BAR_INDENT + build_run_display_toolbar(app).width();
    let tail_w: u16 = tail.iter().map(|s| s.width() as u16).sum();
    if width > used + tail_w {
        spans.push(Span::raw(" ".repeat((width - used - tail_w) as usize)));
        spans.extend(tail);
    }
    Line::from(spans)
}

/// The commit/cancel prompt while an inline edit is open, or the rejection
/// message when the last commit was out of range. Otherwise the core/hart
/// context, which has nowhere better to live.
fn edit_status_spans(app: &App) -> Vec<Span<'static>> {
    if let Some(error) = &app.run.run_edit_error {
        return vec![
            Span::styled(
                format!("✗ {error}"),
                Style::default().fg(theme::DANGER).bold(),
            ),
            Span::raw("  "),
        ];
    }
    if app.run.run_edit.is_some() {
        return vec![
            Span::styled(
                "editing — Enter commits, Esc cancels",
                Style::default().fg(theme::ACCENT),
            ),
            Span::raw("  "),
        ];
    }
    vec![
        Span::styled(format!("scope {}", app.execution_totals().scope.label()), style::label()),
        Span::raw("  "),
    ]
}

// ── Text helpers ────────────────────────────────────────────────────────────

fn view_text(app: &App) -> &'static str {
    if app.run.show_dyn {
        "dyn"
    } else if app.run.show_registers {
        "regs"
    } else {
        "ram"
    }
}

fn region_text(app: &App) -> &'static str {
    match app.run.mem_region {
        MemRegion::Data | MemRegion::Custom => "data",
        MemRegion::Stack => "stack",
        MemRegion::Access => "r/w",
        MemRegion::Heap => "heap",
    }
}

fn format_text(app: &App) -> &'static str {
    match app.run.fmt_mode {
        FormatMode::Hex => "hex",
        FormatMode::Dec => "dec",
        FormatMode::Bin => "bin",
        FormatMode::Str => "str",
    }
}

fn sign_enabled(app: &App) -> bool {
    matches!(app.run.fmt_mode, FormatMode::Dec)
}

fn sign_text(app: &App) -> &'static str {
    if app.run.show_signed { "sgn" } else { "uns" }
}

fn bytes_text(app: &App) -> &'static str {
    match app.run.mem_view_bytes {
        4 => "4b",
        2 => "2b",
        _ => "1b",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn halted_toy16() -> App {
        let mut app =
            App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, "toy16")
                .unwrap();
        for _ in 0..1000 {
            if matches!(app.machine_snapshot().state, raven_engine::MachineState::Halted) {
                break;
            }
            app.single_step();
        }
        app
    }

    /// A trait-driven backend has no per-hart lifecycle, so the badge reads the
    /// machine's own state — it used to report `paused` after a program had
    /// already halted.
    #[test]
    fn trait_backend_reports_halted_not_paused() {
        let app = halted_toy16();
        assert_eq!(
            app.machine_snapshot().state,
            raven_engine::MachineState::Halted
        );
        assert_eq!(state_label(&app).0, "halted");
    }
}
