use crate::ui::app::App;
use crate::ui::pipeline::sim::reg_name;
use crate::ui::pipeline::{
    FuKind, GanttCell, HazardTrace, HazardType, InstrClass, Stage, TraceKind, fu_latency_for_class,
    gantt_max_scroll, gantt_view_rows, gantt_visible_rows, gantt_window_bounds,
};
use crate::ui::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

fn is_atomic_instr(slot: &crate::ui::pipeline::PipeSlot) -> bool {
    matches!(
        slot.instr,
        Some(
            crate::falcon::instruction::Instruction::LrW { .. }
                | crate::falcon::instruction::Instruction::ScW { .. }
                | crate::falcon::instruction::Instruction::AmoswapW { .. }
                | crate::falcon::instruction::Instruction::AmoaddW { .. }
                | crate::falcon::instruction::Instruction::AmoxorW { .. }
                | crate::falcon::instruction::Instruction::AmoandW { .. }
                | crate::falcon::instruction::Instruction::AmoorW { .. }
                | crate::falcon::instruction::Instruction::AmomaxW { .. }
                | crate::falcon::instruction::Instruction::AmominW { .. }
                | crate::falcon::instruction::Instruction::AmomaxuW { .. }
                | crate::falcon::instruction::Instruction::AmominuW { .. }
        )
    )
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

pub fn render_pipeline_main(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();
    let plan = plan_main_layout(area.height, area.width, p.hazard_traces.len());

    if plan.collapsed {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        render_collapsed_strip(f, chunks[0], app);
        render_gantt(f, chunks[1], app);
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

    render_stages(f, chunks[0], app, plan.fu_h == 0);
    if plan.fu_h > 0 {
        render_fu_strip(f, chunks[1], app);
    }
    render_hazards(f, chunks[2], app);
    render_gantt(f, chunks[3], app);
}

/// Sub-9-line fallback: one line summarizing the stages and hazard count so
/// the HISTORY gantt can keep the rest of a very short terminal.
fn render_collapsed_strip(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();
    let stage_labels = ["IF", "ID", "EX", "MEM", "WB"];
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (i, label) in stage_labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme::BORDER)));
        }
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(theme::LABEL_Y).add_modifier(Modifier::BOLD),
        ));
        let text = match p.stages[i].as_ref() {
            None => "—".to_string(),
            Some(s) if s.is_bubble => "◦".to_string(),
            Some(s) => s.disasm.split_whitespace().next().unwrap_or("?").to_string(),
        };
        spans.push(Span::styled(text, Style::default().fg(theme::TEXT)));
    }
    if !p.hazard_traces.is_empty() {
        spans.push(Span::styled(
            format!("  · {} hazards", p.hazard_traces.len()),
            Style::default().fg(theme::PAUSED),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── 5-stage boxes ─────────────────────────────────────────────────────────────

fn render_stages(f: &mut Frame, area: Rect, app: &App, fu_in_ex_title: bool) {
    let p = &app.run.pipeline();
    let stage_labels = ["IF", "ID", "EX", "MEM", "WB"];

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 5); 5])
        .split(area);

    for (i, _stage) in Stage::all().iter().enumerate() {
        let slot = p.stages[i].as_ref();
        let mut stage_badges = stage_status_badges(p, i, slot);

        let border_style = match slot {
            Some(s) if s.class == InstrClass::Unknown => Style::default().fg(Color::DarkGray),
            Some(s) if s.hazard.is_some() => Style::default().fg(s.hazard.unwrap().color()),
            Some(s) if s.is_bubble => Style::default().fg(theme::PAUSED),
            Some(s) if s.is_speculative => Style::default().fg(theme::PAUSED),
            Some(_) => Style::default().fg(theme::ACCENT),
            None => Style::default().fg(theme::BORDER),
        };

        let mut title_label = match slot {
            Some(s) if s.is_speculative && !s.is_bubble => format!("{} ⟪P⟫", stage_labels[i]),
            Some(s) if s.hazard == Some(HazardType::BranchFlush) => {
                format!("{} ⟪X⟫", stage_labels[i])
            }
            _ => stage_labels[i].to_string(),
        };
        // With the FU strip folded away, surface overall FU occupancy here so
        // functional-unit pressure stays visible.
        if i == Stage::EX as usize && fu_in_ex_title {
            let busy = p
                .fu_bank
                .iter()
                .flatten()
                .filter(|fu| fu.slot.as_ref().is_some_and(|s| !s.is_bubble))
                .count();
            let cap: u8 = p.fu_capacity.iter().sum();
            title_label = format!("{title_label} {busy}/{cap}");
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title_label,
                border_style.add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(cols[i]);
        f.render_widget(block, cols[i]);
        if inner.height == 0 || inner.width == 0 {
            continue;
        }

        let lines = match slot {
            None => vec![Line::from(Span::styled(
                "—",
                Style::default().fg(theme::BORDER),
            ))],
            Some(s) if s.is_bubble => {
                let label = bubble_label_for_stage(i, s);
                vec![Line::from(Span::styled(
                    label,
                    border_style.add_modifier(Modifier::BOLD),
                ))]
            }
            Some(s) if s.class == InstrClass::Unknown => {
                // Undecodable instruction — visual indicator that it's ignored
                let dim = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM);
                vec![
                    Line::from(Span::styled(format!("0x{:04X}", s.pc), dim)),
                    Line::from(Span::styled(
                        "⊘ invalid",
                        Style::default()
                            .fg(theme::DANGER)
                            .add_modifier(Modifier::DIM),
                    )),
                    Line::from(Span::styled(format!(".word 0x{:08x}", s.word), dim)),
                ]
            }
            Some(s) => stage_slot_lines(i, s, inner, &mut stage_badges),
        };

        let visible_lines: Vec<_> = lines.into_iter().take(inner.height as usize).collect();
        f.render_widget(Paragraph::new(visible_lines), inner);
    }
}

/// Compact stage-column body. Three inner lines when there is room
/// (PC + badges / disasm / class + regs), two when squeezed
/// (disasm + badges / PC + regs).
fn stage_slot_lines(
    stage_idx: usize,
    s: &crate::ui::pipeline::PipeSlot,
    inner: Rect,
    stage_badges: &mut Vec<(String, Style)>,
) -> Vec<Line<'static>> {
    let w = inner.width as usize;
    let pc_str = format!("0x{:04X}", s.pc);
    let hazard_indicator = s.hazard.map(|h| {
        Span::styled(
            format!(" ⚠{}", compact_stage_hazard_label(stage_idx, Some(s), h)),
            Style::default().fg(h.color()),
        )
    });
    let pred_badge = speculative_compact_badge(s, w);
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

    let reg_str = compact_reg_summary(s);

    if inner.height >= 3 {
        // PC + badges / disasm / class + regs
        let mut pc_spans = vec![Span::styled(pc_str, Style::default().fg(theme::LABEL))];
        pc_spans.extend(badge_spans);

        let (disasm_trunc, _) = s.disasm.unicode_truncate(w.max(4));
        let mut class_spans = vec![Span::styled(
            format!("[{}]", s.class.label()),
            Style::default()
                .fg(s.class.color())
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
        let (disasm_trunc, _) = s.disasm.unicode_truncate(disasm_w);
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
fn compact_reg_summary(s: &crate::ui::pipeline::PipeSlot) -> String {
    let srcs: Vec<&str> = [s.rs1, s.rs2].iter().flatten().map(|r| reg_name(*r)).collect();
    let srcs = srcs.join(",");
    match (s.rd, srcs.is_empty()) {
        (Some(rd), false) => format!("{}←{}", reg_name(rd), srcs),
        (Some(rd), true) => reg_name(rd).to_string(),
        (None, false) => srcs,
        (None, true) => String::new(),
    }
}

fn stage_status_badges(
    p: &crate::ui::pipeline::PipelineSimState,
    stage_idx: usize,
    slot: Option<&crate::ui::pipeline::PipeSlot>,
) -> Vec<(String, Style)> {
    let mut tags: Vec<(String, Style)> = Vec::new();

    if let Some(s) = slot {
        if is_atomic_instr(s) {
            push_badge(
                &mut tags,
                "AT".to_string(),
                Style::default().fg(Color::LightYellow),
            );
        }
        if let Some(h) = s.hazard {
            push_badge(
                &mut tags,
                compact_stage_hazard_label(stage_idx, slot, h).to_string(),
                Style::default().fg(h.color()),
            );
        }
    }

    for trace in &p.hazard_traces {
        if trace.from_stage != stage_idx && trace.to_stage != stage_idx {
            continue;
        }
        match trace.kind {
            TraceKind::Forward => {
                push_badge(
                    &mut tags,
                    "RAW".to_string(),
                    Style::default().fg(HazardType::Raw.color()),
                );
                push_badge(
                    &mut tags,
                    "FWD".to_string(),
                    Style::default().fg(TraceKind::Forward.color()),
                );
            }
            TraceKind::Hazard(h) => {
                push_badge(
                    &mut tags,
                    compact_stage_hazard_label(stage_idx, slot, h).to_string(),
                    Style::default().fg(h.color()),
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

fn bubble_label_for_stage(stage_idx: usize, slot: &crate::ui::pipeline::PipeSlot) -> &'static str {
    match slot.hazard {
        Some(HazardType::BranchFlush) => "✕ squashed",
        Some(HazardType::MemLatency) => match stage_idx {
            x if x == Stage::ID as usize => "waiting for IF",
            x if x > Stage::ID as usize => "front-end bubble",
            _ => "wait",
        },
        _ => "NOP bubble",
    }
}

fn compact_stage_hazard_label(
    stage_idx: usize,
    slot: Option<&crate::ui::pipeline::PipeSlot>,
    h: HazardType,
) -> &'static str {
    match h {
        HazardType::Raw => "RAW",
        HazardType::LoadUse => "LOAD",
        HazardType::BranchFlush => "CTRL",
        HazardType::FuBusy => "FU",
        HazardType::MemLatency => {
            if slot.is_some_and(|s| s.is_bubble) {
                if stage_idx == Stage::ID as usize {
                    "UP"
                } else {
                    "BUBL"
                }
            } else if stage_idx == Stage::IF as usize {
                "IFWT"
            } else if stage_idx == Stage::MEM as usize {
                "MEMWT"
            } else {
                "WAIT"
            }
        }
        HazardType::Waw => "WAW",
        HazardType::War => "WAR",
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

fn render_fu_strip(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();
    let cpi = &app.run.cpi_config;
    let ex_slot = p.stages[Stage::EX as usize].as_ref();
    let wide = !fu_strip_is_compact(area.width);

    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        " FU ",
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    )];
    let mut active_note: Option<String> = None;

    for (idx, fu_kind) in FuKind::all().iter().copied().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme::BORDER)));
        }
        let fu_states = &p.fu_bank[fu_kind.index()];
        let (parallel_slots, mirrored_ex_slot) = split_fu_activity(fu_kind, fu_states, ex_slot);
        let active_slots: Vec<_> = parallel_slots
            .iter()
            .copied()
            .chain(mirrored_ex_slot.into_iter())
            .collect();
        let capacity = p.fu_capacity[fu_kind.index()].max(1);

        if let Some(s) = active_slots.first().copied() {
            let latency_class = match fu_kind {
                FuKind::Alu => s.class,
                FuKind::Mul => InstrClass::Mul,
                FuKind::Div => InstrClass::Div,
                FuKind::Fpu => InstrClass::Fp,
                FuKind::Lsu => match s.class {
                    InstrClass::Store => InstrClass::Store,
                    _ => InstrClass::Load,
                },
                FuKind::Sys => InstrClass::System,
            };
            let total = fu_latency_for_class(latency_class, cpi).max(1);
            let done = total.saturating_sub(s.fu_cycles_left);
            spans.push(Span::styled(
                fu_kind.label().to_string(),
                Style::default()
                    .fg(s.class.color())
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {}/{}", active_slots.len(), capacity),
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
                let summary = if active_slots.len() > 1 {
                    format!("{} (+{} more)", s.disasm, active_slots.len() - 1)
                } else {
                    s.disasm.clone()
                };
                active_note = Some(format!("{summary} ({}/{total})", done + 1));
            }
        } else {
            spans.push(Span::styled(
                fu_kind.label().to_string(),
                Style::default().fg(theme::BORDER),
            ));
            spans.push(Span::styled(
                format!(" 0/{capacity}"),
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

fn render_hazards(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();

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

    if p.hazard_traces.is_empty() {
        let status = if p.halted {
            " ✓ Halted"
        } else if p.faulted {
            " ✗ Fault"
        } else {
            " No active links"
        };
        let mut text = status.to_string();
        if let Some((_, msg)) = p.hazard_msgs.first() {
            text.push_str("  ·  ");
            text.push_str(msg);
        }
        let (trunc, _) = text.unicode_truncate(w);
        lines.push(Line::from(Span::styled(trunc.to_string(), dim)));
    } else {
        let n = p.hazard_traces.len();
        let shown = if n > rows_avail {
            rows_avail.saturating_sub(1)
        } else {
            n
        };
        for trace in p.hazard_traces.iter().take(shown) {
            let detail = trace_detail_for(trace, &p.hazard_msgs);
            lines.push(render_hazard_row(trace, &detail, w));
        }
        if n > shown {
            let hidden = &p.hazard_traces[shown..];
            let fwd = hidden
                .iter()
                .filter(|t| matches!(t.kind, TraceKind::Forward))
                .count();
            let hzd = hidden.len() - fwd;
            lines.push(Line::from(Span::styled(
                format!(" +{} more ({fwd} FWD · {hzd} HZD)", hidden.len()),
                dim,
            )));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// One hazard/forwarding link per line: `[FWD] MEM -> EX  detail…`.
fn render_hazard_row(trace: &HazardTrace, detail: &str, width: usize) -> Line<'static> {
    let badge = format!("[{}]", trace.kind.short_label());
    let route = format!(" {:<10}", trace_stage_summary(trace));
    let used = 1 + UnicodeWidthStr::width(badge.as_str()) + UnicodeWidthStr::width(route.as_str());
    let (detail_trunc, _) = detail.unicode_truncate(width.saturating_sub(used).max(8));
    Line::from(vec![
        Span::raw(" "),
        Span::styled(badge, trace_badge_style(trace.kind)),
        Span::styled(
            route,
            Style::default()
                .fg(trace.kind.color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(detail_trunc.to_string(), Style::default().fg(theme::TEXT)),
    ])
}

fn trace_detail_for(trace: &HazardTrace, hazard_msgs: &[(HazardType, String)]) -> String {
    match trace.kind {
        TraceKind::Forward => trace.detail.clone(),
        TraceKind::Hazard(ht) => {
            if !trace.detail.is_empty() {
                trace.detail.clone()
            } else {
                hazard_msgs
                    .iter()
                    .find(|(msg_ht, _)| *msg_ht == ht)
                    .map(|(_, msg)| msg.clone())
                    .unwrap_or_default()
            }
        }
    }
}

fn trace_stage_summary(trace: &HazardTrace) -> String {
    format!(
        "{} -> {}",
        stage_name_from_idx(trace.from_stage),
        stage_name_from_idx(trace.to_stage)
    )
}

fn stage_name_from_idx(idx: usize) -> &'static str {
    match idx {
        x if x == Stage::IF as usize => Stage::IF.label(),
        x if x == Stage::ID as usize => Stage::ID.label(),
        x if x == Stage::EX as usize => Stage::EX.label(),
        x if x == Stage::MEM as usize => Stage::MEM.label(),
        _ => Stage::WB.label(),
    }
}

fn speculative_compact_badge(
    slot: &crate::ui::pipeline::PipeSlot,
    width: usize,
) -> Option<(String, Style)> {
    if !slot.is_speculative {
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
        Style::default()
            .fg(theme::PAUSED)
            .add_modifier(Modifier::BOLD),
    ))
}

fn trace_badge_style(kind: TraceKind) -> Style {
    Style::default()
        .fg(kind.color())
        .bg(theme::BG_SEP)
        .add_modifier(Modifier::BOLD)
}

// ── Gantt diagram ─────────────────────────────────────────────────────────────

fn render_gantt(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.run.pipeline();

    const LABEL_W: usize = 12;
    const CELL_W: usize = 4;
    let preview_inner_w = area.width.saturating_sub(2) as usize;
    let preview_cols = ((preview_inner_w.saturating_sub(LABEL_W)) / CELL_W).max(1);
    let visible_cols = preview_cols.min(crate::ui::pipeline::MAX_GANTT_COLS);

    let scroll_hint = if p.gantt_scroll == 0 {
        format!(" HISTORY  up to {} cycles · following ", visible_cols)
    } else {
        format!(" HISTORY  scrollback ↑{} · End=follow ", p.gantt_scroll)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(scroll_hint, Style::default().fg(theme::LABEL)));

    let inner = block.inner(area);
    p.gantt_area_rect
        .set((inner.x, inner.y, inner.width, inner.height));
    f.render_widget(block, area);

    if p.gantt.is_empty() {
        f.render_widget(
            Paragraph::new("  — no history yet —").style(
                Style::default()
                    .fg(theme::LABEL)
                    .add_modifier(Modifier::DIM),
            ),
            inner,
        );
        return;
    }

    let max_cols = ((inner.width as usize).saturating_sub(LABEL_W)) / CELL_W;
    let history_cols = max_cols.min(crate::ui::pipeline::MAX_GANTT_COLS).max(1);
    let max_rows = inner.height as usize;
    let visible_capacity = max_rows.saturating_sub(2).max(1);

    let visible_rows = gantt_visible_rows(area.height);
    p.gantt_visible_rows_cache.set(visible_rows);
    let max_scroll = gantt_max_scroll(p, area.height);
    p.gantt_max_scroll_cache.set(max_scroll);
    let scroll = p.gantt_scroll.min(max_scroll);
    let visible_rows = gantt_view_rows(&p.gantt, scroll, visible_capacity);
    let (start_cycle, end_cycle) = gantt_window_bounds(&visible_rows, history_cols);

    let mut header_spans = vec![Span::styled(
        format!("{:<width$}", "instr", width = LABEL_W),
        Style::default().fg(theme::LABEL_Y),
    )];
    for c in start_cycle..end_cycle {
        header_spans.push(Span::styled(
            format!("{:>4}", c % 10000),
            Style::default()
                .fg(theme::LABEL)
                .add_modifier(Modifier::DIM),
        ));
    }

    let separator = format!("{}{}", "-".repeat(LABEL_W), "----".repeat(history_cols));

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(header_spans),
        Line::from(Span::styled(separator, Style::default().fg(theme::BORDER))),
    ];

    for row in visible_rows {
        let is_invalid = row.class == InstrClass::Unknown;
        let row_label = if row.disasm.starts_with("lr.w")
            || row.disasm.starts_with("sc.w")
            || row.disasm.starts_with("amoswap.w")
            || row.disasm.starts_with("amoadd.w")
            || row.disasm.starts_with("amoxor.w")
            || row.disasm.starts_with("amoand.w")
            || row.disasm.starts_with("amoor.w")
            || row.disasm.starts_with("amomax.w")
            || row.disasm.starts_with("amomin.w")
            || row.disasm.starts_with("amomaxu.w")
            || row.disasm.starts_with("amominu.w")
        {
            format!("{} [AT]", row.disasm)
        } else {
            row.disasm.clone()
        };
        let (label, _) = row_label.unicode_truncate(LABEL_W - 1);
        let label_style = if is_invalid {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let mut spans = vec![Span::styled(
            format!("{:<width$}", label, width = LABEL_W),
            label_style,
        )];

        for c in start_cycle..end_cycle {
            let cell = if c < row.first_cycle {
                GanttCell::Empty
            } else {
                let cell_idx = (c - row.first_cycle) as usize;
                row.cells.get(cell_idx).copied().unwrap_or(GanttCell::Empty)
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

fn cell_to_span(cell: GanttCell) -> (&'static str, Style) {
    // Speculative stages: same label as InStage but in orange — shows instruction
    // was fetched/decoded speculatively while a branch was unresolved.
    let spec_style = Style::default().fg(theme::SPECULATIVE);
    match cell {
        GanttCell::Empty => ("·", Style::default().fg(theme::BORDER)),
        GanttCell::InStage(Stage::IF) => ("IF", Style::default().fg(theme::ACCENT)),
        GanttCell::InStage(Stage::ID) => ("ID", Style::default().fg(theme::LABEL_Y)),
        GanttCell::InStage(Stage::EX) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InStage(Stage::MEM) => ("MEM", Style::default().fg(theme::LABEL_Y)),
        GanttCell::InStage(Stage::WB) => ("WB", Style::default().fg(theme::ACCENT)),
        GanttCell::InFu(FuKind::Alu) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Mul) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Div) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Fpu) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Lsu) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Sys) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::Speculative(Stage::IF) => ("IF", spec_style),
        GanttCell::Speculative(Stage::ID) => ("ID", spec_style),
        GanttCell::Speculative(Stage::EX) => ("EX", spec_style),
        GanttCell::Speculative(Stage::MEM) => ("MEM", spec_style),
        GanttCell::Speculative(Stage::WB) => ("WB", spec_style),
        GanttCell::SpeculativeFu(FuKind::Alu) => ("EX", spec_style),
        GanttCell::SpeculativeFu(FuKind::Mul) => ("EX", spec_style),
        GanttCell::SpeculativeFu(FuKind::Div) => ("EX", spec_style),
        GanttCell::SpeculativeFu(FuKind::Fpu) => ("EX", spec_style),
        GanttCell::SpeculativeFu(FuKind::Lsu) => ("EX", spec_style),
        GanttCell::SpeculativeFu(FuKind::Sys) => ("EX", spec_style),
        GanttCell::Stall => ("──", Style::default().fg(theme::PAUSED)),
        GanttCell::Bubble => (
            "NOP",
            Style::default()
                .fg(theme::PAUSED)
                .add_modifier(Modifier::DIM),
        ),
        GanttCell::Flush => ("◀FL", Style::default().fg(theme::DANGER)),
    }
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

    #[test]
    fn mem_latency_bubble_in_id_reads_as_waiting_for_if() {
        let mut slot = PipeSlot::bubble();
        slot.hazard = Some(HazardType::MemLatency);

        assert_eq!(
            bubble_label_for_stage(Stage::ID as usize, &slot),
            "waiting for IF"
        );
        assert_eq!(
            compact_stage_hazard_label(Stage::ID as usize, Some(&slot), HazardType::MemLatency),
            "UP"
        );
    }

    #[test]
    fn mem_latency_on_if_and_mem_uses_stage_specific_badges() {
        let mut if_slot = PipeSlot::from_word(0, 0x0000_0013);
        if_slot.hazard = Some(HazardType::MemLatency);

        let mut mem_slot = PipeSlot::from_word(4, 0x0000_0013);
        mem_slot.hazard = Some(HazardType::MemLatency);

        assert_eq!(
            compact_stage_hazard_label(Stage::IF as usize, Some(&if_slot), HazardType::MemLatency),
            "IFWT"
        );
        assert_eq!(
            compact_stage_hazard_label(
                Stage::MEM as usize,
                Some(&mem_slot),
                HazardType::MemLatency
            ),
            "MEMWT"
        );
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
        let trace = HazardTrace {
            kind: TraceKind::Forward,
            from_stage: Stage::MEM as usize,
            to_stage: Stage::EX as usize,
            detail: "BYPASS".to_string(),
        };

        assert_eq!(trace_stage_summary(&trace), "MEM -> EX");
    }

    #[test]
    fn gantt_fu_cells_render_as_ex_in_history() {
        assert_eq!(cell_to_span(GanttCell::InFu(FuKind::Mul)).0, "EX");
        assert_eq!(cell_to_span(GanttCell::SpeculativeFu(FuKind::Lsu)).0, "EX");
        assert_eq!(cell_to_span(GanttCell::Stall).0, "──");
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
