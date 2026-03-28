use crate::ui::app::App;
use crate::ui::pipeline::{
    BranchPredict, BranchResolve, InstrClass, PipelineMode, fu_latency_for_class,
};
use crate::ui::theme;
use crate::ui::view::components::dense_value;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_pipeline_config(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Pipeline Config ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 4 config rows + latency info + hint
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); 10])
        .split(inner);

    let bool_span = |v: bool| {
        if v {
            Span::styled(
                "on",
                Style::default()
                    .fg(theme::RUNNING)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("off", Style::default().fg(theme::PAUSED))
        }
    };

    let rows_data: Vec<(usize, &str, Vec<Span<'_>>)> = vec![
        (0, "Forwarding", vec![bool_span(p.forwarding)]),
        (
            1,
            "Mode",
            vec![Span::styled(
                match p.mode {
                    PipelineMode::SingleCycle => "Single-cycle",
                    PipelineMode::FunctionalUnits => "Func. Units",
                },
                Style::default().fg(theme::LABEL_Y),
            )],
        ),
        (
            2,
            "Branch resolve",
            vec![Span::styled(
                match p.branch_resolve {
                    BranchResolve::Id => "ID  (+1 flush)",
                    BranchResolve::Ex => "EX  (+2 flush)",
                    BranchResolve::Mem => "MEM (+3 flush)",
                },
                Style::default().fg(theme::LABEL_Y),
            )],
        ),
        (
            3,
            "Branch predict",
            vec![Span::styled(
                match p.predict {
                    BranchPredict::NotTaken => "Not-Taken",
                    BranchPredict::Taken => "Always-Taken",
                },
                Style::default().fg(theme::LABEL_Y),
            )],
        ),
    ];

    let mut rects = [(0u16, 0u16, 0u16); 4];
    for (idx, label, spans) in &rows_data {
        let highlight = p.config_cursor == *idx;
        let hovered = p.hover_config_row == Some(*idx);
        let label_style = if highlight {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::IDLE)
        };
        let mut line_spans = vec![Span::styled(format!("{:<22}", label), label_style)];
        for span in spans.iter().cloned() {
            let text = span.content.to_string();
            line_spans.push(dense_value(
                &text,
                hovered,
                true,
                if highlight {
                    theme::LABEL_Y
                } else {
                    theme::TEXT
                },
            ));
        }
        if rows.len() > *idx {
            let r = rows[*idx];
            f.render_widget(Paragraph::new(Line::from(line_spans)), r);
            if *idx < 4 {
                rects[*idx] = (r.y, r.x, r.x + r.width);
            }
        }
    }
    app.pipeline.config_row_rects.set(rects);

    // Latency info (read-only, derived from global CPI config)
    if rows.len() > 5 {
        let cpi = &app.run.cpi_config;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  ─── Latências EX (via CPI Config global, aba Settings) ───",
                Style::default().fg(theme::BORDER),
            ))),
            rows[5],
        );
        let lat_pairs = [
            ("ALU", fu_latency_for_class(InstrClass::Alu, cpi)),
            ("MUL", fu_latency_for_class(InstrClass::Mul, cpi)),
            ("DIV", fu_latency_for_class(InstrClass::Div, cpi)),
            ("FPU", fu_latency_for_class(InstrClass::Fp, cpi)),
            ("LSU", fu_latency_for_class(InstrClass::Load, cpi)),
        ];
        for (i, (name, lat)) in lat_pairs.iter().enumerate() {
            if rows.len() > 6 + i {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("  {:<6}", name), Style::default().fg(theme::LABEL)),
                        Span::styled(
                            format!("{} cycle(s)", lat),
                            Style::default().fg(theme::TEXT),
                        ),
                    ])),
                    rows[6 + i],
                );
            }
        }
    }
}
