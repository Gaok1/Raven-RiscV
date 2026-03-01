use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use std::cmp::min;
use std::collections::HashSet;

use super::{App, Editor};

pub(super) fn render_editor_status(f: &mut Frame, area: Rect, app: &App) {
    let compile_span = if let Some(msg) = &app.editor.last_assemble_msg {
        let color = if app.editor.last_compile_ok == Some(true) {
            Color::Green
        } else {
            Color::Red
        };
        // Editor: only colored text, neutral background
        Span::styled(msg.clone(), Style::default().fg(color))
    } else {
        Span::raw("Not compiled")
    };
    let build = Line::from(vec![Span::raw("Build status: "), compile_span]);

    // Actions with clickable buttons (hover highlights via mouse coords)
    let inner_x = area.x + 1;
    // actions line is the second content line now (after removing mode line)
    let actions_y = area.y + 1 + 1;
    let mut x = inner_x;
    let import_label = "Import: ";
    let export_label = "Export: ";
    let gap = "   ";
    let btn_ibin = "[BIN]";
    let btn_icode = "[CODE]";
    let btn_ebin = "[BIN]";
    let btn_ecode = "[CODE]";

    let style_btn = |start: u16, txt: &str| {
        let w = txt.len() as u16;
        let hovered = app.mouse_y == actions_y && app.mouse_x >= start && app.mouse_x < start + w;
        if hovered {
            Style::default().fg(Color::Black).bg(Color::LightCyan).add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(Color::Black).bg(Color::DarkGray)
        }
    };

    let mut actions_spans: Vec<Span> = Vec::new();
    actions_spans.push(Span::raw(import_label));
    x += import_label.len() as u16;
    actions_spans.push(Span::styled(btn_ibin, style_btn(x, btn_ibin)));
    x += btn_ibin.len() as u16;
    actions_spans.push(Span::raw(" "));
    x += 1;
    actions_spans.push(Span::styled(btn_icode, style_btn(x, btn_icode)));
    x += btn_icode.len() as u16;
    actions_spans.push(Span::raw(gap));
    x += gap.len() as u16;
    actions_spans.push(Span::raw(export_label));
    x += export_label.len() as u16;
    actions_spans.push(Span::styled(btn_ebin, style_btn(x, btn_ebin)));
    x += btn_ebin.len() as u16;
    actions_spans.push(Span::raw(" "));
    x += 1;
    actions_spans.push(Span::styled(btn_ecode, style_btn(x, btn_ecode)));

    let actions = Line::from(actions_spans);

    // Remove mode line; keep only build status and actions
    let para = Paragraph::new(vec![build, actions]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .border_type(BorderType::Rounded)
            .title("Editor Control"),
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
    let len = app.editor.buf.lines.len();
    let mut start = 0usize;
    if len > visible_h {
        if app.editor.buf.cursor_row <= visible_h / 2 {
            start = 0;
        } else if app.editor.buf.cursor_row >= len.saturating_sub(visible_h / 2) {
            start = len.saturating_sub(visible_h);
        } else {
            start = app.editor.buf.cursor_row - visible_h / 2;
        }
    }
    let end = min(len, start + visible_h);

    let num_width = end.to_string().len();
    let labels = collect_labels(&app.editor.buf.lines);
    let content_w = area.width.saturating_sub(2);
    let mut rows: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let line_str = &app.editor.buf.lines[i];
        let mut line = Line::from(highlight_line(line_str));
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
        if Some(i) == app.editor.diag_line {
            let err_style = Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::UNDERLINED);
            for span in &mut line.spans {
                span.style = span.style.patch(err_style);
            }
        }
        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!("{:>width$}", i + 1, width = num_width),
            Style::default().fg(Color::DarkGray),
        ));
        let marker_style = if Some(i) == app.editor.diag_line {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(" │ ", marker_style));
        spans.extend(line.spans);

        if i == app.editor.buf.cursor_row || Some(i) == app.editor.diag_line {
            if let Some(ghost) = ghost_spans_for_line(line_str, &labels) {
                let gutter_w = (num_width as u16).saturating_add(3);
                let used_w = gutter_w.saturating_add(Editor::char_count(line_str) as u16);
                let remaining = content_w.saturating_sub(used_w);
                if remaining >= 4 {
                    spans.extend(truncate_spans_to_width(ghost, remaining as usize));
                }
            }
        }
        rows.push(Line::from(spans));
    }
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Editor (Risc-v ASM)");
    if let Some(ok) = app.editor.last_compile_ok {
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

    let cur_row = app.editor.buf.cursor_row as u16;
    let cur_col = app.editor.buf.cursor_col as u16;
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
            if ch.is_whitespace() { ws += ch.len_utf8(); } else { break; }
        }
        if ci == ws {
            return vec![Span::styled(s, Style::default().fg(DarkGray))];
        }
    }

    // Split into code and comment parts
    let (code, comment) = if let Some(ci) = comment_idx { (&s[..ci], &s[ci..]) } else { (s, "") };

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
            } else {
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
            }
        }
    }

    // Append the comment part dimmed
    if !comment.is_empty() {
        out.push(Span::styled(comment, Style::default().fg(DarkGray)));
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

fn strip_comments(line: &str) -> &str {
    let c1 = line.find(';');
    let c2 = line.find('#');
    let cut = match (c1, c2) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(i) = cut {
        &line[..i]
    } else {
        line
    }
}

fn ghost_spans_for_line(line: &str, labels: &HashSet<String>) -> Option<Vec<Span<'static>>> {
    use crate::falcon::asm::utils::{
        check_signed, check_u_imm, parse_imm, parse_reg, parse_shamt, split_operands,
    };

    let mut code = strip_comments(line).trim();
    if code.is_empty() {
        return None;
    }

    if code.starts_with('.') {
        return None;
    }

    if let Some((_lab, rest)) = code.split_once(':') {
        code = rest.trim();
        if code.is_empty() {
            return None;
        }
    }

    let mut parts = code.split_whitespace();
    let mnemonic_raw = parts.next()?;
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    let ops_len = ops.len();

    let is_reg = |t: &str| parse_reg(t).is_some();
    let is_imm12 = |t: &str| parse_imm(t).and_then(|v| check_signed(v, 12, "imm").ok()).is_some();
    let is_imm20u = |t: &str| parse_imm(t).and_then(|v| check_u_imm(v, "imm").ok()).is_some();
    let is_shamt = |t: &str| parse_shamt(t).is_ok();
    let is_label = |t: &str| labels.contains(t.trim());
    let is_label_or_imm_even = |t: &str, bits: u32| {
        if let Some(v) = parse_imm(t) {
            v % 2 == 0 && check_signed(v, bits, "off").is_ok()
        } else {
            is_label(t)
        }
    };

    let mnemonic_lc = mnemonic_raw.to_ascii_lowercase();

    let strict_valid = match mnemonic_lc.as_str() {
        // Pseudo-instructions
        "nop" => ops.is_empty(),
        "mv" => ops.len() == 2 && is_reg(&ops[0]) && is_reg(&ops[1]),
        "li" => ops.len() == 2 && is_reg(&ops[0]) && is_imm12(&ops[1]),
        "j" => ops.len() == 1 && is_label_or_imm_even(&ops[0], 21),
        "call" => ops.len() == 1 && is_label_or_imm_even(&ops[0], 21),
        "jr" => ops.len() == 1 && is_reg(&ops[0]),
        "ret" => ops.is_empty(),
        "subi" => {
            if ops.len() == 3 && is_reg(&ops[0]) && is_reg(&ops[1]) {
                if let Some(v) = parse_imm(&ops[2]) {
                    check_signed(-v, 12, "subi").is_ok()
                } else {
                    false
                }
            } else {
                false
            }
        }

        // R-type
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" | "mul"
        | "mulh" | "mulhsu" | "mulhu" | "div" | "divu" | "rem" | "remu" => {
            ops.len() == 3 && is_reg(&ops[0]) && is_reg(&ops[1]) && is_reg(&ops[2])
        }

        // I-type (imm12)
        "addi" | "andi" | "ori" | "xori" | "slti" | "sltiu" => {
            ops.len() == 3 && is_reg(&ops[0]) && is_reg(&ops[1]) && is_imm12(&ops[2])
        }
        "slli" | "srli" | "srai" => {
            ops.len() == 3 && is_reg(&ops[0]) && is_reg(&ops[1]) && is_shamt(&ops[2])
        }

        // Loads / Stores
        "lb" | "lh" | "lw" | "lbu" | "lhu" => {
            use crate::falcon::asm::utils::load_like;
            load_like(&ops)
                .and_then(|(_rd, imm, _rs1)| check_signed(imm, 12, "load").map(|_| ()))
                .is_ok()
        }
        "sb" | "sh" | "sw" => {
            use crate::falcon::asm::utils::store_like;
            store_like(&ops)
                .and_then(|(_rs2, imm, _rs1)| check_signed(imm, 12, "store").map(|_| ()))
                .is_ok()
        }

        // Branches
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            ops.len() == 3
                && is_reg(&ops[0])
                && is_reg(&ops[1])
                && is_label_or_imm_even(&ops[2], 13)
        }

        // U-type
        "lui" => ops.len() == 2 && is_reg(&ops[0]) && is_imm20u(&ops[1]),
        "auipc" => ops.len() == 2 && is_reg(&ops[0]) && is_imm20u(&ops[1]),

        // Jumps
        "jal" => {
            match ops.len() {
                1 => is_label_or_imm_even(&ops[0], 21),
                2 => is_reg(&ops[0]) && is_label_or_imm_even(&ops[1], 21),
                _ => false,
            }
        }
        "jalr" => ops.len() == 3 && is_reg(&ops[0]) && is_reg(&ops[1]) && is_imm12(&ops[2]),

        // System
        "ecall" => ops.is_empty(),
        "ebreak" => ops.is_empty(),
        "halt" => ops.is_empty(),

        _ => {
            // Macro-pseudos are case-sensitive in the assembler first pass.
            match mnemonic_raw {
                "la" => ops.len() == 2 && is_reg(&ops[0]) && is_label(&ops[1]),
                "push" => ops.len() == 1 && is_reg(&ops[0]),
                "pop" => ops.len() == 1 && is_reg(&ops[0]),
                "print" => ops.len() == 1 && is_reg(&ops[0]),
                "printStr" | "printString" => ops.len() == 1 && is_label(&ops[0]),
                "printStrLn" => ops.len() == 1 && is_label(&ops[0]),
                "read" => ops.len() == 1 && is_label(&ops[0]),
                "readByte" => ops.len() == 1 && is_label(&ops[0]),
                "readHalf" => ops.len() == 1 && is_label(&ops[0]),
                "readWord" => ops.len() == 1 && is_label(&ops[0]),
                "randByte" | "randHalf" | "randWord" => ops.len() == 1 && is_label(&ops[0]),
                "randBytes" => ops.len() == 2 && is_label(&ops[0]) && parse_reg(&ops[0]).is_none(),
                _ => return None,
            }
        }
    };

    if strict_valid {
        return None;
    }

    let variants: Vec<Vec<&'static str>> = match mnemonic_lc.as_str() {
        "nop" => vec![vec![]],
        "mv" => vec![vec!["rd", "rs"]],
        "li" => vec![vec!["rd", "imm"]],
        "j" => vec![vec!["label"]],
        "call" => vec![vec!["label"]],
        "jr" => vec![vec!["rs"]],
        "ret" => vec![vec![]],
        "subi" => vec![vec!["rd", "rs1", "imm"]],

        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" | "mul"
        | "mulh" | "mulhsu" | "mulhu" | "div" | "divu" | "rem" | "remu" => {
            vec![vec!["rd", "rs1", "rs2"]]
        }

        "addi" | "andi" | "ori" | "xori" | "slti" | "sltiu" => vec![vec!["rd", "rs1", "imm"]],
        "slli" | "srli" | "srai" => vec![vec!["rd", "rs1", "shamt"]],

        "lb" | "lh" | "lw" | "lbu" | "lhu" => vec![vec!["rd", "imm(rs1)"]],
        "sb" | "sh" | "sw" => vec![vec!["rs2", "imm(rs1)"]],

        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => vec![vec!["rs1", "rs2", "label"]],

        "lui" => vec![vec!["rd", "imm"]],
        "auipc" => vec![vec!["rd", "imm"]],

        "jal" => vec![vec!["label"], vec!["rd", "label"]],
        "jalr" => vec![vec!["rd", "rs1", "imm"]],

        "ecall" => vec![vec![]],
        "ebreak" => vec![vec![]],
        "halt" => vec![vec![]],

        _ => match mnemonic_raw {
            "la" => vec![vec!["rd", "label"]],
            "push" => vec![vec!["rs"]],
            "pop" => vec![vec!["rd"]],
            "print" => vec![vec!["rd"]],
            "printStr" | "printString" => vec![vec!["label"]],
            "printStrLn" => vec![vec!["label"]],
            "read" => vec![vec!["label"]],
            "readByte" => vec![vec!["label"]],
            "readHalf" => vec![vec!["label"]],
            "readWord" => vec![vec!["label"]],
            "randByte" | "randHalf" | "randWord" => vec![vec!["label"]],
            "randBytes" => vec![vec!["label", "len"]],
            _ => return None,
        },
    };

    Some(build_ghost_variants_spans(mnemonic_raw, ops_len, &variants))
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

        let next_idx = if ops_len < needed { Some(ops_len) } else { None };
        for (oi, expr) in operands.iter().enumerate() {
            if oi > 0 {
                out.push(Span::styled(", ".to_string(), base));
            }
            out.extend(style_ghost_operand_expr(expr, base, next_idx == Some(oi)));
        }
    }

    out
}

fn style_ghost_operand_expr(expr: &str, base: Style, is_next: bool) -> Vec<Span<'static>> {
    fn token_style(tok: &str, base: Style, is_next: bool) -> Style {
        let mut style = match tok {
            "rd" => base.fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "rs1" | "rs2" | "rs" => base.fg(Color::Cyan),
            "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" => base.fg(Color::LightGreen),
            "label" => base.fg(Color::Magenta),
            _ if is_reg_token(tok) => base.fg(Color::LightBlue),
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
        let style = token_style(token, base, is_next);
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

fn is_reg_token(tok: &str) -> bool {
    let t = tok.to_ascii_lowercase();
    if let Some(n) = t.strip_prefix('x') {
        if let Ok(v) = n.parse::<u8>() {
            return v < 32;
        }
    }
    matches!(
        t.as_str(),
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
    )
}
