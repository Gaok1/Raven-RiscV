use ratatui::{
    Frame,
    prelude::*,
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::get_steps;
use crate::ui::app::{App, DocsLang};
use crate::ui::theme;

pub fn render_tutorial_overlay(f: &mut Frame, term: Rect, app: &App) {
    let steps = get_steps(app.tutorial.tab);
    if steps.is_empty() {
        return;
    }
    let step = &steps[app.tutorial.step_idx];
    let total = steps.len();
    let idx = app.tutorial.step_idx;

    // Highlight target area — thin targets (≤3 rows, e.g. tab bar) already own a
    // border; coloring it yellow is handled by the caller (ui()) so we skip the
    // overlay here to avoid covering the element.
    let target = (step.target)(term, app);
    if let Some(t) = target {
        if t.height > 3 {
            let highlight = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Yellow));
            f.render_widget(highlight, t);
        }
    }

    let (title, body) = match app.tutorial.lang {
        DocsLang::En => (step.title_en, step.body_en),
        DocsLang::PtBr => (step.title_pt, step.body_pt),
    };

    // Compute popup size
    let max_w: u16 = 64.min(term.width.saturating_sub(2));
    let inner_w = max_w.saturating_sub(2) as usize;
    let body_lines = format_tutorial_lines(body, inner_w);
    let popup_h: u16 = (body_lines.len() as u16) + 6; // 2 border + 1 title + 1 blank + 1 nav + 1 blank
    let popup_h = popup_h.min(term.height.saturating_sub(2));
    let popup_w = max_w;

    // Position popup
    let popup_rect = tutorial_popup_rect(target, popup_w, popup_h, term);

    f.render_widget(Clear, popup_rect);

    let title_str = format!(" ▶ {} ", title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            title_str,
            Style::default().fg(Color::Yellow).bold(),
        ));

    let inner = block.inner(popup_rect);
    f.render_widget(block, popup_rect);

    // Body text
    let body_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    f.render_widget(
        Paragraph::new(Text::from(body_lines))
            .style(Style::default().fg(theme::TEXT)),
        body_area,
    );

    // Nav hint at bottom
    let nav_text = match app.tutorial.lang {
        DocsLang::En => format!(
            "  ← Prev  → Next  [L]=PT-BR  Esc=close  [{}/{}]",
            idx + 1,
            total
        ),
        DocsLang::PtBr => format!(
            "  ← Ant  → Próx  [L]=EN    Esc=fechar  [{}/{}]",
            idx + 1,
            total
        ),
    };
    let nav_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(nav_text).style(Style::default().fg(theme::LABEL)),
        nav_area,
    );
}

fn format_tutorial_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::raw(text.to_string())];
    }
    let hotkey_col_width = detect_hotkey_column_width(text, width);
    let mut lines = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            lines.push(Line::raw(String::new()));
            continue;
        }

        if let Some((lhs, rhs)) = line.split_once("::") {
            push_hotkey_lines(&mut lines, lhs.trim(), rhs.trim(), width, hotkey_col_width);
            continue;
        }

        if line.ends_with(':') {
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default().fg(theme::LABEL_Y).bold(),
            )]));
            continue;
        }

        wrap_styled_line(&mut lines, line, width, Style::default().fg(theme::TEXT));
    }
    lines
}

fn detect_hotkey_column_width(text: &str, width: usize) -> usize {
    let max_lhs = text
        .lines()
        .filter_map(|line| line.split_once("::").map(|(lhs, _)| lhs.trim()))
        .map(str_width)
        .max()
        .unwrap_or(0);
    max_lhs.min(width.saturating_div(2).max(12))
}

fn push_hotkey_lines(
    lines: &mut Vec<Line<'static>>,
    lhs: &str,
    rhs: &str,
    width: usize,
    hotkey_col_width: usize,
) {
    let gap = 2usize;
    let rhs_width = width.saturating_sub(hotkey_col_width + gap);
    let wrapped_rhs = wrap_words(rhs, rhs_width.max(12));
    let hotkey_style = Style::default().fg(theme::HOVER_FG).bg(theme::ACCENT).bold();

    for (idx, chunk) in wrapped_rhs.iter().enumerate() {
        if idx == 0 {
            let padded = format!("{lhs:<hotkey_col_width$}");
            lines.push(Line::from(vec![
                Span::styled(padded, hotkey_style),
                Span::raw("  "),
                Span::styled(chunk.clone(), Style::default().fg(theme::TEXT)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(hotkey_col_width)),
                Span::raw("  "),
                Span::styled(chunk.clone(), Style::default().fg(theme::TEXT)),
            ]));
        }
    }
}

fn wrap_styled_line(
    out: &mut Vec<Line<'static>>,
    text: &str,
    width: usize,
    base_style: Style,
) {
    let mut current_words: Vec<&str> = Vec::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = str_width(word);
        let next_width = if current_words.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };

        if !current_words.is_empty() && next_width > width {
            out.push(make_styled_line(&current_words.join(" "), base_style));
            current_words.clear();
            current_width = 0;
        }

        if !current_words.is_empty() {
            current_width += 1;
        }
        current_words.push(word);
        current_width += word_width;
    }

    if !current_words.is_empty() {
        out.push(make_styled_line(&current_words.join(" "), base_style));
    }
}

fn make_styled_line(text: &str, base_style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(end) = chars[i..].iter().position(|&c| c == ']') {
                let token: String = chars[i..=i + end].iter().collect();
                spans.push(Span::styled(
                    token,
                    Style::default().fg(theme::HOVER_FG).bg(theme::ACCENT).bold(),
                ));
                i += end + 1;
                continue;
            }
        }

        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                let token: String = chars[i + 1..i + 1 + end].iter().collect();
                spans.push(Span::styled(
                    token,
                    Style::default().fg(theme::LABEL_Y).bold(),
                ));
                i += end + 2;
                continue;
            }
        }

        let next_special = chars[i..]
            .iter()
            .position(|&c| c == '[' || c == '`')
            .map(|pos| i + pos)
            .unwrap_or(chars.len());
        let plain: String = chars[i..next_special].iter().collect();
        spans.push(Span::styled(plain, base_style));
        i = next_special;
    }

    Line::from(spans)
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = str_width(word);
        let next_width = if current.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };

        if !current.is_empty() && next_width > width {
            lines.push(current);
            current = String::new();
            current_width = 0;
        }

        if !current.is_empty() {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn str_width(text: &str) -> usize {
    text.chars().count()
}

/// Try positions: below → above → right → left → centered.
pub(crate) fn tutorial_popup_rect(target: Option<Rect>, pw: u16, ph: u16, term: Rect) -> Rect {
    let Some(t) = target else {
        return centered(pw, ph, term);
    };
    let gap = 1;
    let align_x = if t.width >= pw.saturating_add(16) {
        centered(pw, ph, term).x
    } else {
        clamp_x(t.x, pw, term)
    };

    // Below
    let below_y = t.y + t.height + gap;
    if below_y + ph <= term.y + term.height {
        return Rect::new(align_x, below_y, pw, ph);
    }

    // Above
    if t.y >= term.y + ph + gap {
        return Rect::new(align_x, t.y - ph - gap, pw, ph);
    }

    // Right
    let right_x = t.x + t.width + gap;
    if right_x + pw <= term.x + term.width {
        let y = clamp_y(t.y, ph, term);
        return Rect::new(right_x, y, pw, ph);
    }

    // Left
    if t.x >= term.x + pw + gap {
        let y = clamp_y(t.y, ph, term);
        return Rect::new(t.x - pw - gap, y, pw, ph);
    }

    // Centered fallback
    centered(pw, ph, term)
}

fn centered(pw: u16, ph: u16, term: Rect) -> Rect {
    Rect::new(
        term.x + term.width.saturating_sub(pw) / 2,
        term.y + term.height.saturating_sub(ph) / 2,
        pw,
        ph,
    )
}

fn clamp_x(preferred: u16, pw: u16, term: Rect) -> u16 {
    let max_x = (term.x + term.width).saturating_sub(pw);
    preferred.min(max_x).max(term.x)
}

fn clamp_y(preferred: u16, ph: u16, term: Rect) -> u16 {
    let max_y = (term.y + term.height).saturating_sub(ph);
    preferred.min(max_y).max(term.y)
}
