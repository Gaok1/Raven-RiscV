use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::cmp::min;
use std::collections::HashSet;

use super::components::panel::{self, PanelKind};
use super::components::{ControlState, SbGeom, SpanRow, Toolbar, vertical_scrollbar};
use super::style;
use super::{App, Editor};
use crate::ui::app::FileTabId;
use crate::ui::theme;
use raven_engine::capability::BitRole;

/// A control in the editor's action row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorActionBtn {
    ImportBin,
    ImportCode,
    ExportBin,
    ExportCode,
    Run,
    Format,
}

/// Rows the borderless Editor header occupies — the file tabs, a blank, then
/// the action bar. See [`crate::ui::view::cache::CACHE_HEADER_H`] on the gap.
pub(crate) const EDITOR_HEADER_H: u16 = 3;

/// Columns ` Editor ` claims before the file tabs begin.
const TITLE_W: u16 = 10;

/// Split the Editor tab into (header, body). One model for the renderer and the
/// mouse, which used to carry identical `Length(5), Length(1), Min(3)` chunk
/// lists in two files.
pub(crate) fn editor_chunks(area: Rect) -> (Rect, Rect) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(EDITOR_HEADER_H), Constraint::Min(3)])
        .split(area);
    (parts[0], parts[1])
}

/// Row the file-tab strip renders on.
pub(crate) fn editor_file_tabs_row(area: Rect) -> u16 {
    area.y
}

/// First column the file-tab strip renders at.
pub(crate) fn editor_file_tabs_origin(area: Rect) -> u16 {
    area.x + TITLE_W
}

/// Row the action bar renders on, given the editor header area.
pub(crate) fn editor_actions_row(area: Rect) -> u16 {
    area.y + 2
}

/// First column the action bar renders at.
pub(crate) fn editor_actions_origin(area: Rect) -> u16 {
    area.x + 1
}

/// The editor's action row as a [`Toolbar`] — the single source of truth for
/// rendering it and for mapping a click column back to a button.
///
/// The two used to be separate calculations: the renderer walked `x +=
/// btn.len()` down one function while `mouse::handle_editor_actions_click`
/// walked the same sums down another, and any change to a label had to be made
/// in both or the click targets slid out from under the words.
///
/// The labels also lost their brackets. `[BIN]` had a filled background *and*
/// brackets, saying "clickable" twice while the bracket was supposed to mean a
/// keyboard key everywhere else.
pub(crate) fn build_editor_action_bar(
    hovered: Option<EditorActionBtn>,
) -> Toolbar<EditorActionBtn> {
    let state = |btn: EditorActionBtn| ControlState::chip(false, hovered == Some(btn));
    let mut bar = Toolbar::with_gap(1);
    bar.text(vec![Span::styled("import", crate::ui::view::style::label())])
        .action(EditorActionBtn::ImportBin, "bin", state(EditorActionBtn::ImportBin), theme::TEXT)
        .action(
            EditorActionBtn::ImportCode,
            "code",
            state(EditorActionBtn::ImportCode),
            theme::TEXT,
        )
        .separator()
        .text(vec![Span::styled("export", crate::ui::view::style::label())])
        .action(EditorActionBtn::ExportBin, "bin", state(EditorActionBtn::ExportBin), theme::TEXT)
        .action(
            EditorActionBtn::ExportCode,
            "code",
            state(EditorActionBtn::ExportCode),
            theme::TEXT,
        )
        .separator()
        .action(EditorActionBtn::Run, "▶ run", state(EditorActionBtn::Run), theme::RUNNING)
        .action(EditorActionBtn::Format, "format", state(EditorActionBtn::Format), theme::TEXT);
    bar
}

/// The editor's file tab strip: one chip per file, then `[+]` (new file) and
/// `[✕]` (delete active file, confirmed by a second click). Single source of
/// truth for render and mouse hit-testing.
pub(crate) fn build_file_tab_bar(app: &App) -> Toolbar<FileTabId> {
    let hover = app.editor.hover_file_tab;
    let mut bar = Toolbar::with_gap(1);
    for (i, file) in app.editor.files.iter().enumerate() {
        bar.value(
            FileTabId::File(i),
            &format!(" {} ", file.name),
            ControlState::chip(
                i == app.editor.active_file,
                hover == Some(FileTabId::File(i)),
            ),
            theme::ACCENT,
        );
    }
    // Bare glyphs: `action` already gives them the button surface, so the
    // brackets were saying "clickable" a second time in the notation the design
    // system reserves for keyboard keys.
    bar.action(
        FileTabId::New,
        "+",
        if hover == Some(FileTabId::New) {
            ControlState::Hovered
        } else {
            ControlState::Normal
        },
        Color::Green,
    );
    if app.editor.files.len() > 1 {
        let armed = app
            .editor
            .file_delete_armed
            .is_some_and(|t| t.elapsed().as_secs() < 3);
        bar.action(
            FileTabId::Delete,
            if armed { "✕ delete?" } else { "✕" },
            if hover == Some(FileTabId::Delete) {
                ControlState::Hovered
            } else {
                ControlState::Normal
            },
            theme::DANGER,
        );
    }
    bar
}

/// The Editor tab's chrome, in two borderless lines.
///
/// ```text
///  Editor   main.s  +                                  ln 1  col 1  addr hints
///  import bin code │ export bin code │ ▶ run  format    ✓ Assembled 22 instructions…
/// ```
///
/// It was an `Editor Control` box of five rows — two of them border — holding a
/// `Build status:` line, a `Build size:` line that repeated the instruction and
/// byte counts the status line had just given, and the action bar; with the file
/// tabs on a sixth row below it. Line 1 is now *which file, where the cursor is*
/// and line 2 *what you can do, and how the last build went*.
pub(super) fn render_editor_header(f: &mut Frame, area: Rect, app: &App) {
    // ── Line 1: identity · files · cursor ──
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::styled(" Editor ", style::title()));
    row.gap(editor_file_tabs_origin(area).saturating_sub(row.cursor()));
    for span in build_file_tab_bar(app).spans() {
        row.push(span);
    }

    let mut tail = style::readout("ln", app.editor.buf.cursor_row + 1, theme::TEXT);
    tail.push(Span::raw("   "));
    tail.extend(style::readout(
        "col",
        app.editor.buf.cursor_col + 1,
        theme::TEXT,
    ));
    if app.editor.show_addr_hints {
        // A flag that is on, said as a word. `[addr]` spelled it with the
        // bracket this UI reserves for a keyboard key.
        tail.push(Span::raw("   "));
        tail.push(Span::styled("addr hints", style::label()));
    }
    tail.push(Span::raw(" "));
    push_right(&mut row, tail, area);
    let line1 = row.into_line();

    // ── Line 2: actions · build result ──
    let actions_y = editor_actions_row(area);
    let hovered: Option<EditorActionBtn> = (app.mouse_y == actions_y)
        .then(|| build_editor_action_bar(None).hit(app.mouse_x, editor_actions_origin(area)))
        .flatten();

    let mut row = SpanRow::new(area.x, actions_y);
    row.gap(editor_actions_origin(area).saturating_sub(area.x));
    for span in build_editor_action_bar(hovered).spans() {
        row.push(span);
    }

    let build = if app.editor.last_ok_elf_bytes.is_some() {
        vec![Span::styled(
            "ELF binary — read-only",
            Style::default().fg(theme::PAUSED),
        )]
    } else if let Some(msg) = &app.editor.last_assemble_msg {
        let ok = app.editor.last_compile_ok == Some(true);
        vec![Span::styled(
            format!("{} {msg}", if ok { "✓" } else { "✗" }),
            if ok { style::success() } else { style::danger() },
        )]
    } else {
        vec![Span::styled("not assembled", style::label())]
    };
    let mut tail = build;
    tail.push(Span::raw(" "));
    push_right(&mut row, tail, area);

    f.render_widget(
        Paragraph::new(vec![line1, Line::raw(""), row.into_line()]),
        area,
    );
}

/// Push `tail` so it ends against the right edge of `area`, keeping at least a
/// three-column gap from whatever the row already holds.
fn push_right(row: &mut SpanRow, tail: Vec<Span<'static>>, area: Rect) {
    let w: u16 = tail.iter().map(|s| s.width() as u16).sum();
    row.gap(
        (area.x + area.width)
            .saturating_sub(w)
            .saturating_sub(row.cursor())
            .max(3),
    );
    for span in tail {
        row.push(span);
    }
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

    // Compute bar rows for find/goto
    let bar_rows: u16 = if app.editor.find_open {
        if app.editor.replace_open { 2 } else { 1 }
    } else if app.editor.goto_open {
        1
    } else {
        0
    };
    // B3: encoding overlay row (Ctrl+E)
    let enc_row: u16 = if app.editor.show_encoding { 1 } else { 0 };

    let inner_h = area.height.saturating_sub(2);
    let content_h = inner_h.saturating_sub(bar_rows + enc_row);
    let visible_h = content_h as usize;
    // Inform the editor buffer of the visible height so page_up/page_down are accurate.
    app.editor.buf.page_size.set(visible_h);

    let len = app.editor.buf.lines.len();
    // Use edge-margin scrolling (not centering) so the scroll offset stays stable
    // during mouse drag — prevents the feedback loop that caused fast-scroll artifacts.
    let start = app.editor.buf.stable_scroll_start(visible_h);
    let end = min(len, start + visible_h);

    let num_width = if end > 0 { end.to_string().len() } else { 1 };
    let labels = collect_labels(&app.editor.buf.lines);
    // One column reserved on the right for the vertical scrollbar.
    let content_w = area.width.saturating_sub(3);
    let query_char_len = app.editor.find_query.chars().count();
    let show_hints = app.editor.show_addr_hints;
    let hint_w: usize = if show_hints { 11 } else { 0 }; // "0x00000000 " = 11 chars

    // Compute highlight_word: the identifier under the cursor, if it's a known label
    let highlight_word: Option<String> = {
        let row = app.editor.buf.cursor_row;
        if row < app.editor.buf.lines.len() {
            let line = &app.editor.buf.lines[row];
            let col = app.editor.buf.cursor_col;
            let word = word_at_col(line, col);
            if !word.is_empty()
                && (labels.contains(&word) || app.editor.label_to_line.contains_key(&word))
            {
                Some(word)
            } else {
                None
            }
        } else {
            None
        }
    };

    // A5: heat map — precompute max exec count over visible lines for scaling
    let exec_max: u64 = if app.run.show_exec_count {
        (start..end)
            .filter_map(|i| {
                app.editor
                    .line_to_addr
                    .get(&i)
                    .and_then(|addr| app.session.exec_counts.get(addr))
                    .copied()
            })
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    let mut rows: Vec<Line> = Vec::with_capacity(end.saturating_sub(start));
    for i in start..end {
        let line_str: &str = &app.editor.buf.lines[i];
        let mut line = Line::from(highlight_line(line_str, app.architecture.assembler()));
        if let Some(((sr, sc), (er, ec))) = app.editor.buf.selection_range() {
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

        // Apply find match highlighting
        if app.editor.find_open && query_char_len > 0 {
            apply_find_matches(
                &mut line,
                i,
                &app.editor.find_matches,
                app.editor.find_current,
                query_char_len,
            );
        }

        // Apply label highlight under cursor (underline all occurrences)
        if let Some(ref hw) = highlight_word {
            apply_label_highlight(&mut line, line_str, hw);
        }

        if Some(i) == app.editor.diag_line {
            let err_style = Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::UNDERLINED);
            for span in &mut line.spans {
                span.style = span.style.patch(err_style);
            }
        }

        let mut spans = Vec::new();

        // Optional address hint gutter
        if show_hints {
            let addr_text = if let Some(&addr) = app.editor.line_to_addr.get(&i) {
                format!("{addr:08x} ")
            } else {
                "         ".to_string()
            };
            spans.push(Span::styled(
                addr_text,
                Style::default().fg(Color::Rgb(80, 100, 80)),
            ));
        }

        // A4: check if this line has a breakpoint set
        let is_bp = app
            .editor
            .line_to_addr
            .get(&i)
            .map_or(false, |addr| app.session.breakpoints.contains(addr));

        // Line number — dim for normal, bright red for breakpoint lines
        spans.push(Span::styled(
            format!("{:>width$}", i + 1, width = num_width),
            if is_bp {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
        let marker_style = if Some(i) == app.editor.diag_line {
            Style::default().fg(Color::Red)
        } else if is_bp {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let marker_str = if is_bp { " ● " } else { " │ " };
        spans.push(Span::styled(marker_str, marker_style));
        spans.extend(line.spans);

        if i == app.editor.buf.cursor_row {
            if let Some(ghost) = ghost_spans_for_line(line_str, app.architecture.assembler()) {
                let gutter_w = (hint_w as u16) + (num_width as u16) + 3;
                let used_w = gutter_w.saturating_add(Editor::char_count(line_str) as u16);
                let remaining = content_w.saturating_sub(used_w);
                if remaining >= 4 {
                    spans.extend(truncate_spans_to_width(ghost, remaining as usize));
                }
            }
        }

        // A5: heat map — tint line background by exec frequency (cursor row takes precedence)
        let mut row_line = Line::from(spans);
        if i == app.editor.buf.cursor_row {
            row_line = row_line.style(Style::default().bg(Color::Rgb(40, 40, 55)));
        } else if exec_max > 0 {
            if let Some(&addr) = app.editor.line_to_addr.get(&i) {
                if let Some(&count) = app.session.exec_counts.get(&addr) {
                    if count > 0 {
                        // ratio 0.0..=1.0 scaled with sqrt for better distribution
                        let ratio = (count as f64 / exec_max as f64).sqrt() as f32;
                        // cold Rgb(20,20,60) → hot Rgb(180,30,20)
                        let r = (20.0 + ratio * 160.0) as u8;
                        let g = (20.0 - ratio * 0.0_f32).max(0.0) as u8; // stays ~20
                        let b = (60.0 - ratio * 50.0).max(0.0) as u8;
                        row_line = row_line.style(Style::default().bg(Color::Rgb(r, g, b)));
                    }
                }
            }
        }
        rows.push(row_line);
    }

    // `Source`, not `Editor (RISC-V 32 (RV32IMAF) ASM)`: the header above already
    // says Editor, and nesting a parenthesis inside a parenthesis inside a title
    // is not a title.
    //
    // Which ISA this is, and whether it assembled, are both *state*, so they
    // share the right-hand slot the panel keeps for it — the compile flag as a
    // coloured dot, the same badge shape the run state uses.
    let mut state = vec![Span::styled(
        format!("{} asm", app.architecture.descriptor().display_name),
        style::label(),
    )];
    if let Some(ok) = app.editor.last_compile_ok {
        let (txt, color) = if ok {
            ("● assembled", theme::RUNNING)
        } else {
            ("● failed", theme::DANGER)
        };
        state.push(Span::styled("  ", style::label()));
        state.push(Span::styled(txt, Style::default().fg(color)));
    }
    let block = panel::panel_state_spans(
        "Source",
        state,
        PanelKind::Custom(Color::DarkGray),
    );

    // Render block border
    f.render_widget(block, area);

    // Render content to inner sub-area (excluding bar rows and scrollbar column)
    let content_area = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(3),
        content_h,
    );
    f.render_widget(Paragraph::new(rows), content_area);

    // Draggable vertical scrollbar on the inner right column.
    let sb_area = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        content_h,
    );
    if len > visible_h && visible_h > 0 {
        vertical_scrollbar(f, sb_area, len, visible_h, start);
        app.editor.sb.set(Some(SbGeom {
            start: sb_area.y,
            len: sb_area.height,
            cross: sb_area.x + sb_area.width.saturating_sub(1),
            content: len,
            viewport: visible_h,
            offset: start,
            max: len - visible_h,
        }));
    } else {
        app.editor.sb.set(None);
    }

    // Render find/goto bar
    if bar_rows > 0 {
        let bar_area = Rect::new(
            area.x + 1,
            area.y + 1 + content_h,
            area.width.saturating_sub(2),
            bar_rows,
        );
        render_find_goto_bar(f, bar_area, app);
    }

    // B3: Encoding overlay — show binary encoding of the current instruction
    if enc_row > 0 {
        let enc_area = Rect::new(
            area.x + 1,
            area.y + 1 + content_h + bar_rows,
            area.width.saturating_sub(2),
            1,
        );
        render_encoding_bar(f, enc_area, app);
    }

    // Cursor placement
    if (app.editor.find_open || app.editor.goto_open) && bar_rows > 0 {
        let bar_y = area.y + 1 + content_h;
        let (query, prefix_len) = if app.editor.goto_open {
            (
                &app.editor.goto_query,
                "  Go to line (1-XXXXX): ".len() as u16,
            )
        } else if app.editor.find_in_replace {
            (&app.editor.replace_query, " Repl: ".len() as u16)
        } else {
            (&app.editor.find_query, " Find: ".len() as u16)
        };
        let cursor_x = (area.x + 1 + prefix_len + query.chars().count() as u16)
            .min(area.x + area.width.saturating_sub(2));
        let cursor_y = bar_y + if app.editor.find_in_replace { 1 } else { 0 };
        f.set_cursor_position((cursor_x, cursor_y));
    } else {
        let cur_row = app.editor.buf.cursor_row as u16;
        let cur_col = app.editor.buf.cursor_col as u16;
        let gutter = (hint_w + num_width + 3) as u16;
        let cursor_x = area.x + 1 + gutter + cur_col;
        let cursor_y = area.y + 1 + (cur_row.saturating_sub(start as u16));
        if cursor_y < area.y + 1 + content_h && cursor_x < area.x + area.width {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn render_find_goto_bar(f: &mut Frame, area: Rect, app: &App) {
    let sep = Style::default()
        .fg(Color::DarkGray)
        .bg(Color::Rgb(30, 30, 50));
    let label_s = Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 50));
    let text_s = Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 50));
    let focus_s = Style::default()
        .fg(Color::Yellow)
        .bg(Color::Rgb(30, 30, 50));
    let info_s = Style::default()
        .fg(Color::DarkGray)
        .bg(Color::Rgb(30, 30, 50));

    if app.editor.goto_open {
        let match_info = format!("  Go to line (1-{}):", app.editor.buf.lines.len());
        let line = Line::from(vec![
            Span::styled(match_info, label_s),
            Span::styled(format!(" {}", app.editor.goto_query), text_s),
            Span::styled("  [esc] close  [enter] jump", info_s),
        ]);
        f.render_widget(Paragraph::new(line).style(sep), area);
    } else {
        // Find bar (row 0)
        let match_count = app.editor.find_matches.len();
        let current_disp = if match_count > 0 {
            app.editor.find_current + 1
        } else {
            0
        };
        let status = if app.editor.find_query.is_empty() {
            String::new()
        } else if match_count == 0 {
            "  (no matches)".to_string()
        } else {
            format!("  {}/{}", current_disp, match_count)
        };

        let find_is_focus = !app.editor.find_in_replace;
        let find_text_s = if find_is_focus { focus_s } else { text_s };

        let find_line = Line::from(vec![
            Span::styled(" Find: ", label_s),
            Span::styled(app.editor.find_query.clone(), find_text_s),
            Span::styled(status, info_s),
            Span::styled("  [tab] replace  [esc] close  [enter] next", info_s),
        ]);

        f.render_widget(
            Paragraph::new(find_line).style(sep),
            Rect::new(area.x, area.y, area.width, 1),
        );

        // Replace bar (row 1, only if replace_open)
        if app.editor.replace_open && area.height >= 2 {
            let rep_is_focus = app.editor.find_in_replace;
            let rep_text_s = if rep_is_focus { focus_s } else { text_s };
            let rep_line = Line::from(vec![
                Span::styled(" Repl: ", label_s),
                Span::styled(app.editor.replace_query.clone(), rep_text_s),
                Span::styled("  [enter] replace current", info_s),
            ]);
            f.render_widget(
                Paragraph::new(rep_line).style(sep),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }
}

fn apply_find_matches(
    line: &mut Line,
    row: usize,
    matches: &[(usize, usize)],
    current: usize,
    query_char_len: usize,
) {
    if query_char_len == 0 {
        return;
    }
    let mut positions: Vec<(usize, bool)> = Vec::new();
    for (i, &(r, c)) in matches.iter().enumerate() {
        if r == row {
            positions.push((c, i == current));
        }
    }
    if positions.is_empty() {
        return;
    }
    for (col_start, is_current) in positions {
        let col_end = col_start + query_char_len;
        let bg = if is_current {
            Color::Yellow
        } else {
            Color::Rgb(80, 80, 120)
        };
        let fg = if is_current {
            Color::Black
        } else {
            Color::White
        };
        overlay_range(line, col_start, col_end, Style::default().fg(fg).bg(bg));
    }
}

fn overlay_range(line: &mut Line, start: usize, end: usize, style: Style) {
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
                let b = Editor::byte_at(&content, start - span_start);
                new_spans.push(Span::styled(content[..b].to_string(), span.style));
            }
            let sel_from = start.max(span_start);
            let sel_to = end.min(span_end);
            let bf = Editor::byte_at(&content, sel_from - span_start);
            let bt = Editor::byte_at(&content, sel_to - span_start);
            new_spans.push(Span::styled(content[bf..bt].to_string(), style));
            if span_end > end {
                let b = Editor::byte_at(&content, end - span_start);
                new_spans.push(Span::styled(content[b..].to_string(), span.style));
            }
        }
        char_pos += len;
    }
    line.spans = new_spans;
}

/// Highlight the operand part of an assembler directive line.
fn highlight_directive_rest<'a>(directive: &str, rest: &'a str) -> Vec<Span<'a>> {
    let dir = directive.to_ascii_lowercase();
    match dir.as_str() {
        // Data directives: comma-separated numeric/float values
        ".byte" | ".half" | ".word" | ".dword" | ".float" => {
            let mut out: Vec<Span<'a>> = Vec::new();
            // keep leading whitespace raw
            let trimmed_start = rest.len() - rest.trim_start().len();
            if trimmed_start > 0 {
                out.push(Span::raw(&rest[..trimmed_start]));
            }
            let values = &rest[trimmed_start..];
            // split by comma, color each token
            let mut remaining = values;
            loop {
                if let Some(comma_pos) = remaining.find(',') {
                    let tok = remaining[..comma_pos].trim();
                    let tok_color = if tok.starts_with('"') || tok.starts_with('\'') {
                        Color::Green
                    } else {
                        Color::Magenta
                    };
                    out.push(Span::styled(
                        remaining[..comma_pos].to_string(),
                        Style::default().fg(tok_color),
                    ));
                    out.push(Span::raw(","));
                    remaining = &remaining[comma_pos + 1..];
                } else {
                    let tok = remaining.trim();
                    let tok_color = if tok.starts_with('"') || tok.starts_with('\'') {
                        Color::Green
                    } else {
                        Color::Magenta
                    };
                    out.push(Span::styled(
                        remaining.to_string(),
                        Style::default().fg(tok_color),
                    ));
                    break;
                }
            }
            out
        }
        // String directives: everything is the string literal
        ".ascii" | ".asciz" | ".string" => {
            vec![Span::styled(
                rest.to_string(),
                Style::default().fg(Color::Green),
            )]
        }
        // Symbol directives: the name after whitespace is a label/symbol
        ".globl" | ".global" | ".extern" | ".weak" => {
            let ws_end = rest.len() - rest.trim_start().len();
            let mut out = Vec::new();
            if ws_end > 0 {
                out.push(Span::raw(&rest[..ws_end]));
            }
            out.push(Span::styled(
                &rest[ws_end..],
                Style::default().fg(Color::Yellow),
            ));
            out
        }
        _ => vec![Span::raw(rest)],
    }
}

fn highlight_line<'a>(s: &'a str, assembler: &dyn raven_engine::Assembler) -> Vec<Span<'a>> {
    use Color::*;
    if s.is_empty() {
        return vec![Span::raw("")];
    }

    // Detect start of comment (';' or '#')
    let c1 = s.find(';');
    let c2 = s.find('#');
    let comment_idx = match (c1, c2) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    // If the line is comment-only (first non-space is ';' or '#'), dim the whole line
    if let Some(ci) = comment_idx {
        let mut ws = 0usize;
        for ch in s.chars() {
            if ch.is_whitespace() {
                ws += ch.len_utf8();
            } else {
                break;
            }
        }
        if ci == ws {
            let trimmed = &s[ws..];
            if trimmed.starts_with("##!") {
                // Block comment separator — bold green
                let mut v = Vec::new();
                if ws > 0 {
                    v.push(Span::raw(s[..ws].to_string()));
                }
                v.push(Span::styled(
                    s[ws..].to_string(),
                    Style::default()
                        .fg(Color::Rgb(80, 180, 80))
                        .add_modifier(Modifier::BOLD),
                ));
                return v;
            }
            if trimmed.starts_with("#!") {
                let mut v = Vec::new();
                if ws > 0 {
                    v.push(Span::raw(s[..ws].to_string()));
                }
                v.push(Span::styled(
                    "#!",
                    Style::default().fg(Color::Rgb(100, 200, 100)),
                ));
                v.push(Span::styled(
                    s[ws + 2..].to_string(),
                    Style::default().fg(Color::Rgb(160, 220, 140)),
                ));
                return v;
            }
            return vec![Span::styled(s, Style::default().fg(DarkGray))];
        }
    }

    // Split into code and comment parts
    let (code, comment) = if let Some(ci) = comment_idx {
        (&s[..ci], &s[ci..])
    } else {
        (s, "")
    };

    let mut out = Vec::new();

    // Highlight the code part (same logic as before)
    if !code.is_empty() {
        let mut lead_len = 0usize;
        for ch in code.chars() {
            if ch.is_whitespace() {
                lead_len += ch.len_utf8();
            } else {
                break;
            }
        }
        if lead_len > 0 {
            out.push(Span::raw(&code[..lead_len]));
        }
        let trimmed = &code[lead_len..];

        if !trimmed.is_empty() {
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
            } else if first.starts_with('.') {
                // Assembler directive — distinct color from mnemonics
                out.push(Span::styled(first, Style::default().fg(Color::LightYellow)));
                if !rest.is_empty() {
                    out.extend(highlight_directive_rest(first, rest));
                }
            } else {
                out.push(Span::styled(
                    first,
                    Style::default().fg(Cyan).add_modifier(Modifier::BOLD),
                ));

                let mut token = String::new();
                for ch in rest.chars() {
                    if ",()\t ".contains(ch) {
                        if !token.is_empty() {
                            out.push(color_operand(&token, assembler));
                            token.clear();
                        }
                        out.push(Span::raw(ch.to_string()));
                    } else {
                        token.push(ch);
                    }
                }
                if !token.is_empty() {
                    out.push(color_operand(&token, assembler));
                }
            }
        }
    }

    // Append the comment part; #! visible comments get a distinct color
    if !comment.is_empty() {
        if comment.starts_with("##!") {
            out.push(Span::styled(
                comment.to_string(),
                Style::default()
                    .fg(Color::Rgb(80, 180, 80))
                    .add_modifier(Modifier::BOLD),
            ));
        } else if comment.starts_with("#!") {
            out.push(Span::styled(
                "#!",
                Style::default().fg(Color::Rgb(100, 200, 100)),
            ));
            out.push(Span::styled(
                &comment[2..],
                Style::default().fg(Color::Rgb(160, 220, 140)),
            ));
        } else {
            out.push(Span::styled(comment, Style::default().fg(DarkGray)));
        }
    }

    out
}

fn color_operand(tok: &str, assembler: &dyn raven_engine::Assembler) -> Span<'static> {
    use Color::*;
    let is_imm = tok.starts_with("0x")
        || tok.starts_with("0X")
        || tok.starts_with("0b")
        || tok.starts_with("0B")
        || tok.parse::<i32>().is_ok();
    let style = if assembler.is_register(tok) {
        Style::default().fg(Green)
    } else if is_imm {
        Style::default().fg(Magenta)
    } else {
        Style::default()
    };
    Span::styled(tok.to_string(), style)
}

fn collect_labels(lines: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in lines {
        let code = strip_comments(line);
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        if let Some((lab, _rest)) = code.split_once(':') {
            let lab = lab.trim();
            if !lab.is_empty() {
                out.insert(lab.to_string());
            }
        }
    }
    out
}

/// Extract the identifier word at the given character column.
fn word_at_col(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return String::new();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    if !is_word(chars[col]) {
        return String::new();
    }
    let start = (0..=col)
        .rev()
        .take_while(|&i| i < chars.len() && is_word(chars[i]))
        .last()
        .unwrap_or(col);
    let end = (col..chars.len())
        .take_while(|&i| is_word(chars[i]))
        .last()
        .map(|i| i + 1)
        .unwrap_or(col + 1);
    chars[start..end].iter().collect()
}

/// Underline all occurrences of `word` in the line's spans.
fn apply_label_highlight(line: &mut Line, line_str: &str, word: &str) {
    if word.is_empty() || !line_str.contains(word) {
        return;
    }
    let uline = Style::default()
        .add_modifier(Modifier::UNDERLINED)
        .fg(Color::Rgb(200, 200, 100));
    // Find byte offsets of all occurrences
    let mut search = line_str;
    let mut byte_off = 0usize;
    let mut ranges: Vec<(usize, usize)> = Vec::new(); // char start, char end
    while let Some(idx) = search.find(word) {
        let char_start = Editor::char_count(&line_str[..byte_off + idx]);
        let char_end = char_start + Editor::char_count(word);
        ranges.push((char_start, char_end));
        byte_off += idx + word.len();
        search = &line_str[byte_off..];
    }
    for (cs, ce) in ranges {
        let mut char_pos = 0usize;
        let mut new_spans: Vec<Span> = Vec::new();
        for span in line.spans.drain(..) {
            let content = span.content.to_string();
            let len = Editor::char_count(&content);
            let sp_start = char_pos;
            let sp_end = char_pos + len;
            if sp_end <= cs || sp_start >= ce {
                new_spans.push(Span::styled(content, span.style));
            } else {
                if sp_start < cs {
                    let b = Editor::byte_at(&content, cs - sp_start);
                    new_spans.push(Span::styled(content[..b].to_string(), span.style));
                }
                let sel_from = cs.max(sp_start);
                let sel_to = ce.min(sp_end);
                let b0 = Editor::byte_at(&content, sel_from - sp_start);
                let b1 = Editor::byte_at(&content, sel_to - sp_start);
                new_spans.push(Span::styled(
                    content[b0..b1].to_string(),
                    span.style.patch(uline),
                ));
                if sp_end > ce {
                    let b = Editor::byte_at(&content, ce - sp_start);
                    new_spans.push(Span::styled(content[b..].to_string(), span.style));
                }
            }
            char_pos += len;
        }
        line.spans = new_spans;
    }
}

fn strip_comments(line: &str) -> &str {
    let c1 = line.find(';');
    let c2 = line.find('#');
    let cut = match (c1, c2) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(i) = cut { &line[..i] } else { line }
}

fn ghost_spans_for_line(
    line: &str,
    assembler: &dyn raven_engine::Assembler,
) -> Option<Vec<Span<'static>>> {
    let mut code = strip_comments(line).trim();
    if code.is_empty() || code.starts_with('.') {
        return None;
    }
    if let Some((_label, rest)) = code.split_once(':') {
        code = rest.trim();
        if code.is_empty() {
            return None;
        }
    }

    let mut parts = code.splitn(2, char::is_whitespace);
    let mnemonic = parts.next()?;
    let operands = parts.next().unwrap_or("").trim();
    let forms = assembler.instruction_forms(mnemonic);
    if forms.is_empty() {
        return None;
    }

    let operand_count = operands
        .split(',')
        .filter(|operand| !operand.trim().is_empty())
        .count();
    let variants: Vec<Vec<&'static str>> = forms
        .iter()
        .map(|form| {
            form.split(',')
                .map(str::trim)
                .filter(|operand| !operand.is_empty())
                .collect()
        })
        .collect();
    if !operands.ends_with(',') && variants.iter().any(|form| form.len() == operand_count) {
        return None;
    }

    Some(build_ghost_variants_spans(
        mnemonic,
        operand_count,
        &variants,
        assembler,
    ))
}

fn truncate_spans_to_width(spans: Vec<Span<'static>>, max_chars: usize) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let content = span.content.to_string();
        let len = Editor::char_count(&content);
        if used + len <= max_chars {
            out.push(span);
            used += len;
            continue;
        }
        let take = max_chars.saturating_sub(used);
        if take == 0 {
            break;
        }
        let byte = Editor::byte_at(&content, take);
        out.push(Span::styled(content[..byte].to_string(), span.style));
        break;
    }
    out
}

fn build_ghost_variants_spans(
    mnemonic_raw: &str,
    ops_len: usize,
    variants: &[Vec<&'static str>],
    assembler: &dyn raven_engine::Assembler,
) -> Vec<Span<'static>> {
    let base = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);
    let variant_sep = Span::styled("  |  ".to_string(), base);

    let mut out: Vec<Span<'static>> = Vec::new();
    out.push(Span::styled("  ".to_string(), base));

    for (vi, operands) in variants.iter().enumerate() {
        if vi > 0 {
            out.push(variant_sep.clone());
        }

        let needed = operands.len();
        let compatible = ops_len <= needed;
        let complete_by_count = ops_len == needed;

        let mut mnemonic_style = base;
        if compatible {
            mnemonic_style = mnemonic_style.add_modifier(Modifier::BOLD);
        }
        if complete_by_count {
            mnemonic_style = mnemonic_style.add_modifier(Modifier::UNDERLINED);
        }

        out.push(Span::styled(mnemonic_raw.to_string(), mnemonic_style));

        if !operands.is_empty() {
            out.push(Span::styled(" ".to_string(), base));
        }

        let next_idx = if ops_len < needed {
            Some(ops_len)
        } else {
            None
        };
        for (oi, expr) in operands.iter().enumerate() {
            if oi > 0 {
                out.push(Span::styled(", ".to_string(), base));
            }
            out.extend(style_ghost_operand_expr(
                expr,
                base,
                next_idx == Some(oi),
                assembler,
            ));
        }
    }

    out
}

fn style_ghost_operand_expr(
    expr: &str,
    base: Style,
    is_next: bool,
    assembler: &dyn raven_engine::Assembler,
) -> Vec<Span<'static>> {
    fn token_style(
        tok: &str,
        base: Style,
        is_next: bool,
        assembler: &dyn raven_engine::Assembler,
    ) -> Style {
        let mut style = match tok {
            "rd" => base.fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "rs1" | "rs2" | "rs" => base.fg(Color::Cyan),
            "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" => base.fg(Color::LightGreen),
            "label" => base.fg(Color::Magenta),
            _ if assembler.is_register(tok) => base.fg(Color::LightBlue),
            _ => base,
        };

        if is_next {
            style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        }
        style
    }

    let mut out = Vec::new();
    let mut token = String::new();
    let flush = |out: &mut Vec<Span<'static>>, token: &mut String| {
        if token.is_empty() {
            return;
        }
        let style = token_style(token, base, is_next, assembler);
        out.push(Span::styled(std::mem::take(token), style));
    };

    for ch in expr.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            flush(&mut out, &mut token);
            out.push(Span::styled(ch.to_string(), base));
        }
    }
    flush(&mut out, &mut token);
    out
}

/// The one-line encoding strip under the editor.
///
/// The bit groups, their names and their colours all come from the backend's
/// declared layout, so an 8-bit SAP instruction and a 32-bit RV32 one render
/// through the same code. When a backend declares no layout the strip still
/// shows the raw encoding rather than inventing field boundaries.
fn render_encoding_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(20, 20, 45);
    let tag = Span::styled(
        " ENC ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(80, 80, 160)),
    );
    let note = |text: &'static str| {
        Line::from(vec![
            tag.clone(),
            Span::styled(text, Style::default().fg(Color::DarkGray).bg(bg)),
        ])
    };

    let cursor_row = app.editor.buf.cursor_row;
    let line = match app.editor.line_to_addr.get(&cursor_row) {
        None => note(" (assemble first — or not an instruction line)"),
        Some(&addr) => match encoding_spans(app, u64::from(addr), bg) {
            None => note(" (no instruction at this line)"),
            Some(mut spans) => {
                spans.insert(0, tag);
                Line::from(spans)
            }
        },
    };

    f.render_widget(
        ratatui::widgets::Paragraph::new(line).style(Style::default().bg(bg)),
        area,
    );
}

/// The encoding at `address` as `0x…` followed by one bit group per declared
/// field, then the field names in the same order.
fn encoding_spans(app: &App, address: u64, bg: Color) -> Option<Vec<Span<'static>>> {
    let info = app
        .code()?
        .inspect(address, &app.instruction_bytes_at(address))?;

    let digits = (usize::from(info.encoding_bits) + 3) / 4;
    let mut spans = vec![Span::styled(
        format!(" 0x{:0digits$x}  ", info.encoding, digits = digits),
        Style::default().fg(Color::Rgb(200, 200, 100)).bg(bg),
    )];

    if !info.layout_is_complete() {
        spans.push(Span::styled(
            format!(
                "{:0bits$b}",
                info.encoding,
                bits = usize::from(info.encoding_bits)
            ),
            Style::default().fg(Color::Gray).bg(bg),
        ));
        return Some(spans);
    }

    let mut remaining = u32::from(info.encoding_bits);
    for field in &info.layout {
        let width = u32::from(field.width);
        remaining -= width;
        // Widened so the shift is exact: a field as wide as the whole word —
        // x86-64 declares its encoding as one 64-bit run — has no `1 << width`
        // inside a `u64`, and the masked shift would produce a mask of 0.
        let mask = ((1u128 << width) - 1) as u64;
        let value = (info.encoding >> remaining) & mask;
        spans.push(Span::styled(
            format!("{value:0width$b} ", width = width as usize),
            Style::default().fg(bit_role_color(field.role)).bg(bg),
        ));
    }

    let names = info
        .layout
        .iter()
        .map(|field| field.label.as_str())
        .collect::<Vec<_>>()
        .join("|");
    spans.push(Span::styled(
        format!(" [{names}]"),
        Style::default().fg(Color::DarkGray).bg(bg),
    ));
    Some(spans)
}

/// Same role-to-hue mapping the Run tab's field map uses, so a field keeps its
/// colour when the user moves between the two panes.
fn bit_role_color(role: BitRole) -> Color {
    match role {
        BitRole::Opcode => Color::Cyan,
        BitRole::Destination => Color::LightGreen,
        BitRole::Source => Color::LightMagenta,
        BitRole::Immediate => Color::Blue,
        BitRole::Function => Color::Yellow,
        BitRole::Other => Color::DarkGray,
    }
}

#[cfg(test)]
mod ghost_tests {
    fn ghost(id: &str, line: &str) -> Option<String> {
        let architecture = crate::arch::lookup(id).unwrap();
        super::ghost_spans_for_line(line, architecture.assembler()).map(|spans| {
            spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect()
        })
    }

    #[test]
    fn mnemonic_helpers_come_from_the_active_isa() {
        assert!(ghost("riscv32", "lw").unwrap().contains("imm(rs1)"));
        assert!(ghost("toy16", "load").unwrap().contains("[addr]"));
        assert!(ghost("sap", "lda").unwrap().contains("address"));
        assert!(ghost("sap", "lw").is_none());
        assert!(ghost("toy16", "lda").is_none());
    }
}

#[cfg(test)]
mod encoding_bar_tests {
    use super::*;

    /// The strip's text for the first line of `id`'s own sample program.
    fn strip(id: &str) -> String {
        let app =
            App::new_with_architecture(None, crate::falcon::jit::BackendKind::None, id).unwrap();
        let address = app
            .editor
            .line_to_addr
            .values()
            .min()
            .copied()
            .unwrap_or_else(|| panic!("{id}: sample program produced no instruction line"));
        encoding_spans(&app, u64::from(address), Color::Reset)
            .unwrap_or_else(|| panic!("{id}: no encoding at 0x{address:X}"))
            .into_iter()
            .map(|span| span.content.into_owned())
            .collect()
    }

    /// The strip is the ISA's own encoding, not RV32's. Splitting every word
    /// into `funct7|rs2|rs1|f3|rd|opcode` — the old behaviour — named fields
    /// SAP and Toy16 do not have and padded their encodings out to 32 bits.
    #[test]
    fn each_backend_shows_its_own_field_names() {
        for (id, own_field, foreign_field) in [
            ("riscv32", "opcode", "—"),
            ("toy16", "rd", "funct7"),
            ("sap", "opcode", "funct7"),
            ("x86_64", "encoding", "funct7"),
        ] {
            let strip = strip(id);
            assert!(strip.contains(own_field), "{id}: no {own_field} in {strip}");
            assert!(
                !strip.contains(foreign_field),
                "{id}: borrowed {foreign_field} from another ISA in {strip}"
            );
        }
    }

    /// The bit groups cover the instruction's real width — 8 bits for SAP, 16
    /// for Toy16, 32 for RV32 — because they come from the declared layout.
    #[test]
    fn bit_groups_cover_the_isa_instruction_width() {
        for (id, bits) in [("riscv32", 32), ("toy16", 16), ("sap", 8), ("x86_64", 64)] {
            let strip = strip(id);
            let hex_end = strip
                .find("  ")
                .expect("hex prefix is followed by two spaces");
            let counted: usize = strip[hex_end..]
                .chars()
                .take_while(|c| matches!(c, '0' | '1' | ' '))
                .filter(|c| matches!(c, '0' | '1'))
                .count();
            assert_eq!(counted, bits, "{id}: {strip}");
        }
    }
}
