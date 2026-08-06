//! Main-tab rendering for a model that has left program order.
//!
//! Where the in-order view shows stage boxes, these models have no stages worth
//! drawing: the state that matters moved into the structures that replaced
//! them. So the screen becomes a workbench of tables, each answering one
//! question.
//!
//! - **Stations** — what every issued instruction is still missing. An operand
//!   either holds its value or names the entry that owes it, and that is the
//!   renaming made visible. Free slots are drawn rather than omitted: a full
//!   bank beside a stalling front end is what a structural hazard looks like.
//! - **Reorder buffer** — program order, and how far each entry got. Only the
//!   head may commit, however long anyone younger has been ready.
//! - **Register table** — which registers currently name a producer instead of
//!   a value.
//! - **Issue queue** — fetched, not yet issued.
//!
//! None of it names a model. A scoreboard reports an empty buffer and the empty
//! panel is the point — results reach the register file in whatever order the
//! units finish, which is exactly why it cannot promise a precise exception.

mod focus;
mod wires;

use crate::ui::app::App;
use crate::ui::pipeline::{OooFocus, OooPanel, OooRowHit};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{Align, Col, DataTable};
use crate::ui::view::style;
use focus::{Highlight, Role};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use raven_engine::capability::{
    PipelineDynamicInspect, PipelineEntryPhase, PipelineInspect, PipelineOperandView,
    PipelineSlotView,
};
use wires::WireLayer;

/// A row's paint, decided by how it relates to the hovered one.
///
/// Every cell of a dimmed row drops to one muted grey: the chain has to be what
/// the eye finds, and it cannot be if everything around it keeps its colours.
#[derive(Clone, Copy)]
struct Paint(Role);

impl Paint {
    fn style(self, color: Color) -> Style {
        let style = Style::default().fg(if self.0.keeps_color() {
            color
        } else {
            theme::HL_DIM
        });
        if self.0 == Role::Primary {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    fn line(self, text: impl Into<String>, color: Color) -> Line<'static> {
        Line::from(Span::styled(text.into(), self.style(color)))
    }

    /// The lead cell: a caret on the focused row, an arrow on the rows tied to
    /// it, nothing on the rest. Direction is the content — up is what this row
    /// waits for, down is what waits for it.
    fn marker(self) -> Span<'static> {
        match self.0 {
            Role::Primary => Span::styled(
                "▸",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Role::Producer => Span::styled("↑", Style::default().fg(theme::WIRE_PRODUCER)),
            Role::Consumer => Span::styled("↓", Style::default().fg(theme::WIRE_CONSUMER)),
            Role::Neutral | Role::Dim => Span::raw(" "),
        }
    }
}

/// Rows the strip under the two tables wants: its own heading, plus content.
const STRIP_H: u16 = 8;
const HAZARDS_H: u16 = 4;
/// Below this the tables stack instead of sitting side by side.
const SIDE_BY_SIDE_W: u16 = 84;

/// Columns the wire gutter takes between the two tables.
const GUTTER_W: u16 = 5;

pub(super) fn render_pipeline_ooo(
    f: &mut Frame,
    area: Rect,
    app: &App,
    pipeline: &dyn PipelineInspect,
    model: &dyn PipelineDynamicInspect,
) {
    let view = app.run.pipeline_view();
    // One hover, resolved once, applied everywhere: the tables, the wires
    // between them, and the row down in the history.
    let highlight = focus::resolve(model, view.ooo_hover);
    view.ooo_row_hits.borrow_mut().clear();

    // The Gantt keeps the bottom of the screen — it is the one view that shows
    // an instruction finishing early and waiting, which is what the buffer is
    // for. The tables take what is left above it.
    let gantt_h = area.height.saturating_sub(STRIP_H + HAZARDS_H).max(6);
    let tables_h = area
        .height
        .saturating_sub(gantt_h + STRIP_H + HAZARDS_H)
        .max(7);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tables_h),
            Constraint::Length(STRIP_H.min(area.height.saturating_sub(tables_h))),
            Constraint::Length(HAZARDS_H),
            Constraint::Min(0),
        ])
        .split(area);

    render_structures(f, rows[0], app, model, &highlight);
    if rows[1].height > 2 {
        render_strip(f, rows[1], app, pipeline, model, &highlight);
    }
    if rows[2].height > 1 {
        super::main_view::render_hazards(f, rows[2], pipeline);
    }
    if rows[3].height > 1 {
        super::main_view::render_gantt_focused(f, rows[3], app, pipeline, highlight.address);
    }
}

/// The two structure tables, and the wires between them.
///
/// A station row and a buffer row are the same instruction seen from two
/// structures. Drawing the focused chain as one spine down the gutter is what
/// turns "these rows share a colour" into "these rows are one dependency".
fn render_structures(
    f: &mut Frame,
    area: Rect,
    app: &App,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
) {
    // A model with no buffer gives the whole width to its units — an empty
    // panel taking half the screen would say less than a full one.
    if model.buffer_capacity() == 0 {
        render_stations(
            f,
            area,
            app,
            model,
            highlight,
            &mut WireLayer::new(Rect::ZERO),
        );
        return;
    }
    if area.width < SIDE_BY_SIDE_W {
        let half = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2); 2])
            .split(area);
        let mut wires = WireLayer::new(Rect::ZERO);
        render_stations(f, half[0], app, model, highlight, &mut wires);
        render_buffer(f, half[1], app, model, highlight, &mut wires);
        return;
    }
    let side = area.width.saturating_sub(GUTTER_W) / 2;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side),
            Constraint::Length(GUTTER_W),
            Constraint::Min(0),
        ])
        .split(area);
    let mut wires = WireLayer::new(cols[1]);
    render_stations(f, cols[0], app, model, highlight, &mut wires);
    render_buffer(f, cols[2], app, model, highlight, &mut wires);
    wires.paint(f);
}

/// Record where a row was drawn, so the mouse resolves a hover against what is
/// actually on screen rather than against a layout recomputed from scratch.
fn record(app: &App, panel: OooPanel, row: usize, inner: Rect, line: u16) {
    let y = inner.y + line;
    if y >= inner.y + inner.height {
        return;
    }
    app.run
        .pipeline_view()
        .ooo_row_hits
        .borrow_mut()
        .push(OooRowHit {
            focus: OooFocus { panel, row },
            y,
            x0: inner.x,
            x1: inner.x + inner.width,
        });
}

// ── Stations ─────────────────────────────────────────────────────────────────

/// Below this the operand columns go: a station without them still shows the
/// structural picture, and two columns of `…` would show nothing at all.
fn stations_are_wide(w: u16) -> bool {
    w >= 56
}

fn render_stations(
    f: &mut Frame,
    area: Rect,
    app: &App,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
    wires: &mut WireLayer,
) {
    let total = model.station_count();
    let busy = (0..total)
        .filter_map(|index| model.station(index))
        .filter(|station| station.phase.occupied())
        .count();
    let inner = render_panel(
        f,
        area,
        panel::panel(
            format!("RESERVATION STATIONS · {busy}/{total}"),
            PanelKind::Accent,
        ),
    );
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let wide = stations_are_wide(inner.width);

    let mut cols = vec![
        Col::new("", Constraint::Length(1), Align::Left),
        Col::new("SLOT", Constraint::Length(6), Align::Left),
        Col::new("TAG", Constraint::Length(5), Align::Left),
        Col::new("OP", Constraint::Min(8), Align::Left),
    ];
    if wide {
        cols.push(Col::new("SRC 1", Constraint::Length(11), Align::Left));
        cols.push(Col::new("SRC 2", Constraint::Length(11), Align::Left));
    }
    cols.push(Col::new("STATE", Constraint::Length(7), Align::Left));

    let mut table = DataTable::new(cols).zebra(theme::BG_PANEL);
    for index in 0..total.min(inner.height.saturating_sub(1) as usize) {
        let Some(station) = model.station(index) else {
            continue;
        };
        let free = !station.phase.occupied();
        // A free slot is dim whatever the hover is doing: there is no
        // instruction in it to be part of anyone's chain.
        let role = if free {
            Role::Dim
        } else {
            highlight.role(station.tag)
        };
        let paint = Paint(role);
        // Header row, then one line per station.
        let line = 1 + index as u16;
        record(app, OooPanel::Stations, index, inner, line);
        if role.in_chain() {
            wires.connect_left(inner.y + line, role);
        }

        let mut cells = vec![
            Line::from(paint.marker()),
            paint.line(
                station.name.to_string(),
                if free { theme::BORDER } else { theme::LABEL_Y },
            ),
            paint.line(
                station
                    .tag
                    .map_or("··".to_string(), |tag| format!("#{tag}")),
                theme::LABEL,
            ),
            paint.line(
                station
                    .slot
                    .map_or("—".to_string(), |slot| slot.disassembly.to_string()),
                if free { theme::BORDER } else { theme::TEXT },
            ),
        ];
        if wide {
            cells.push(operand_cell(paint, station.operands[0]));
            cells.push(operand_cell(paint, station.operands[1]));
        }
        cells.push(phase_cell(paint, station.phase, station.slot));
        table = table.row(cells);
    }
    f.render_widget(table.build(), inner);
}

/// An operand that has its value reads as the register; one that does not names
/// the entry it is waiting for. That difference is the whole of renaming.
fn operand_cell(paint: Paint, operand: Option<PipelineOperandView<'_>>) -> Line<'static> {
    match operand {
        None => paint.line("·", theme::BORDER),
        Some(operand) => match operand.producer {
            Some(tag) => Line::from(vec![
                Span::styled(format!("{} ", operand.register), paint.style(theme::LABEL)),
                Span::styled(format!("←#{tag}"), paint.style(theme::WIRE_PRODUCER)),
            ]),
            None => paint.line(operand.register.to_string(), theme::CACHE_D),
        },
    }
}

/// The three reasons work is not producing yet are worth telling apart: waiting
/// on an operand is a true dependency, armed is a structural limit, and
/// finished-but-waiting is the buffer keeping order.
fn phase_cell(
    paint: Paint,
    phase: PipelineEntryPhase,
    slot: Option<PipelineSlotView<'_>>,
) -> Line<'static> {
    let (label, color) = match phase {
        PipelineEntryPhase::Free => ("free".to_string(), theme::BORDER),
        PipelineEntryPhase::Waiting => ("wait".to_string(), theme::PAUSED),
        PipelineEntryPhase::Armed => ("armed".to_string(), theme::SPECULATIVE),
        PipelineEntryPhase::Executing => (
            slot.map_or("exec".to_string(), |slot| {
                format!("exec {}", slot.cycles_remaining)
            }),
            theme::RUNNING,
        ),
        PipelineEntryPhase::Finished => ("ready".to_string(), theme::METRIC_CYC),
        PipelineEntryPhase::Committing => ("commit".to_string(), theme::CPI_PANEL),
    };
    paint.line(label, color)
}

// ── Reorder buffer ───────────────────────────────────────────────────────────

fn buffer_is_wide(w: u16) -> bool {
    w >= 46
}

fn render_buffer(
    f: &mut Frame,
    area: Rect,
    app: &App,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
    wires: &mut WireLayer,
) {
    let used = model.buffer_count();
    let head = model.buffer_entry(0).map_or_else(
        || "empty".to_string(),
        |entry| format!("head #{}", entry.tag),
    );
    let inner = render_panel(
        f,
        area,
        panel::panel(
            format!(
                "REORDER BUFFER · {used}/{} · {head}",
                model.buffer_capacity()
            ),
            PanelKind::Accent,
        ),
    );
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let wide = buffer_is_wide(inner.width);

    let mut cols = vec![
        Col::new("", Constraint::Length(1), Align::Left),
        Col::new("TAG", Constraint::Length(5), Align::Left),
    ];
    if wide {
        cols.push(Col::new("PC", Constraint::Length(8), Align::Left));
    }
    cols.push(Col::new("OP", Constraint::Min(8), Align::Left));
    if wide {
        cols.push(Col::new("DST", Constraint::Length(5), Align::Left));
    }
    cols.push(Col::new("PHASE", Constraint::Length(7), Align::Left));

    let mut table = DataTable::new(cols).zebra(theme::BG_PANEL);
    for index in 0..used.min(inner.height.saturating_sub(1) as usize) {
        let Some(entry) = model.buffer_entry(index) else {
            continue;
        };
        let role = highlight.role(Some(entry.tag));
        let paint = Paint(role);
        let line = 1 + index as u16;
        record(app, OooPanel::Buffer, index, inner, line);
        if role.in_chain() {
            wires.connect_right(inner.y + line, role);
        }

        // The head is the only entry that can retire, so it is marked whether
        // or not it is ready to — unless the hover has something of its own to
        // say about this row.
        let lead = if role == Role::Neutral && index == 0 {
            Span::styled("▸", Style::default().fg(theme::CPI_PANEL))
        } else {
            paint.marker()
        };
        let text = if entry.speculative {
            theme::SPECULATIVE
        } else {
            theme::TEXT
        };
        let mut cells = vec![
            Line::from(lead),
            paint.line(format!("#{}", entry.tag), theme::LABEL),
        ];
        if wide {
            cells.push(paint.line(format!("0x{:04X}", entry.slot.address), theme::LABEL));
        }
        cells.push(paint.line(entry.slot.disassembly.to_string(), text));
        if wide {
            cells.push(paint.line(
                entry.slot.destination.unwrap_or("—").to_string(),
                theme::METRIC_IPC,
            ));
        }
        cells.push(phase_cell(paint, entry.phase, Some(entry.slot)));
        table = table.row(cells);
    }
    f.render_widget(table.build(), inner);
}

// ── The strip: units · registers · queue ─────────────────────────────────────

fn render_strip(
    f: &mut Frame,
    area: Rect,
    app: &App,
    pipeline: &dyn PipelineInspect,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);
    render_units(f, cols[0], pipeline);
    render_registers(f, cols[1], app, model, highlight);
    render_queue(f, cols[2], app, model, highlight);
}

/// The other half of an issue stall: the station list says "no free slot", this
/// says "no free unit".
fn render_units(f: &mut Frame, area: Rect, pipeline: &dyn PipelineInspect) {
    let inner = render_panel(f, area, panel::panel("UNITS", PanelKind::Plain));
    if inner.height == 0 {
        return;
    }
    let lines: Vec<Line<'static>> = (0..pipeline.unit_count())
        .filter_map(|index| pipeline.unit(index))
        .map(|unit| {
            let bar: String = "█".repeat(unit.active.min(unit.capacity))
                + &"░".repeat(unit.capacity.saturating_sub(unit.active));
            Line::from(vec![
                Span::styled(
                    format!(" {:<6}", unit.name),
                    Style::default().fg(theme::LABEL_Y),
                ),
                Span::styled(bar, Style::default().fg(theme::RUNNING)),
                Span::styled(
                    format!(" {}/{}", unit.active, unit.capacity),
                    style::label(),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Which registers name a producer rather than a value.
fn render_registers(
    f: &mut Frame,
    area: Rect,
    app: &App,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
) {
    let renamed = model.rename_count();
    let inner = render_panel(
        f,
        area,
        panel::panel(format!("REGISTERS OWED · {renamed}"), PanelKind::Plain),
    );
    if inner.height == 0 {
        return;
    }
    if renamed == 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " nothing in flight",
                Style::default().fg(theme::BORDER),
            ))),
            inner,
        );
        return;
    }
    let lines: Vec<Line<'static>> = (0..renamed.min(inner.height as usize))
        .filter_map(|index| Some((index, model.rename(index)?)))
        .map(|(index, rename)| {
            let paint = Paint(highlight.role(Some(rename.producer)));
            record(app, OooPanel::Registers, index, inner, index as u16);
            Line::from(vec![
                paint.marker(),
                Span::styled(format!("{:<6}", rename.register), paint.style(theme::TEXT)),
                Span::styled(
                    format!("←#{}", rename.producer),
                    paint.style(theme::WIRE_PRODUCER),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Fetched and not yet issued — what the front end has run ahead to.
fn render_queue(
    f: &mut Frame,
    area: Rect,
    app: &App,
    model: &dyn PipelineDynamicInspect,
    highlight: &Highlight,
) {
    let queued = model.queue_count();
    let inner = render_panel(
        f,
        area,
        panel::panel(format!("ISSUE QUEUE · {queued}"), PanelKind::Plain),
    );
    if inner.height == 0 {
        return;
    }
    let lines: Vec<Line<'static>> = (0..queued.min(inner.height as usize))
        .filter_map(|index| Some((index, model.queued(index)?)))
        .map(|(index, slot)| {
            // Nothing has issued yet, so nothing can be waiting on it: only the
            // row under the cursor is ever lit here.
            let paint = Paint(highlight.queue_role(index));
            record(app, OooPanel::Queue, index, inner, index as u16);
            Line::from(vec![
                paint.marker(),
                Span::styled(
                    format!("0x{:04X} ", slot.address),
                    paint.style(theme::LABEL),
                ),
                Span::styled(
                    slot.disassembly.to_string(),
                    paint.style(if slot.speculative {
                        theme::SPECULATIVE
                    } else {
                        theme::TEXT
                    }),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// The palette a phase reads in, so a test can assert the six states are told
/// apart without reaching into the renderer.
#[cfg(test)]
fn phase_color(phase: PipelineEntryPhase) -> ratatui::style::Color {
    match phase_cell(Paint(Role::Neutral), phase, None)
        .spans
        .first()
        .and_then(|s| s.style.fg)
    {
        Some(color) => color,
        None => theme::TEXT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    /// Six phases, six looks. A screen that painted "waiting on an operand" and
    /// "finished, waiting its turn" the same way would hide the distinction the
    /// buffer exists to make.
    #[test]
    fn every_phase_reads_differently() {
        let phases = [
            PipelineEntryPhase::Free,
            PipelineEntryPhase::Waiting,
            PipelineEntryPhase::Armed,
            PipelineEntryPhase::Executing,
            PipelineEntryPhase::Finished,
            PipelineEntryPhase::Committing,
        ];
        let mut seen: Vec<Color> = Vec::new();
        for phase in phases {
            let color = phase_color(phase);
            assert!(!seen.contains(&color), "{phase:?} repeats a colour");
            seen.push(color);
        }
    }

    /// The screen a user actually gets: pick Tomasulo on a backend that
    /// declares its datapath, run it, and the structures have to be on screen.
    #[test]
    fn selecting_tomasulo_draws_the_workbench() {
        use ratatui::{Terminal, backend::TestBackend};
        use raven_engine::capability::PipelineSettingValue;

        let mut app = crate::ui::app::App::new_with_architecture(
            Some(64 * 1024),
            crate::falcon::jit::BackendKind::None,
            "toy16",
        )
        .unwrap();
        app.set_pipeline_enabled(true);
        app.tab = crate::ui::app::Tab::Pipeline;

        // Through the settings row, the way a user reaches it.
        for _ in 0..4 {
            let showing = app.pipeline_tuning().unwrap().setting(0).unwrap().value;
            if showing == PipelineSettingValue::Choice("Tomasulo + ROB") {
                break;
            }
            app.pipeline_tuning_mut().unwrap().adjust(0, true);
        }
        assert!(
            app.pipeline_dynamic().is_some(),
            "the model is running, so the tab has structures to draw"
        );
        for _ in 0..12 {
            app.single_step();
        }

        let mut terminal = Terminal::new(TestBackend::new(160, 44)).unwrap();
        terminal
            .draw(|f| {
                super::render_pipeline_ooo(
                    f,
                    Rect::new(0, 0, 160, 40),
                    &app,
                    app.pipeline().unwrap(),
                    app.pipeline_dynamic().unwrap(),
                )
            })
            .unwrap();

        // The renderer records where every row landed, which is what the mouse
        // hit-tests against — a screen with no hitboxes cannot be inspected.
        assert!(
            !app.run.pipeline_view().ooo_row_hits.borrow().is_empty(),
            "no row recorded a hitbox"
        );

        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for heading in [
            "RESERVATION STATIONS",
            "REORDER BUFFER",
            "REGISTERS OWED",
            "ISSUE QUEUE",
        ] {
            assert!(screen.contains(heading), "{heading} is missing");
        }

        // Now point at a buffer row and draw again: the focused row has to be
        // marked on screen, or the hover changed nothing a user can see.
        let hovered = app
            .run
            .pipeline_view()
            .ooo_row_hits
            .borrow()
            .iter()
            .find(|hit| hit.focus.panel == OooPanel::Buffer)
            .map(|hit| hit.focus);
        assert!(hovered.is_some(), "the buffer drew at least one row");
        app.run.pipeline_view_mut().ooo_hover = hovered;

        terminal
            .draw(|f| {
                super::render_pipeline_ooo(
                    f,
                    Rect::new(0, 0, 160, 40),
                    &app,
                    app.pipeline().unwrap(),
                    app.pipeline_dynamic().unwrap(),
                )
            })
            .unwrap();
        let focused: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            focused.contains('▸'),
            "the focused row is not marked on screen"
        );
    }

    /// The wire is the claim that two rows are one instruction. It needs both
    /// ends: one anchor is a row nobody is tied to, and drawing half a wire
    /// would say something false.
    #[test]
    fn a_wire_needs_both_ends() {
        use ratatui::{Terminal, backend::TestBackend};

        let painted = |anchors: &[(u16, bool)]| {
            let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
            terminal
                .draw(|f| {
                    let mut wires = WireLayer::new(Rect::new(8, 1, GUTTER_W, 8));
                    for (y, left) in anchors {
                        if *left {
                            wires.connect_left(*y, Role::Primary);
                        } else {
                            wires.connect_right(*y, Role::Producer);
                        }
                    }
                    wires.paint(f);
                })
                .unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };

        let one_end = painted(&[(3, true)]);
        assert!(
            !one_end.contains('◀') && !one_end.contains('│'),
            "half a wire is a claim with nothing on the other side"
        );

        let both_ends = painted(&[(3, true), (5, false)]);
        assert!(both_ends.contains('◀'), "no stub into the station row");
        assert!(both_ends.contains('▶'), "no stub into the buffer row");
        assert!(both_ends.contains('│'), "the two stubs are not joined");
    }

    /// A dependency has two ends, and the hover has to find both: what the
    /// focused instruction is waiting for, and who is waiting for it.
    #[test]
    fn hovering_a_station_lights_both_ends_of_its_dependency() {
        use raven_engine::capability::{
            PipelineEntryPhase, PipelineOperandView, PipelineStationView,
        };

        /// Three stations: a producer, the one waiting on it, and a bystander.
        struct Bank;
        impl PipelineDynamicInspect for Bank {
            fn station_count(&self) -> usize {
                3
            }
            fn station(&self, index: usize) -> Option<PipelineStationView<'_>> {
                let waits = |producer| {
                    [
                        Some(PipelineOperandView {
                            register: "q",
                            producer,
                        }),
                        None,
                    ]
                };
                Some(match index {
                    0 => PipelineStationView {
                        name: "DIV0",
                        unit: 0,
                        tag: Some(1),
                        slot: None,
                        phase: PipelineEntryPhase::Executing,
                        operands: waits(None),
                    },
                    1 => PipelineStationView {
                        name: "ALU0",
                        unit: 1,
                        tag: Some(2),
                        slot: None,
                        phase: PipelineEntryPhase::Waiting,
                        operands: waits(Some(1)),
                    },
                    _ => PipelineStationView {
                        name: "ALU1",
                        unit: 1,
                        tag: Some(3),
                        slot: None,
                        phase: PipelineEntryPhase::Armed,
                        operands: [None, None],
                    },
                })
            }
            fn rename_count(&self) -> usize {
                0
            }
            fn rename(&self, _: usize) -> Option<raven_engine::capability::PipelineRenameView<'_>> {
                None
            }
            fn queue_count(&self) -> usize {
                0
            }
            fn queued(&self, _: usize) -> Option<PipelineSlotView<'_>> {
                None
            }
        }

        // Point at the waiter: it names #1 as what it needs.
        let waiter = focus::resolve(
            &Bank,
            Some(OooFocus {
                panel: OooPanel::Stations,
                row: 1,
            }),
        );
        assert_eq!(waiter.role(Some(2)), Role::Primary);
        assert_eq!(waiter.role(Some(1)), Role::Producer, "what it waits for");
        assert_eq!(waiter.role(Some(3)), Role::Dim, "and nothing else");

        // Point at the producer: the same pair, read from the other end.
        let producer = focus::resolve(
            &Bank,
            Some(OooFocus {
                panel: OooPanel::Stations,
                row: 0,
            }),
        );
        assert_eq!(producer.role(Some(1)), Role::Primary);
        assert_eq!(producer.role(Some(2)), Role::Consumer, "who waits for it");

        // No hover, no dimming: the screen must not look faded when the mouse
        // is somewhere else entirely.
        let idle = focus::resolve(&Bank, None);
        assert!(!idle.active());
        for tag in 1..=3 {
            assert_eq!(idle.role(Some(tag)), Role::Neutral);
        }
    }

    /// An operand with a producer has to say so: the tag is the difference
    /// between "waiting" and "waiting for #3".
    #[test]
    fn an_owed_operand_names_the_entry_that_owes_it() {
        let owed = operand_cell(
            Paint(Role::Neutral),
            Some(PipelineOperandView {
                register: "q",
                producer: Some(3),
            }),
        );
        let text: String = owed
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "q ←#3");

        let held = operand_cell(
            Paint(Role::Neutral),
            Some(PipelineOperandView {
                register: "q",
                producer: None,
            }),
        );
        let text: String = held
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "q", "a value in hand is just the register");
    }
}
