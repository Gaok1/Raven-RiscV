use crate::ui::theme;
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

fn labeled_value_concept<'a>(
    items: &[(&'a str, &'a str, Color, bool)],
    gap: &'a str,
) -> Vec<Line<'a>> {
    let mut top: Vec<Span<'a>> = Vec::new();
    let mut bottom: Vec<Span<'a>> = Vec::new();

    for (idx, (label, value, color, active)) in items.iter().enumerate() {
        let label_text = format!("{label} ");
        top.push(Span::styled(
            label_text.clone(),
            Style::default().fg(theme::LABEL),
        ));
        top.push(Span::styled(
            *value,
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        ));

        bottom.push(Span::raw(" ".repeat(label_text.chars().count())));
        bottom.push(Span::styled(
            if *active {
                "─".repeat(value.chars().count())
            } else {
                " ".repeat(value.chars().count())
            },
            Style::default().fg(theme::ACCENT),
        ));

        if idx + 1 < items.len() {
            top.push(Span::raw(gap));
            bottom.push(Span::raw(" ".repeat(gap.chars().count())));
        }
    }

    vec![Line::from(top), Line::from(bottom)]
}

fn concept_block<'a>(
    title: &'a str,
    sample: Vec<Line<'a>>,
    note: &'a str,
    accent: Color,
) -> Paragraph<'a> {
    Paragraph::new({
        let mut lines = vec![Line::styled(
            title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )];
        lines.push(Line::raw(""));
        lines.extend(sample);
        lines.push(Line::raw(""));
        lines.push(Line::styled(note, Style::default().fg(theme::LABEL)));
        lines
    })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::BG_PANEL)),
    )
}

pub fn render_splash(
    f: &mut Frame,
    _started: std::time::Instant,
    _duration_secs: f64,
    _mem_size: usize,
) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .margin(2)
        .split(area);

    let title = vec![
        Line::styled(
            "Button UI Study",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "Temporary visual proof of concept for Run / Cache / Pipeline controls",
            Style::default().fg(theme::LABEL),
        ),
        Line::styled(
            "fixed preview screen  ·  mouse disabled here on purpose",
            Style::default().fg(theme::IDLE),
        ),
    ];
    f.render_widget(Paragraph::new(title), outer[0]);

    let concept_a = concept_block(
        "Concept A · Quiet labels + active values",
        labeled_value_concept(
            &[
                ("Core", "0/3", theme::TEXT, true),
                ("View", "REGS", theme::TEXT, true),
                ("Format", "HEX", theme::TEXT, false),
                ("State", "PAUSE", theme::PAUSED, true),
            ],
            "    ",
        ),
        "Labels stay quiet. Only the chosen value gets emphasis and underline.",
        theme::ACCENT,
    );

    let concept_b = concept_block(
        "Concept B · Rail separators",
        vec![
            Line::from(vec![
                Span::styled("Core 0/3", Style::default().fg(theme::TEXT)),
                Span::styled("  │  ", Style::default().fg(theme::BORDER)),
                Span::styled("View REGS", Style::default().fg(theme::TEXT)),
                Span::styled("  │  ", Style::default().fg(theme::BORDER)),
                Span::styled("Format HEX", Style::default().fg(theme::TEXT)),
                Span::styled("  │  ", Style::default().fg(theme::BORDER)),
                Span::styled("Speed 1x", Style::default().fg(theme::TEXT)),
                Span::styled("  │  ", Style::default().fg(theme::BORDER)),
                Span::styled(
                    "State RUN",
                    Style::default()
                        .fg(theme::RUNNING)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                "                     hover on value only",
                Style::default().fg(theme::LABEL),
            )]),
        ],
        "More panel-like. Good if you want a cleaner instrumentation feel.",
        theme::METRIC_CYC,
    );

    let concept_c = concept_block(
        "Concept C · Split semantic pills only where it matters",
        vec![
            Line::from(vec![
                Span::styled("Core", Style::default().fg(theme::LABEL)),
                Span::raw("  "),
                Span::styled("0/3", Style::default().fg(theme::TEXT)),
                Span::raw("    "),
                Span::styled("Speed", Style::default().fg(theme::LABEL)),
                Span::raw("  "),
                Span::styled("1x", Style::default().fg(theme::TEXT)),
                Span::raw("    "),
                Span::styled("State", Style::default().fg(theme::LABEL)),
                Span::raw("  "),
                Span::styled(
                    " RUN ",
                    Style::default()
                        .fg(Color::Rgb(0, 0, 0))
                        .bg(theme::RUNNING)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("    "),
                Span::styled(
                    " Reset ",
                    Style::default()
                        .fg(theme::DANGER)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                "Only semantic actions become pills; the rest stays editorial.",
                Style::default().fg(theme::LABEL),
            )]),
        ],
        "Keeps the bar light, while RUN / PAUSE / FAULT still pop immediately.",
        theme::RUNNING,
    );

    let concept_d = concept_block(
        "Concept D · Dense technical strip",
        vec![
            Line::from(vec![
                Span::styled("core", Style::default().fg(theme::IDLE)),
                Span::raw(" "),
                Span::styled("0/3", Style::default().fg(theme::TEXT)),
                Span::raw("   "),
                Span::styled("view", Style::default().fg(theme::IDLE)),
                Span::raw(" "),
                Span::styled("regs", Style::default().fg(theme::TEXT)),
                Span::raw("   "),
                Span::styled("fmt", Style::default().fg(theme::IDLE)),
                Span::raw(" "),
                Span::styled("hex", Style::default().fg(theme::TEXT)),
                Span::raw("   "),
                Span::styled("state", Style::default().fg(theme::IDLE)),
                Span::raw(" "),
                Span::styled(
                    "pause",
                    Style::default()
                        .fg(theme::PAUSED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled("reset", Style::default().fg(theme::DANGER)),
            ]),
            Line::from(vec![Span::styled(
                "Minimal and tool-like; strongest fit if you want a stricter lab instrument look.",
                Style::default().fg(theme::LABEL),
            )]),
        ],
        "Lowercase / dense / compact. Probably the most bare-metal option.",
        theme::PAUSED,
    );

    f.render_widget(concept_a, outer[1]);
    f.render_widget(concept_b, outer[2]);
    f.render_widget(concept_c, outer[3]);
    f.render_widget(concept_d, outer[4]);

    let footer = Rect::new(outer[5].x, outer[5].y, outer[5].width, 1);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("preview locked ", Style::default().fg(theme::LABEL)),
            Span::styled(
                "SHOWCASE",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  ·  close and tell me which concept to keep",
                Style::default().fg(theme::IDLE),
            ),
        ])),
        footer,
    );
}
