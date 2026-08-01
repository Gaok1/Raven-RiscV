use crate::ui::app::App;
use crate::ui::pipeline::{FuKind, InstrClass, gantt_visible_rows};
use raven_riscv_engine::capability::{
    PipelineHazardKind, PipelineInspect, PipelineInstructionClass, PipelineSlotView,
    PipelineStageRole, PipelineTimelineCell, PipelineTimelineState, PipelineTraceKind,
    PipelineTraceView,
};
use crate::ui::theme;
use crate::ui::view::style;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

fn class_label(class: PipelineInstructionClass) -> &'static str {
    match class {
        PipelineInstructionClass::Alu => "ALU",
        PipelineInstructionClass::Multiply => "MUL",
        PipelineInstructionClass::Divide => "DIV",
        PipelineInstructionClass::Load => "Load",
        PipelineInstructionClass::Store => "Store",
        PipelineInstructionClass::Branch => "Branch",
        PipelineInstructionClass::Jump => "Jump",
        PipelineInstructionClass::System => "System",
        PipelineInstructionClass::FloatingPoint => "FP",
        PipelineInstructionClass::Unknown => "?",
    }
}

fn class_color(class: PipelineInstructionClass) -> Color {
    match class {
        PipelineInstructionClass::Alu => Color::Cyan,
        PipelineInstructionClass::Multiply => Color::Magenta,
        PipelineInstructionClass::Divide => Color::Red,
        PipelineInstructionClass::Load => Color::Green,
        PipelineInstructionClass::Store => Color::Yellow,
        PipelineInstructionClass::Branch => Color::LightYellow,
        PipelineInstructionClass::Jump => Color::LightMagenta,
        PipelineInstructionClass::System => Color::Gray,
        PipelineInstructionClass::FloatingPoint => Color::LightCyan,
        PipelineInstructionClass::Unknown => Color::DarkGray,
    }
}

fn hazard_color(hazard: PipelineHazardKind) -> Color {
    match hazard {
        PipelineHazardKind::ReadAfterWrite | PipelineHazardKind::LoadUse => {
            Color::Rgb(225, 180, 80)
        }
        PipelineHazardKind::BranchFlush => Color::Rgb(210, 72, 68),
        PipelineHazardKind::FunctionalUnitBusy => Color::Rgb(195, 105, 250),
        PipelineHazardKind::MemoryLatency => Color::Rgb(110, 175, 220),
        PipelineHazardKind::WriteAfterWrite => Color::Rgb(115, 178, 235),
        PipelineHazardKind::WriteAfterRead => Color::Rgb(88, 200, 148),
    }
}

fn trace_color(kind: PipelineTraceKind) -> Color {
    match kind {
        PipelineTraceKind::Hazard(hazard) => hazard_color(hazard),
        PipelineTraceKind::Forward => Color::Rgb(110, 175, 220),
    }
}

fn trace_short_label(kind: PipelineTraceKind) -> &'static str {
    match kind {
        PipelineTraceKind::Hazard(PipelineHazardKind::ReadAfterWrite) => "RAW",
        PipelineTraceKind::Hazard(PipelineHazardKind::LoadUse) => "LOAD",
        PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush) => "CTRL",
        PipelineTraceKind::Hazard(PipelineHazardKind::FunctionalUnitBusy) => "FU",
        PipelineTraceKind::Hazard(PipelineHazardKind::MemoryLatency) => "MEM",
        PipelineTraceKind::Hazard(PipelineHazardKind::WriteAfterWrite) => "WAW",
        PipelineTraceKind::Hazard(PipelineHazardKind::WriteAfterRead) => "WAR",
        PipelineTraceKind::Forward => "FWD",
    }
}

/// Vertical budget for the Main subtab: compact strips on top, HISTORY gets
/// the remainder. Shared with the tutorial so highlight rects stay in sync.
pub(crate) struct MainLayoutPlan {
    pub(crate) stages_h: u16,
    pub(crate) fu_h: u16,
    pub(crate) hazards_h: u16,
    pub(crate) collapsed: bool,
}

/// Height ladder (h = content height below the header):
///   h ≥ 18          — stage strip 5, FU strip 1, hazards up to 4
///   12 ≤ h < 18     — FU detail folds into the EX column title, hazards ≤ 3
///   9 ≤ h < 12      — stage columns drop to 2 inner lines, hazards ≤ 2
///   h < 9           — stages+hazards collapse to one status line
/// Widths under 72 also drop the third stage line (class+regs).
pub(crate) fn plan_main_layout(h: u16, w: u16, n_traces: usize) -> MainLayoutPlan {
    if h < 9 {
        return MainLayoutPlan {
            stages_h: 1,
            fu_h: 0,
            hazards_h: 0,
            collapsed: true,
        };
    }
    let rows_cap: u16 = if h >= 18 {
        3
    } else if h >= 12 {
        2
    } else {
        1
    };
    let rows = (n_traces.max(1) as u16).min(rows_cap);
    MainLayoutPlan {
        stages_h: if h >= 12 && w >= 72 { 5 } else { 4 },
        fu_h: if h >= 18 { 1 } else { 0 },
        hazards_h: 1 + rows,
        collapsed: false,
    }
}

pub fn render_pipeline_main(
    f: &mut Frame,
    area: Rect,
    app: &App,
    pipeline: &dyn PipelineInspect,
) {
    let plan = plan_main_layout(area.height, area.width, pipeline.trace_count());

    if plan.collapsed {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        render_collapsed_strip(f, chunks[0], pipeline);
        render_gantt(f, chunks[1], app, pipeline);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(plan.stages_h),
            Constraint::Length(plan.fu_h),
            Constraint::Length(plan.hazards_h),
            Constraint::Min(1),
        ])
        .split(area);

    render_stages(f, chunks[0], pipeline, plan.fu_h == 0);
    if plan.fu_h > 0 {
        render_fu_strip(f, chunks[1], app, pipeline);
    }
    render_hazards(f, chunks[2], pipeline);
    render_gantt(f, chunks[3], app, pipeline);
}

/// Sub-9-line fallback: one line summarizing the stages and hazard count so
/// the HISTORY gantt can keep the rest of a very short terminal.
fn render_collapsed_strip(f: &mut Frame, area: Rect, pipeline: &dyn PipelineInspect) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for i in 0..pipeline.stage_count() {
        let stage = pipeline.stage(i).unwrap();
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme::BORDER)));
        }
        spans.push(Span::styled(
            format!("{} ", stage.name),
            Style::default().fg(theme::LABEL_Y).add_modifier(Modifier::BOLD),
        ));
        let text = match stage.slot {
            None => "—".to_string(),
            Some(slot) if slot.bubble => "◦".to_string(),
            Some(slot) => slot
                .disassembly
                .split_whitespace()
                .next()
                .unwrap_or("?")
                .to_string(),
        };
        spans.push(Span::styled(text, Style::default().fg(theme::TEXT)));
    }
    if pipeline.trace_count() > 0 {
        spans.push(Span::styled(
            format!("  · {} hazards", pipeline.trace_count()),
            Style::default().fg(theme::PAUSED),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Stage boxes ───────────────────────────────────────────────────────────────

fn render_stages(
    f: &mut Frame,
    area: Rect,
    pipeline: &dyn PipelineInspect,
    fu_in_ex_title: bool,
) {
    // Laid out in the graph's flow order, not index order: a backend whose
    // stages are not numbered along the datapath still draws left to right in
    // the order instructions actually move through it.
    let order = pipeline.stage_order();
    let stage_count = order.len().max(1);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, stage_count as u32); stage_count])
        .split(area);

    for (column, &i) in order.iter().enumerate() {
        let Some(stage) = pipeline.stage(i) else {
            continue;
        };
        let column_area = cols[column];
        let slot = stage.slot;
        let mut stage_badges = stage_status_badges(pipeline, i, slot.as_ref());

        let border_style = match slot.as_ref() {
            Some(slot) if slot.class == PipelineInstructionClass::Unknown => {
                Style::default().fg(Color::DarkGray)
            }
            Some(slot) if slot.hazard.is_some() => {
                Style::default().fg(hazard_color(slot.hazard.unwrap()))
            }
            Some(slot) if slot.bubble => style::warning(),
            Some(slot) if slot.speculative => style::warning(),
            Some(_) => Style::default().fg(theme::ACCENT),
            None => Style::default().fg(theme::BORDER),
        };

        let mut title_label = match slot.as_ref() {
            Some(slot) if slot.speculative && !slot.bubble => format!("{} ⟪P⟫", stage.name),
            Some(slot) if slot.hazard == Some(PipelineHazardKind::BranchFlush) => {
                format!("{} ⟪X⟫", stage.name)
            }
            _ => stage.name.to_string(),
        };
        // With the FU strip folded away, surface overall FU occupancy on the
        // stage that owns the functional units — the one that executes,
        // whatever this ISA calls it.
        if stage.role == PipelineStageRole::Execute && fu_in_ex_title {
            let units: Vec<_> = (0..pipeline.unit_count())
                .filter_map(|unit| pipeline.unit(unit))
                .collect();
            let busy: usize = units.iter().map(|unit| unit.active).sum();
            let cap: usize = units.iter().map(|unit| unit.capacity).sum();
            title_label = format!("{title_label} {busy}/{cap}");
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title_label,
                border_style.add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(column_area);
        f.render_widget(block, column_area);
        if inner.height == 0 || inner.width == 0 {
            continue;
        }

        let lines = match slot.as_ref() {
            None => vec![Line::from(Span::styled(
                "—",
                Style::default().fg(theme::BORDER),
            ))],
            Some(slot) if slot.bubble => {
                let label = bubble_label_for_stage(stage.role, slot);
                vec![Line::from(Span::styled(
                    label,
                    border_style.add_modifier(Modifier::BOLD),
                ))]
            }
            Some(slot) if slot.class == PipelineInstructionClass::Unknown => {
                // Undecodable instruction — visual indicator that it's ignored
                let dim = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM);
                vec![
                    Line::from(Span::styled(format!("0x{:04X}", slot.address), dim)),
                    Line::from(Span::styled(
                        "⊘ invalid",
                        style::danger().add_modifier(Modifier::DIM),
                    )),
                    Line::from(Span::styled(slot.disassembly.to_string(), dim)),
                ]
            }
            Some(slot) => stage_slot_lines(stage.role, slot, inner, &mut stage_badges),
        };

        let visible_lines: Vec<_> = lines.into_iter().take(inner.height as usize).collect();
        f.render_widget(Paragraph::new(visible_lines), inner);
    }
}

/// Compact stage-column body. Three inner lines when there is room
/// (PC + badges / disasm / class + regs), two when squeezed
/// (disasm + badges / PC + regs).
fn stage_slot_lines(
    role: PipelineStageRole,
    slot: &PipelineSlotView<'_>,
    inner: Rect,
    stage_badges: &mut Vec<(String, Style)>,
) -> Vec<Line<'static>> {
    let w = inner.width as usize;
    let pc_str = format!("0x{:04X}", slot.address);
    let hazard_indicator = slot.hazard.map(|hazard| {
        Span::styled(
            format!(" ⚠{}", compact_stage_hazard_label(role, Some(slot), hazard)),
            Style::default().fg(hazard_color(hazard)),
        )
    });
    let pred_badge = speculative_compact_badge(slot, w);
    let pred_w = pred_badge
        .as_ref()
        .map_or(0, |(label, _)| UnicodeWidthStr::width(label.as_str()));
    trim_stage_badges_to_fit(stage_badges, w, pred_w);

    let mut badge_spans: Vec<Span<'static>> = Vec::new();
    if let Some(h) = hazard_indicator.filter(|_| stage_badges.is_empty()) {
        badge_spans.push(h);
    }
    badge_spans.extend(
        stage_badges
            .iter()
            .map(|(label, style)| Span::styled(label.clone(), *style)),
    );
    if let Some((label, style)) = pred_badge {
        badge_spans.push(Span::styled(label, style));
    }

    let reg_str = compact_reg_summary(slot);

    if inner.height >= 3 {
        // PC + badges / disasm / class + regs
        let mut pc_spans = vec![Span::styled(pc_str, Style::default().fg(theme::LABEL))];
        pc_spans.extend(badge_spans);

        let (disasm_trunc, _) = slot.disassembly.unicode_truncate(w.max(4));
        let mut class_spans = vec![Span::styled(
            format!("[{}]", class_label(slot.class)),
            Style::default()
                .fg(class_color(slot.class))
                .add_modifier(Modifier::DIM),
        )];
        if !reg_str.is_empty() {
            class_spans.push(Span::styled(
                format!(" {reg_str}"),
                Style::default().fg(theme::BORDER),
            ));
        }
        vec![
            Line::from(pc_spans),
            Line::from(Span::styled(
                disasm_trunc.to_string(),
                Style::default().fg(theme::TEXT),
            )),
            Line::from(class_spans),
        ]
    } else {
        // disasm + badges / PC + regs
        let badge_w: usize = stage_badges
            .iter()
            .map(|(label, _)| UnicodeWidthStr::width(label.as_str()))
            .sum();
        let disasm_w = w
            .saturating_sub(badge_w)
            .saturating_sub(pred_w)
            .saturating_sub(1)
            .max(4);
        let (disasm_trunc, _) = slot.disassembly.unicode_truncate(disasm_w);
        let mut disasm_spans = vec![Span::styled(
            disasm_trunc.to_string(),
            Style::default().fg(theme::TEXT),
        )];
        disasm_spans.extend(badge_spans);

        let mut pc_spans = vec![Span::styled(pc_str, Style::default().fg(theme::LABEL))];
        if !reg_str.is_empty() {
            pc_spans.push(Span::styled(
                format!(" {reg_str}"),
                Style::default().fg(theme::BORDER),
            ));
        }
        vec![Line::from(disasm_spans), Line::from(pc_spans)]
    }
}

/// "a0←a1,a2"-style register summary for a stage column.
fn compact_reg_summary(slot: &PipelineSlotView<'_>) -> String {
    let srcs: Vec<&str> = slot.sources.iter().flatten().copied().collect();
    let srcs = srcs.join(",");
    match (slot.destination, srcs.is_empty()) {
        (Some(destination), false) => format!("{destination}←{srcs}"),
        (Some(destination), true) => destination.to_string(),
        (None, false) => srcs,
        (None, true) => String::new(),
    }
}

fn stage_status_badges(
    pipeline: &dyn PipelineInspect,
    stage_idx: usize,
    slot: Option<&PipelineSlotView<'_>>,
) -> Vec<(String, Style)> {
    let mut tags: Vec<(String, Style)> = Vec::new();
    let role = pipeline
        .stage(stage_idx)
        .map(|stage| stage.role)
        .unwrap_or_default();

    if let Some(slot) = slot {
        if slot.atomic {
            push_badge(
                &mut tags,
                "AT".to_string(),
                Style::default().fg(Color::LightYellow),
            );
        }
        if let Some(hazard) = slot.hazard {
            push_badge(
                &mut tags,
                compact_stage_hazard_label(role, Some(slot), hazard).to_string(),
                Style::default().fg(hazard_color(hazard)),
            );
        }
    }

    for index in 0..pipeline.trace_count() {
        let trace = pipeline.trace(index).unwrap();
        if trace.from_stage != stage_idx && trace.to_stage != stage_idx {
            continue;
        }
        match trace.kind {
            PipelineTraceKind::Forward => {
                push_badge(
                    &mut tags,
                    "RAW".to_string(),
                    Style::default().fg(hazard_color(PipelineHazardKind::ReadAfterWrite)),
                );
                push_badge(
                    &mut tags,
                    "FWD".to_string(),
                    Style::default().fg(trace_color(PipelineTraceKind::Forward)),
                );
            }
            PipelineTraceKind::Hazard(hazard) => {
                push_badge(
                    &mut tags,
                    compact_stage_hazard_label(role, slot, hazard).to_string(),
                    Style::default().fg(hazard_color(hazard)),
                );
            }
        }
    }

    tags.truncate(3);
    tags.into_iter()
        .map(|(label, style)| (format!(" [{label}]"), style.add_modifier(Modifier::BOLD)))
        .collect()
}

fn push_badge(tags: &mut Vec<(String, Style)>, label: String, style: Style) {
    if tags.iter().any(|(existing, _)| existing == &label) {
        return;
    }
    tags.push((label, style));
}

/// Why a stage is holding a bubble, phrased from the stage's role rather than
/// its index — "waiting for fetch" is true of any pipeline, "waiting for IF"
/// only of one that names a stage IF.
fn bubble_label_for_stage(role: PipelineStageRole, slot: &PipelineSlotView<'_>) -> &'static str {
    use PipelineStageRole::*;
    match slot.hazard {
        Some(PipelineHazardKind::BranchFlush) => "✕ squashed",
        Some(PipelineHazardKind::MemoryLatency) => match role {
            Decode => "waiting for fetch",
            Issue | Execute | Memory | Writeback | Commit => "front-end bubble",
            Fetch | Other => "wait",
        },
        _ => "NOP bubble",
    }
}

fn compact_stage_hazard_label(
    role: PipelineStageRole,
    slot: Option<&PipelineSlotView<'_>>,
    hazard: PipelineHazardKind,
) -> &'static str {
    match hazard {
        PipelineHazardKind::ReadAfterWrite => "RAW",
        PipelineHazardKind::LoadUse => "LOAD",
        PipelineHazardKind::BranchFlush => "CTRL",
        PipelineHazardKind::FunctionalUnitBusy => "FU",
        PipelineHazardKind::MemoryLatency => {
            if slot.is_some_and(|slot| slot.bubble) {
                if role == PipelineStageRole::Decode {
                    "UP"
                } else {
                    "BUBL"
                }
            } else {
                match role {
                    PipelineStageRole::Fetch => "IFWT",
                    PipelineStageRole::Memory => "MEMWT",
                    _ => "WAIT",
                }
            }
        }
        PipelineHazardKind::WriteAfterWrite => "WAW",
        PipelineHazardKind::WriteAfterRead => "WAR",
    }
}

fn trim_stage_badges_to_fit(stage_badges: &mut Vec<(String, Style)>, width: usize, pred_w: usize) {
    let reserved = pred_w.saturating_add(9);
    while !stage_badges.is_empty()
        && stage_badges
            .iter()
            .map(|(label, _)| UnicodeWidthStr::width(label.as_str()))
            .sum::<usize>()
            + reserved
            > width
    {
        stage_badges.pop();
    }
}

// ── EX box expandido (Functional Units mode) ──────────────────────────────────

/// One-line functional-unit strip: every FU stays visible as `LABEL busy/cap`
/// (with a mini progress bar on wide terminals), and the first active FU's
/// instruction is echoed at the end of the line.
/// Below this width the FU strip drops the mini bars and trailing disasm note,
/// keeping only `LABEL busy/cap` per unit.
pub(crate) fn fu_strip_is_compact(w: u16) -> bool {
    w < 90
}

fn render_fu_strip(
    f: &mut Frame,
    area: Rect,
    app: &App,
    pipeline: &dyn PipelineInspect,
) {
    let cpi = &app.run.cpi_config;
    let wide = !fu_strip_is_compact(area.width);

    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        " FU ",
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    )];
    let mut active_note: Option<String> = None;

    for index in 0..pipeline.unit_count() {
        if index > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme::BORDER)));
        }
        let unit = pipeline.unit(index).unwrap();

        if let Some(slot) = unit.first {
            let total = pipeline_latency(unit.latency_class, cpi);
            let done = total.saturating_sub(slot.cycles_remaining);
            spans.push(Span::styled(
                unit.name.to_string(),
                Style::default()
                    .fg(class_color(slot.class))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {}/{}", unit.active, unit.capacity),
                Style::default().fg(theme::LABEL_Y),
            ));
            if wide {
                let filled = if total > 1 {
                    ((done as usize) * 3 / (total as usize)).min(3)
                } else {
                    3
                };
                let bar: String = (0..3).map(|j| if j < filled { '▰' } else { '▱' }).collect();
                spans.push(Span::styled(
                    format!(" {bar}"),
                    Style::default().fg(theme::RUNNING),
                ));
            }
            if active_note.is_none() {
                let summary = if unit.active > 1 {
                    format!("{} (+{} more)", slot.disassembly, unit.active - 1)
                } else {
                    slot.disassembly.to_string()
                };
                active_note = Some(format!("{summary} ({}/{total})", done + 1));
            }
        } else {
            spans.push(Span::styled(
                unit.name.to_string(),
                Style::default().fg(theme::BORDER),
            ));
            spans.push(Span::styled(
                format!(" 0/{}", unit.capacity),
                Style::default()
                    .fg(theme::BORDER)
                    .add_modifier(Modifier::DIM),
            ));
        }
    }

    if let Some(note) = active_note.filter(|_| wide) {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(note, Style::default().fg(theme::TEXT)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn pipeline_latency(class: PipelineInstructionClass, cpi: &crate::ui::app::CpiConfig) -> u8 {
    let extra = match class {
        PipelineInstructionClass::Alu => cpi.alu,
        PipelineInstructionClass::Multiply => cpi.mul,
        PipelineInstructionClass::Divide => cpi.div,
        PipelineInstructionClass::Load => cpi.load,
        PipelineInstructionClass::Store => cpi.store,
        PipelineInstructionClass::System => cpi.system,
        PipelineInstructionClass::FloatingPoint => cpi.fp,
        PipelineInstructionClass::Branch
        | PipelineInstructionClass::Jump
        | PipelineInstructionClass::Unknown => 0,
    };
    u8::try_from(1u64.saturating_add(extra)).unwrap_or(u8::MAX)
}

fn slot_belongs_to_fu_kind(slot: &crate::ui::pipeline::PipeSlot, fu_kind: FuKind) -> bool {
    match fu_kind {
        FuKind::Alu => matches!(
            slot.class,
            InstrClass::Alu | InstrClass::Branch | InstrClass::Jump
        ),
        FuKind::Mul => matches!(slot.class, InstrClass::Mul),
        FuKind::Div => matches!(slot.class, InstrClass::Div),
        FuKind::Fpu => matches!(slot.class, InstrClass::Fp),
        FuKind::Lsu => matches!(slot.class, InstrClass::Load | InstrClass::Store),
        FuKind::Sys => matches!(slot.class, InstrClass::System),
    }
}

fn split_fu_activity<'a>(
    fu_kind: FuKind,
    fu_states: &'a [crate::ui::pipeline::FuState],
    ex_slot: Option<&'a crate::ui::pipeline::PipeSlot>,
) -> (
    Vec<&'a crate::ui::pipeline::PipeSlot>,
    Option<&'a crate::ui::pipeline::PipeSlot>,
) {
    let mut parallel_slots = Vec::new();
    let mut mirrored_ex_slot = None;

    for fu in fu_states {
        let Some(slot) = fu.slot.as_ref().filter(|s| !s.is_bubble) else {
            continue;
        };
        let is_ex_mirror = ex_slot.is_some_and(|ex| {
            ex.seq != 0
                && ex.seq == slot.seq
                && ex.pc == slot.pc
                && slot_belongs_to_fu_kind(ex, fu_kind)
        });
        if is_ex_mirror {
            mirrored_ex_slot = Some(slot);
        } else {
            parallel_slots.push(slot);
        }
    }

    (parallel_slots, mirrored_ex_slot)
}

// ── Hazard messages ────────────────────────────────────────────────────────────

fn render_hazards(f: &mut Frame, area: Rect, pipeline: &dyn PipelineInspect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Hazards / Forwarding ",
            Style::default().fg(theme::LABEL_Y),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let rows_avail = inner.height as usize;
    let w = inner.width as usize;
    let dim = Style::default()
        .fg(theme::LABEL)
        .add_modifier(Modifier::DIM);
    let mut lines: Vec<Line<'static>> = Vec::new();

    if pipeline.trace_count() == 0 {
        let status = pipeline.status();
        let status = if status.halted {
            " ✓ Halted"
        } else if status.faulted {
            " ✗ Fault"
        } else {
            " No active links"
        };
        let mut text = status.to_string();
        if let Some(message) = pipeline.status_message() {
            text.push_str("  ·  ");
            text.push_str(message);
        }
        let (trunc, _) = text.unicode_truncate(w);
        lines.push(Line::from(Span::styled(trunc.to_string(), dim)));
    } else {
        let n = pipeline.trace_count();
        let shown = if n > rows_avail {
            rows_avail.saturating_sub(1)
        } else {
            n
        };
        for index in 0..shown {
            let trace = pipeline.trace(index).unwrap();
            lines.push(render_hazard_row(pipeline, &trace, w));
        }
        if n > shown {
            let fwd = (shown..n)
                .filter_map(|index| pipeline.trace(index))
                .filter(|trace| trace.kind == PipelineTraceKind::Forward)
                .count();
            let hidden = n - shown;
            let hzd = hidden - fwd;
            lines.push(Line::from(Span::styled(
                format!(" +{hidden} more ({fwd} FWD · {hzd} HZD)"),
                dim,
            )));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// One hazard/forwarding link per line: `[FWD] MEM -> EX  detail…`.
fn render_hazard_row(
    pipeline: &dyn PipelineInspect,
    trace: &PipelineTraceView<'_>,
    width: usize,
) -> Line<'static> {
    let badge = format!("[{}]", trace_short_label(trace.kind));
    let route = format!(" {:<10}", trace_stage_summary(pipeline, trace));
    let used = 1 + UnicodeWidthStr::width(badge.as_str()) + UnicodeWidthStr::width(route.as_str());
    let (detail_trunc, _) = trace
        .detail
        .unicode_truncate(width.saturating_sub(used).max(8));
    Line::from(vec![
        Span::raw(" "),
        Span::styled(badge, trace_badge_style(trace.kind)),
        Span::styled(
            route,
            Style::default()
                .fg(trace_color(trace.kind))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(detail_trunc.to_string(), Style::default().fg(theme::TEXT)),
    ])
}

fn trace_stage_summary(
    pipeline: &dyn PipelineInspect,
    trace: &PipelineTraceView<'_>,
) -> String {
    format!(
        "{} -> {}",
        stage_name_from_idx(pipeline, trace.from_stage),
        stage_name_from_idx(pipeline, trace.to_stage)
    )
}

fn stage_name_from_idx(pipeline: &dyn PipelineInspect, index: usize) -> &str {
    pipeline.stage(index).map_or("?", |stage| stage.name)
}

fn speculative_compact_badge(
    slot: &PipelineSlotView<'_>,
    width: usize,
) -> Option<(String, Style)> {
    if !slot.speculative {
        return None;
    }
    let badge = if slot.predicted_taken {
        " ⟪P:T⟫"
    } else {
        " ⟪P:N⟫"
    };
    if width < badge.len() + 10 {
        return None;
    }
    Some((
        badge.to_string(),
        style::warning().add_modifier(Modifier::BOLD),
    ))
}

fn trace_badge_style(kind: PipelineTraceKind) -> Style {
    Style::default()
        .fg(trace_color(kind))
        .bg(theme::BG_SEP)
        .add_modifier(Modifier::BOLD)
}

// ── Gantt diagram ─────────────────────────────────────────────────────────────

fn render_gantt(
    f: &mut Frame,
    area: Rect,
    app: &App,
    pipeline: &dyn PipelineInspect,
) {
    let view = app.run.pipeline_view();

    const LABEL_W: usize = 12;
    const CELL_W: usize = 4;
    let preview_inner_w = area.width.saturating_sub(2) as usize;
    let preview_cols = ((preview_inner_w.saturating_sub(LABEL_W)) / CELL_W).max(1);
    let visible_cols = preview_cols.min(crate::ui::pipeline::MAX_GANTT_COLS);

    let scroll_hint = if view.gantt_scroll == 0 {
        format!(" HISTORY  up to {} cycles · following ", visible_cols)
    } else {
        format!(" HISTORY  scrollback ↑{} · End=follow ", view.gantt_scroll)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(scroll_hint, Style::default().fg(theme::LABEL)));

    let inner = block.inner(area);
    view.gantt_area_rect
        .set((inner.x, inner.y, inner.width, inner.height));
    f.render_widget(block, area);

    if pipeline.timeline_len() == 0 {
        f.render_widget(
            Paragraph::new("  — no history yet —")
                .style(style::label().add_modifier(Modifier::DIM)),
            inner,
        );
        return;
    }

    let max_cols = ((inner.width as usize).saturating_sub(LABEL_W)) / CELL_W;
    let history_cols = max_cols.clamp(1, crate::ui::pipeline::MAX_GANTT_COLS);
    let max_rows = inner.height as usize;
    let visible_capacity = max_rows.saturating_sub(2).max(1);

    let visible_rows = gantt_visible_rows(area.height);
    view.gantt_visible_rows_cache.set(visible_rows);
    let max_scroll = pipeline.timeline_len().saturating_sub(visible_rows.max(1));
    view.gantt_max_scroll_cache.set(max_scroll);
    let scroll = view.gantt_scroll.min(max_scroll);
    let end_row = pipeline.timeline_len().saturating_sub(scroll);
    let start_row = end_row.saturating_sub(visible_capacity.max(1));
    let visible_rows = start_row..end_row;
    let (start_cycle, end_cycle) = timeline_window_bounds(pipeline, visible_rows.clone(), history_cols);

    let mut header_spans = vec![Span::styled(
        format!("{:<width$}", "instr", width = LABEL_W),
        Style::default().fg(theme::LABEL_Y),
    )];
    for c in start_cycle..end_cycle {
        header_spans.push(Span::styled(
            format!("{:>4}", c % 10000),
            style::label().add_modifier(Modifier::DIM),
        ));
    }

    let separator = format!("{}{}", "-".repeat(LABEL_W), "----".repeat(history_cols));

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(header_spans),
        Line::from(Span::styled(separator, Style::default().fg(theme::BORDER))),
    ];

    for row_index in visible_rows {
        let row = pipeline.timeline_row(row_index).unwrap();
        let is_invalid = row.class == PipelineInstructionClass::Unknown;
        let row_label = if row.atomic {
            format!("{} [AT]", row.disassembly)
        } else {
            row.disassembly.to_string()
        };
        let (label, _) = row_label.unicode_truncate(LABEL_W - 1);
        let label_style = if is_invalid {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        } else {
            style::value()
        };
        let mut spans = vec![Span::styled(
            format!("{:<width$}", label, width = LABEL_W),
            label_style,
        )];

        for c in start_cycle..end_cycle {
            let cell = if c < row.first_cycle {
                PipelineTimelineCell {
                    label: "·",
                    state: PipelineTimelineState::Empty,
                    role: PipelineStageRole::Other,
                }
            } else {
                let cell_idx = (c - row.first_cycle) as usize;
                pipeline.timeline_cell(row_index, cell_idx).unwrap_or(PipelineTimelineCell {
                    label: "·",
                    state: PipelineTimelineState::Empty,
                    role: PipelineStageRole::Other,
                })
            };
            let (text, style) = if is_invalid {
                let (t, _) = cell_to_span(cell);
                (
                    t,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                )
            } else {
                cell_to_span(cell)
            };
            spans.push(Span::styled(format!("{:>4}", text), style));
        }

        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn timeline_window_bounds(
    pipeline: &dyn PipelineInspect,
    rows: std::ops::Range<usize>,
    history_cols: usize,
) -> (u64, u64) {
    let mut min_start = None;
    let mut max_end = None;
    for index in rows {
        let row = pipeline.timeline_row(index).unwrap();
        if row.cells == 0 {
            continue;
        }
        min_start = Some(min_start.map_or(row.first_cycle, |start: u64| start.min(row.first_cycle)));
        let row_end = row.first_cycle.saturating_add(row.cells as u64);
        max_end = Some(max_end.map_or(row_end, |end: u64| end.max(row_end)));
    }
    let min_start = min_start.unwrap_or(0);
    let end = max_end.unwrap_or(min_start).max(min_start + 1);
    let start = end
        .saturating_sub(history_cols.max(1) as u64)
        .max(min_start);
    (start, end)
}

fn cell_to_span(cell: PipelineTimelineCell<'_>) -> (&str, Style) {
    let style = match cell.state {
        PipelineTimelineState::Empty => Style::default().fg(theme::BORDER),
        // Coloured by the stage's role, so the timeline matches the stage
        // boxes on any ISA rather than only where stages are named IF/ID/…
        PipelineTimelineState::Active => match cell.role {
            PipelineStageRole::Fetch | PipelineStageRole::Writeback => {
                Style::default().fg(theme::ACCENT)
            }
            PipelineStageRole::Decode | PipelineStageRole::Memory => {
                Style::default().fg(theme::LABEL_Y)
            }
            _ => style::success(),
        },
        PipelineTimelineState::Speculative => Style::default().fg(theme::SPECULATIVE),
        PipelineTimelineState::Stalled => style::warning(),
        PipelineTimelineState::Bubble => style::warning().add_modifier(Modifier::DIM),
        PipelineTimelineState::Flushed => style::danger(),
    };
    (cell.label, style)
}

#[cfg(test)]
mod tests {
    use super::{
        bubble_label_for_stage, cell_to_span, compact_stage_hazard_label, plan_main_layout,
        split_fu_activity, trace_stage_summary,
    };
    use crate::ui::pipeline::{
        FuKind, FuState, GanttCell, HazardTrace, HazardType, PipeSlot, Stage, TraceKind,
    };
    use raven_riscv_engine::capability::{
        PipelineHazardKind, PipelineInstructionClass, PipelineSlotView, PipelineStageRole,
        PipelineTimelineCell, PipelineTimelineState, PipelineTraceKind, PipelineTraceView,
    };

    fn slot(hazard: Option<PipelineHazardKind>, bubble: bool) -> PipelineSlotView<'static> {
        PipelineSlotView {
            address: 0,
            disassembly: "addi x0, x0, 0",
            class: PipelineInstructionClass::Alu,
            destination: None,
            sources: [None, None],
            bubble,
            speculative: false,
            predicted_taken: false,
            hazard,
            atomic: false,
            cycles_remaining: 0,
        }
    }

    /// Stage wording follows the stage's *role*, so it stays true on a backend
    /// that does not call its stages IF/ID/EX/MEM/WB.
    #[test]
    fn mem_latency_bubble_in_decode_reads_as_waiting_for_fetch() {
        let slot = slot(Some(PipelineHazardKind::MemoryLatency), true);

        assert_eq!(
            bubble_label_for_stage(PipelineStageRole::Decode, &slot),
            "waiting for fetch"
        );
        assert_eq!(
            compact_stage_hazard_label(
                PipelineStageRole::Decode,
                Some(&slot),
                PipelineHazardKind::MemoryLatency
            ),
            "UP"
        );
    }

    #[test]
    fn mem_latency_badges_follow_the_stage_role() {
        let waiting = slot(Some(PipelineHazardKind::MemoryLatency), false);

        for (role, expected) in [
            (PipelineStageRole::Fetch, "IFWT"),
            (PipelineStageRole::Memory, "MEMWT"),
            // A role with no specific wording still gets a sensible badge
            // rather than falling through to a fetch/memory one.
            (PipelineStageRole::Commit, "WAIT"),
        ] {
            assert_eq!(
                compact_stage_hazard_label(
                    role,
                    Some(&waiting),
                    PipelineHazardKind::MemoryLatency
                ),
                expected,
                "{role:?}"
            );
        }
    }

    #[test]
    fn main_layout_gives_history_the_remainder_at_full_height() {
        let plan = plan_main_layout(30, 120, 2);
        assert!(!plan.collapsed);
        assert_eq!(plan.stages_h, 5);
        assert_eq!(plan.fu_h, 1);
        assert_eq!(plan.hazards_h, 1 + 2);
        // HISTORY inherits 30 - 9 = 21 lines (~70%).
        assert!(30 - plan.stages_h - plan.fu_h - plan.hazards_h >= 18);
    }

    #[test]
    fn main_layout_folds_fu_strip_then_stage_lines_as_height_shrinks() {
        let mid = plan_main_layout(14, 120, 5);
        assert_eq!(mid.fu_h, 0);
        assert_eq!(mid.stages_h, 5);
        assert_eq!(mid.hazards_h, 1 + 2);

        let short = plan_main_layout(10, 120, 5);
        assert_eq!(short.stages_h, 4);
        assert_eq!(short.hazards_h, 1 + 1);

        let tiny = plan_main_layout(8, 120, 5);
        assert!(tiny.collapsed);
    }

    #[test]
    fn main_layout_drops_third_stage_line_on_narrow_widths() {
        assert_eq!(plan_main_layout(30, 71, 0).stages_h, 4);
        assert_eq!(plan_main_layout(30, 72, 0).stages_h, 5);
    }

    #[test]
    fn fu_strip_goes_compact_below_ninety_columns() {
        assert!(super::fu_strip_is_compact(89));
        assert!(!super::fu_strip_is_compact(90));
    }

    #[test]
    fn pipeline_tab_renders_at_every_compression_breakpoint_without_panicking() {
        use ratatui::{Terminal, backend::TestBackend};
        use std::collections::VecDeque;

        let mut app = crate::ui::app::App::new(None);
        app.editor.last_ok_text = Some(vec![0x0000_0013]);
        {
            let p = app.run.pipeline_mut();
            p.stages[0] = Some(PipeSlot::from_word(0, 0x0000_0013));
            p.stages[2] = Some(PipeSlot::from_word(8, 0x0000_0013));
            p.hazard_traces = (0..5)
                .map(|i| HazardTrace {
                    kind: if i % 2 == 0 {
                        TraceKind::Forward
                    } else {
                        TraceKind::Hazard(HazardType::LoadUse)
                    },
                    from_stage: Stage::MEM as usize,
                    to_stage: Stage::EX as usize,
                    detail: format!("trace {i}"),
                })
                .collect();
            p.gantt = (0..12)
                .map(|i| crate::ui::pipeline::GanttRow {
                    gantt_id: i + 1,
                    pc: (i * 4) as u32,
                    disasm: format!("addi x{i}, x{i}, 1"),
                    class: crate::ui::pipeline::InstrClass::Alu,
                    cells: VecDeque::from(vec![GanttCell::InStage(Stage::IF); 4]),
                    first_cycle: i,
                    done: false,
                    last_stage: None,
                })
                .collect();
        }

        for (w, h) in [
            (120, 40),
            (100, 30),
            (80, 24),
            (70, 20),
            (60, 16),
            (40, 10),
            (20, 6),
        ] {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|f| {
                    crate::ui::view::pipeline::render_pipeline(f, f.area(), &app);
                })
                .unwrap_or_else(|e| panic!("render at {w}x{h} failed: {e}"));
        }
    }

    #[test]
    fn compact_trace_summary_uses_stage_route_labels() {
        let app = crate::ui::app::App::new(None);
        let trace = PipelineTraceView {
            kind: PipelineTraceKind::Forward,
            from_stage: Stage::MEM as usize,
            to_stage: Stage::EX as usize,
            detail: "BYPASS",
        };

        assert_eq!(trace_stage_summary(app.run.pipeline_inspect(), &trace), "MEM -> EX");
    }

    #[test]
    fn gantt_fu_cells_render_as_ex_in_history() {
        assert_eq!(
            cell_to_span(PipelineTimelineCell {
                role: PipelineStageRole::Other,
                label: "EX",
                state: PipelineTimelineState::Active,
            })
            .0,
            "EX"
        );
        assert_eq!(
            cell_to_span(PipelineTimelineCell {
                role: PipelineStageRole::Execute,
                label: "EX",
                state: PipelineTimelineState::Speculative,
            })
            .0,
            "EX"
        );
        assert_eq!(
            cell_to_span(PipelineTimelineCell {
                role: PipelineStageRole::Other,
                label: "──",
                state: PipelineTimelineState::Stalled,
            })
            .0,
            "──"
        );
    }

    #[test]
    fn split_fu_activity_excludes_serial_ex_mirror_from_parallel_count() {
        let mut ex_slot = PipeSlot::from_word(0, 0x0000_0013);
        ex_slot.seq = 7;

        let fu_states = vec![FuState {
            kind: Some(FuKind::Alu),
            slot: Some(ex_slot.clone()),
            busy_cycles_left: 0,
        }];

        let (parallel, mirror) = split_fu_activity(FuKind::Alu, &fu_states, Some(&ex_slot));
        assert!(parallel.is_empty());
        assert!(mirror.is_some());
    }
}
