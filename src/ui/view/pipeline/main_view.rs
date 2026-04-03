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

pub fn render_pipeline_main(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    // The main EX view always shows the functional units so students can
    // reason about potential parallelism even when execution is serialized.
    let stages_h: u16 = 9;
    let max_trace_rows = area
        .height
        .saturating_sub(stages_h)
        .saturating_sub(5)
        .clamp(3, 8);
    let trace_rows = p.hazard_traces.len().min(max_trace_rows as usize) as u16;
    let legend_rows = if p.hazard_traces.is_empty() { 0 } else { 1 };
    let msg_rows = if p.hazard_msgs.is_empty() {
        1
    } else {
        p.hazard_msgs.len().min(2) as u16
    };
    let remaining_after_stages = area.height.saturating_sub(stages_h);
    let hazards_cap = remaining_after_stages.saturating_sub(3);
    let hazards_h: u16 = if hazards_cap == 0 {
        0
    } else {
        (2 + trace_rows + legend_rows + msg_rows)
            .min(hazards_cap)
            .clamp(1, 13)
    };

    // Layout: stages | hazards (3) | gantt (rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(stages_h),
            Constraint::Length(hazards_h),
            Constraint::Min(1),
        ])
        .split(area);

    render_stages(f, chunks[0], app);
    render_hazards(f, chunks[1], app);
    render_gantt(f, chunks[2], app);
}

// ── 5-stage boxes ─────────────────────────────────────────────────────────────

fn render_stages(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
    let stage_labels = ["IF", "ID", "EX", "MEM", "WB"];

    // EX always gets the expanded functional-unit panel.
    let constraints: Vec<Constraint> = vec![
        Constraint::Ratio(1, 7),
        Constraint::Ratio(1, 7),
        Constraint::Ratio(3, 7),
        Constraint::Ratio(1, 7),
        Constraint::Ratio(1, 7),
    ];

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, _stage) in Stage::all().iter().enumerate() {
        if i == Stage::EX as usize {
            render_fu_box(f, cols[i], app);
            continue;
        }

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

        let title_label = match slot {
            Some(s) if s.is_speculative && !s.is_bubble => format!("{} ⟪P⟫", stage_labels[i]),
            Some(s) if s.hazard == Some(HazardType::BranchFlush) => {
                format!("{} ⟪X⟫", stage_labels[i])
            }
            _ => stage_labels[i].to_string(),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title_label,
                border_style.add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(cols[i]);
        f.render_widget(block, cols[i]);
        let content_area = inner;

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
                let mut lines = vec![
                    Line::from(Span::styled(format!("0x{:04X}", s.pc), dim)),
                    Line::from(Span::styled(
                        "⊘ invalid",
                        Style::default()
                            .fg(theme::DANGER)
                            .add_modifier(Modifier::DIM),
                    )),
                    Line::from(Span::styled(format!(".word 0x{:08x}", s.word), dim)),
                ];
                if content_area.height >= 4 {
                    lines.push(Line::from(Span::styled("(ignored)", dim)));
                }
                lines
            }
            Some(s) => {
                let w = inner.width as usize;
                let pc_str = format!("0x{:04X}", s.pc);
                let class_color = s.class.color();
                let hazard_indicator = s.hazard.map(|h| {
                    Span::styled(
                        format!(" ⚠{}", compact_stage_hazard_label(i, Some(s), h)),
                        Style::default().fg(h.color()),
                    )
                });
                let pred_badge = speculative_compact_badge(s, w);
                let pred_w = pred_badge
                    .as_ref()
                    .map_or(0, |(label, _)| UnicodeWidthStr::width(label.as_str()));
                trim_stage_badges_to_fit(&mut stage_badges, w, pred_w);
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
                if let Some(h) = hazard_indicator.filter(|_| stage_badges.is_empty()) {
                    disasm_spans.push(h);
                }
                disasm_spans.extend(
                    stage_badges
                        .iter()
                        .map(|(label, style)| Span::styled(label.clone(), *style)),
                );
                if let Some((label, style)) = pred_badge {
                    disasm_spans.push(Span::styled(label, style));
                }

                // rd/rs1/rs2
                let reg_str = {
                    let mut parts = Vec::new();
                    if let Some(rd) = s.rd {
                        parts.push(format!("rd={}", reg_name(rd)));
                    }
                    if let Some(rs1) = s.rs1 {
                        parts.push(format!("rs1={}", reg_name(rs1)));
                    }
                    if let Some(rs2) = s.rs2 {
                        parts.push(format!("rs2={}", reg_name(rs2)));
                    }
                    parts.join(" ")
                };

                let mut lines = vec![
                    Line::from(Span::styled(pc_str, Style::default().fg(theme::LABEL))),
                    Line::from(disasm_spans),
                    Line::from(Span::styled(
                        format!("[{}]", s.class.label()),
                        Style::default().fg(class_color).add_modifier(Modifier::DIM),
                    )),
                ];
                if content_area.height >= 5 {
                    if let Some(pred_line) = speculative_detail_line(s, w) {
                        lines.push(pred_line);
                    }
                }
                if content_area.height >= 4 && !reg_str.is_empty() {
                    lines.push(Line::from(Span::styled(
                        reg_str,
                        Style::default().fg(theme::BORDER),
                    )));
                }
                lines
            }
        };

        if content_area.height > 0 {
            let visible_lines: Vec<_> = lines
                .into_iter()
                .take(content_area.height as usize)
                .collect();
            f.render_widget(Paragraph::new(visible_lines), content_area);
        }
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

fn render_fu_box(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
    let ex_slot = p.stages[Stage::EX as usize].as_ref();

    // Borda colorida baseada no estado do EX slot
    let border_style = match ex_slot {
        Some(s) if s.hazard.is_some() => Style::default().fg(s.hazard.unwrap().color()),
        Some(s) if s.is_bubble => Style::default().fg(theme::PAUSED),
        Some(_) => Style::default().fg(theme::ACCENT),
        None => Style::default().fg(theme::BORDER),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            "EX (UFs)",
            border_style.add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);
    let fu_area = inner;

    // Lista de UFs: (nome, classe que a ocupa, latência via CPI config)
    let cpi = &app.run.cpi_config;
    let fu_defs = FuKind::all();

    // Cada FU ocupa uma linha dentro do box
    let row_constraints: Vec<Constraint> = fu_defs.iter().map(|_| Constraint::Length(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(fu_area);

    for (i, fu_kind) in fu_defs.iter().copied().enumerate() {
        if i >= rows.len() {
            break;
        }

        let fu_states = &p.fu_bank[fu_kind.index()];
        let (parallel_slots, mirrored_ex_slot) = split_fu_activity(fu_kind, fu_states, ex_slot);
        let active_slots: Vec<_> = parallel_slots
            .iter()
            .copied()
            .chain(mirrored_ex_slot.into_iter())
            .collect();
        let active_slot = active_slots.first().copied();
        let is_active = active_slot.is_some();
        let capacity = p.fu_capacity[fu_kind.index()].max(1);
        let parallel_count = parallel_slots.len();
        let shows_ex_mirror = mirrored_ex_slot.is_some();

        let line = if is_active {
            let s = active_slot.unwrap();
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
            let total = fu_latency_for_class(latency_class, cpi);
            let done = total.saturating_sub(s.fu_cycles_left);
            // Barra de progresso
            let bar_w = (rows[i].width as usize).saturating_sub(26).min(12).max(4);
            let filled = if total > 1 {
                ((done as usize) * bar_w / (total as usize)).min(bar_w)
            } else {
                bar_w
            };
            let bar: String = (0..bar_w)
                .map(|j| if j < filled { '█' } else { '░' })
                .collect();

            let w = rows[i].width as usize;
            let disasm_w = w.saturating_sub(26);
            let summary = if active_slots.len() > 1 {
                format!("{} (+{} more)", s.disasm, active_slots.len() - 1)
            } else {
                s.disasm.clone()
            };
            let (disasm_trunc, _) = summary.unicode_truncate(disasm_w);
            let occupancy_label = if shows_ex_mirror && parallel_count == 0 {
                format!(" EX {}/{} ", parallel_count, capacity)
            } else {
                format!(" {}/{} {}/{} ", done + 1, total, parallel_count, capacity)
            };

            Line::from(vec![
                Span::styled(
                    format!("{:<3} ", fu_kind.label()),
                    Style::default()
                        .fg(s.class.color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<width$}", disasm_trunc, width = disasm_w),
                    Style::default().fg(theme::TEXT),
                ),
                Span::styled(
                    occupancy_label,
                    if shows_ex_mirror && parallel_count == 0 {
                        Style::default().fg(theme::PAUSED)
                    } else {
                        Style::default().fg(theme::LABEL_Y)
                    },
                ),
                Span::styled(
                    bar,
                    if shows_ex_mirror && parallel_count == 0 {
                        Style::default().fg(theme::PAUSED)
                    } else {
                        Style::default().fg(theme::RUNNING)
                    },
                ),
            ])
        } else if fu_kind == FuKind::Sys {
            Line::from(vec![
                Span::styled(
                    format!("{:<3} ", fu_kind.label()),
                    Style::default().fg(theme::BORDER),
                ),
                Span::styled(
                    format!("trap / syscall handoff  0/{capacity}"),
                    Style::default()
                        .fg(theme::LABEL)
                        .add_modifier(Modifier::DIM),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!("{:<3} ", fu_kind.label()),
                    Style::default().fg(theme::BORDER),
                ),
                Span::styled(
                    format!("—  0/{capacity}"),
                    Style::default()
                        .fg(theme::BORDER)
                        .add_modifier(Modifier::DIM),
                ),
            ])
        };

        f.render_widget(Paragraph::new(line), rows[i]);
    }
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
    let p = &app.pipeline;

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Hazard / Forwarding Map ",
            Style::default().fg(theme::LABEL_Y),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let compact = hazard_map_is_compact(inner);
    let cols = if compact {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(7), Constraint::Min(24)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(7),
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(inner)
    };

    let kind_area = cols[0];
    let map_area = cols[1];
    let info_area = if compact { cols[1] } else { cols[2] };

    let row_cap = map_area.height.saturating_sub(1) as usize;
    let mut kind_lines: Vec<Line<'_>> = vec![trace_kind_header(kind_area.width as usize)];
    let mut map_lines: Vec<Line<'_>> = vec![if compact {
        compact_trace_header(map_area.width as usize)
    } else {
        stage_trace_header(map_area.width as usize)
    }];
    let mut info_lines: Vec<Line<'_>> = if compact {
        Vec::new()
    } else {
        vec![trace_detail_header(info_area.width as usize)]
    };

    if p.hazard_traces.is_empty() {
        kind_lines.push(Line::from(Span::styled(
            "  —",
            Style::default()
                .fg(theme::LABEL)
                .add_modifier(Modifier::DIM),
        )));
        map_lines.push(Line::from(Span::styled(
            if p.halted {
                "  ✓ Halted"
            } else if p.faulted {
                "  ✗ Fault"
            } else {
                "  No active links"
            },
            Style::default()
                .fg(theme::LABEL)
                .add_modifier(Modifier::DIM),
        )));
        let fallback = if p.hazard_msgs.is_empty() {
            "  No textual hazard notes this cycle".to_string()
        } else {
            p.hazard_msgs[0].1.clone()
        };
        if compact {
            let (trunc, _) = fallback.unicode_truncate(map_area.width.saturating_sub(2) as usize);
            map_lines.push(Line::from(Span::styled(
                format!("  {}", trunc),
                Style::default()
                    .fg(theme::LABEL)
                    .add_modifier(Modifier::DIM),
            )));
        } else {
            let (trunc, _) = fallback.unicode_truncate(info_area.width.saturating_sub(2) as usize);
            info_lines.push(Line::from(Span::styled(
                format!("  {}", trunc),
                Style::default()
                    .fg(theme::LABEL)
                    .add_modifier(Modifier::DIM),
            )));
        }
    } else {
        let mut rendered = 0usize;
        for (i, trace) in p.hazard_traces.iter().enumerate() {
            if rendered >= row_cap {
                break;
            }
            let detail = trace_detail_for(trace, &p.hazard_msgs);
            kind_lines.push(render_trace_kind_line(trace, kind_area.width as usize));
            map_lines.push(if compact {
                render_compact_trace_line(trace, &detail, map_area.width as usize)
            } else {
                render_trace_map_line(trace, map_area.width as usize)
            });
            if !compact {
                info_lines.push(render_trace_detail_line(
                    trace,
                    &detail,
                    info_area.width as usize,
                ));
            }
            rendered += 1;
            if i + 1 == p.hazard_traces.len() && rendered < row_cap {
                kind_lines.push(Line::from(Span::styled(
                    "  i",
                    Style::default()
                        .fg(theme::LABEL)
                        .add_modifier(Modifier::DIM),
                )));
                map_lines.push(if compact {
                    render_compact_trace_legend(map_area.width as usize)
                } else {
                    Line::from(Span::styled(" ", Style::default()))
                });
                if !compact {
                    info_lines.push(render_trace_legend(info_area.width as usize));
                }
            }
        }
    }

    f.render_widget(Paragraph::new(kind_lines), kind_area);
    f.render_widget(Paragraph::new(map_lines), map_area);
    if !compact {
        f.render_widget(Paragraph::new(info_lines), info_area);
    }
}

fn hazard_map_is_compact(area: Rect) -> bool {
    area.width < 72
}

fn trace_kind_header(width: usize) -> Line<'static> {
    let label = format!("{:<width$}", "TYPE", width = width.max(4));
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    ))
}

fn trace_detail_header(width: usize) -> Line<'static> {
    let label = format!("{:<width$}", "DETAIL", width = width.max(6));
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    ))
}

fn render_trace_kind_line(trace: &HazardTrace, width: usize) -> Line<'static> {
    let text = format!("[{}]", trace.kind.short_label());
    let mut padded = format!("{:<width$}", text, width = width.max(text.len()));
    if padded.len() > width {
        padded.truncate(width);
    }
    Line::from(Span::styled(padded, trace_badge_style(trace.kind)))
}

fn stage_trace_header(width: usize) -> Line<'static> {
    let stage_cols = trace_stage_cols(width);
    let mut chars = vec![' '; width.max(1)];
    for &col in &stage_cols {
        if col < chars.len() {
            chars[col] = '│';
        }
    }
    for (label, col) in [
        ("IF", stage_cols[0]),
        ("ID", stage_cols[1]),
        ("EX", stage_cols[2]),
        ("MEM", stage_cols[3]),
        ("WB", stage_cols[4]),
    ] {
        for (i, ch) in label.chars().enumerate() {
            if col + i < chars.len() {
                chars[col + i] = ch;
            }
        }
    }
    Line::from(Span::styled(
        chars.into_iter().collect::<String>(),
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    ))
}

fn compact_trace_header(width: usize) -> Line<'static> {
    let label = format!("{:<width$}", "PATH / DETAIL", width = width.max(13));
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme::LABEL_Y)
            .add_modifier(Modifier::BOLD),
    ))
}

fn trace_stage_cols(width: usize) -> [usize; 5] {
    let usable = width.saturating_sub(34).max(16);
    let start = 2usize;
    [
        start,
        start + usable / 4,
        start + usable / 2,
        start + usable * 3 / 4,
        start + usable,
    ]
}

fn render_trace_map_line(trace: &HazardTrace, width: usize) -> Line<'static> {
    let width = width.max(20);
    let cols = trace_stage_cols(width);
    let from_col = cols[trace.from_stage.min(4)];
    let to_col = cols[trace.to_stage.min(4)];
    let mut chars = vec![' '; width];

    for &col in &cols {
        if col < width {
            chars[col] = '┆';
        }
    }

    if from_col == to_col {
        if from_col < width {
            chars[from_col] = '●';
        }
    } else if from_col < to_col {
        let (src_char, line_char, dst_char) = trace_glyphs(trace.kind, true);
        if from_col < width {
            chars[from_col] = src_char;
        }
        for ch in chars.iter_mut().take(to_col).skip(from_col + 1) {
            *ch = line_char;
        }
        if to_col < width {
            chars[to_col] = dst_char;
        }
    } else {
        let (src_char, line_char, dst_char) = trace_glyphs(trace.kind, false);
        if from_col < width {
            chars[from_col] = src_char;
        }
        for ch in chars.iter_mut().take(from_col).skip(to_col + 1) {
            *ch = line_char;
        }
        if to_col < width {
            chars[to_col] = dst_char;
        }
    }

    for &col in &cols {
        if col < width && chars[col] == ' ' {
            chars[col] = '┆';
        }
    }
    let graphic = chars.into_iter().collect::<String>();
    Line::from(Span::styled(
        graphic,
        Style::default()
            .fg(trace.kind.color())
            .add_modifier(Modifier::BOLD),
    ))
}

fn render_compact_trace_line(trace: &HazardTrace, detail: &str, width: usize) -> Line<'static> {
    let text = format!("{}  {}", trace_stage_summary(trace), detail);
    let (trunc, _) = text.unicode_truncate(width.max(12));
    Line::from(Span::styled(
        trunc.to_string(),
        Style::default()
            .fg(trace.kind.color())
            .add_modifier(Modifier::BOLD),
    ))
}

fn render_trace_detail_line(trace: &HazardTrace, detail: &str, width: usize) -> Line<'static> {
    let prefix = match trace.kind {
        TraceKind::Forward => "bypass  ",
        TraceKind::Hazard(HazardType::LoadUse) => "stall   ",
        TraceKind::Hazard(HazardType::BranchFlush) => "squash  ",
        TraceKind::Hazard(HazardType::MemLatency) => "wait    ",
        _ => "hazard  ",
    };
    let text = format!("{}{}", prefix, detail);
    let (trunc, _) = text.unicode_truncate(width.max(8));
    Line::from(Span::styled(
        trunc.to_string(),
        Style::default().fg(theme::TEXT),
    ))
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

fn render_compact_trace_legend(width: usize) -> Line<'static> {
    let text = "  path  producer -> consumer";
    let (trunc, _) = text.unicode_truncate(width.max(8));
    Line::from(Span::styled(
        trunc.to_string(),
        Style::default()
            .fg(theme::LABEL)
            .add_modifier(Modifier::DIM),
    ))
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

#[cfg(test)]
mod tests {
    use super::{
        bubble_label_for_stage, cell_to_span, compact_stage_hazard_label, hazard_map_is_compact,
        split_fu_activity, trace_stage_summary,
    };
    use crate::ui::pipeline::{
        FuKind, FuState, GanttCell, HazardTrace, HazardType, PipeSlot, Stage, TraceKind,
    };
    use ratatui::layout::Rect;

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
    fn hazard_map_switches_to_compact_layout_when_width_is_tight() {
        assert!(hazard_map_is_compact(Rect::new(0, 0, 71, 5)));
        assert!(!hazard_map_is_compact(Rect::new(0, 0, 72, 5)));
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
    fn gantt_fu_cells_render_fu_labels() {
        assert_eq!(cell_to_span(GanttCell::InFu(FuKind::Mul)).0, "MUL");
        assert_eq!(cell_to_span(GanttCell::SpeculativeFu(FuKind::Lsu)).0, "LSU");
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

fn render_trace_legend(width: usize) -> Line<'static> {
    let compact = width < 72;
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled("[FWD]", trace_badge_style(TraceKind::Forward)),
        Span::styled(
            if compact {
                " bypass path  "
            } else {
                " producer bypass path into consumer  "
            },
            Style::default().fg(theme::TEXT),
        ),
        Span::styled(
            "[HZD]",
            Style::default()
                .fg(theme::PAUSED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if compact {
                " dependency / squash"
            } else {
                " dependency, stall, or squash path"
            },
            Style::default().fg(theme::TEXT),
        ),
    ])
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

fn speculative_detail_line(
    slot: &crate::ui::pipeline::PipeSlot,
    width: usize,
) -> Option<Line<'static>> {
    if !slot.is_speculative {
        return None;
    }
    let target = slot.predicted_target.unwrap_or(slot.pc.wrapping_add(4));
    let summary = if slot.predicted_taken {
        format!("⟪pred taken -> 0x{:04X}⟫", target)
    } else {
        format!("⟪pred not-taken -> 0x{:04X}⟫", target)
    };
    let (trunc, _) = summary.unicode_truncate(width.max(8));
    Some(Line::from(Span::styled(
        trunc.to_string(),
        Style::default()
            .fg(theme::PAUSED)
            .add_modifier(Modifier::DIM),
    )))
}

fn trace_badge_style(kind: TraceKind) -> Style {
    Style::default()
        .fg(kind.color())
        .bg(theme::BG_SEP)
        .add_modifier(Modifier::BOLD)
}

fn trace_glyphs(kind: TraceKind, forward_dir: bool) -> (char, char, char) {
    match kind {
        TraceKind::Forward => {
            if forward_dir {
                ('◆', '┈', '▶')
            } else {
                ('◆', '┈', '◀')
            }
        }
        TraceKind::Hazard(HazardType::BranchFlush) => {
            if forward_dir {
                ('●', '═', '✕')
            } else {
                ('●', '═', '✕')
            }
        }
        TraceKind::Hazard(_) => {
            if forward_dir {
                ('●', '━', '▶')
            } else {
                ('●', '━', '◀')
            }
        }
    }
}

// ── Gantt diagram ─────────────────────────────────────────────────────────────

fn render_gantt(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    const LABEL_W: usize = 12;
    const CELL_W: usize = 4;
    let preview_inner_w = area.width.saturating_sub(2) as usize;
    let preview_cols = ((preview_inner_w.saturating_sub(LABEL_W)) / CELL_W).max(1);
    let visible_cols = preview_cols.min(crate::ui::pipeline::MAX_GANTT_COLS);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            format!(" HISTORY  up to {} cycles ", visible_cols),
            Style::default().fg(theme::LABEL),
        ));

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
    let spec_style = Style::default().fg(Color::Rgb(220, 140, 40));
    match cell {
        GanttCell::Empty => ("·", Style::default().fg(theme::BORDER)),
        GanttCell::InStage(Stage::IF) => ("IF", Style::default().fg(theme::ACCENT)),
        GanttCell::InStage(Stage::ID) => ("ID", Style::default().fg(theme::LABEL_Y)),
        GanttCell::InStage(Stage::EX) => ("EX", Style::default().fg(theme::RUNNING)),
        GanttCell::InStage(Stage::MEM) => ("MEM", Style::default().fg(theme::LABEL_Y)),
        GanttCell::InStage(Stage::WB) => ("WB", Style::default().fg(theme::ACCENT)),
        GanttCell::InFu(FuKind::Alu) => ("ALU", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Mul) => ("MUL", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Div) => ("DIV", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Fpu) => ("FPU", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Lsu) => ("LSU", Style::default().fg(theme::RUNNING)),
        GanttCell::InFu(FuKind::Sys) => ("SYS", Style::default().fg(theme::RUNNING)),
        GanttCell::Speculative(Stage::IF) => ("IF", spec_style),
        GanttCell::Speculative(Stage::ID) => ("ID", spec_style),
        GanttCell::Speculative(Stage::EX) => ("EX", spec_style),
        GanttCell::Speculative(Stage::MEM) => ("MEM", spec_style),
        GanttCell::Speculative(Stage::WB) => ("WB", spec_style),
        GanttCell::SpeculativeFu(FuKind::Alu) => ("ALU", spec_style),
        GanttCell::SpeculativeFu(FuKind::Mul) => ("MUL", spec_style),
        GanttCell::SpeculativeFu(FuKind::Div) => ("DIV", spec_style),
        GanttCell::SpeculativeFu(FuKind::Fpu) => ("FPU", spec_style),
        GanttCell::SpeculativeFu(FuKind::Lsu) => ("LSU", spec_style),
        GanttCell::SpeculativeFu(FuKind::Sys) => ("SYS", spec_style),
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
