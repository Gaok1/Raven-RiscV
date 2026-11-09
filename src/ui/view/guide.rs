// ui/view/guide.rs — the help guide overlay.
//
//   ╭ Guide · Virtual Memory ───────────────────────────────────────────────╮
//   │   1  Start here          │ How translation works                     ↑│
//   │ ▌ 2  How translation …   │ ─────────────────────                     █│
//   │   3  walk: one transl…   │                                           █│
//   │   4  tlb: the TLB …      │ Raven uses Sv32, the paging scheme RISC-V ║│
//   │   …                      │ defines for 32-bit machines. Memory is …  ║│
//   ╰ ↑↓ scroll  ←→ section  t guided tour  l português  Esc close ──────────╯
//
// The text is laid out for the width it gets, every frame: paragraphs wrap,
// lists and two-column rows hang their continuation lines under their own
// text. The input handlers read the layout back (section starts, line count,
// rectangles) from `GuideState`, so a click and a key agree with the screen.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::app::{App, DocsLang};
use crate::ui::guide::{self, Block, Section};
use crate::ui::theme;
use crate::ui::view::components::lists::vertical_scrollbar;
use crate::ui::view::components::overlay::{self, OverlayStyle};
use crate::ui::view::style;

/// Columns of the contents list, its separator included.
const CONTENTS_W: u16 = 32;
/// Below this inner width the contents list gives its room to the text.
const MIN_SPLIT_W: u16 = 76;
/// The widest the overlay gets: lines much longer than this are hard to read.
const MAX_W: u16 = 112;
/// The widest a two-column row's first column gets.
const MAX_KEY_W: usize = 24;

/// Where the guide sits on a screen of `area`.
pub(crate) fn guide_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(MAX_W);
    let height = area.height.saturating_sub(2);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub(crate) fn render_guide(f: &mut Frame, area: Rect, app: &App) {
    let state = &app.guide;
    let lang = app.tutorial.lang;
    let Some(sections) = guide::guide(app.tab, lang) else {
        return;
    };
    let popup = guide_rect(area);
    state.popup.set(popup);

    let title = match lang {
        DocsLang::En => format!(" Guide · {} ", app.tab.label()),
        DocsLang::PtBr => format!(" Guia · {} ", app.tab.label()),
    };
    let inner = overlay::overlay(
        f,
        popup,
        OverlayStyle {
            border: theme::ACCENT,
            title: Span::styled(title, style::title()),
            bottom: Some(hints(lang)),
        },
    );
    if inner.width < 20 || inner.height < 3 {
        return;
    }

    let (contents, body) = if inner.width >= MIN_SPLIT_W {
        (
            Rect::new(inner.x, inner.y, CONTENTS_W - 1, inner.height),
            Rect::new(
                inner.x + CONTENTS_W,
                inner.y,
                inner.width - CONTENTS_W,
                inner.height,
            ),
        )
    } else {
        (Rect::default(), inner)
    };
    state.contents.set(contents);
    state.body.set(body);

    // One column of margin on the left, the scrollbar on the right, and one
    // column between the two so the text never touches the bar.
    let text_w = usize::from(body.width.saturating_sub(3));
    let (lines, anchors) = layout(sections, text_w);
    let viewport = usize::from(body.height);
    state.lines.set(lines.len());
    state.viewport.set(viewport);
    state.text_width.set(text_w);
    *state.anchors.borrow_mut() = anchors;

    // The layout can shrink under the reader (a wider window, the other
    // language): clamp here rather than wait for the next key.
    let scroll = state.scroll.min(state.max_scroll());
    let end = (scroll + viewport).min(lines.len());
    f.render_widget(
        Paragraph::new(lines[scroll..end].to_vec()),
        Rect::new(body.x + 1, body.y, body.width.saturating_sub(2), body.height),
    );
    vertical_scrollbar(f, body, lines.len(), viewport, scroll);

    if contents.width > 0 {
        render_contents(f, contents, app, sections, scroll);
        let rule = Style::default().fg(theme::BORDER);
        let column: Vec<Line> = (0..inner.height).map(|_| Line::styled("│", rule)).collect();
        f.render_widget(
            Paragraph::new(column),
            Rect::new(inner.x + CONTENTS_W - 1, inner.y, 1, inner.height),
        );
    }
}

/// The section list: the one on screen marked, the one under the pointer lit.
fn render_contents(f: &mut Frame, area: Rect, app: &App, sections: &[Section], scroll: usize) {
    let state = &app.guide;
    let current = state.section_at(scroll);
    let width = usize::from(area.width);
    let lines: Vec<Line> = sections
        .iter()
        .enumerate()
        .take(usize::from(area.height))
        .map(|(i, section)| {
            let on = i == current;
            let marker = if on { "▌" } else { " " };
            let label = format!("{:>2}  {}", i + 1, section.title);
            let label = fit(&label, width.saturating_sub(2));
            let mut text = if on {
                Style::default().fg(theme::ACCENT).bold()
            } else {
                Style::default().fg(theme::TEXT)
            };
            if state.hover == Some(i) {
                text = text.bg(theme::HOVER_ROW_BG);
            }
            Line::from(vec![
                Span::styled(marker, Style::default().fg(theme::ACCENT)),
                Span::styled(format!("{label:<w$}", w = width.saturating_sub(1)), text),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}

/// The keys the guide answers to, along its bottom edge.
fn hints(lang: DocsLang) -> Line<'static> {
    let pairs: &[(&str, &str)] = match lang {
        DocsLang::En => &[
            ("↑↓", "scroll"),
            ("←→", "section"),
            ("t", "guided tour"),
            ("l", "português"),
            ("Esc", "close"),
        ],
        DocsLang::PtBr => &[
            ("↑↓", "rolar"),
            ("←→", "seção"),
            ("t", "tour guiado"),
            ("l", "english"),
            ("Esc", "fechar"),
        ],
    };
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in pairs {
        spans.push(Span::styled(*key, style::key()));
        spans.push(Span::styled(format!(" {what}  "), style::label()));
    }
    Line::from(spans)
}

/// Cut `text` to `width` columns, with an ellipsis when it did not fit.
fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        if out.width() + 2 > width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

// ── Layout ───────────────────────────────────────────────────────────────────

/// The guide as lines `width` columns wide, and the line each section starts
/// on.
pub(crate) fn layout(sections: &[Section], width: usize) -> (Vec<Line<'static>>, Vec<usize>) {
    let width = width.max(20);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut anchors = Vec::with_capacity(sections.len());
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            gap(&mut lines);
            lines.push(Line::raw(""));
        }
        anchors.push(lines.len());
        lines.push(Line::styled(
            section.title.to_string(),
            Style::default().fg(theme::ACCENT).bold(),
        ));
        lines.push(Line::styled(
            "─".repeat(section.title.width()),
            Style::default().fg(theme::ACCENT),
        ));
        lines.push(Line::raw(""));
        for block in section.blocks {
            push_block(&mut lines, block, width);
        }
    }
    // The text ends on its last word, not on a blank.
    while lines.last().is_some_and(|line| line.width() == 0) {
        lines.pop();
    }
    (lines, anchors)
}

/// A blank line, unless the text already ends in one.
fn gap(lines: &mut Vec<Line<'static>>) {
    if lines.last().is_some_and(|line| line.width() > 0) {
        lines.push(Line::raw(""));
    }
}

fn push_block(lines: &mut Vec<Line<'static>>, block: &Block, width: usize) {
    let text = Style::default().fg(theme::TEXT);
    match block {
        Block::H(heading) => {
            gap(lines);
            lines.push(Line::styled(
                heading.to_string(),
                Style::default().fg(theme::TEXT).bold(),
            ));
        }
        Block::P(paragraph) => {
            lines.extend(wrap(inline(paragraph, text), width, Vec::new(), 0));
            gap(lines);
        }
        Block::List(items) => {
            for item in *items {
                let bullet = vec![Span::styled(" • ", Style::default().fg(theme::ACCENT))];
                lines.extend(wrap(inline(item, text), width, bullet, 3));
            }
            gap(lines);
        }
        Block::Keys(rows) => {
            let widest = rows.iter().map(|(key, _)| key.width()).max().unwrap_or(0);
            let key_w = (widest + 3).min(MAX_KEY_W).min(width / 3);
            let key_style = Style::default().fg(theme::LABEL_Y).bold();
            for (key, meaning) in *rows {
                let first = if key.width() + 2 <= key_w {
                    vec![Span::styled(format!(" {key:<w$}", w = key_w - 1), key_style)]
                } else {
                    // Too long for the column: on a line of its own.
                    lines.push(Line::from(Span::styled(format!(" {key}"), key_style)));
                    vec![Span::raw(" ".repeat(key_w))]
                };
                lines.extend(wrap(inline(meaning, text), width, first, key_w));
            }
            gap(lines);
        }
        Block::Pre(art) => {
            for line in art.lines() {
                lines.push(Line::styled(format!("  {line}"), text));
            }
            gap(lines);
        }
    }
}

/// Split `source` at its marks: `**term**` and `` `key` ``.
fn inline(source: &str, base: Style) -> Vec<(String, Style)> {
    let term = Style::default().fg(theme::LABEL_Y).bold();
    let key = style::key();
    let (mut in_term, mut in_key) = (false, false);
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = source.chars().peekable();
    let style_now = |in_term: bool, in_key: bool| {
        if in_key {
            key
        } else if in_term {
            term
        } else {
            base
        }
    };
    while let Some(ch) = chars.next() {
        let toggle = match ch {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                Some(true)
            }
            '`' => Some(false),
            _ => None,
        };
        match toggle {
            Some(is_term) => {
                if !buf.is_empty() {
                    out.push((std::mem::take(&mut buf), style_now(in_term, in_key)));
                }
                if is_term {
                    in_term = !in_term;
                } else {
                    in_key = !in_key;
                }
            }
            // A key or value stays on one line: `L1 +1` must not wrap.
            None if ch == ' ' && in_key => buf.push('\u{a0}'),
            None => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        out.push((buf, style_now(in_term, in_key)));
    }
    out
}

/// A run of text with no space in it, possibly in several styles (`` `v`. ``
/// is a key and a full stop): wrapping never splits one.
enum Unit {
    Word(Vec<(String, Style)>),
    Space(Style),
}

fn units(segments: Vec<(String, Style)>) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut word: Vec<(String, Style)> = Vec::new();
    for (text, style) in segments {
        let mut piece = String::new();
        for ch in text.chars() {
            if ch == ' ' {
                if !piece.is_empty() {
                    word.push((std::mem::take(&mut piece), style));
                }
                if !word.is_empty() {
                    out.push(Unit::Word(std::mem::take(&mut word)));
                }
                out.push(Unit::Space(style));
            } else {
                piece.push(ch);
            }
        }
        if !piece.is_empty() {
            word.push((piece, style));
        }
    }
    if !word.is_empty() {
        out.push(Unit::Word(word));
    }
    out
}

/// A paragraph of guide text — `**term**` and `` `key` `` marks included —
/// wrapped to `width`, for a screen that wants to say a few lines in the
/// guide's voice.
pub(crate) fn paragraph(source: &str, width: usize, indent: usize) -> Vec<Line<'static>> {
    let first = vec![Span::raw(" ".repeat(indent))];
    wrap(
        inline(source, Style::default().fg(theme::TEXT)),
        width,
        first,
        indent,
    )
}

/// Word-wrap `segments` to `width`, the first line opening with `first` and
/// the rest indented by `indent` columns.
fn wrap(
    segments: Vec<(String, Style)>,
    width: usize,
    first: Vec<Span<'static>>,
    indent: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut start: usize = first.iter().map(Span::width).sum();
    let mut used = start;
    let mut current = first;
    let mut space: Option<Style> = None;
    for unit in units(segments) {
        match unit {
            Unit::Space(style) => {
                if used > start {
                    space = Some(style);
                }
            }
            Unit::Word(pieces) => {
                let word_w: usize = pieces.iter().map(|(text, _)| text.width()).sum();
                let space_w = usize::from(space.is_some());
                if used > start && used + space_w + word_w > width {
                    lines.push(Line::from(std::mem::take(&mut current)));
                    current.push(Span::raw(" ".repeat(indent)));
                    start = indent;
                    used = indent;
                    space = None;
                }
                if let Some(style) = space.take() {
                    current.push(Span::styled(" ", style));
                    used += 1;
                }
                for (text, style) in pieces {
                    used += text.width();
                    current.push(Span::styled(text, style));
                }
            }
        }
    }
    lines.push(Line::from(current));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn paragraphs_wrap_at_words_and_keep_marks_whole() {
        let lines = wrap(
            inline("press `v`. then **walk** the table", Style::default()),
            12,
            Vec::new(),
            0,
        );
        let texts: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(texts, ["press v.", "then walk", "the table"]);
        assert!(lines.iter().all(|line| line.width() <= 12));
    }

    #[test]
    fn a_marked_value_never_wraps_inside() {
        let lines = wrap(inline("cost: `L1 +1` here", Style::default()), 9, Vec::new(), 0);
        let texts: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(texts, ["cost:", "L1\u{a0}+1", "here"]);
    }

    #[test]
    fn continuation_lines_hang_under_the_text() {
        let first = vec![Span::raw(" key  ")];
        let lines = wrap(inline("one two three four", Style::default()), 14, first, 6);
        let texts: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(texts, [" key  one two", "      three", "      four"]);
    }

    #[test]
    fn every_vm_section_has_an_anchor_and_fits_the_width() {
        for sections in [
            guide::guide(crate::ui::app::Tab::Tlb, DocsLang::En).unwrap(),
            guide::guide(crate::ui::app::Tab::Tlb, DocsLang::PtBr).unwrap(),
        ] {
            let (lines, anchors) = layout(sections, 60);
            assert_eq!(anchors.len(), sections.len());
            assert!(anchors.windows(2).all(|pair| pair[0] < pair[1]));
            for line in &lines {
                // Diagrams are drawn as written; everything else wraps.
                let is_art = text(line).contains('│') || text(line).contains('─');
                assert!(is_art || line.width() <= 60, "{:?}", text(line));
            }
        }
    }

    #[test]
    fn no_mark_is_left_open() {
        for sections in [
            guide::guide(crate::ui::app::Tab::Tlb, DocsLang::En).unwrap(),
            guide::guide(crate::ui::app::Tab::Tlb, DocsLang::PtBr).unwrap(),
        ] {
            let (lines, _) = layout(sections, 80);
            for line in &lines {
                let shown = text(line);
                assert!(!shown.contains('`') && !shown.contains("**"), "{shown:?}");
            }
        }
    }
}
