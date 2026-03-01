use comfy_table::{
    presets::ASCII_BORDERS_ONLY, Cell as ACell, ContentArrangement, Row as ARow, Table as ATable,
};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::App;

#[derive(Clone, Copy)]
struct DocRow {
    ty: &'static str,
    mnemonic: &'static str,
    operands: &'static str,
    desc: &'static str,
    expands: &'static str,
}

const DOCS: &[DocRow] = &[
    // ---------- R-type ----------
    DocRow { ty: "R", mnemonic: "add", operands: "rd, rs1, rs2", desc: "rd = rs1 + rs2 (signed)", expands: "" },
    DocRow { ty: "R", mnemonic: "sub", operands: "rd, rs1, rs2", desc: "rd = rs1 - rs2 (signed)", expands: "" },
    DocRow { ty: "R", mnemonic: "and", operands: "rd, rs1, rs2", desc: "rd = rs1 & rs2 (bitwise)", expands: "" },
    DocRow { ty: "R", mnemonic: "or", operands: "rd, rs1, rs2", desc: "rd = rs1 | rs2 (bitwise)", expands: "" },
    DocRow { ty: "R", mnemonic: "xor", operands: "rd, rs1, rs2", desc: "rd = rs1 ^ rs2 (bitwise)", expands: "" },
    DocRow { ty: "R", mnemonic: "sll", operands: "rd, rs1, rs2", desc: "rd = rs1 << (rs2 & 31)", expands: "" },
    DocRow { ty: "R", mnemonic: "srl", operands: "rd, rs1, rs2", desc: "rd = logical rs1 >> (rs2 & 31)", expands: "" },
    DocRow { ty: "R", mnemonic: "sra", operands: "rd, rs1, rs2", desc: "rd = arithmetic rs1 >> (rs2 & 31)", expands: "" },
    DocRow { ty: "R", mnemonic: "slt", operands: "rd, rs1, rs2", desc: "rd = 1 if rs1 < rs2 (signed) else 0", expands: "" },
    DocRow { ty: "R", mnemonic: "sltu", operands: "rd, rs1, rs2", desc: "rd = 1 if rs1 < rs2 (unsigned) else 0", expands: "" },
    // ---------- M extension ----------
    DocRow { ty: "R(M)", mnemonic: "mul", operands: "rd, rs1, rs2", desc: "rd = (rs1 * rs2) low 32b", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "mulh", operands: "rd, rs1, rs2", desc: "rd = (rs1 * rs2) high 32b signed", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "mulhsu", operands: "rd, rs1, rs2", desc: "rd = (signed rs1 * unsigned rs2) high 32b", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "mulhu", operands: "rd, rs1, rs2", desc: "rd = (rs1 * rs2) high 32b unsigned", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "div", operands: "rd, rs1, rs2", desc: "rd = rs1 / rs2 (signed)", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "divu", operands: "rd, rs1, rs2", desc: "rd = rs1 / rs2 (unsigned)", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "rem", operands: "rd, rs1, rs2", desc: "rd = rs1 % rs2 (signed)", expands: "" },
    DocRow { ty: "R(M)", mnemonic: "remu", operands: "rd, rs1, rs2", desc: "rd = rs1 % rs2 (unsigned)", expands: "" },
    // ---------- I-type ----------
    DocRow { ty: "I", mnemonic: "addi", operands: "rd, rs1, imm", desc: "rd = rs1 + imm (12-bit signed)", expands: "" },
    DocRow { ty: "I", mnemonic: "xori", operands: "rd, rs1, imm", desc: "rd = rs1 ^ imm", expands: "" },
    DocRow { ty: "I", mnemonic: "ori", operands: "rd, rs1, imm", desc: "rd = rs1 | imm", expands: "" },
    DocRow { ty: "I", mnemonic: "andi", operands: "rd, rs1, imm", desc: "rd = rs1 & imm", expands: "" },
    DocRow { ty: "I", mnemonic: "slti", operands: "rd, rs1, imm", desc: "rd = 1 if rs1 < imm (signed) else 0", expands: "" },
    DocRow { ty: "I", mnemonic: "sltiu", operands: "rd, rs1, imm", desc: "rd = 1 if rs1 < imm (unsigned) else 0", expands: "" },
    DocRow { ty: "I", mnemonic: "slli", operands: "rd, rs1, shamt", desc: "rd = rs1 << shamt (0..31)", expands: "" },
    DocRow { ty: "I", mnemonic: "srli", operands: "rd, rs1, shamt", desc: "rd = logical rs1 >> shamt", expands: "" },
    DocRow { ty: "I", mnemonic: "srai", operands: "rd, rs1, shamt", desc: "rd = arithmetic rs1 >> shamt", expands: "" },
    // ---------- Loads ----------
    DocRow { ty: "Load", mnemonic: "lb", operands: "rd, imm(rs1)", desc: "Load 1 byte signed from memory[rs1+imm]", expands: "" },
    DocRow { ty: "Load", mnemonic: "lh", operands: "rd, imm(rs1)", desc: "Load 2 bytes signed from memory[rs1+imm]", expands: "" },
    DocRow { ty: "Load", mnemonic: "lw", operands: "rd, imm(rs1)", desc: "Load 4 bytes from memory[rs1+imm]", expands: "" },
    DocRow { ty: "Load", mnemonic: "lbu", operands: "rd, imm(rs1)", desc: "Load 1 byte unsigned from memory[rs1+imm]", expands: "" },
    DocRow { ty: "Load", mnemonic: "lhu", operands: "rd, imm(rs1)", desc: "Load 2 bytes unsigned from memory[rs1+imm]", expands: "" },
    // ---------- Stores ----------
    DocRow { ty: "Store", mnemonic: "sb", operands: "rs2, imm(rs1)", desc: "Store low 1 byte of rs2 to memory[rs1+imm]", expands: "" },
    DocRow { ty: "Store", mnemonic: "sh", operands: "rs2, imm(rs1)", desc: "Store low 2 bytes of rs2 to memory[rs1+imm]", expands: "" },
    DocRow { ty: "Store", mnemonic: "sw", operands: "rs2, imm(rs1)", desc: "Store 4 bytes of rs2 to memory[rs1+imm]", expands: "" },
    // ---------- Branches ----------
    DocRow { ty: "Branch", mnemonic: "beq", operands: "rs1, rs2, label", desc: "Branch if rs1==rs2. label: instruction label", expands: "" },
    DocRow { ty: "Branch", mnemonic: "bne", operands: "rs1, rs2, label", desc: "Branch if rs1!=rs2. label: instruction label", expands: "" },
    DocRow { ty: "Branch", mnemonic: "blt", operands: "rs1, rs2, label", desc: "Branch if rs1<rs2 (signed). label: instruction label", expands: "" },
    DocRow { ty: "Branch", mnemonic: "bge", operands: "rs1, rs2, label", desc: "Branch if rs1>=rs2 (signed). label: instruction label", expands: "" },
    DocRow { ty: "Branch", mnemonic: "bltu", operands: "rs1, rs2, label", desc: "Branch if rs1<rs2 (unsigned). label: instruction label", expands: "" },
    DocRow { ty: "Branch", mnemonic: "bgeu", operands: "rs1, rs2, label", desc: "Branch if rs1>=rs2 (unsigned). label: instruction label", expands: "" },
    // ---------- U-type ----------
    DocRow { ty: "U", mnemonic: "lui", operands: "rd, imm20", desc: "rd = imm20 << 12 (upper 20 bits)", expands: "" },
    DocRow { ty: "U", mnemonic: "auipc", operands: "rd, imm20", desc: "rd = PC + (imm20 << 12)", expands: "" },
    // ---------- Jumps ----------
    DocRow { ty: "Jump", mnemonic: "jal", operands: "label | rd, label", desc: "Jump and link. If only label is given: rd=ra", expands: "" },
    DocRow { ty: "Jump", mnemonic: "jalr", operands: "rd, rs1, imm", desc: "Jump to rs1+imm & ~1; rd=return addr", expands: "" },
    // ---------- System ----------
    DocRow { ty: "SYS", mnemonic: "ecall", operands: "", desc: "System call. a7 selects service; a0 holds arg/result", expands: "" },
    DocRow { ty: "SYS", mnemonic: "ebreak", operands: "", desc: "Stop execution (debug break)", expands: "" },
    DocRow { ty: "SYS", mnemonic: "halt", operands: "", desc: "Stop execution (alias of ebreak)", expands: "" },
    // ---------- Pseudo-instructions ----------
    DocRow { ty: "Pseudo", mnemonic: "nop", operands: "", desc: "No operation", expands: "addi x0, x0, 0" },
    DocRow { ty: "Pseudo", mnemonic: "mv", operands: "rd, rs", desc: "Move rd = rs", expands: "addi rd, rs, 0" },
    DocRow { ty: "Pseudo", mnemonic: "li", operands: "rd, imm12", desc: "Load small immediate (12-bit) into rd", expands: "addi rd, x0, imm" },
    DocRow { ty: "Pseudo", mnemonic: "subi", operands: "rd, rs1, imm", desc: "rd = rs1 - imm", expands: "addi rd, rs1, -imm" },
    DocRow { ty: "Pseudo", mnemonic: "j", operands: "label", desc: "Unconditional jump to label", expands: "jal x0, label" },
    DocRow { ty: "Pseudo", mnemonic: "call", operands: "label", desc: "Call subroutine", expands: "jal ra, label" },
    DocRow { ty: "Pseudo", mnemonic: "jr", operands: "rs", desc: "Jump register", expands: "jalr x0, rs, 0" },
    DocRow { ty: "Pseudo", mnemonic: "ret", operands: "", desc: "Return", expands: "jalr x0, ra, 0" },
    DocRow { ty: "Pseudo", mnemonic: "la", operands: "rd, label", desc: "Load address of label into rd", expands: "lui rd, hi; addi rd, rd, lo" },
    DocRow { ty: "Pseudo", mnemonic: "push", operands: "rs", desc: "sp -= 4; store rs at 4(sp)", expands: "addi sp, sp, -4; sw rs, 4(sp)" },
    DocRow { ty: "Pseudo", mnemonic: "pop", operands: "rd", desc: "load rd from 4(sp); sp += 4", expands: "lw rd, 4(sp); addi sp, sp, 4" },
    DocRow { ty: "Pseudo", mnemonic: "print", operands: "rd", desc: "Print integer in rd (ecall a7=1000, a0=value)", expands: "addi a7, x0, 1000; addi a0, rd, 0; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "printStr", operands: "label", desc: "Print NUL string at label without newline", expands: "addi a7, x0, 1001; lui a0, hi; addi a0, a0, lo; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "printStrLn", operands: "label", desc: "Print NUL string at label and newline", expands: "addi a7, x0, 1002; lui a0, hi; addi a0, a0, lo; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "read", operands: "label", desc: "Read line into memory at label; NUL-terminate", expands: "addi a7, x0, 1003; lui a0, hi; addi a0, a0, lo; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "readByte", operands: "label", desc: "Read number and store 1 byte at label", expands: "addi a7, x0, 1010; lui a0, hi; addi a0, a0, lo; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "readHalf", operands: "label", desc: "Read number and store 2 bytes at label (little-endian)", expands: "addi a7, x0, 1011; lui a0, hi; addi a0, a0, lo; ecall" },
    DocRow { ty: "Pseudo", mnemonic: "readWord", operands: "label", desc: "Read number and store 4 bytes at label (little-endian)", expands: "addi a7, x0, 1012; lui a0, hi; addi a0, a0, lo; ecall" },
];

fn build_docs_table_string(width: u16) -> String {
    build_docs_table_filtered(width, "")
}

fn build_docs_table_filtered(width: u16, query: &str) -> String {
    let q = query.to_lowercase();
    let filtered: Vec<&DocRow> = DOCS.iter()
        .filter(|r| q.is_empty()
            || r.mnemonic.to_lowercase().contains(&q)
            || r.operands.to_lowercase().contains(&q)
            || r.desc.to_lowercase().contains(&q)
            || r.ty.to_lowercase().contains(&q))
        .collect();

    let mut table = ATable::new();
    table.load_preset(ASCII_BORDERS_ONLY);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    if width > 0 {
        table.set_width(width);
    }

    table.set_header(ARow::from(vec![
        ACell::new("Type"),
        ACell::new("Mnemonic"),
        ACell::new("Operands"),
        ACell::new("Description"),
        ACell::new("Expands"),
    ]));

    for r in filtered {
        table.add_row(ARow::from(vec![
            ACell::new(r.ty),
            ACell::new(r.mnemonic),
            ACell::new(r.operands),
            ACell::new(r.desc),
            ACell::new(r.expands),
        ]));
    }

    table.to_string()
}

fn is_register_token(token: &str) -> bool {
    if let Some(n) = token.strip_prefix('x') {
        if let Ok(v) = n.parse::<u8>() {
            return v <= 31;
        }
    }

    matches!(token, "ra" | "sp")
        || (token.starts_with('a') && token[1..].parse::<u8>().is_ok_and(|v| v <= 7))
        || (token.starts_with('t') && token[1..].parse::<u8>().is_ok_and(|v| v <= 6))
        || (token.starts_with('s') && token[1..].parse::<u8>().is_ok_and(|v| v <= 11))
}

fn style_for_token(token: &str) -> Option<Style> {
    match token {
        "rd" => Some(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        "rs1" | "rs2" | "rs" => Some(Style::default().fg(Color::Cyan)),
        "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" => {
            Some(Style::default().fg(Color::LightGreen))
        }
        "label" => Some(Style::default().fg(Color::Magenta)),
        _ if is_register_token(token) => Some(Style::default().fg(Color::LightBlue)),
        _ => None,
    }
}

fn style_table_line(line: &str) -> Line<'_> {
    let mut spans: Vec<Span> = Vec::new();
    let mut token = String::new();
    let mut sep = String::new();

    let flush_sep = |spans: &mut Vec<Span>, sep: &mut String| {
        if !sep.is_empty() {
            spans.push(Span::raw(std::mem::take(sep)));
        }
    };
    let flush_token = |spans: &mut Vec<Span>, token: &mut String| {
        if token.is_empty() {
            return;
        }
        let t = std::mem::take(token);
        if let Some(style) = style_for_token(&t) {
            spans.push(Span::styled(t, style));
        } else {
            spans.push(Span::raw(t));
        }
    };

    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            flush_sep(&mut spans, &mut sep);
            token.push(ch);
        } else {
            flush_token(&mut spans, &mut token);
            sep.push(ch);
        }
    }
    flush_token(&mut spans, &mut token);
    flush_sep(&mut spans, &mut sep);

    Line::from(spans)
}

pub(crate) fn docs_body_line_count(width: u16) -> usize {
    build_docs_table_string(width)
        .lines()
        .count()
        .saturating_sub(4)
}

pub(super) fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    // Reserve 1 extra line for search bar if open
    let search_bar_h: u16 = if app.docs.search_open { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(search_bar_h),
            Constraint::Min(0),
        ])
        .split(area);

    let meta_area = chunks[0];
    let search_area = chunks[1];
    let table_area = chunks[2];

    // Show Ctrl+F hint in the header
    let search_hint = if app.docs.search_open { "" } else { "  Ctrl+F=search" };
    let meta_lines = vec![
        Line::from(vec![
            Span::styled(
                "Instruction Reference • Up/Down/PgUp/PgDn scroll",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(search_hint, Style::default().fg(Color::DarkGray)),
        ]),
        style_table_line(
            "Legend: rd=dest, rs1/rs2/rs=src, imm/shamt=imm, label=label • Pseudo: see Expands",
        ),
    ];
    f.render_widget(Paragraph::new(meta_lines), meta_area);

    // Render search bar
    if app.docs.search_open {
        let bar_style = Style::default().fg(Color::DarkGray).bg(Color::Rgb(30, 30, 50));
        let label_s = Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 50));
        let text_s = Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 30, 50));
        let info_s = Style::default().fg(Color::DarkGray).bg(Color::Rgb(30, 30, 50));
        let bar_line = Line::from(vec![
            Span::styled(" Find: ", label_s),
            Span::styled(app.docs.search_query.clone(), text_s),
            Span::styled("  Esc=close", info_s),
        ]);
        f.render_widget(Paragraph::new(bar_line).style(bar_style), search_area);

        // Set cursor in search bar
        let prefix_len = " Find: ".len() as u16;
        let cursor_x = (search_area.x + prefix_len + app.docs.search_query.chars().count() as u16)
            .min(search_area.x + search_area.width.saturating_sub(1));
        if search_area.height > 0 {
            f.set_cursor_position((cursor_x, search_area.y));
        }
    }

    if table_area.height == 0 || table_area.width == 0 {
        return;
    }

    // Use filtered table when search query is non-empty
    let table_str = if app.docs.search_open && !app.docs.search_query.is_empty() {
        build_docs_table_filtered(table_area.width, &app.docs.search_query)
    } else {
        build_docs_table_string(table_area.width)
    };
    let all_lines: Vec<&str> = table_str.lines().collect();

    if all_lines.is_empty() {
        return;
    }

    if all_lines.len() < 4 || table_area.height < 4 {
        let lines = all_lines
            .iter()
            .take(table_area.height as usize)
            .map(|l| style_table_line(l))
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), table_area);
        return;
    }

    let header_lines = &all_lines[0..3];
    let footer_line = all_lines[all_lines.len() - 1];
    let body_lines = &all_lines[3..all_lines.len() - 1];

    let viewport_h = table_area.height.saturating_sub(4) as usize;
    if viewport_h == 0 {
        let lines = header_lines
            .iter()
            .map(|l| style_table_line(l))
            .chain(std::iter::once(style_table_line(footer_line)))
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), table_area);
        return;
    }

    let max_start = body_lines.len().saturating_sub(viewport_h);
    let start = app.docs.scroll.min(max_start);
    let end = (start + viewport_h).min(body_lines.len());

    let mut lines = Vec::with_capacity(3 + viewport_h + 1);
    lines.extend(header_lines.iter().map(|l| style_table_line(l)));
    lines.extend(body_lines[start..end].iter().map(|l| style_table_line(l)));

    let rendered_body = end - start;
    for _ in rendered_body..viewport_h {
        lines.push(Line::raw(""));
    }

    lines.push(style_table_line(footer_line));

    f.render_widget(Paragraph::new(lines), table_area);
}
