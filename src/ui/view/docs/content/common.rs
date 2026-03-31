use ratatui::prelude::*;

pub(super) fn h1(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        s,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ))
}

pub(super) fn h2(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        s,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

pub(super) fn kv(key: &'static str, val: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<12}"), Style::default().fg(Color::Yellow)),
        Span::styled(val, Style::default().fg(Color::White)),
    ])
}

pub(super) fn note(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {s}"),
        Style::default().fg(Color::DarkGray),
    ))
}

pub(super) fn blank() -> Line<'static> {
    Line::raw("")
}

pub(super) fn raw(s: &'static str) -> Line<'static> {
    Line::raw(s)
}

pub(super) fn mono(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        s,
        Style::default().fg(Color::Rgb(180, 180, 200)),
    ))
}

pub(super) fn trow(
    a7: &'static str,
    name: &'static str,
    args: &'static str,
    ret: &'static str,
    notes: &'static str,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {a7:<6}"), Style::default().fg(Color::LightGreen)),
        Span::styled(
            format!("{name:<16}"),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{args:<28}"), Style::default().fg(Color::White)),
        Span::styled(format!("{ret:<10}"), Style::default().fg(Color::Yellow)),
        Span::styled(notes, Style::default().fg(Color::DarkGray)),
    ])
}

pub(super) fn thead() -> Line<'static> {
    Line::from(vec![Span::styled(
        "  a7    Name            Arguments                    Return    Notes",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )])
}

pub(super) fn tsep() -> Line<'static> {
    Line::styled(
        "  ──────────────────────────────────────────────────────────────────────",
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )
}
