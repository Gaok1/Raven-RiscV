use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::cmp::min;

use super::App;
use crate::ui::app::Lang;
use crate::ui::i18n::T;

use comfy_table::{Table as ATable, Row as ARow, Cell as ACell, presets::ASCII_BORDERS_ONLY, ContentArrangement};

pub(super) fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    // Split header (1 row) and body
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .split(area);

    // Header: language toggle on the right
    let lang_label = match app.lang { Lang::EN => "[EN]", Lang::PT => "[PT]" };
    let btn_width = 4u16;
    let header_area = chunks[0];
    let btn_rect = Rect::new(
        header_area.x + header_area.width.saturating_sub(btn_width),
        header_area.y,
        btn_width,
        1,
    );

    // Draw left header hint
    let header_hint = match app.lang {
        Lang::EN => "Instruction Reference  •  Up/Down/PageUp/PageDown",
        Lang::PT => "Referencia de Instrucoes  •  Cima/Baixo/PagUp/PagDn",
    };
    let left = Paragraph::new(header_hint);
    f.render_widget(left, header_area);

    // Draw language button
    let button = Paragraph::new(Line::from(Span::styled(
        lang_label,
        Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD),
    )));
    f.render_widget(button, btn_rect);

    // Body: build an ASCII table of instructions grouped by format
    let title_t = T::new("Instruction Reference (EN)", "Referencia de Instrucoes (PT-BR)");
    let h_format_t = T::new("Format", "Formato");
    let h_mnemonic_t = T::new("Mnemonic", "Mnemonico");
    let h_syntax_t = T::new("Syntax", "Sintaxe");
    let h_desc_t = T::new("Description", "Descricao");

    let mut rows_all: Vec<(String, String, String, String)> = Vec::new();
    let mut push = |fmt: &str, mnem: &str, syn: &str, desc: &str| {
        rows_all.push((fmt.to_string(), mnem.to_string(), syn.to_string(), desc.to_string()));
    };

    // R-type
    push("R", "add",  "add rd, rs1, rs2", "rd = rs1 + rs2 (signed)" );
    push("R", "sub",  "sub rd, rs1, rs2", "rd = rs1 - rs2 (signed)" );
    push("R", "and",  "and rd, rs1, rs2", "rd = rs1 & rs2 (bitwise)" );
    push("R", "or",   "or rd, rs1, rs2",  "rd = rs1 | rs2 (bitwise)" );
    push("R", "xor",  "xor rd, rs1, rs2", "rd = rs1 ^ rs2 (bitwise)" );
    push("R", "sll",  "sll rd, rs1, rs2", "rd = rs1 << (rs2 & 31)" );
    push("R", "srl",  "srl rd, rs1, rs2", "rd = logical rs1 >> (rs2 & 31)" );
    push("R", "sra",  "sra rd, rs1, rs2", "rd = arithmetic rs1 >> (rs2 & 31)" );
    push("R", "slt",  "slt rd, rs1, rs2", "rd = 1 if rs1 < rs2 (signed) else 0" );
    push("R", "sltu", "sltu rd, rs1, rs2", "rd = 1 if rs1 < rs2 (unsigned) else 0" );
    // M extension
    push("R(M)", "mul",    "mul rd, rs1, rs2",   "rd = (rs1 * rs2) low 32b" );
    push("R(M)", "mulh",   "mulh rd, rs1, rs2",  "rd = (rs1 * rs2) high 32b signed" );
    push("R(M)", "mulhsu", "mulhsu rd, rs1, rs2","rd = (signed rs1 * unsigned rs2) high 32b" );
    push("R(M)", "mulhu",  "mulhu rd, rs1, rs2", "rd = (rs1 * rs2) high 32b unsigned" );
    push("R(M)", "div",    "div rd, rs1, rs2",   "rd = rs1 / rs2 (signed)" );
    push("R(M)", "divu",   "divu rd, rs1, rs2",  "rd = rs1 / rs2 (unsigned)" );
    push("R(M)", "rem",    "rem rd, rs1, rs2",   "rd = rs1 % rs2 (signed)" );
    push("R(M)", "remu",   "remu rd, rs1, rs2",  "rd = rs1 % rs2 (unsigned)" );

    // I-type
    push("I", "addi", "addi rd, rs1, imm",  "rd = rs1 + imm (12-bit signed)" );
    push("I", "xori", "xori rd, rs1, imm",  "rd = rs1 ^ imm" );
    push("I", "ori",  "ori rd, rs1, imm",   "rd = rs1 | imm" );
    push("I", "andi", "andi rd, rs1, imm",  "rd = rs1 & imm" );
    push("I", "slli", "slli rd, rs1, shamt","rd = rs1 << shamt (0..31)" );
    push("I", "srli", "srli rd, rs1, shamt","rd = logical rs1 >> shamt" );
    push("I", "srai", "srai rd, rs1, shamt","rd = arithmetic rs1 >> shamt" );

    // Loads
    push("Load", "lb",  "lb rd, imm(rs1)",  "Load 1 byte signed from memory[rs1+imm]" );
    push("Load", "lh",  "lh rd, imm(rs1)",  "Load 2 bytes signed from memory[rs1+imm]" );
    push("Load", "lw",  "lw rd, imm(rs1)",  "Load 4 bytes from memory[rs1+imm]" );
    push("Load", "lbu", "lbu rd, imm(rs1)", "Load 1 byte unsigned from memory[rs1+imm]" );
    push("Load", "lhu", "lhu rd, imm(rs1)", "Load 2 bytes unsigned from memory[rs1+imm]" );

    // Stores
    push("Store", "sb", "sb rs2, imm(rs1)", "Store low 1 byte of rs2 to memory[rs1+imm]" );
    push("Store", "sh", "sh rs2, imm(rs1)", "Store low 2 bytes of rs2 to memory[rs1+imm]" );
    push("Store", "sw", "sw rs2, imm(rs1)", "Store 4 bytes of rs2 to memory[rs1+imm]" );

    // Branches
    push("Branch", "beq",  "beq rs1, rs2, label",  "Branch if rs1==rs2. label: instruction label" );
    push("Branch", "bne",  "bne rs1, rs2, label",  "Branch if rs1!=rs2. label: instruction label" );
    push("Branch", "blt",  "blt rs1, rs2, label",  "Branch if rs1<rs2 (signed). label: instruction label" );
    push("Branch", "bge",  "bge rs1, rs2, label",  "Branch if rs1>=rs2 (signed). label: instruction label" );
    push("Branch", "bltu", "bltu rs1, rs2, label", "Branch if rs1<rs2 (unsigned). label: instruction label" );
    push("Branch", "bgeu", "bgeu rs1, rs2, label", "Branch if rs1>=rs2 (unsigned). label: instruction label" );

    // U-type
    push("U", "lui",   "lui rd, imm20",  "rd = imm20 << 12 (upper 20 bits)" );
    push("U", "auipc", "auipc rd, imm20","rd = PC + (imm20 << 12)" );

    // Jumps
    push("J", "jal",  "jal rd, label",      "Jump and link. rd=return addr; label: instruction label" );
    push("I", "jalr", "jalr rd, rs1, imm",  "Jump to rs1+imm & ~1; rd=return addr" );

    // System
    push("SYS", "ecall", "ecall", "System call. a7 selects service; a0 holds arg/result" );
    push("SYS", "halt",  "halt",  "Stop execution" );

    // Pseudo-instructions (assembler)
    push("Pseudo", "nop",   "nop",               "No operation" );
    push("Pseudo", "mv",    "mv rd, rs",         "Move rd = rs (addi rd, rs, 0)" );
    push("Pseudo", "li",    "li rd, imm12",      "Load small immediate (12-bit) into rd" );
    push("Pseudo", "subi",  "subi rd, rs1, imm", "rd = rs1 - imm (addi with negative)" );
    push("Pseudo", "j",     "j label",           "Unconditional jump to label (instruction label)" );
    push("Pseudo", "call",  "call label",        "Call subroutine (jal ra, label). label: instruction label" );
    push("Pseudo", "jr",    "jr rs",             "Jump register (jalr x0, rs, 0)" );
    push("Pseudo", "ret",   "ret",               "Return (jalr x0, ra, 0)" );
    push("Pseudo", "la",    "la rd, label",      "Load address of label into rd (lui/addi). label: data or instruction label" );
    push("Pseudo", "push",  "push rs",           "sp -= 4; store rs at 4(sp)" );
    push("Pseudo", "pop",   "pop rd",            "load rd from 0(sp); sp += 4" );
    push("Pseudo", "print",     "print rd",              "Print integer in rd (ecall a7=1, a0=rd)" );
    push("Pseudo", "printStr",  "printStr label",       "Print NUL string at label without newline (data label)" );
    push("Pseudo", "printStrLn","printStrLn label",     "Print NUL string at label and newline (data label)" );
    push("Pseudo", "read",      "read label",            "Read line into memory at label; NUL-terminate (data label)" );
    push("Pseudo", "readByte",  "readByte label",       "Read number (dec/0xhex) and store 1 byte at label" );
    push("Pseudo", "readHalf",  "readHalf label",       "Read number and store 2 bytes at label (little-endian)" );
    push("Pseudo", "readWord",  "readWord label",       "Read number and store 4 bytes at label (little-endian)" );

    // Compute pagination: available inner height for the ASCII table
    let body_area = chunks[1];
    let inner_h = body_area.height.saturating_sub(2) as usize; // outer Paragraph borders
    // ASCII table uses: top border + header + header border + rows + bottom border
    // so data rows that fit = inner_h.saturating_sub(4)
    let page_rows = inner_h.saturating_sub(4);
    let total_rows = rows_all.len();
    let max_start = total_rows.saturating_sub(page_rows);
    let start = app.docs_scroll.min(max_start);
    let end = min(total_rows, start + page_rows);

    // Build ASCII table for this page
    let mut t = ATable::new();
    t.load_preset(ASCII_BORDERS_ONLY);
    t.set_content_arrangement(ContentArrangement::Dynamic);
    // Try to constrain table width to the inner width of Paragraph block
    let inner_w = body_area.width.saturating_sub(2) as u16;
    if inner_w > 0 { t.set_width(inner_w); }
    t.set_header(ARow::from(vec![
        ACell::new(h_format_t.get(app.lang)),
        ACell::new(h_mnemonic_t.get(app.lang)),
        ACell::new(h_syntax_t.get(app.lang)),
        ACell::new(h_desc_t.get(app.lang)),
    ]));
    for (fmt, m, syn, desc) in rows_all[start..end].iter() {
        t.add_row(ARow::from(vec![ACell::new(fmt), ACell::new(m), ACell::new(syn), ACell::new(desc)]));
    }

    let table_str = t.to_string();
    let lines: Vec<Line> = table_str
        .lines()
        .map(|l| Line::raw(l.to_string()))
        .collect();

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title_t.get(app.lang)));

    f.render_widget(para, body_area);
}

// Public helper: total number of data rows in Docs (for clamping scroll)
pub fn docs_total_rows() -> usize {
    let mut rows_all: Vec<(String, String, String, String)> = Vec::new();
    let mut push = |fmt: &str, mnem: &str, syn: &str, desc: &str| {
        rows_all.push((fmt.to_string(), mnem.to_string(), syn.to_string(), desc.to_string()));
    };
    // Keep in sync with render_docs rows
    // R-type
    push("R", "add",  "add rd, rs1, rs2", "rd = rs1 + rs2 (signed)" );
    push("R", "sub",  "sub rd, rs1, rs2", "rd = rs1 - rs2 (signed)" );
    push("R", "and",  "and rd, rs1, rs2", "rd = rs1 & rs2 (bitwise)" );
    push("R", "or",   "or rd, rs1, rs2",  "rd = rs1 | rs2 (bitwise)" );
    push("R", "xor",  "xor rd, rs1, rs2", "rd = rs1 ^ rs2 (bitwise)" );
    push("R", "sll",  "sll rd, rs1, rs2", "rd = rs1 << (rs2 & 31)" );
    push("R", "srl",  "srl rd, rs1, rs2", "rd = logical rs1 >> (rs2 & 31)" );
    push("R", "sra",  "sra rd, rs1, rs2", "rd = arithmetic rs1 >> (rs2 & 31)" );
    push("R", "slt",  "slt rd, rs1, rs2", "rd = 1 if rs1 < rs2 (signed) else 0" );
    push("R", "sltu", "sltu rd, rs1, rs2", "rd = 1 if rs1 < rs2 (unsigned) else 0" );
    // M extension
    push("R(M)", "mul",    "mul rd, rs1, rs2",   "rd = (rs1 * rs2) low 32b" );
    push("R(M)", "mulh",   "mulh rd, rs1, rs2",  "rd = (rs1 * rs2) high 32b signed" );
    push("R(M)", "mulhsu", "mulhsu rd, rs1, rs2","rd = (signed rs1 * unsigned rs2) high 32b" );
    push("R(M)", "mulhu",  "mulhu rd, rs1, rs2", "rd = (rs1 * rs2) high 32b unsigned" );
    push("R(M)", "div",    "div rd, rs1, rs2",   "rd = rs1 / rs2 (signed)" );
    push("R(M)", "divu",   "divu rd, rs1, rs2",  "rd = rs1 / rs2 (unsigned)" );
    push("R(M)", "rem",    "rem rd, rs1, rs2",   "rd = rs1 % rs2 (signed)" );
    push("R(M)", "remu",   "remu rd, rs1, rs2",  "rd = rs1 % rs2 (unsigned)" );
    // I-type
    push("I", "addi", "addi rd, rs1, imm",  "rd = rs1 + imm (12-bit signed)" );
    push("I", "xori", "xori rd, rs1, imm",  "rd = rs1 ^ imm" );
    push("I", "ori",  "ori rd, rs1, imm",   "rd = rs1 | imm" );
    push("I", "andi", "andi rd, rs1, imm",  "rd = rs1 & imm" );
    push("I", "slli", "slli rd, rs1, shamt","rd = rs1 << shamt (0..31)" );
    push("I", "srli", "srli rd, rs1, shamt","rd = logical rs1 >> shamt" );
    push("I", "srai", "srai rd, rs1, shamt","rd = arithmetic rs1 >> shamt" );
    // Loads
    push("Load", "lb",  "lb rd, imm(rs1)",  "Load 1 byte signed from memory[rs1+imm]" );
    push("Load", "lh",  "lh rd, imm(rs1)",  "Load 2 bytes signed from memory[rs1+imm]" );
    push("Load", "lw",  "lw rd, imm(rs1)",  "Load 4 bytes from memory[rs1+imm]" );
    push("Load", "lbu", "lbu rd, imm(rs1)", "Load 1 byte unsigned from memory[rs1+imm]" );
    push("Load", "lhu", "lhu rd, imm(rs1)", "Load 2 bytes unsigned from memory[rs1+imm]" );
    // Stores
    push("Store", "sb", "sb rs2, imm(rs1)", "Store low 1 byte of rs2 to memory[rs1+imm]" );
    push("Store", "sh", "sh rs2, imm(rs1)", "Store low 2 bytes of rs2 to memory[rs1+imm]" );
    push("Store", "sw", "sw rs2, imm(rs1)", "Store 4 bytes of rs2 to memory[rs1+imm]" );
    // Branches
    push("Branch", "beq",  "beq rs1, rs2, label",  "Branch if rs1==rs2. label: instruction label" );
    push("Branch", "bne",  "bne rs1, rs2, label",  "Branch if rs1!=rs2. label: instruction label" );
    push("Branch", "blt",  "blt rs1, rs2, label",  "Branch if rs1<rs2 (signed). label: instruction label" );
    push("Branch", "bge",  "bge rs1, rs2, label",  "Branch if rs1>=rs2 (signed). label: instruction label" );
    push("Branch", "bltu", "bltu rs1, rs2, label", "Branch if rs1<rs2 (unsigned). label: instruction label" );
    push("Branch", "bgeu", "bgeu rs1, rs2, label", "Branch if rs1>=rs2 (unsigned). label: instruction label" );
    // U-type
    push("U", "lui",   "lui rd, imm20",  "rd = imm20 << 12 (upper 20 bits)" );
    push("U", "auipc", "auipc rd, imm20","rd = PC + (imm20 << 12)" );
    // Jumps
    push("J", "jal",  "jal rd, label",      "Jump and link. rd=return addr; label: instruction label" );
    push("I", "jalr", "jalr rd, rs1, imm",  "Jump to rs1+imm & ~1; rd=return addr" );
    // System
    push("SYS", "ecall", "ecall", "System call. a7 selects service; a0 holds arg/result" );
    push("SYS", "halt",  "halt",  "Stop execution" );
    // Pseudo-instructions (assembler)
    push("Pseudo", "nop",   "nop",               "No operation" );
    push("Pseudo", "mv",    "mv rd, rs",         "Move rd = rs (addi rd, rs, 0)" );
    push("Pseudo", "li",    "li rd, imm12",      "Load small immediate (12-bit) into rd" );
    push("Pseudo", "subi",  "subi rd, rs1, imm", "rd = rs1 - imm (addi with negative)" );
    push("Pseudo", "j",     "j label",           "Unconditional jump to label (instruction label)" );
    push("Pseudo", "call",  "call label",        "Call subroutine (jal ra, label). label: instruction label" );
    push("Pseudo", "jr",    "jr rs",             "Jump register (jalr x0, rs, 0)" );
    push("Pseudo", "ret",   "ret",               "Return (jalr x0, ra, 0)" );
    push("Pseudo", "la",    "la rd, label",      "Load address of label into rd (lui/addi). label: data or instruction label" );
    push("Pseudo", "push",  "push rs",           "sp -= 4; store rs at 4(sp)" );
    push("Pseudo", "pop",   "pop rd",            "load rd from 0(sp); sp += 4" );
    push("Pseudo", "print",     "print rd",              "Print integer in rd (ecall a7=1, a0=rd)" );
    push("Pseudo", "printStr",  "printStr label",       "Print NUL string at label without newline (data label)" );
    push("Pseudo", "printStrLn","printStrLn label",     "Print NUL string at label and newline (data label)" );
    push("Pseudo", "read",      "read label",            "Read line into memory at label; NUL-terminate (data label)" );
    push("Pseudo", "readByte",  "readByte label",       "Read number (dec/0xhex) and store 1 byte at label" );
    push("Pseudo", "readHalf",  "readHalf label",       "Read number and store 2 bytes at label (little-endian)" );
    push("Pseudo", "readWord",  "readWord label",       "Read number and store 4 bytes at label (little-endian)" );
    rows_all.len()
}

