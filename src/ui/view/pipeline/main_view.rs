use crate::ui::app::App;
use crate::ui::pipeline::sim::reg_name;
use crate::ui::pipeline::{
    GanttCell, HazardTrace, HazardType, InstrClass, PipelineMode, Stage, TraceKind,
    fu_latency_for_class,
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

pub fn render_pipeline_main(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    // Height of the stages row depends on mode
    let stages_h: u16 = if p.mode == PipelineMode::FunctionalUnits {
        9
    } else {
        6
    };
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
    let hazards_h: u16 = (2 + trace_rows + legend_rows + msg_rows)
        .min(area.height.saturating_sub(stages_h).saturating_sub(3))
        .clamp(4, 13);

    // Layout: stages | hazards (3) | gantt (rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(stages_h),
            Constraint::Length(hazards_h),
            Constraint::Min(4),
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

    // In FU mode, EX gets 3/7 of the width; others get 1/7 each
    let constraints: Vec<Constraint> = if p.mode == PipelineMode::FunctionalUnits {
        vec![
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(3, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
        ]
    } else {
        vec![Constraint::Ratio(1, 5); 5]
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, _stage) in Stage::all().iter().enumerate() {
        // EX in FU mode gets its own renderer
        if i == Stage::EX as usize && p.mode == PipelineMode::FunctionalUnits {
            render_fu_box(f, cols[i], app);
            continue;
        }

        let slot = p.stages[i].as_ref();
        let mut stage_badges = stage_status_badges(p, i, slot);

        let border_style = match slot {
            Some(s) if s.class == InstrClass::Unknown => Style::default().fg(Color::DarkGray),
            Some(s) if s.is_bubble => Style::default().fg(theme::PAUSED),
            Some(s) if s.hazard.is_some() => Style::default().fg(s.hazard.unwrap().color()),
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
                let label = match s.hazard {
                    Some(HazardType::BranchFlush) => "✕ squashed",
                    _ => "NOP",
                };
                vec![Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(theme::PAUSED)
                        .add_modifier(Modifier::BOLD),
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
                        format!(
                            " ⚠{}",
                            match h {
                                HazardType::Raw => "RAW",
                                HazardType::LoadUse => "LU",
                                HazardType::BranchFlush => "BR",
                                HazardType::FuBusy => "FU",
                                HazardType::MemLatency => "MEM",
                                HazardType::Waw => "WAW",
                                HazardType::War => "WAR",
                            }
                        ),
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
        if let Some(h) = s.hazard {
            push_badge(
                &mut tags,
                compact_stage_hazard_label(h).to_string(),
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
                    compact_stage_hazard_label(h).to_string(),
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

fn compact_stage_hazard_label(h: HazardType) -> &'static str {
    match h {
        HazardType::Raw => "RAW",
        HazardType::LoadUse => "LOAD",
        HazardType::BranchFlush => "CTRL",
        HazardType::FuBusy => "FU",
        HazardType::MemLatency => "MEM",
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

fn fu_matches(class: InstrClass, fu_class: InstrClass) -> bool {
    match fu_class {
        InstrClass::Load => matches!(class, InstrClass::Load | InstrClass::Store),
        other => class == other,
    }
}

fn render_fu_box(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;
    let ex_slot = p.stages[Stage::EX as usize].as_ref();

    // Borda colorida baseada no estado do EX slot
    let border_style = match ex_slot {
        Some(s) if s.is_bubble => Style::default().fg(theme::PAUSED),
        Some(s) if s.hazard.is_some() => Style::default().fg(s.hazard.unwrap().color()),
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
    let fu_defs: &[(&str, InstrClass, u8)] = &[
        (
            "ALU",
            InstrClass::Alu,
            fu_latency_for_class(InstrClass::Alu, cpi),
        ),
        (
            "MUL",
            InstrClass::Mul,
            fu_latency_for_class(InstrClass::Mul, cpi),
        ),
        (
            "DIV",
            InstrClass::Div,
            fu_latency_for_class(InstrClass::Div, cpi),
        ),
        (
            "FPU",
            InstrClass::Fp,
            fu_latency_for_class(InstrClass::Fp, cpi),
        ),
        (
            "LSU",
            InstrClass::Load,
            fu_latency_for_class(InstrClass::Load, cpi),
        ),
    ];

    // Cada FU ocupa uma linha dentro do box
    let row_constraints: Vec<Constraint> = fu_defs.iter().map(|_| Constraint::Length(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(fu_area);

    for (i, (fu_name, fu_class, latency)) in fu_defs.iter().enumerate() {
        if i >= rows.len() {
            break;
        }

        // Checar se o slot atual em EX usa esta UF
        let is_active = ex_slot
            .map(|s| !s.is_bubble && fu_matches(s.class, *fu_class))
            .unwrap_or(false);

        let line = if is_active {
            let s = ex_slot.unwrap();
            let total = *latency;
            let done = total.saturating_sub(s.fu_cycles_left);
            // Barra de progresso
            let bar_w = (rows[i].width as usize).saturating_sub(20).min(12).max(4);
            let filled = if total > 1 {
                ((done as usize) * bar_w / (total as usize)).min(bar_w)
            } else {
                bar_w
            };
            let bar: String = (0..bar_w)
                .map(|j| if j < filled { '█' } else { '░' })
                .collect();

            let w = rows[i].width as usize;
            let disasm_w = w.saturating_sub(20);
            let (disasm_trunc, _) = s.disasm.unicode_truncate(disasm_w);

            Line::from(vec![
                Span::styled(
                    format!("{:<3} ", fu_name),
                    Style::default()
                        .fg(s.class.color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<width$}", disasm_trunc, width = disasm_w),
                    Style::default().fg(theme::TEXT),
                ),
                Span::styled(
                    format!(" {}/{} ", done + 1, total),
                    Style::default().fg(theme::LABEL_Y),
                ),
                Span::styled(bar, Style::default().fg(theme::RUNNING)),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!("{:<3} ", fu_name),
                    Style::default().fg(theme::BORDER),
                ),
                Span::styled(
                    "—",
                    Style::default()
                        .fg(theme::BORDER)
                        .add_modifier(Modifier::DIM),
                ),
            ])
        };

        f.render_widget(Paragraph::new(line), rows[i]);
    }
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

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(7),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(inner);

    let kind_area = cols[0];
    let map_area = cols[1];
    let info_area = cols[2];

    let row_cap = map_area.height.saturating_sub(1) as usize;
    let mut kind_lines: Vec<Line<'_>> = vec![trace_kind_header(kind_area.width as usize)];
    let mut map_lines: Vec<Line<'_>> = vec![stage_trace_header(map_area.width as usize)];
    let mut info_lines: Vec<Line<'_>> = vec![trace_detail_header(info_area.width as usize)];

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
        let (trunc, _) = fallback.unicode_truncate(info_area.width.saturating_sub(2) as usize);
        info_lines.push(Line::from(Span::styled(
            format!("  {}", trunc),
            Style::default()
                .fg(theme::LABEL)
                .add_modifier(Modifier::DIM),
        )));
    } else {
        let mut rendered = 0usize;
        for (i, trace) in p.hazard_traces.iter().enumerate() {
            if rendered >= row_cap {
                break;
            }
            let detail = trace_detail_for(trace, &p.hazard_msgs);
            kind_lines.push(render_trace_kind_line(trace, kind_area.width as usize));
            map_lines.push(render_trace_map_line(trace, map_area.width as usize));
            info_lines.push(render_trace_detail_line(
                trace,
                &detail,
                info_area.width as usize,
            ));
            rendered += 1;
            if i + 1 == p.hazard_traces.len() && rendered < row_cap {
                kind_lines.push(Line::from(Span::styled(
                    "  i",
                    Style::default()
                        .fg(theme::LABEL)
                        .add_modifier(Modifier::DIM),
                )));
                map_lines.push(Line::from(Span::styled(" ", Style::default())));
                info_lines.push(render_trace_legend(info_area.width as usize));
            }
        }
    }

    f.render_widget(Paragraph::new(kind_lines), kind_area);
    f.render_widget(Paragraph::new(map_lines), map_area);
    f.render_widget(Paragraph::new(info_lines), info_area);
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

fn render_trace_detail_line(trace: &HazardTrace, detail: &str, width: usize) -> Line<'static> {
    let prefix = match trace.kind {
        TraceKind::Forward => "bypass  ",
        TraceKind::Hazard(HazardType::LoadUse) => "stall   ",
        TraceKind::Hazard(HazardType::BranchFlush) => "squash  ",
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
        TraceKind::Hazard(ht) => hazard_msgs
            .iter()
            .find(|(msg_ht, _)| *msg_ht == ht)
            .map(|(_, msg)| msg.clone())
            .unwrap_or_else(|| trace.detail.clone()),
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
    let visible_cols = ((preview_inner_w.saturating_sub(LABEL_W)) / CELL_W).max(1);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            format!(
                " HISTORY  last {} cycles ",
                visible_cols.min(crate::ui::pipeline::MAX_GANTT_COLS)
            ),
            Style::default().fg(theme::LABEL),
        ));

    let inner = block.inner(area);
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
    let max_rows = inner.height as usize;

    let visible_rows: Vec<_> = p
        .gantt
        .iter()
        .skip(p.gantt_scroll)
        .take(max_rows.saturating_sub(2))
        .collect();

    let start_cycle = visible_rows
        .iter()
        .map(|r| r.first_cycle)
        .min()
        .unwrap_or(p.cycle_count.saturating_sub(max_cols as u64));
    let end_cycle = start_cycle + max_cols as u64;

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

    let separator = format!("{}{}", "-".repeat(LABEL_W), "----".repeat(max_cols));

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(header_spans),
        Line::from(Span::styled(separator, Style::default().fg(theme::BORDER))),
    ];

    for row in visible_rows {
        let is_invalid = row.class == InstrClass::Unknown;
        let (label, _) = row.disasm.unicode_truncate(LABEL_W - 1);
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
            let cell_idx = c.saturating_sub(row.first_cycle) as usize;
            let cell = row.cells.get(cell_idx).copied().unwrap_or(GanttCell::Empty);
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
        GanttCell::InStage(Stage::MEM) => ("MM", Style::default().fg(theme::LABEL_Y)),
        GanttCell::InStage(Stage::WB) => ("WB", Style::default().fg(theme::ACCENT)),
        GanttCell::Speculative(Stage::IF) => ("IF", spec_style),
        GanttCell::Speculative(Stage::ID) => ("ID", spec_style),
        GanttCell::Speculative(Stage::EX) => ("EX", spec_style),
        GanttCell::Speculative(Stage::MEM) => ("MM", spec_style),
        GanttCell::Speculative(Stage::WB) => ("WB", spec_style),
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
