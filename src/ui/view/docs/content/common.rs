use ratatui::prelude::*;

const COL_A7_W: usize = 6;
const COL_NAME_W: usize = 16;
const COL_ARGS_W: usize = 28;
const COL_RET_W: usize = 10;
const COL_NOTES_W: usize = 30;

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

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() || width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let next_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };

        if next_len > width && !current.is_empty() {
            lines.push(current);
            current = String::new();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if current.is_empty() {
        lines.push(String::new());
    } else {
        lines.push(current);
    }

    lines
}

pub(super) fn trow_wrapped(
    a7: &'static str,
    name: &'static str,
    args: &'static str,
    ret: &'static str,
    notes: &'static str,
) -> Vec<Line<'static>> {
    let notes_lines = wrap_words(notes, COL_NOTES_W);
    let mut out = Vec::with_capacity(notes_lines.len());

    for (idx, note_line) in notes_lines.into_iter().enumerate() {
        let (a7_cell, name_cell, args_cell, ret_cell) = if idx == 0 {
            (
                format!("  {a7:<COL_A7_W$}"),
                format!("{name:<COL_NAME_W$}"),
                format!("{args:<COL_ARGS_W$}"),
                format!("{ret:<COL_RET_W$}"),
            )
        } else {
            (
                format!("  {:<COL_A7_W$}", ""),
                format!("{:<COL_NAME_W$}", ""),
                format!("{:<COL_ARGS_W$}", ""),
                format!("{:<COL_RET_W$}", ""),
            )
        };

        out.push(Line::from(vec![
            Span::styled(a7_cell, Style::default().fg(Color::LightGreen)),
            Span::styled(
                name_cell,
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(args_cell, Style::default().fg(Color::White)),
            Span::styled(ret_cell, Style::default().fg(Color::Yellow)),
            Span::styled(note_line, Style::default().fg(Color::DarkGray)),
        ]));
    }

    out
}

pub(super) fn thead() -> Line<'static> {
    Line::from(vec![Span::styled(
        format!(
            "  {:<COL_A7_W$}{:<COL_NAME_W$}{:<COL_ARGS_W$}{:<COL_RET_W$}Notes",
            "a7", "Name", "Arguments", "Return"
        ),
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
