use crate::ui::app::App;
use crate::ui::pipeline::{
    BranchPredict, BranchResolve, InstrClass, PipelineBypassConfig, fu_latency_for_class,
};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, bool_value, dense_value, label_span};
use crate::ui::view::style;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

/// A pipeline config-row value: a boolean (rendered true/false green/red) or a
/// neutral selector/count string.
enum Val {
    Bool(bool),
    Text(String),
}

const CONFIG_CONTENT_W: u16 = 52;
const CONFIG_LABEL_W: usize = 18;
const LATENCY_LABEL_W: usize = 8;

pub fn render_pipeline_config(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.pipeline;

    let inner = render_panel(
        f,
        area,
        panel::panel(" Pipeline Settings ", PanelKind::Accent),
    );

    let content_width = inner.width.min(CONFIG_CONTENT_W);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(content_width),
            Constraint::Fill(1),
        ])
        .split(inner);
    let content = cols[1];

    // 7 config rows + spacer + 3 description lines + spacer + latency info
    let row_count = content.height.max(1) as usize;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); row_count])
        .split(content);

    let uf = |k: crate::ui::pipeline::FuKind| Val::Text(p.fu_capacity[k.index()].to_string());

    let rows_data: Vec<(usize, &str, Val)> = vec![
        (0, "EX->EX", Val::Bool(p.bypass.ex_to_ex)),
        (1, "MEM->EX", Val::Bool(p.bypass.mem_to_ex)),
        (2, "WB->ID", Val::Bool(p.bypass.wb_to_id)),
        (3, "Store->Load", Val::Bool(p.bypass.store_to_load)),
        (4, "Execution", Val::Text(p.mode.label().to_string())),
        (
            5,
            "Branch resolve",
            Val::Text(
                match p.branch_resolve {
                    BranchResolve::Id => "ID  (+1 flush)",
                    BranchResolve::Ex => "EX  (+2 flush)",
                    BranchResolve::Mem => "MEM (+3 flush)",
                }
                .to_string(),
            ),
        ),
        (
            6,
            "Branch predict",
            Val::Text(
                match p.predict {
                    BranchPredict::NotTaken => "Not-Taken",
                    BranchPredict::Taken => "Always-Taken",
                    BranchPredict::Btfnt => "BTFNT",
                    BranchPredict::TwoBit => "2-bit Dynamic",
                }
                .to_string(),
            ),
        ),
        (7, "ALU UFs", uf(crate::ui::pipeline::FuKind::Alu)),
        (8, "MUL UFs", uf(crate::ui::pipeline::FuKind::Mul)),
        (9, "DIV UFs", uf(crate::ui::pipeline::FuKind::Div)),
        (10, "FPU UFs", uf(crate::ui::pipeline::FuKind::Fpu)),
        (11, "LSU UFs", uf(crate::ui::pipeline::FuKind::Lsu)),
        (12, "SYS UFs", uf(crate::ui::pipeline::FuKind::Sys)),
    ];

    let mut rects = [(0u16, 0u16, 0u16); PipelineBypassConfig::CONFIG_ROWS];
    for (idx, label, val) in &rows_data {
        let highlight = p.config_cursor == *idx;
        let hovered = p.hover_config_row == Some(*idx);
        let state = ControlState::from(highlight, hovered);
        let value = match val {
            Val::Bool(b) => bool_value(*b, hovered),
            Val::Text(s) => dense_value(
                s,
                hovered,
                true,
                if highlight { theme::LABEL_Y } else { theme::TEXT },
            ),
        };
        let line_spans = vec![
            label_span(
                format!("{:<width$}", label, width = CONFIG_LABEL_W),
                state,
                theme::IDLE,
            ),
            Span::raw("  "),
            value,
        ];
        if rows.len() > *idx {
            let r = rows[*idx];
            f.render_widget(Paragraph::new(Line::from(line_spans)), r);
            if *idx < PipelineBypassConfig::CONFIG_ROWS {
                rects[*idx] = (r.y, r.x, r.x + r.width);
            }
        }
    }
    app.pipeline.config_row_rects.set(rects);

    let desc_row = p.config_cursor;
    let desc_lines = config_description_lines(desc_row);
    if rows.len() > 16 {
        for (i, line) in desc_lines.into_iter().enumerate() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(line, style::label()))),
                rows[15 + i],
            );
        }
    }

    // Latency info (read-only, derived from global CPI config)
    if rows.len() > 26 {
        let cpi = &app.run.cpi_config;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "--- EX latencies (from global CPI Config, Settings tab) ---",
                Style::default().fg(theme::BORDER),
            )))
            .alignment(Alignment::Center),
            rows[19],
        );
        let lat_pairs = [
            ("ALU", fu_latency_for_class(InstrClass::Alu, cpi)),
            ("MUL", fu_latency_for_class(InstrClass::Mul, cpi)),
            ("DIV", fu_latency_for_class(InstrClass::Div, cpi)),
            ("FPU", fu_latency_for_class(InstrClass::Fp, cpi)),
            ("Load", fu_latency_for_class(InstrClass::Load, cpi)),
            ("Store", fu_latency_for_class(InstrClass::Store, cpi)),
            ("SYS", fu_latency_for_class(InstrClass::System, cpi)),
        ];
        for (i, (name, lat)) in lat_pairs.iter().enumerate() {
            if rows.len() > 20 + i {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            format!("{:<width$}", name, width = LATENCY_LABEL_W),
                            style::label(),
                        ),
                        Span::raw("  "),
                        Span::styled(format!("{} cycle(s)", lat), style::value()),
                    ])),
                    rows[20 + i],
                );
            }
        }
    }
}

fn config_description_lines(row: usize) -> [&'static str; 3] {
    match row {
        0 => [
            "EX->EX forwards a result produced in EX directly into the next",
            "instruction's EX inputs. This removes many back-to-back RAW stalls,",
            "but it does not help loads whose data is only ready later.",
        ],
        1 => [
            "MEM->EX forwards values that become ready in MEM into a waiting EX",
            "consumer on the following cycle. This is the key path for late ALU",
            "results and for reducing load-use stalls when data is ready in MEM.",
        ],
        2 => [
            "WB->ID lets Decode observe a register value that is being written back",
            "in the same cycle. This avoids extra waiting when the consumer is",
            "still in ID while the producer has just reached WB.",
        ],
        3 => [
            "Store->Load lets a younger load read data from an older in-flight",
            "store to the same address instead of waiting for memory/cache state",
            "to be updated first.",
        ],
        4 => [
            "Serialized keeps execution behavior close to the current single-EX",
            "model. Parallel UFs allows independent instructions to overlap once",
            "the simulator can dispatch them safely without hazards.",
        ],
        5 => [
            "Branch resolve is where the branch/jump outcome becomes authoritative.",
            "Later stages are more realistic for deep pipelines but increase the",
            "mispredict penalty because more younger instructions must be flushed.",
        ],
        6 => [
            "Not-Taken and Always-Taken are fixed policies. BTFNT predicts",
            "backward branches taken and forward branches not taken. 2-bit Dynamic",
            "learns per-PC history and only flips after repeated misses.",
        ],
        7 | 8 | 9 | 10 | 11 | 12 => [
            "These counts control how many functional units of each type exist",
            "when the execution model is Parallel UFs. In Serialized mode they",
            "are kept in config but do not change execution behavior yet.",
        ],
        _ => [
            "Use Enter or click to change the selected pipeline option.",
            "The highlighted row explains which hazards or penalties the option",
            "changes in the model.",
        ],
    }
}
