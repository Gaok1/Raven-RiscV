use crate::falcon::{self, memory::Bus};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use super::App;
use super::memory::imem_address_in_range;
use super::registers::reg_name;

pub(super) fn render_instruction_details(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Min(4),
        ])
        .split(area);

    let context = detail_context(app);
    render_detail_header(f, chunks[0], &context);
    render_bit_fields(f, chunks[1], context.word, context.format);
    render_field_values(f, chunks[2], context.word, context.format);
}

pub(super) fn disasm_word(word: u32) -> String {
    match falcon::decoder::decode(word) {
        Ok(instruction) => pretty_instr(&instruction),
        Err(error) => format!("<decode error: {error}>"),
    }
}

struct DetailContext {
    addr: u32,
    word: u32,
    disasm: String,
    origin: DetailOrigin,
    format: EncFormat,
}

enum DetailOrigin {
    Hover,
    ProgramCounter,
}

fn detail_context(app: &App) -> DetailContext {
    if let Some(addr) = app.hover_imem_addr {
        let word = app.mem.load32(addr).unwrap_or(0);
        DetailContext {
            addr,
            word,
            disasm: disasm_word(word),
            origin: DetailOrigin::Hover,
            format: detect_format(word),
        }
    } else if imem_address_in_range(app, app.cpu.pc) {
        let word = app.mem.load32(app.cpu.pc).unwrap_or(0);
        DetailContext {
            addr: app.cpu.pc,
            word,
            disasm: disasm_word(word),
            origin: DetailOrigin::ProgramCounter,
            format: detect_format(word),
        }
    } else {
        DetailContext {
            addr: app.cpu.pc,
            word: 0,
            disasm: "<PC out of RAM>".to_string(),
            origin: DetailOrigin::ProgramCounter,
            format: detect_format(0),
        }
    }
}

fn render_detail_header(f: &mut Frame, area: Rect, context: &DetailContext) {
    let suffix = match context.origin {
        DetailOrigin::Hover => " (hover)",
        DetailOrigin::ProgramCounter => " (PC)",
    };

    let lines = vec![
        Line::from(format!("Addr = 0x{:08x}{suffix}", context.addr)),
        Line::from(format!("Word = 0x{:08x}", context.word)),
        Line::from(format!("Instr = {}", context.disasm)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Instruction Details");

    f.render_widget(Paragraph::new(lines).block(block), area);
}

#[derive(Copy, Clone)]
enum EncFormat {
    R,
    I,
    S,
    B,
    U,
    J,
}

fn detect_format(word: u32) -> EncFormat {
    match word & 0x7f {
        0x03 | 0x13 | 0x1b | 0x67 | 0x73 => EncFormat::I,
        0x23 => EncFormat::S,
        0x63 => EncFormat::B,
        0x37 | 0x17 => EncFormat::U,
        0x6f => EncFormat::J,
        _ => EncFormat::R,
    }
}

fn render_bit_fields(f: &mut Frame, area: Rect, word: u32, format: EncFormat) {
    let (title, segments) = bit_segments(format);
    let labels = segment_labels(&segments);
    let bits = segment_bits(word, &segments);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title(title);

    let lines = vec![Line::from(labels), Line::from(bits)];
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

struct Segment {
    label: &'static str,
    width: u8,
    color: Color,
}

fn bit_segments(format: EncFormat) -> (&'static str, Vec<Segment>) {
    use EncFormat::*;
    match format {
        R => (
            "Field map (R-type)",
            vec![
                Segment {
                    label: "funct7",
                    width: 7,
                    color: Color::Red,
                },
                Segment {
                    label: "rs2",
                    width: 5,
                    color: Color::LightRed,
                },
                Segment {
                    label: "rs1",
                    width: 5,
                    color: Color::LightMagenta,
                },
                Segment {
                    label: "funct3",
                    width: 3,
                    color: Color::Yellow,
                },
                Segment {
                    label: "rd",
                    width: 5,
                    color: Color::LightGreen,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
        I => (
            "Field map (I-type)",
            vec![
                Segment {
                    label: "imm[11:0]",
                    width: 12,
                    color: Color::Blue,
                },
                Segment {
                    label: "rs1",
                    width: 5,
                    color: Color::LightMagenta,
                },
                Segment {
                    label: "funct3",
                    width: 3,
                    color: Color::Yellow,
                },
                Segment {
                    label: "rd",
                    width: 5,
                    color: Color::LightGreen,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
        S => (
            "Field map (S-type)",
            vec![
                Segment {
                    label: "imm[11:5]",
                    width: 7,
                    color: Color::Blue,
                },
                Segment {
                    label: "rs2",
                    width: 5,
                    color: Color::LightRed,
                },
                Segment {
                    label: "rs1",
                    width: 5,
                    color: Color::LightMagenta,
                },
                Segment {
                    label: "funct3",
                    width: 3,
                    color: Color::Yellow,
                },
                Segment {
                    label: "imm[4:0]",
                    width: 5,
                    color: Color::Blue,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
        B => (
            "Field map (B-type)",
            vec![
                Segment {
                    label: "imm[12]",
                    width: 1,
                    color: Color::Blue,
                },
                Segment {
                    label: "imm[10:5]",
                    width: 6,
                    color: Color::Blue,
                },
                Segment {
                    label: "rs2",
                    width: 5,
                    color: Color::LightRed,
                },
                Segment {
                    label: "rs1",
                    width: 5,
                    color: Color::LightMagenta,
                },
                Segment {
                    label: "funct3",
                    width: 3,
                    color: Color::Yellow,
                },
                Segment {
                    label: "imm[4:1]",
                    width: 4,
                    color: Color::Blue,
                },
                Segment {
                    label: "imm[11]",
                    width: 1,
                    color: Color::Blue,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
        U => (
            "Field map (U-type)",
            vec![
                Segment {
                    label: "imm[31:12]",
                    width: 20,
                    color: Color::Blue,
                },
                Segment {
                    label: "rd",
                    width: 5,
                    color: Color::LightGreen,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
        J => (
            "Field map (J-type)",
            vec![
                Segment {
                    label: "imm[20]",
                    width: 1,
                    color: Color::Blue,
                },
                Segment {
                    label: "imm[10:1]",
                    width: 10,
                    color: Color::Blue,
                },
                Segment {
                    label: "imm[11]",
                    width: 1,
                    color: Color::Blue,
                },
                Segment {
                    label: "imm[19:12]",
                    width: 8,
                    color: Color::Blue,
                },
                Segment {
                    label: "rd",
                    width: 5,
                    color: Color::LightGreen,
                },
                Segment {
                    label: "opcode",
                    width: 7,
                    color: Color::Cyan,
                },
            ],
        ),
    }
}

fn segment_labels(segments: &[Segment]) -> Vec<Span<'static>> {
    segments
        .iter()
        .map(|segment| {
            let bar = "▮".repeat(segment.width.max(1) as usize);
            Span::styled(
                format!("{bar} {} ", segment.label),
                Style::default().fg(segment.color),
            )
        })
        .collect()
}

fn segment_bits(word: u32, segments: &[Segment]) -> Vec<Span<'static>> {
    let bit_string = format!("{:032b}", word);
    let mut spans = Vec::new();
    let mut index = 0usize;

    for (i, segment) in segments.iter().enumerate() {
        let end = index + segment.width as usize;
        let slice = &bit_string[index..end];
        spans.push(Span::styled(
            slice.to_string(),
            Style::default().fg(segment.color),
        ));
        if i + 1 < segments.len() {
            spans.push(Span::raw(" "));
        }
        index = end;
    }

    spans
}

fn render_field_values(f: &mut Frame, area: Rect, word: u32, format: EncFormat) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .border_type(BorderType::Rounded)
        .title("Parsed fields");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    match format {
        EncFormat::R => push_r_fields(&mut lines, word),
        EncFormat::I => push_i_fields(&mut lines, word),
        EncFormat::S => push_s_fields(&mut lines, word),
        EncFormat::B => push_b_fields(&mut lines, word),
        EncFormat::U => push_u_fields(&mut lines, word),
        EncFormat::J => push_j_fields(&mut lines, word),
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(para, inner);
}

fn push_r_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let funct7 = (word >> 25) & 0x7f;
    let rs2 = (word >> 20) & 0x1f;
    let rs1 = (word >> 15) & 0x1f;
    let funct3 = (word >> 12) & 0x7;
    let rd = (word >> 7) & 0x1f;
    let opcode = word & 0x7f;
    lines.push(Line::from(format!(
        "funct7={:#04x}  rs2={}  rs1={}  funct3={:#03x}  rd={}  opcode={:#04x}",
        funct7, rs2, rs1, funct3, rd, opcode
    )));
}

fn push_i_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let imm = (((word >> 20) as i32) << 20) >> 20;
    let rs1 = (word >> 15) & 0x1f;
    let funct3 = (word >> 12) & 0x7;
    let rd = (word >> 7) & 0x1f;
    let opcode = word & 0x7f;
    lines.push(Line::from(format!(
        "imm={}  rs1={}  funct3={:#03x}  rd={}  opcode={:#04x}",
        imm, rs1, funct3, rd, opcode
    )));
    if matches!(funct3, 0x1 | 0x5) {
        let shamt = (word >> 20) & 0x1f;
        let funct7 = (word >> 25) & 0x7f;
        lines.push(Line::from(format!(
            "(shift) funct7={:#04x} shamt={} rs1={} rd={}",
            funct7, shamt, rs1, rd
        )));
    }
}

fn push_s_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let imm_4_0 = (word >> 7) & 0x1f;
    let funct3 = (word >> 12) & 0x7;
    let rs1 = (word >> 15) & 0x1f;
    let rs2 = (word >> 20) & 0x1f;
    let imm_11_5 = (word >> 25) & 0x7f;
    let opcode = word & 0x7f;
    let imm = (((((imm_11_5 << 5) | imm_4_0) as i32) << 20) >> 20) as i32;
    lines.push(Line::from(format!(
        "imm[11:5]={:#04x} imm[4:0]={:#03x} => imm={}  rs2={} rs1={} funct3={:#03x} opcode={:#04x}",
        imm_11_5, imm_4_0, imm, rs2, rs1, funct3, opcode
    )));
}

fn push_b_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let b12 = (word >> 31) & 0x1;
    let b10_5 = (word >> 25) & 0x3f;
    let rs2 = (word >> 20) & 0x1f;
    let rs1 = (word >> 15) & 0x1f;
    let funct3 = (word >> 12) & 0x7;
    let b4_1 = (word >> 8) & 0xf;
    let b11 = (word >> 7) & 0x1;
    let opcode = word & 0x7f;
    let imm =
        (((((b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1)) as i32) << 19) >> 19) as i32;
    lines.push(Line::from(format!(
        "b12={} b11={} b10:5={:#04x} b4:1={:#03x} => imm={}  rs2={} rs1={} f3={:#03x} opc={:#04x}",
        b12, b11, b10_5, b4_1, imm, rs2, rs1, funct3, opcode
    )));
}

fn push_u_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let rd = (word >> 7) & 0x1f;
    let opcode = word & 0x7f;
    let imm = (word & 0xfffff000) as i32;
    lines.push(Line::from(format!(
        "imm[31:12]={:#07x} => imm={}  rd={} opc={:#04x}",
        imm >> 12,
        imm,
        rd,
        opcode
    )));
}

fn push_j_fields(lines: &mut Vec<Line<'static>>, word: u32) {
    let b20 = (word >> 31) & 1;
    let b10_1 = (word >> 21) & 0x3ff;
    let b11 = (word >> 20) & 1;
    let b19_12 = (word >> 12) & 0xff;
    let rd = (word >> 7) & 0x1f;
    let opcode = word & 0x7f;
    let imm =
        (((((b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1)) as i32) << 11) >> 11) as i32;
    lines.push(Line::from(format!(
        "b20={} b19:12={:#04x} b11={} b10:1={:#05x} => imm={} rd={} opc={:#04x}",
        b20, b19_12, b11, b10_1, imm, rd, opcode
    )));
}

fn pretty_instr(instruction: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;

    match *instruction {
        Add { rd, rs1, rs2 } => format_reg3("add", rd, rs1, rs2),
        Sub { rd, rs1, rs2 } => format_reg3("sub", rd, rs1, rs2),
        And { rd, rs1, rs2 } => format_reg3("and", rd, rs1, rs2),
        Or { rd, rs1, rs2 } => format_reg3("or", rd, rs1, rs2),
        Xor { rd, rs1, rs2 } => format_reg3("xor", rd, rs1, rs2),
        Sll { rd, rs1, rs2 } => format_reg3("sll", rd, rs1, rs2),
        Srl { rd, rs1, rs2 } => format_reg3("srl", rd, rs1, rs2),
        Sra { rd, rs1, rs2 } => format_reg3("sra", rd, rs1, rs2),
        Slt { rd, rs1, rs2 } => format_reg3("slt", rd, rs1, rs2),
        Sltu { rd, rs1, rs2 } => format_reg3("sltu", rd, rs1, rs2),
        Mul { rd, rs1, rs2 } => format_reg3("mul", rd, rs1, rs2),
        Mulh { rd, rs1, rs2 } => format_reg3("mulh", rd, rs1, rs2),
        Mulhsu { rd, rs1, rs2 } => format_reg3("mulhsu", rd, rs1, rs2),
        Mulhu { rd, rs1, rs2 } => format_reg3("mulhu", rd, rs1, rs2),
        Div { rd, rs1, rs2 } => format_reg3("div", rd, rs1, rs2),
        Divu { rd, rs1, rs2 } => format_reg3("divu", rd, rs1, rs2),
        Rem { rd, rs1, rs2 } => format_reg3("rem", rd, rs1, rs2),
        Remu { rd, rs1, rs2 } => format_reg3("remu", rd, rs1, rs2),
        Addi { rd, rs1, imm } => format_reg_imm("addi", rd, rs1, imm),
        Andi { rd, rs1, imm } => format_reg_imm("andi", rd, rs1, imm),
        Ori { rd, rs1, imm } => format_reg_imm("ori", rd, rs1, imm),
        Xori { rd, rs1, imm } => format_reg_imm("xori", rd, rs1, imm),
        Slti { rd, rs1, imm } => format_reg_imm("slti", rd, rs1, imm),
        Sltiu { rd, rs1, imm } => format_reg_imm("sltiu", rd, rs1, imm),
        Slli { rd, rs1, shamt } => format_reg_imm("slli", rd, rs1, shamt as i32),
        Srli { rd, rs1, shamt } => format_reg_imm("srli", rd, rs1, shamt as i32),
        Srai { rd, rs1, shamt } => format_reg_imm("srai", rd, rs1, shamt as i32),
        Lb { rd, rs1, imm } => format_load("lb", rd, rs1, imm),
        Lh { rd, rs1, imm } => format_load("lh", rd, rs1, imm),
        Lw { rd, rs1, imm } => format_load("lw", rd, rs1, imm),
        Lbu { rd, rs1, imm } => format_load("lbu", rd, rs1, imm),
        Lhu { rd, rs1, imm } => format_load("lhu", rd, rs1, imm),
        Sb { rs2, rs1, imm } => format_store("sb", rs2, rs1, imm),
        Sh { rs2, rs1, imm } => format_store("sh", rs2, rs1, imm),
        Sw { rs2, rs1, imm } => format_store("sw", rs2, rs1, imm),
        Beq { rs1, rs2, imm } => format_branch("beq", rs1, rs2, imm),
        Bne { rs1, rs2, imm } => format_branch("bne", rs1, rs2, imm),
        Blt { rs1, rs2, imm } => format_branch("blt", rs1, rs2, imm),
        Bge { rs1, rs2, imm } => format_branch("bge", rs1, rs2, imm),
        Bltu { rs1, rs2, imm } => format_branch("bltu", rs1, rs2, imm),
        Bgeu { rs1, rs2, imm } => format_branch("bgeu", rs1, rs2, imm),
        Lui { rd, imm } => format_single_reg_imm("lui", rd, imm),
        Auipc { rd, imm } => format_single_reg_imm("auipc", rd, imm),
        Jal { rd, imm } => format_single_reg_imm("jal", rd, imm),
        Jalr { rd, rs1, imm } => format_reg_imm("jalr", rd, rs1, imm),
        Ecall => "ecall".to_string(),
        Halt => "halt".to_string(),
    }
}

fn format_reg3(mnemonic: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!(
        "{:<4} {}, {}, {}",
        mnemonic,
        reg_name(rd),
        reg_name(rs1),
        reg_name(rs2)
    )
}

fn format_reg_imm(mnemonic: &str, rd: u8, rs1: u8, imm: i32) -> String {
    format!(
        "{:<4} {}, {}, {}",
        mnemonic,
        reg_name(rd),
        reg_name(rs1),
        imm
    )
}

fn format_load(mnemonic: &str, rd: u8, rs1: u8, imm: i32) -> String {
    format!(
        "{:<4} {}, {}({})",
        mnemonic,
        reg_name(rd),
        imm,
        reg_name(rs1)
    )
}

fn format_store(mnemonic: &str, rs2: u8, rs1: u8, imm: i32) -> String {
    format!(
        "{:<4} {}, {}({})",
        mnemonic,
        reg_name(rs2),
        imm,
        reg_name(rs1)
    )
}

fn format_branch(mnemonic: &str, rs1: u8, rs2: u8, imm: i32) -> String {
    format!(
        "{:<4} {}, {}, {}",
        mnemonic,
        reg_name(rs1),
        reg_name(rs2),
        imm
    )
}

fn format_single_reg_imm(mnemonic: &str, rd: u8, imm: i32) -> String {
    format!("{:<4} {}, {}", mnemonic, reg_name(rd), imm)
}
