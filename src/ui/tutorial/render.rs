use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
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
    let body_lines = wrap_text(body, inner_w);
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
        Paragraph::new(body)
            .style(Style::default().fg(theme::TEXT))
            .wrap(Wrap { trim: false }),
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

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current = word.to_string();
            } else if current.len() + 1 + word.len() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current.clone());
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
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
