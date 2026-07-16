use std::cell::Cell;

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::app::{App, RunButton};
use crate::ui::theme;

/// Span row that tracks the terminal x-cursor as spans are pushed, so button
/// hitboxes can be recorded from real rendered widths instead of hand-counted
/// character offsets.
pub(crate) struct SpanRow {
    spans: Vec<Span<'static>>,
    x: u16,
    y: u16,
}

impl SpanRow {
    pub(crate) fn new(x: u16, y: u16) -> Self {
        Self {
            spans: Vec::new(),
            x,
            y,
        }
    }

    pub(crate) fn push(&mut self, span: Span<'static>) {
        self.x = self
            .x
            .saturating_add(UnicodeWidthStr::width(span.content.as_ref()) as u16);
        self.spans.push(span);
    }

    pub(crate) fn gap(&mut self, n: u16) {
        self.push(Span::raw(" ".repeat(n as usize)));
    }

    /// Current x-cursor; capture before pushing a button's spans and pass to
    /// [`SpanRow::record_hitbox`] afterwards.
    pub(crate) fn cursor(&self) -> u16 {
        self.x
    }

    pub(crate) fn record_hitbox(&self, start: u16, rect: &Cell<(u16, u16, u16)>) {
        rect.set((self.y, start, self.x));
    }

    pub(crate) fn into_line(self) -> Line<'static> {
        Line::from(self.spans)
    }
}

pub(crate) fn push_dense_pair(
    spans: &mut Vec<Span<'static>>,
    label: &str,
    value: &str,
    hovered: bool,
    active: bool,
    active_color: Color,
) {
    if !spans.is_empty() {
        spans.push(Span::raw("   "));
    }
    spans.push(Span::styled(
        label.to_string(),
        Style::default().fg(theme::IDLE),
    ));
    spans.push(Span::raw(" "));
    spans.push(dense_value(value, hovered, active, active_color));
}

pub(crate) fn dense_value(text: &str, hovered: bool, active: bool, color: Color) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    };
    Span::styled(text.to_string(), style)
}

/// "Execution" box shared by the Cache and Virtual Memory tabs: speed / state /
/// reset controls on line 1 (hitboxes recorded into the caller's cells, hover
/// driven by `app.hover_run_button`) and Cycles / CPI / Instrs on line 2.
pub(crate) fn render_exec_controls(
    f: &mut Frame,
    area: Rect,
    app: &App,
    speed_btn: &Cell<(u16, u16, u16)>,
    state_btn: &Cell<(u16, u16, u16)>,
    reset_btn: &Cell<(u16, u16, u16)>,
    hint: &str,
) {
    let speed_text = app.run.speed.label();

    let hover_reset = app.hover_run_button == Some(RunButton::Reset);
    let hover_speed = app.hover_run_button == Some(RunButton::Speed);
    let hover_state = app.hover_run_button == Some(RunButton::State);

    let (state_text, state_color) = if app.run.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };

    let (total, cpi, instr) = if let Some(pipeline) = app.aggregate_pipeline_snapshot() {
        let cycles = pipeline.cycles;
        let committed = pipeline.committed;
        let cpi = if committed > 0 {
            cycles as f64 / committed as f64
        } else {
            0.0
        };
        (cycles, cpi, committed)
    } else {
        (
            app.run.mem().total_program_cycles(),
            app.run.mem().overall_cpi(),
            app.run.mem().instruction_count,
        )
    };

    let mut spans = Vec::new();
    let inner_for_hits = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .inner(area);
    let line1_y = inner_for_hits.y;
    let mut x = inner_for_hits.x;
    let speed_x0 = x + "speed ".len() as u16;
    let speed_x1 = speed_x0 + speed_text.len() as u16;
    x = speed_x1 + 3;
    let state_x0 = x + "state ".len() as u16;
    let state_x1 = state_x0 + state_text.len() as u16;
    x = state_x1 + 3;
    let reset_x0 = x;
    let reset_x1 = reset_x0 + "reset".len() as u16;
    speed_btn.set((line1_y, speed_x0, speed_x1));
    state_btn.set((line1_y, state_x0, state_x1));
    reset_btn.set((line1_y, reset_x0, reset_x1));

    push_dense_pair(
        &mut spans,
        "speed",
        speed_text,
        hover_speed,
        true,
        theme::TEXT,
    );
    push_dense_pair(
        &mut spans,
        "state",
        state_text,
        hover_state,
        true,
        state_color,
    );
    spans.push(Span::raw("   "));
    spans.push(dense_action("reset", theme::DANGER, hover_reset));
    spans.push(Span::styled(
        hint.to_string(),
        Style::default().fg(theme::LABEL),
    ));
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::styled(
            format!(" Cycles:{total}"),
            Style::default().fg(theme::METRIC_CYC),
        ),
        Span::raw("  "),
        Span::styled(
            format!("CPI:{cpi:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        ),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(theme::LABEL)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .border_type(BorderType::Rounded)
        .title(Span::styled("Execution", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

pub(crate) fn dense_action(text: &str, color: Color, hovered: bool) -> Span<'static> {
    let style = if hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };
    Span::styled(text.to_string(), style)
}
