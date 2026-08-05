use crate::ui::app::App;
use crate::ui::pipeline::{
    BranchPredict, BranchResolve, CPI_ROWS, PIPELINE_CONFIG_ROWS, PipelineBypassConfig,
};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{
    ControlState, bool_value, dense_value, edit_value, label_span,
};
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

const CONFIG_LABEL_W: usize = 16;
const LATENCY_LABEL_W: usize = 8;

/// Where each config row sits in the grouped list: the section rule it follows,
/// its label, and how to read its value off the config.
///
/// The `usize` is the row's index in `config_cursor` / `config_row_rects` — the
/// keyboard walks that index, so the groups may re-order the *display* but never
/// the numbering.
const GROUPS: &[(&str, &[usize])] = &[
    ("FORWARDING", &[0, 1, 2, 3]),
    ("CONTROL", &[4, 5, 6]),
    ("FUNCTIONAL UNITS", &[7, 8, 9, 10, 11, 12]),
];

/// Left column width — the settings list. The rest of the panel goes to the
/// explanation, which is what the old layout could not fit: it centred a
/// 52-column strip in a 158-column panel and clipped every description line
/// mid-word ("forwards a result produced in EX directly int").
const SETTINGS_W: u16 = 40;

/// Right column width — wide enough for the longest CPI description and the
/// three-line explanations, and no wider, so the pair can be centred.
const EXPLAIN_W: u16 = 78;

/// A [`panel::section_rule`] set in from the panel border by a space, so the
/// rule reads as a label on the column rather than as a second frame edge.
fn section(name: &str, width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(panel::section_rule(name, width.saturating_sub(2)).spans);
    Line::from(spans)
}

pub fn render_pipeline_config(f: &mut Frame, area: Rect, app: &App) {
    let Some(p) = app.pipeline_config() else {
        f.render_widget(
            Paragraph::new("This backend exposes a fixed teaching pipeline; it has no tunable datapath settings.")
                .alignment(Alignment::Center)
                .style(style::label()),
            area,
        );
        return;
    };
    let view = app.run.pipeline_view();

    let inner = render_panel(f, area, panel::panel("Pipeline Settings", PanelKind::Accent));
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let uf = |k: crate::ui::pipeline::FuKind| Val::Text(p.fu_capacity[k.index()].to_string());
    let rows_data: Vec<(&str, Val)> = vec![
        ("EX→EX", Val::Bool(p.bypass.ex_to_ex)),
        ("MEM→EX", Val::Bool(p.bypass.mem_to_ex)),
        ("WB→ID", Val::Bool(p.bypass.wb_to_id)),
        ("Store→Load", Val::Bool(p.bypass.store_to_load)),
        ("Execution", Val::Text(p.mode.label().to_string())),
        (
            "Branch resolve",
            Val::Text(
                match p.branch_resolve {
                    BranchResolve::Id => "ID (+1 flush)",
                    BranchResolve::Ex => "EX (+2 flush)",
                    BranchResolve::Mem => "MEM (+3 flush)",
                }
                .to_string(),
            ),
        ),
        (
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
        ("ALU units", uf(crate::ui::pipeline::FuKind::Alu)),
        ("MUL units", uf(crate::ui::pipeline::FuKind::Mul)),
        ("DIV units", uf(crate::ui::pipeline::FuKind::Div)),
        ("FPU units", uf(crate::ui::pipeline::FuKind::Fpu)),
        ("LSU units", uf(crate::ui::pipeline::FuKind::Lsu)),
        ("SYS units", uf(crate::ui::pipeline::FuKind::Sys)),
    ];

    // Sized to its content and centred, so the page sits in the middle of the
    // panel instead of hugging the left border with the slack on the right.
    let settings_w = SETTINGS_W.min(inner.width);
    let explain_w = EXPLAIN_W.min(inner.width.saturating_sub(settings_w));
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(settings_w),
            Constraint::Length(explain_w),
            Constraint::Min(0),
        ])
        .split(inner);

    render_settings_list(f, cols[1], app, &rows_data);
    if cols[2].width >= 24 {
        render_explanation(f, cols[2], app, view.config_cursor, &rows_data);
    }
}

/// The left column: the thirteen settings under three section rules.
fn render_settings_list(f: &mut Frame, area: Rect, app: &App, rows_data: &[(&str, Val)]) {
    let view = app.run.pipeline_view();
    let mut rects = [(0u16, 0u16, 0u16); PIPELINE_CONFIG_ROWS];
    let mut lines: Vec<Line<'static>> = Vec::new();

    for (group, indices) in GROUPS {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        lines.push(section(group, area.width));
        for &idx in *indices {
            let Some((label, val)) = rows_data.get(idx) else {
                continue;
            };
            let selected = view.config_cursor == idx;
            let hovered = view.hover_config_row == Some(idx);
            let value = match val {
                Val::Bool(b) => bool_value(*b, hovered),
                Val::Text(s) => dense_value(
                    s,
                    hovered,
                    true,
                    if selected { theme::LABEL_Y } else { theme::TEXT },
                ),
            };
            // The row's screen y is where it lands in this Vec — one source for
            // both the render and the hit-test, so a regrouping cannot drift.
            let y = area.y + lines.len() as u16;
            if y < area.y + area.height {
                rects[idx] = (y, area.x, area.x + area.width);
            }
            lines.push(Line::from(vec![
                Span::raw(" "),
                label_span(
                    format!("{:<width$}", label, width = CONFIG_LABEL_W),
                    ControlState::from(selected, hovered),
                    theme::IDLE,
                ),
                value,
            ]));
        }
    }

    app.run.pipeline_view().config_row_rects.set(rects);
    f.render_widget(Paragraph::new(lines), area);
}

/// The right column: the per-class cycle costs, then what the selected setting
/// does.
///
/// The costs are editable here now. They used to live on the Settings tab and
/// this column could only echo them read-only, under a line telling the reader
/// which other tab to go and change them on — two tabs away from the datapath
/// whose timing they set.
fn render_explanation(
    f: &mut Frame,
    area: Rect,
    app: &App,
    cursor: usize,
    rows_data: &[(&str, Val)],
) {
    let view = app.run.pipeline_view();
    // Capped, not stretched to the column: a 116-column rule reads as a divider
    // across the page rather than as a heading over the paragraph below it.
    let rule_w = area.width.min(74);

    let mut lines = vec![section("CYCLES PER INSTRUCTION", rule_w)];
    let mut rects = view.config_row_rects.get();
    let names = crate::ui::app::CpiConfig::field_names();
    let descs = crate::ui::app::CpiConfig::descriptions();

    for i in 0..CPI_ROWS.min(names.len()) {
        let row = PipelineBypassConfig::CONFIG_ROWS + i;
        let selected = cursor == row;
        let hovered = view.hover_config_row == Some(row);
        let editing = selected.then(|| view.cpi_edit.as_deref()).flatten();

        let y = area.y + lines.len() as u16;
        if y < area.y + area.height {
            rects[row] = (y, area.x, area.x + area.width);
        }
        lines.push(Line::from(vec![
            Span::raw(" "),
            label_span(
                format!("{:<width$}", names[i], width = LATENCY_LABEL_W + 2),
                ControlState::from(selected, hovered),
                theme::IDLE,
            ),
            // Right-aligned in a fixed column: `0` and `19` are different
            // widths, and letting them set their own pushed the description
            // beside them one column left on every single-digit row.
            edit_value(
                &format!("{:>2}", app.session.cpi_config.get(i)),
                editing,
                hovered,
                if selected { theme::LABEL_Y } else { theme::TEXT },
            ),
            Span::styled(
                format!("  {}", descs.get(i).copied().unwrap_or("")),
                Style::default().fg(theme::BORDER),
            ),
        ]));
    }
    view.config_row_rects.set(rects);

    lines.push(Line::raw(""));
    let name = rows_data.get(cursor).map_or_else(
        || names.get(cursor - PipelineBypassConfig::CONFIG_ROWS).copied().unwrap_or(""),
        |(label, _)| *label,
    );
    lines.push(section(name, rule_w));
    lines.push(Line::raw(""));
    for text in config_description_lines(cursor) {
        lines.push(Line::from(Span::styled(
            format!("  {text}"),
            style::label(),
        )));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn config_description_lines(row: usize) -> [&'static str; 3] {
    match row {
        0 => [
            "EX→EX forwards a result produced in EX directly into the next",
            "instruction's EX inputs. This removes many back-to-back RAW stalls,",
            "but it does not help loads whose data is only ready later.",
        ],
        1 => [
            "MEM→EX forwards values that become ready in MEM into a waiting EX",
            "consumer on the following cycle. This is the key path for late ALU",
            "results and for reducing load-use stalls when data is ready in MEM.",
        ],
        2 => [
            "WB→ID lets Decode observe a register value that is being written back",
            "in the same cycle. This avoids extra waiting when the consumer is",
            "still in ID while the producer has just reached WB.",
        ],
        3 => [
            "Store→Load lets a younger load read data from an older in-flight",
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
