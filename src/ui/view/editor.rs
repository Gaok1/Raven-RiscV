use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use std::cmp::min;

use super::{App, Editor, EditorMode};

pub(super) fn render_editor_status(f: &mut Frame, area: Rect, app: &App) {
    let (mode_text, mode_color) = match app.mode {
        EditorMode::Insert => ("INSERT", Color::Green),
        EditorMode::Command => ("COMMAND", Color::Blue),
    };
    let mode = Line::from(vec![
        Span::raw("Mode: "),
        Span::styled(mode_text, Style::default().fg(mode_color)),
    ]);

    let compile_span = if let Some(msg) = &app.last_assemble_msg {
        let color = if app.last_compile_ok == Some(true) {
            Color::Green
        } else {
            Color::Red
        };
        // Editor: only colored text, neutral background
        Span::styled(msg.clone(), Style::default().fg(color))
    } else {
        Span::raw("Not compiled")
    };
    let build = Line::from(vec![Span::raw("Build: "), compile_span]);

    let commands = Line::from(
        "Commands: Esc=Command  |  Ctrl+R=Assemble  |  Ctrl+O=Import  |  Ctrl+S=Export",
    );

    let para = Paragraph::new(vec![mode, build, commands]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .border_type(BorderType::Rounded)
            .title("Editor Status"),
    );
    f.render_widget(para, area);
}

pub(super) fn render_editor(f: &mut Frame, area: Rect, app: &App) {
    fn apply_selection(line: &mut Line, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let mut char_pos = 0;
        let mut new_spans = Vec::new();
        for span in line.spans.drain(..) {
            let content = span.content.to_string();
            let len = Editor::char_count(&content);
            let span_start = char_pos;
            let span_end = char_pos + len;
            if span_end <= start || span_start >= end {
                new_spans.push(Span::styled(content, span.style));
            } else {
                if span_start < start {
                    let byte = Editor::byte_at(&content, start - span_start);
                    new_spans.push(Span::styled(content[..byte].to_string(), span.style));
                }
                let sel_from = start.max(span_start);
                let sel_to = end.min(span_end);
                let byte_start = Editor::byte_at(&content, sel_from - span_start);
                let byte_end = Editor::byte_at(&content, sel_to - span_start);
                let mut sel_style = span.style;
                sel_style = sel_style.bg(Color::Blue);
                new_spans.push(Span::styled(
                    content[byte_start..byte_end].to_string(),
                    sel_style,
                ));
                if span_end > end {
                    let byte = Editor::byte_at(&content, end - span_start);
                    new_spans.push(Span::styled(content[byte..].to_string(), span.style));
                }
            }
            char_pos += len;
        }
        line.spans = new_spans;
    }

    let visible_h = area.height.saturating_sub(2) as usize;
    let len = app.editor.lines.len();
    let mut start = 0usize;
    if len > visible_h {
        if app.editor.cursor_row <= visible_h / 2 {
            start = 0;
        } else if app.editor.cursor_row >= len.saturating_sub(visible_h / 2) {
            start = len.saturating_sub(visible_h);
        } else {
            start = app.editor.cursor_row - visible_h / 2;
        }
    }
    let end = min(len, start + visible_h);

    let num_width = end.to_string().len();
    let mut rows: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let line_str = &app.editor.lines[i];
        let mut line = Line::from(highlight_line(line_str));
        if let Some(((sr, sc), (er, ec))) = app.editor.selection_range() {
            if i >= sr && i <= er {
                let (sel_start, sel_end) = if sr == er {
                    (sc, ec)
                } else if i == sr {
                    (sc, Editor::char_count(line_str))
                } else if i == er {
                    (0, ec)
                } else {
                    (0, Editor::char_count(line_str))
                };
                apply_selection(&mut line, sel_start, sel_end);
            }
        }
        if Some(i) == app.diag_line {
            line = line.style(
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::UNDERLINED),
            );
        }
        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!("{:>width$}", i + 1, width = num_width),
            Style::default().fg(Color::DarkGray),
        ));
        let marker_style = if Some(i) == app.diag_line {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(" │ ", marker_style));
        spans.extend(line.spans);
        rows.push(Line::from(spans));
    }
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Editor (RISC-V ASM) - Esc: Command, i: Insert, Ctrl+R: Assemble");
    if let Some(ok) = app.last_compile_ok {
        let (txt, color) = if ok {
            ("[OK]", Color::Green)
        } else {
            ("[ERROR]", Color::Red)
        };
        let flag = Line::styled(txt, Style::default().fg(color)).right_aligned();
        block = block.title(flag);
    }
    let para = Paragraph::new(rows).block(block);

    f.render_widget(para, area);

    let cur_row = app.editor.cursor_row as u16;
    let cur_col = app.editor.cursor_col as u16;
    let gutter = (num_width + 3) as u16;
    let cursor_x = area.x + 1 + gutter + cur_col;
    let cursor_y = area.y + 1 + (cur_row - start as u16);
    if cursor_y < area.y + area.height && cursor_x < area.x + area.width {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn highlight_line(s: &str) -> Vec<Span<'_>> {
    use Color::*;
    if s.is_empty() {
        return vec![Span::raw("")];
    }

    let mut out = Vec::new();

    let mut lead_len = 0usize;
    for ch in s.chars() {
        if ch.is_whitespace() {
            lead_len += ch.len_utf8();
        } else {
            break;
        }
    }
    if lead_len > 0 {
        out.push(Span::raw(&s[..lead_len]));
    }
    let trimmed = &s[lead_len..];

    let first_end = trimmed
        .char_indices()
        .find(|&(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(trimmed.len());

    let first = &trimmed[..first_end];
    let rest = &trimmed[first_end..];

    if first.ends_with(':') {
        out.push(Span::styled(first, Style::default().fg(Yellow)));
        if !rest.is_empty() {
            out.push(Span::raw(rest));
        }
        return out;
    }

    out.push(Span::styled(
        first,
        Style::default().fg(Cyan).add_modifier(Modifier::BOLD),
    ));

    let mut token = String::new();
    for ch in rest.chars() {
        if ",()\t ".contains(ch) {
            if !token.is_empty() {
                out.push(color_operand(&token));
                token.clear();
            }
            out.push(Span::raw(ch.to_string()));
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        out.push(color_operand(&token));
    }

    out
}

fn color_operand(tok: &str) -> Span<'static> {
    use Color::*;
    let is_xreg = tok.starts_with('x') && tok[1..].chars().all(|c| c.is_ascii_digit());
    let is_alias = matches!(
        tok,
        "zero"
            | "ra"
            | "sp"
            | "gp"
            | "tp"
            | "s0"
            | "fp"
            | "s1"
            | "s2"
            | "s3"
            | "s4"
            | "s5"
            | "s6"
            | "s7"
            | "s8"
            | "s9"
            | "s10"
            | "s11"
            | "t0"
            | "t1"
            | "t2"
            | "t3"
            | "t4"
            | "t5"
            | "t6"
            | "a0"
            | "a1"
            | "a2"
            | "a3"
            | "a4"
            | "a5"
            | "a6"
            | "a7"
    );
    let is_imm = tok.starts_with("0x") || tok.parse::<i32>().is_ok();
    let style = if is_xreg || is_alias {
        Style::default().fg(Green)
    } else if is_imm {
        Style::default().fg(Magenta)
    } else {
        Style::default()
    };
    Span::styled(tok.to_string(), style)
}
