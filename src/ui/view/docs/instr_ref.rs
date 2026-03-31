use super::chrome::{render_filter_bar, render_page_tabs, render_tab_hint, separator_line};
use crate::ui::theme;
use crate::ui::view::App;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

const TY_W: usize = 8;
const MNE_W: usize = 13;
const OPS_W: usize = 21;
const EXP_W: usize = 26;
const SHOW_EXP_MIN_W: usize = 95;

#[derive(Clone, Copy)]
struct DocRow {
    ty: &'static str,
    mnemonic: &'static str,
    operands: &'static str,
    desc: &'static str,
    expands: &'static str,
}

macro_rules! row {
    ($ty:expr, $mne:expr, $ops:expr, $desc:expr) => {
        DocRow {
            ty: $ty,
            mnemonic: $mne,
            operands: $ops,
            desc: $desc,
            expands: "",
        }
    };
    ($ty:expr, $mne:expr, $ops:expr, $desc:expr, $exp:expr) => {
        DocRow {
            ty: $ty,
            mnemonic: $mne,
            operands: $ops,
            desc: $desc,
            expands: $exp,
        }
    };
}

const DOCS: &[DocRow] = &[
    row!("R", "add", "rd, rs1, rs2", "rd = rs1 + rs2"),
    row!("R", "sub", "rd, rs1, rs2", "rd = rs1 - rs2"),
    row!("R", "and", "rd, rs1, rs2", "rd = rs1 & rs2"),
    row!("R", "or", "rd, rs1, rs2", "rd = rs1 | rs2"),
    row!("R", "xor", "rd, rs1, rs2", "rd = rs1 ^ rs2"),
    row!("R", "sll", "rd, rs1, rs2", "rd = rs1 << (rs2 & 31)"),
    row!("R", "srl", "rd, rs1, rs2", "rd = logical rs1 >> (rs2 & 31)"),
    row!(
        "R",
        "sra",
        "rd, rs1, rs2",
        "rd = arithmetic rs1 >> (rs2 & 31)"
    ),
    row!(
        "R",
        "slt",
        "rd, rs1, rs2",
        "rd = 1 if rs1 < rs2 (signed) else 0"
    ),
    row!(
        "R",
        "sltu",
        "rd, rs1, rs2",
        "rd = 1 if rs1 < rs2 (unsigned) else 0"
    ),
    row!("M", "mul", "rd, rs1, rs2", "rd = (rs1 * rs2) low 32 bits"),
    row!(
        "M",
        "mulh",
        "rd, rs1, rs2",
        "rd = (rs1 * rs2) high 32 bits signed"
    ),
    row!(
        "M",
        "mulhsu",
        "rd, rs1, rs2",
        "rd = (signed rs1 * unsigned rs2) high 32 bits"
    ),
    row!(
        "M",
        "mulhu",
        "rd, rs1, rs2",
        "rd = (rs1 * rs2) high 32 bits unsigned"
    ),
    row!(
        "M",
        "div",
        "rd, rs1, rs2",
        "rd = rs1 / rs2 (signed integer division)"
    ),
    row!("M", "divu", "rd, rs1, rs2", "rd = rs1 / rs2 (unsigned)"),
    row!(
        "M",
        "rem",
        "rd, rs1, rs2",
        "rd = rs1 % rs2 (signed remainder)"
    ),
    row!("M", "remu", "rd, rs1, rs2", "rd = rs1 % rs2 (unsigned)"),
    row!(
        "I",
        "addi",
        "rd, rs1, imm",
        "rd = rs1 + imm (12-bit signed)"
    ),
    row!("I", "xori", "rd, rs1, imm", "rd = rs1 ^ imm"),
    row!("I", "ori", "rd, rs1, imm", "rd = rs1 | imm"),
    row!("I", "andi", "rd, rs1, imm", "rd = rs1 & imm"),
    row!(
        "I",
        "slti",
        "rd, rs1, imm",
        "rd = 1 if rs1 < imm (signed) else 0"
    ),
    row!(
        "I",
        "sltiu",
        "rd, rs1, imm",
        "rd = 1 if rs1 < imm (unsigned) else 0"
    ),
    row!(
        "I",
        "slli",
        "rd, rs1, shamt",
        "rd = rs1 << shamt  (shamt 0..31)"
    ),
    row!("I", "srli", "rd, rs1, shamt", "rd = logical rs1 >> shamt"),
    row!(
        "I",
        "srai",
        "rd, rs1, shamt",
        "rd = arithmetic rs1 >> shamt"
    ),
    row!(
        "Load",
        "lb",
        "rd, imm(rs1)",
        "Load 1 byte signed from mem[rs1+imm]"
    ),
    row!(
        "Load",
        "lh",
        "rd, imm(rs1)",
        "Load 2 bytes signed from mem[rs1+imm]"
    ),
    row!(
        "Load",
        "lw",
        "rd, imm(rs1)",
        "Load 4 bytes from mem[rs1+imm]"
    ),
    row!(
        "Load",
        "lbu",
        "rd, imm(rs1)",
        "Load 1 byte unsigned from mem[rs1+imm]"
    ),
    row!(
        "Load",
        "lhu",
        "rd, imm(rs1)",
        "Load 2 bytes unsigned from mem[rs1+imm]"
    ),
    row!(
        "Store",
        "sb",
        "rs2, imm(rs1)",
        "Store low 1 byte of rs2 to mem[rs1+imm]"
    ),
    row!(
        "Store",
        "sh",
        "rs2, imm(rs1)",
        "Store low 2 bytes of rs2 to mem[rs1+imm]"
    ),
    row!(
        "Store",
        "sw",
        "rs2, imm(rs1)",
        "Store 4 bytes of rs2 to mem[rs1+imm]"
    ),
    row!("Branch", "beq", "rs1, rs2, label", "Branch if rs1 == rs2"),
    row!("Branch", "bne", "rs1, rs2, label", "Branch if rs1 != rs2"),
    row!(
        "Branch",
        "blt",
        "rs1, rs2, label",
        "Branch if rs1 < rs2 (signed)"
    ),
    row!(
        "Branch",
        "bge",
        "rs1, rs2, label",
        "Branch if rs1 >= rs2 (signed)"
    ),
    row!(
        "Branch",
        "bltu",
        "rs1, rs2, label",
        "Branch if rs1 < rs2 (unsigned)"
    ),
    row!(
        "Branch",
        "bgeu",
        "rs1, rs2, label",
        "Branch if rs1 >= rs2 (unsigned)"
    ),
    row!(
        "U",
        "lui",
        "rd, imm20",
        "rd = imm20 << 12  (loads upper 20 bits)"
    ),
    row!("U", "auipc", "rd, imm20", "rd = PC + (imm20 << 12)"),
    row!(
        "Jump",
        "jal",
        "label | rd, label",
        "Jump and link; rd defaults to ra"
    ),
    row!(
        "Jump",
        "jalr",
        "rd, rs1, imm",
        "Jump to rs1+imm & ~1; rd = return addr"
    ),
    row!(
        "SYS",
        "ecall",
        "",
        "System call — a7 selects service, a0 = arg/result"
    ),
    row!(
        "SYS",
        "ebreak",
        "",
        "Pause execution (debug breakpoint; resumable)"
    ),
    row!(
        "SYS",
        "halt",
        "",
        "End-of-hart stop; distinct from ebreak and not resumable"
    ),
    row!(
        "SYS",
        "fence",
        "",
        "Memory barrier (no-op in single-core simulation)"
    ),
    row!("Pseudo", "nop", "", "No operation", "addi x0, x0, 0"),
    row!("Pseudo", "mv", "rd, rs", "rd = rs", "addi rd, rs, 0"),
    row!(
        "Pseudo",
        "li",
        "rd, imm12",
        "Load 12-bit immediate into rd",
        "addi rd, x0, imm"
    ),
    row!(
        "Pseudo",
        "subi",
        "rd, rs1, imm",
        "rd = rs1 - imm",
        "addi rd, rs1, -imm"
    ),
    row!("Pseudo", "neg", "rd, rs", "rd = -rs", "sub rd, x0, rs"),
    row!(
        "Pseudo",
        "not",
        "rd, rs",
        "rd = ~rs  (bitwise NOT)",
        "xori rd, rs, -1"
    ),
    row!(
        "Pseudo",
        "seqz",
        "rd, rs",
        "rd = 1 if rs == 0 else 0",
        "sltiu rd, rs, 1"
    ),
    row!(
        "Pseudo",
        "snez",
        "rd, rs",
        "rd = 1 if rs != 0 else 0",
        "sltu rd, x0, rs"
    ),
    row!(
        "Pseudo",
        "sltz",
        "rd, rs",
        "rd = 1 if rs < 0 else 0",
        "slt rd, rs, x0"
    ),
    row!(
        "Pseudo",
        "sgtz",
        "rd, rs",
        "rd = 1 if rs > 0 else 0",
        "slt rd, x0, rs"
    ),
    row!(
        "Pseudo",
        "la",
        "rd, label",
        "Load address of label into rd",
        "lui rd, hi; addi rd, rd, lo"
    ),
    row!(
        "Pseudo",
        "j",
        "label",
        "Unconditional jump to label",
        "jal x0, label"
    ),
    row!(
        "Pseudo",
        "call",
        "label",
        "Call subroutine at label",
        "jal ra, label"
    ),
    row!(
        "Pseudo",
        "jr",
        "rs",
        "Jump register (indirect)",
        "jalr x0, rs, 0"
    ),
    row!(
        "Pseudo",
        "ret",
        "",
        "Return from subroutine",
        "jalr x0, ra, 0"
    ),
    row!(
        "Pseudo",
        "push",
        "rs",
        "sp -= 4; store rs at 0(sp)",
        "addi sp,sp,-4; sw rs,0(sp)"
    ),
    row!(
        "Pseudo",
        "pop",
        "rd",
        "load rd from 0(sp); sp += 4",
        "lw rd,0(sp); addi sp,sp,4"
    ),
    row!(
        "Pseudo",
        "bez/beqz",
        "rs, label",
        "Branch if rs == 0",
        "beq rs, x0, label"
    ),
    row!(
        "Pseudo",
        "bnez",
        "rs, label",
        "Branch if rs != 0",
        "bne rs, x0, label"
    ),
    row!(
        "Pseudo",
        "bltz",
        "rs, label",
        "Branch if rs < 0",
        "blt rs, x0, label"
    ),
    row!(
        "Pseudo",
        "bgez",
        "rs, label",
        "Branch if rs >= 0",
        "bge rs, x0, label"
    ),
    row!(
        "Pseudo",
        "blez",
        "rs, label",
        "Branch if rs <= 0",
        "bge x0, rs, label"
    ),
    row!(
        "Pseudo",
        "bgtz",
        "rs, label",
        "Branch if rs > 0",
        "blt x0, rs, label"
    ),
    row!(
        "Pseudo",
        "bgt",
        "rs1, rs2, label",
        "Branch if rs1 > rs2 (signed)",
        "blt rs2, rs1, label"
    ),
    row!(
        "Pseudo",
        "ble",
        "rs1, rs2, label",
        "Branch if rs1 <= rs2 (signed)",
        "bge rs2, rs1, label"
    ),
    row!(
        "Pseudo",
        "bgtu",
        "rs1, rs2, label",
        "Branch if rs1 > rs2 (unsigned)",
        "bltu rs2, rs1, label"
    ),
    row!(
        "Pseudo",
        "bleu",
        "rs1, rs2, label",
        "Branch if rs1 <= rs2 (unsigned)",
        "bgeu rs2, rs1, label"
    ),
    row!(
        "Pseudo",
        "print",
        "rd",
        "Print integer in rd (a7=1000)",
        "addi a7,x0,1000; mv a0,rd; ecall"
    ),
    row!(
        "Pseudo",
        "print_str",
        "label",
        "Print NUL string at label",
        "strlen loop; write(a0=1,a1=buf,a2=len) [syscall 64]"
    ),
    row!(
        "Pseudo",
        "print_str_ln",
        "label",
        "Print NUL string + newline",
        "strlen loop; write buf; write '\\n' via stack [syscall 64]"
    ),
    row!(
        "Pseudo",
        "read",
        "label",
        "Read up to 256 bytes from stdin",
        "read(a0=0,a1=buf,a2=256) [syscall 63]"
    ),
    row!(
        "Pseudo",
        "read_byte",
        "label",
        "Read decimal → store 1 byte (RAVEN)",
        "addi a7,x0,1010; la a0,label; ecall"
    ),
    row!(
        "Pseudo",
        "read_half",
        "label",
        "Read decimal → store 2 bytes (RAVEN)",
        "addi a7,x0,1011; la a0,label; ecall"
    ),
    row!(
        "Pseudo",
        "read_word",
        "label",
        "Read decimal → store 4 bytes (RAVEN)",
        "addi a7,x0,1012; la a0,label; ecall"
    ),
    row!(
        "Pseudo",
        "random",
        "rd",
        "rd = random 32-bit word (getrandom)",
        "getrandom syscall via stack (4 bytes)"
    ),
    row!(
        "Pseudo",
        "random_bytes",
        "label, n",
        "Fill n random bytes at label",
        "getrandom(label, n, 0) syscall"
    ),
    row!(
        "F",
        "flw",
        "frd, imm(rs1)",
        "Load f32 from mem[rs1+imm] into frd"
    ),
    row!(
        "F",
        "fsw",
        "frs2, imm(rs1)",
        "Store f32 in frs2 to mem[rs1+imm]"
    ),
    row!(
        "F",
        "fadd.s",
        "frd, frs1, frs2",
        "frd = frs1 + frs2 (single precision)"
    ),
    row!("F", "fsub.s", "frd, frs1, frs2", "frd = frs1 - frs2"),
    row!("F", "fmul.s", "frd, frs1, frs2", "frd = frs1 * frs2"),
    row!("F", "fdiv.s", "frd, frs1, frs2", "frd = frs1 / frs2"),
    row!("F", "fsqrt.s", "frd, frs1", "frd = sqrt(frs1)"),
    row!(
        "F",
        "fmin.s",
        "frd, frs1, frs2",
        "frd = min(frs1, frs2)  (IEEE 754)"
    ),
    row!(
        "F",
        "fmax.s",
        "frd, frs1, frs2",
        "frd = max(frs1, frs2)  (IEEE 754)"
    ),
    row!(
        "F",
        "fmadd.s",
        "frd, frs1, frs2, frs3",
        "frd = frs1*frs2 + frs3  (fused)"
    ),
    row!(
        "F",
        "fmsub.s",
        "frd, frs1, frs2, frs3",
        "frd = frs1*frs2 - frs3  (fused)"
    ),
    row!(
        "F",
        "fnmadd.s",
        "frd, frs1, frs2, frs3",
        "frd = -(frs1*frs2) - frs3  (fused)"
    ),
    row!(
        "F",
        "fnmsub.s",
        "frd, frs1, frs2, frs3",
        "frd = -(frs1*frs2) + frs3  (fused)"
    ),
    row!(
        "F",
        "fsgnj.s",
        "frd, frs1, frs2",
        "frd = |frs1| with sign of frs2"
    ),
    row!(
        "F",
        "fsgnjn.s",
        "frd, frs1, frs2",
        "frd = |frs1| with negated sign of frs2"
    ),
    row!(
        "F",
        "fsgnjx.s",
        "frd, frs1, frs2",
        "frd = |frs1| with XOR of signs"
    ),
    row!(
        "F",
        "feq.s",
        "rd, frs1, frs2",
        "rd = 1 if frs1 == frs2 (ordered) else 0"
    ),
    row!(
        "F",
        "flt.s",
        "rd, frs1, frs2",
        "rd = 1 if frs1 < frs2  (ordered) else 0"
    ),
    row!(
        "F",
        "fle.s",
        "rd, frs1, frs2",
        "rd = 1 if frs1 <= frs2 (ordered) else 0"
    ),
    row!(
        "F",
        "fclass.s",
        "rd, frs1",
        "Classify frs1 → bitmask in rd (see ISA)"
    ),
    row!(
        "F",
        "fcvt.w.s",
        "rd, frs1[, rm]",
        "Convert f32 → i32; rm = rounding mode"
    ),
    row!(
        "F",
        "fcvt.wu.s",
        "rd, frs1[, rm]",
        "Convert f32 → u32; rm = rounding mode"
    ),
    row!("F", "fcvt.s.w", "frd, rs1", "Convert i32 → f32"),
    row!("F", "fcvt.s.wu", "frd, rs1", "Convert u32 → f32"),
    row!(
        "F",
        "fmv.x.w",
        "rd, frs1",
        "Copy float bits → int register (no conversion)"
    ),
    row!(
        "F",
        "fmv.w.x",
        "frd, rs1",
        "Copy int bits → float register (no conversion)"
    ),
    row!(
        "F",
        "fmv.s",
        "frd, frs",
        "Copy float register",
        "fsgnj.s frd, frs, frs"
    ),
    row!(
        "F",
        "fneg.s",
        "frd, frs",
        "Negate: frd = -frs",
        "fsgnjn.s frd, frs, frs"
    ),
    row!(
        "F",
        "fabs.s",
        "frd, frs",
        "Absolute value: frd = |frs|",
        "fsgnjx.s frd, frs, frs"
    ),
    row!("Dir", ".data", "", "Switch to initialized data section"),
    row!("Dir", ".text", "", "Switch to code section"),
    row!(
        "Dir",
        ".bss",
        "",
        "Switch to BSS (zero-initialized) section"
    ),
    row!(
        "Dir",
        ".section",
        "name",
        "Switch to named section (.text or .data)"
    ),
    row!("Dir", ".byte", "val[,...]", "Emit 1-byte integer value(s)"),
    row!(
        "Dir",
        ".half",
        "val[,...]",
        "Emit 2-byte value(s) little-endian"
    ),
    row!(
        "Dir",
        ".word",
        "val[,...]",
        "Emit 4-byte value(s) little-endian"
    ),
    row!(
        "Dir",
        ".dword",
        "val[,...]",
        "Emit 8-byte value(s) little-endian"
    ),
    row!(
        "Dir",
        ".float",
        "val[,...]",
        "Emit IEEE 754 f32 value(s) (4 bytes each)"
    ),
    row!(
        "Dir",
        ".ascii",
        "\"str\"",
        "Emit string bytes (no NUL terminator)"
    ),
    row!(
        "Dir",
        ".asciz",
        "\"str\"",
        "Emit string bytes + NUL terminator"
    ),
    row!("Dir", ".string", "\"str\"", "Alias of .asciz"),
    row!("Dir", ".space", "n", "Reserve n zero bytes"),
    row!("Dir", ".align", "n", "Align PC to 2^n byte boundary"),
    row!("Dir", ".globl", "sym", "Mark symbol as global / exported"),
    row!(
        "Dir",
        ".equ",
        "sym, val",
        "Define symbolic constant (equate)"
    ),
];

fn ty_bit(ty: &str) -> u16 {
    match ty {
        "R" => 1 << 0,
        "M" => 1 << 1,
        "I" => 1 << 2,
        "Load" => 1 << 3,
        "Store" => 1 << 4,
        "Branch" => 1 << 5,
        "U" => 1 << 6,
        "Jump" => 1 << 7,
        "SYS" => 1 << 8,
        "Pseudo" => 1 << 9,
        "F" => 1 << 10,
        "Dir" => 1 << 11,
        _ => 0,
    }
}

fn ty_color(ty: &str) -> Color {
    match ty {
        "R" => Color::Yellow,
        "M" => Color::LightRed,
        "I" => Color::Green,
        "Load" => Color::Cyan,
        "Store" => Color::LightBlue,
        "Branch" => Color::Magenta,
        "U" => Color::LightYellow,
        "Jump" => Color::LightCyan,
        "SYS" => Color::Red,
        "Pseudo" => Color::LightMagenta,
        "F" => Color::LightGreen,
        "Dir" => Color::Gray,
        _ => Color::White,
    }
}

fn filtered_rows(query: &str, type_filter: u16) -> Vec<&'static DocRow> {
    let q = query.to_lowercase();
    DOCS.iter()
        .filter(|r| (type_filter & ty_bit(r.ty)) != 0)
        .filter(|r| {
            q.is_empty()
                || r.mnemonic.to_lowercase().contains(&q)
                || r.operands.to_lowercase().contains(&q)
                || r.desc.to_lowercase().contains(&q)
                || r.expands.to_lowercase().contains(&q)
                || r.ty.to_lowercase().contains(&q)
        })
        .collect()
}

pub(crate) fn docs_body_line_count(_width: u16, query: &str, type_filter: u16) -> usize {
    if type_filter == 0 {
        return 0;
    }
    filtered_rows(query, type_filter).len()
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
        "rd" | "rd2" => Some(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        "rs1" | "rs2" | "rs3" | "rs" | "rt" => Some(Style::default().fg(Color::Cyan)),
        "frd" | "frd2" => Some(Style::default().fg(Color::Yellow)),
        "frs" | "frs1" | "frs2" | "frs3" => Some(Style::default().fg(Color::LightYellow)),
        "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" | "n" => {
            Some(Style::default().fg(Color::LightGreen))
        }
        "label" => Some(Style::default().fg(Color::Magenta)),
        "rm" => Some(Style::default().fg(Color::LightYellow)),
        "sym" => Some(Style::default().fg(Color::LightBlue)),
        _ if is_register_token(token) => Some(Style::default().fg(Color::LightBlue)),
        _ => None,
    }
}

fn color_text(s: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut token = String::new();
    let mut sep = String::new();

    let flush_sep = |spans: &mut Vec<Span<'static>>, sep: &mut String| {
        if !sep.is_empty() {
            spans.push(Span::raw(std::mem::take(sep)));
        }
    };
    let flush_token = |spans: &mut Vec<Span<'static>>, token: &mut String| {
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

    for ch in s.chars() {
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

    spans
}

fn pad_or_truncate(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let len = s.chars().count();
    if len > width {
        let n = width.saturating_sub(1);
        let truncated: String = s.chars().take(n).collect();
        format!("{truncated}…")
    } else {
        format!("{s:<width$}")
    }
}

fn col_widths(width: usize) -> (usize, bool) {
    let show_exp = width >= SHOW_EXP_MIN_W;
    let fixed = TY_W + 1 + MNE_W + 1 + OPS_W + 1;
    let exp_overhead = if show_exp { 1 + EXP_W } else { 0 };
    let desc_w = width.saturating_sub(fixed + exp_overhead).max(8);
    (desc_w, show_exp)
}

fn render_col_header(width: usize) -> Line<'static> {
    let (desc_w, show_exp) = col_widths(width);
    let hdr_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![
        Span::styled(format!("{:<8}", "Type"), hdr_style),
        Span::raw(" "),
        Span::styled(format!("{:<13}", "Mnemonic"), hdr_style),
        Span::raw(" "),
        Span::styled(format!("{:<21}", "Operands"), hdr_style),
        Span::raw(" "),
        Span::styled(pad_or_truncate("Description", desc_w), hdr_style),
    ];
    if show_exp {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("{:<26}", "Expands to"), hdr_style));
    }
    Line::from(spans)
}

fn render_doc_row(row: &DocRow, desc_w: usize, show_exp: bool) -> Line<'static> {
    let color = ty_color(row.ty);
    let badge = format!("{:>8}", format!("[{}]", row.ty));
    let mne = format!("{:<13}", row.mnemonic);

    let ops_len = row.operands.chars().count();
    let mut ops_spans = color_text(row.operands);
    if ops_len < OPS_W {
        ops_spans.push(Span::raw(" ".repeat(OPS_W - ops_len)));
    }

    let desc = pad_or_truncate(row.desc, desc_w);

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(
            badge,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            mne,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    spans.extend(ops_spans);
    spans.push(Span::raw(" "));
    spans.push(Span::raw(desc));

    if show_exp && !row.expands.is_empty() {
        spans.push(Span::raw(" "));
        let exp_text = format!("→ {}", row.expands);
        let exp = pad_or_truncate(&exp_text, EXP_W);
        spans.push(Span::styled(
            exp,
            Style::default().fg(Color::Rgb(100, 100, 120)),
        ));
    }

    Line::from(spans)
}

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let search_bar_h: u16 = if app.docs.search_open { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(search_bar_h),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let tab_area = chunks[0];
    let meta_area = chunks[1];
    let search_area = chunks[2];
    let filter_area = chunks[3];
    let table_area = chunks[4];

    let search_hint = if app.docs.search_open {
        "  Ctrl+F=search"
    } else {
        ""
    };
    let filter_hint = if !app.docs.search_open {
        "  ←/→=filter  Space=toggle"
    } else {
        ""
    };
    let tab_hint = format!("{search_hint}{filter_hint}  ↑/↓=scroll");
    render_page_tabs(f, tab_area, app);
    render_tab_hint(f, tab_area, app, 3, tab_hint);

    let meta_lines = vec![
        Line::from(vec![
            Span::styled("rd", Style::default().fg(Color::Yellow).bold()),
            Span::styled("=dst  ", Style::default().fg(theme::LABEL)),
            Span::styled("rs1/rs2", Style::default().fg(Color::Cyan)),
            Span::styled("=src  ", Style::default().fg(theme::LABEL)),
            Span::styled("frd", Style::default().fg(Color::Yellow)),
            Span::styled("=float dst  ", Style::default().fg(theme::LABEL)),
            Span::styled("frs1/frs2", Style::default().fg(Color::LightYellow)),
            Span::styled("=float src  ", Style::default().fg(theme::LABEL)),
            Span::styled("imm", Style::default().fg(Color::LightGreen)),
            Span::styled("=immediate  ", Style::default().fg(theme::LABEL)),
            Span::styled("label", Style::default().fg(Color::Magenta)),
            Span::styled("=symbol", Style::default().fg(theme::LABEL)),
        ]),
        separator_line(area.width),
    ];
    f.render_widget(Paragraph::new(meta_lines), meta_area);

    if app.docs.search_open {
        let bar_style = Style::default().fg(theme::LABEL).bg(Color::Rgb(30, 30, 50));
        let bar_line = Line::from(vec![
            Span::styled(
                " Find: ",
                Style::default()
                    .fg(theme::ACCENT)
                    .bg(Color::Rgb(30, 30, 50)),
            ),
            Span::styled(
                app.docs.search_query.clone(),
                Style::default()
                    .fg(theme::LABEL_Y)
                    .bg(Color::Rgb(30, 30, 50)),
            ),
            Span::styled(
                "  Esc=close",
                Style::default().fg(theme::LABEL).bg(Color::Rgb(30, 30, 50)),
            ),
        ]);
        f.render_widget(Paragraph::new(bar_line).style(bar_style), search_area);

        let prefix_len = " Find: ".len() as u16;
        let cursor_x = (search_area.x + prefix_len + app.docs.search_query.chars().count() as u16)
            .min(search_area.x + search_area.width.saturating_sub(1));
        if search_area.height > 0 {
            f.set_cursor_position((cursor_x, search_area.y));
        }
    }

    render_filter_bar(f, filter_area, app);

    if table_area.height == 0 || table_area.width == 0 {
        return;
    }

    let w = table_area.width as usize;
    let (desc_w, show_exp) = col_widths(w);

    let table_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(table_area);

    f.render_widget(Paragraph::new(render_col_header(w)), table_chunks[0]);
    f.render_widget(Paragraph::new(separator_line(w as u16)), table_chunks[1]);

    let data_area = table_chunks[2];
    if data_area.height == 0 {
        return;
    }

    let rows = if app.docs.type_filter == 0 {
        vec![]
    } else {
        let q = if app.docs.search_open {
            app.docs.search_query.as_str()
        } else {
            ""
        };
        filtered_rows(q, app.docs.type_filter)
    };

    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                "  (no results — adjust filter or search query)",
                Style::default().fg(Color::DarkGray),
            )),
            data_area,
        );
        return;
    }

    let viewport_h = data_area.height as usize;
    let max_start = rows.len().saturating_sub(viewport_h);
    let start = app.docs.scroll.min(max_start);
    let end = (start + viewport_h).min(rows.len());
    let lines: Vec<Line<'static>> = rows[start..end]
        .iter()
        .map(|r| render_doc_row(r, desc_w, show_exp))
        .collect();

    f.render_widget(Paragraph::new(lines), data_area);
}
