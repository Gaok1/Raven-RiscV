use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::ui::app::{DocsLang, DocsPage};
use crate::ui::theme;
use super::App;

// ── Type-filter bit constants ──────────────────────────────────────────────────

const TY_R:      u16 = 1 << 0;
const TY_M:      u16 = 1 << 1;
const TY_I:      u16 = 1 << 2;
const TY_LOAD:   u16 = 1 << 3;
const TY_STORE:  u16 = 1 << 4;
const TY_BRANCH: u16 = 1 << 5;
const TY_U:      u16 = 1 << 6;
const TY_JUMP:   u16 = 1 << 7;
const TY_SYS:    u16 = 1 << 8;
const TY_PSEUDO: u16 = 1 << 9;
const TY_F:      u16 = 1 << 10;
const TY_DIR:    u16 = 1 << 11;

pub(crate) const ALL_MASK: u16 = 0x0FFF;

/// Filter bar items: (display_label, type_bit, color).
/// Index 0 = "All" (special — bit=0 means toggle-all), 1–12 = individual types.
pub(crate) const FILTER_ITEMS: &[(&str, u16, Color)] = &[
    ("All",    0,         Color::White),
    ("R",      TY_R,      Color::Yellow),
    ("M",      TY_M,      Color::LightRed),
    ("I",      TY_I,      Color::Green),
    ("Load",   TY_LOAD,   Color::Cyan),
    ("Store",  TY_STORE,  Color::LightBlue),
    ("Branch", TY_BRANCH, Color::Magenta),
    ("U",      TY_U,      Color::LightYellow),
    ("Jump",   TY_JUMP,   Color::LightCyan),
    ("SYS",    TY_SYS,    Color::Red),
    ("Pseudo", TY_PSEUDO, Color::LightMagenta),
    ("F",      TY_F,      Color::LightGreen),
    ("Dir",    TY_DIR,    Color::Gray),
];

// ── Column layout constants ────────────────────────────────────────────────────

/// Width of the type badge column: "[Branch]" = 8 chars.
const TY_W: usize = 8;
/// Width of the mnemonic column.
const MNE_W: usize = 13;
/// Width of the operands column.
const OPS_W: usize = 21;
/// Width of the expands column (shown only on wide terminals).
const EXP_W: usize = 26;
/// Minimum terminal width required to show the expands column.
const SHOW_EXP_MIN_W: usize = 95;

// ── Instruction table data ─────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct DocRow {
    ty:       &'static str,
    mnemonic: &'static str,
    operands: &'static str,
    desc:     &'static str,
    expands:  &'static str,
}

macro_rules! row {
    ($ty:expr, $mne:expr, $ops:expr, $desc:expr) => {
        DocRow { ty: $ty, mnemonic: $mne, operands: $ops, desc: $desc, expands: "" }
    };
    ($ty:expr, $mne:expr, $ops:expr, $desc:expr, $exp:expr) => {
        DocRow { ty: $ty, mnemonic: $mne, operands: $ops, desc: $desc, expands: $exp }
    };
}

const DOCS: &[DocRow] = &[
    // ── R-type ──────────────────────────────────────────────────────────────────
    row!("R", "add",    "rd, rs1, rs2",   "rd = rs1 + rs2"),
    row!("R", "sub",    "rd, rs1, rs2",   "rd = rs1 - rs2"),
    row!("R", "and",    "rd, rs1, rs2",   "rd = rs1 & rs2"),
    row!("R", "or",     "rd, rs1, rs2",   "rd = rs1 | rs2"),
    row!("R", "xor",    "rd, rs1, rs2",   "rd = rs1 ^ rs2"),
    row!("R", "sll",    "rd, rs1, rs2",   "rd = rs1 << (rs2 & 31)"),
    row!("R", "srl",    "rd, rs1, rs2",   "rd = logical rs1 >> (rs2 & 31)"),
    row!("R", "sra",    "rd, rs1, rs2",   "rd = arithmetic rs1 >> (rs2 & 31)"),
    row!("R", "slt",    "rd, rs1, rs2",   "rd = 1 if rs1 < rs2 (signed) else 0"),
    row!("R", "sltu",   "rd, rs1, rs2",   "rd = 1 if rs1 < rs2 (unsigned) else 0"),
    // ── M extension ─────────────────────────────────────────────────────────────
    row!("M", "mul",    "rd, rs1, rs2",   "rd = (rs1 * rs2) low 32 bits"),
    row!("M", "mulh",   "rd, rs1, rs2",   "rd = (rs1 * rs2) high 32 bits signed"),
    row!("M", "mulhsu", "rd, rs1, rs2",   "rd = (signed rs1 * unsigned rs2) high 32 bits"),
    row!("M", "mulhu",  "rd, rs1, rs2",   "rd = (rs1 * rs2) high 32 bits unsigned"),
    row!("M", "div",    "rd, rs1, rs2",   "rd = rs1 / rs2 (signed integer division)"),
    row!("M", "divu",   "rd, rs1, rs2",   "rd = rs1 / rs2 (unsigned)"),
    row!("M", "rem",    "rd, rs1, rs2",   "rd = rs1 % rs2 (signed remainder)"),
    row!("M", "remu",   "rd, rs1, rs2",   "rd = rs1 % rs2 (unsigned)"),
    // ── I-type ──────────────────────────────────────────────────────────────────
    row!("I", "addi",   "rd, rs1, imm",   "rd = rs1 + imm (12-bit signed)"),
    row!("I", "xori",   "rd, rs1, imm",   "rd = rs1 ^ imm"),
    row!("I", "ori",    "rd, rs1, imm",   "rd = rs1 | imm"),
    row!("I", "andi",   "rd, rs1, imm",   "rd = rs1 & imm"),
    row!("I", "slti",   "rd, rs1, imm",   "rd = 1 if rs1 < imm (signed) else 0"),
    row!("I", "sltiu",  "rd, rs1, imm",   "rd = 1 if rs1 < imm (unsigned) else 0"),
    row!("I", "slli",   "rd, rs1, shamt", "rd = rs1 << shamt  (shamt 0..31)"),
    row!("I", "srli",   "rd, rs1, shamt", "rd = logical rs1 >> shamt"),
    row!("I", "srai",   "rd, rs1, shamt", "rd = arithmetic rs1 >> shamt"),
    // ── Loads ───────────────────────────────────────────────────────────────────
    row!("Load", "lb",  "rd, imm(rs1)",   "Load 1 byte signed from mem[rs1+imm]"),
    row!("Load", "lh",  "rd, imm(rs1)",   "Load 2 bytes signed from mem[rs1+imm]"),
    row!("Load", "lw",  "rd, imm(rs1)",   "Load 4 bytes from mem[rs1+imm]"),
    row!("Load", "lbu", "rd, imm(rs1)",   "Load 1 byte unsigned from mem[rs1+imm]"),
    row!("Load", "lhu", "rd, imm(rs1)",   "Load 2 bytes unsigned from mem[rs1+imm]"),
    // ── Stores ──────────────────────────────────────────────────────────────────
    row!("Store", "sb", "rs2, imm(rs1)",  "Store low 1 byte of rs2 to mem[rs1+imm]"),
    row!("Store", "sh", "rs2, imm(rs1)",  "Store low 2 bytes of rs2 to mem[rs1+imm]"),
    row!("Store", "sw", "rs2, imm(rs1)",  "Store 4 bytes of rs2 to mem[rs1+imm]"),
    // ── Branches ────────────────────────────────────────────────────────────────
    row!("Branch", "beq",  "rs1, rs2, label", "Branch if rs1 == rs2"),
    row!("Branch", "bne",  "rs1, rs2, label", "Branch if rs1 != rs2"),
    row!("Branch", "blt",  "rs1, rs2, label", "Branch if rs1 < rs2 (signed)"),
    row!("Branch", "bge",  "rs1, rs2, label", "Branch if rs1 >= rs2 (signed)"),
    row!("Branch", "bltu", "rs1, rs2, label", "Branch if rs1 < rs2 (unsigned)"),
    row!("Branch", "bgeu", "rs1, rs2, label", "Branch if rs1 >= rs2 (unsigned)"),
    // ── U-type ──────────────────────────────────────────────────────────────────
    row!("U", "lui",   "rd, imm20",       "rd = imm20 << 12  (loads upper 20 bits)"),
    row!("U", "auipc", "rd, imm20",       "rd = PC + (imm20 << 12)"),
    // ── Jumps ───────────────────────────────────────────────────────────────────
    row!("Jump", "jal",  "label | rd, label", "Jump and link; rd defaults to ra"),
    row!("Jump", "jalr", "rd, rs1, imm",      "Jump to rs1+imm & ~1; rd = return addr"),
    // ── System ──────────────────────────────────────────────────────────────────
    row!("SYS", "ecall",  "", "System call — a7 selects service, a0 = arg/result"),
    row!("SYS", "ebreak", "", "Stop execution (debug breakpoint)"),
    row!("SYS", "halt",   "", "Stop execution (alias of ebreak)"),
    row!("SYS", "fence",  "", "Memory barrier (no-op in single-core simulation)"),
    // ── Pseudo — basic ──────────────────────────────────────────────────────────
    row!("Pseudo", "nop",    "",              "No operation",                        "addi x0, x0, 0"),
    row!("Pseudo", "mv",     "rd, rs",        "rd = rs",                             "addi rd, rs, 0"),
    row!("Pseudo", "li",     "rd, imm12",     "Load 12-bit immediate into rd",       "addi rd, x0, imm"),
    row!("Pseudo", "subi",   "rd, rs1, imm",  "rd = rs1 - imm",                      "addi rd, rs1, -imm"),
    row!("Pseudo", "neg",    "rd, rs",        "rd = -rs",                            "sub rd, x0, rs"),
    row!("Pseudo", "not",    "rd, rs",        "rd = ~rs  (bitwise NOT)",             "xori rd, rs, -1"),
    row!("Pseudo", "seqz",   "rd, rs",        "rd = 1 if rs == 0 else 0",            "sltiu rd, rs, 1"),
    row!("Pseudo", "snez",   "rd, rs",        "rd = 1 if rs != 0 else 0",            "sltu rd, x0, rs"),
    row!("Pseudo", "sltz",   "rd, rs",        "rd = 1 if rs < 0 else 0",             "slt rd, rs, x0"),
    row!("Pseudo", "sgtz",   "rd, rs",        "rd = 1 if rs > 0 else 0",             "slt rd, x0, rs"),
    // ── Pseudo — load address / control flow ────────────────────────────────────
    row!("Pseudo", "la",     "rd, label",     "Load address of label into rd",       "lui rd, hi; addi rd, rd, lo"),
    row!("Pseudo", "j",      "label",         "Unconditional jump to label",         "jal x0, label"),
    row!("Pseudo", "call",   "label",         "Call subroutine at label",            "jal ra, label"),
    row!("Pseudo", "jr",     "rs",            "Jump register (indirect)",            "jalr x0, rs, 0"),
    row!("Pseudo", "ret",    "",              "Return from subroutine",              "jalr x0, ra, 0"),
    // ── Pseudo — stack ──────────────────────────────────────────────────────────
    row!("Pseudo", "push",   "rs",            "sp -= 4; store rs at 0(sp)",          "addi sp,sp,-4; sw rs,0(sp)"),
    row!("Pseudo", "pop",    "rd",            "load rd from 0(sp); sp += 4",         "lw rd,0(sp); addi sp,sp,4"),
    // ── Pseudo — branches vs zero (one-register) ────────────────────────────────
    row!("Pseudo", "bez/beqz","rs, label",    "Branch if rs == 0",                   "beq rs, x0, label"),
    row!("Pseudo", "bnez",   "rs, label",     "Branch if rs != 0",                   "bne rs, x0, label"),
    row!("Pseudo", "bltz",   "rs, label",     "Branch if rs < 0",                    "blt rs, x0, label"),
    row!("Pseudo", "bgez",   "rs, label",     "Branch if rs >= 0",                   "bge rs, x0, label"),
    row!("Pseudo", "blez",   "rs, label",     "Branch if rs <= 0",                   "bge x0, rs, label"),
    row!("Pseudo", "bgtz",   "rs, label",     "Branch if rs > 0",                    "blt x0, rs, label"),
    // ── Pseudo — branches (two-register, swapped) ───────────────────────────────
    row!("Pseudo", "bgt",    "rs1, rs2, label","Branch if rs1 > rs2 (signed)",       "blt rs2, rs1, label"),
    row!("Pseudo", "ble",    "rs1, rs2, label","Branch if rs1 <= rs2 (signed)",      "bge rs2, rs1, label"),
    row!("Pseudo", "bgtu",   "rs1, rs2, label","Branch if rs1 > rs2 (unsigned)",     "bltu rs2, rs1, label"),
    row!("Pseudo", "bleu",   "rs1, rs2, label","Branch if rs1 <= rs2 (unsigned)",    "bgeu rs2, rs1, label"),
    // ── Pseudo — I/O and syscall helpers ────────────────────────────────────────
    row!("Pseudo", "print",      "rd",         "Print integer in rd (a7=1000)",       "addi a7,x0,1000; mv a0,rd; ecall"),
    row!("Pseudo", "print_str",    "label",     "Print NUL string at label",           "strlen loop; write(a0=1,a1=buf,a2=len) [syscall 64]"),
    row!("Pseudo", "print_str_ln","label",     "Print NUL string + newline",          "strlen loop; write buf; write '\\n' via stack [syscall 64]"),
    row!("Pseudo", "read",        "label",     "Read up to 256 bytes from stdin",     "read(a0=0,a1=buf,a2=256) [syscall 63]"),
    row!("Pseudo", "read_byte",   "label",     "Read decimal → store 1 byte (RAVEN)","addi a7,x0,1010; la a0,label; ecall"),
    row!("Pseudo", "read_half",   "label",     "Read decimal → store 2 bytes (RAVEN)","addi a7,x0,1011; la a0,label; ecall"),
    row!("Pseudo", "read_word",   "label",     "Read decimal → store 4 bytes (RAVEN)","addi a7,x0,1012; la a0,label; ecall"),
    row!("Pseudo", "random",      "rd",        "rd = random 32-bit word (getrandom)", "getrandom syscall via stack (4 bytes)"),
    row!("Pseudo", "random_bytes","label, n",  "Fill n random bytes at label",        "getrandom(label, n, 0) syscall"),
    // ── F extension — loads / stores ────────────────────────────────────────────
    row!("F", "flw",      "frd, imm(rs1)",      "Load f32 from mem[rs1+imm] into frd"),
    row!("F", "fsw",      "frs2, imm(rs1)",     "Store f32 in frs2 to mem[rs1+imm]"),
    // ── F extension — arithmetic ────────────────────────────────────────────────
    row!("F", "fadd.s",   "frd, frs1, frs2",    "frd = frs1 + frs2 (single precision)"),
    row!("F", "fsub.s",   "frd, frs1, frs2",    "frd = frs1 - frs2"),
    row!("F", "fmul.s",   "frd, frs1, frs2",    "frd = frs1 * frs2"),
    row!("F", "fdiv.s",   "frd, frs1, frs2",    "frd = frs1 / frs2"),
    row!("F", "fsqrt.s",  "frd, frs1",           "frd = sqrt(frs1)"),
    row!("F", "fmin.s",   "frd, frs1, frs2",    "frd = min(frs1, frs2)  (IEEE 754)"),
    row!("F", "fmax.s",   "frd, frs1, frs2",    "frd = max(frs1, frs2)  (IEEE 754)"),
    row!("F", "fmadd.s",  "frd, frs1, frs2, frs3","frd = frs1*frs2 + frs3  (fused)"),
    row!("F", "fmsub.s",  "frd, frs1, frs2, frs3","frd = frs1*frs2 - frs3  (fused)"),
    row!("F", "fnmadd.s", "frd, frs1, frs2, frs3","frd = -(frs1*frs2) - frs3  (fused)"),
    row!("F", "fnmsub.s", "frd, frs1, frs2, frs3","frd = -(frs1*frs2) + frs3  (fused)"),
    // ── F extension — sign injection ────────────────────────────────────────────
    row!("F", "fsgnj.s",  "frd, frs1, frs2",    "frd = |frs1| with sign of frs2"),
    row!("F", "fsgnjn.s", "frd, frs1, frs2",    "frd = |frs1| with negated sign of frs2"),
    row!("F", "fsgnjx.s", "frd, frs1, frs2",    "frd = |frs1| with XOR of signs"),
    // ── F extension — compare / classify ────────────────────────────────────────
    row!("F", "feq.s",    "rd, frs1, frs2",     "rd = 1 if frs1 == frs2 (ordered) else 0"),
    row!("F", "flt.s",    "rd, frs1, frs2",     "rd = 1 if frs1 < frs2  (ordered) else 0"),
    row!("F", "fle.s",    "rd, frs1, frs2",     "rd = 1 if frs1 <= frs2 (ordered) else 0"),
    row!("F", "fclass.s", "rd, frs1",            "Classify frs1 → bitmask in rd (see ISA)"),
    // ── F extension — conversions ────────────────────────────────────────────────
    row!("F", "fcvt.w.s",  "rd, frs1[, rm]",    "Convert f32 → i32; rm = rounding mode"),
    row!("F", "fcvt.wu.s", "rd, frs1[, rm]",    "Convert f32 → u32; rm = rounding mode"),
    row!("F", "fcvt.s.w",  "frd, rs1",           "Convert i32 → f32"),
    row!("F", "fcvt.s.wu", "frd, rs1",           "Convert u32 → f32"),
    // ── F extension — bit moves ──────────────────────────────────────────────────
    row!("F", "fmv.x.w",  "rd, frs1",            "Copy float bits → int register (no conversion)"),
    row!("F", "fmv.w.x",  "frd, rs1",            "Copy int bits → float register (no conversion)"),
    // ── F extension — pseudos ────────────────────────────────────────────────────
    row!("F", "fmv.s",    "frd, frs",            "Copy float register",                "fsgnj.s frd, frs, frs"),
    row!("F", "fneg.s",   "frd, frs",            "Negate: frd = -frs",                 "fsgnjn.s frd, frs, frs"),
    row!("F", "fabs.s",   "frd, frs",            "Absolute value: frd = |frs|",        "fsgnjx.s frd, frs, frs"),
    // ── Directives ──────────────────────────────────────────────────────────────
    row!("Dir", ".data",    "",              "Switch to initialized data section"),
    row!("Dir", ".text",    "",              "Switch to code section"),
    row!("Dir", ".bss",     "",              "Switch to BSS (zero-initialized) section"),
    row!("Dir", ".section", "name",         "Switch to named section (.text or .data)"),
    row!("Dir", ".byte",    "val[,...]",    "Emit 1-byte integer value(s)"),
    row!("Dir", ".half",    "val[,...]",    "Emit 2-byte value(s) little-endian"),
    row!("Dir", ".word",    "val[,...]",    "Emit 4-byte value(s) little-endian"),
    row!("Dir", ".dword",   "val[,...]",    "Emit 8-byte value(s) little-endian"),
    row!("Dir", ".float",   "val[,...]",    "Emit IEEE 754 f32 value(s) (4 bytes each)"),
    row!("Dir", ".ascii",   "\"str\"",      "Emit string bytes (no NUL terminator)"),
    row!("Dir", ".asciz",   "\"str\"",      "Emit string bytes + NUL terminator"),
    row!("Dir", ".string",  "\"str\"",      "Alias of .asciz"),
    row!("Dir", ".space",   "n",            "Reserve n zero bytes"),
    row!("Dir", ".align",   "n",            "Align PC to 2^n byte boundary"),
    row!("Dir", ".globl",   "sym",          "Mark symbol as global / exported"),
    row!("Dir", ".equ",     "sym, val",     "Define symbolic constant (equate)"),
];

// ── Filtering ──────────────────────────────────────────────────────────────────

fn ty_bit(ty: &str) -> u16 {
    match ty {
        "R"      => TY_R,
        "M"      => TY_M,
        "I"      => TY_I,
        "Load"   => TY_LOAD,
        "Store"  => TY_STORE,
        "Branch" => TY_BRANCH,
        "U"      => TY_U,
        "Jump"   => TY_JUMP,
        "SYS"    => TY_SYS,
        "Pseudo" => TY_PSEUDO,
        "F"      => TY_F,
        "Dir"    => TY_DIR,
        _        => 0,
    }
}

fn ty_color(ty: &str) -> Color {
    match ty {
        "R"      => Color::Yellow,
        "M"      => Color::LightRed,
        "I"      => Color::Green,
        "Load"   => Color::Cyan,
        "Store"  => Color::LightBlue,
        "Branch" => Color::Magenta,
        "U"      => Color::LightYellow,
        "Jump"   => Color::LightCyan,
        "SYS"    => Color::Red,
        "Pseudo" => Color::LightMagenta,
        "F"      => Color::LightGreen,
        "Dir"    => Color::Gray,
        _        => Color::White,
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
    if type_filter == 0 { return 0; }
    filtered_rows(query, type_filter).len()
}

// ── Token coloring ─────────────────────────────────────────────────────────────

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
        // Integer destination register
        "rd" | "rd2" => Some(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        // Integer source registers
        "rs1" | "rs2" | "rs3" | "rs" | "rt" => Some(Style::default().fg(Color::Cyan)),
        // Float destination register
        "frd" | "frd2" => Some(Style::default().fg(Color::Yellow)),
        // Float source registers
        "frs" | "frs1" | "frs2" | "frs3" => Some(Style::default().fg(Color::LightYellow)),
        // Immediates / shifts / parts
        "imm" | "imm12" | "imm20" | "shamt" | "hi" | "lo" | "n" => {
            Some(Style::default().fg(Color::LightGreen))
        }
        "label" => Some(Style::default().fg(Color::Magenta)),
        "rm"    => Some(Style::default().fg(Color::LightYellow)),
        "sym"   => Some(Style::default().fg(Color::LightBlue)),
        _ if is_register_token(token) => Some(Style::default().fg(Color::LightBlue)),
        _ => None,
    }
}

/// Tokenize a string and apply per-token colors, returning owned spans.
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
        if token.is_empty() { return; }
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

/// Pad a string to `width` chars, or truncate with "…" if too long.
fn pad_or_truncate(s: &str, width: usize) -> String {
    if width == 0 { return String::new(); }
    let len = s.chars().count();
    if len > width {
        let n = width.saturating_sub(1);
        let truncated: String = s.chars().take(n).collect();
        format!("{truncated}\u{2026}")  // …
    } else {
        format!("{s:<width$}")
    }
}

// ── Instruction reference rendering ───────────────────────────────────────────

/// Compute layout-dependent column widths.
fn col_widths(width: usize) -> (usize, bool) {
    let show_exp = width >= SHOW_EXP_MIN_W;
    // fixed overhead: TY_W + 1sep + MNE_W + 1sep + OPS_W + 1sep = 8+1+13+1+21+1 = 45
    // with exp: 45 + 1sep + EXP_W = 45+1+26 = 72
    let fixed = TY_W + 1 + MNE_W + 1 + OPS_W + 1;
    let exp_overhead = if show_exp { 1 + EXP_W } else { 0 };
    let desc_w = width.saturating_sub(fixed + exp_overhead).max(8);
    (desc_w, show_exp)
}

fn render_col_header(width: usize) -> Line<'static> {
    let (desc_w, show_exp) = col_widths(width);
    let hdr_style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD);
    let mut spans = vec![
        Span::styled(format!("{:<8}", "Type"),    hdr_style),
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

fn render_separator(width: usize) -> Line<'static> {
    Line::styled(
        "─".repeat(width.min(300)),
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )
}

fn render_doc_row(row: &DocRow, desc_w: usize, show_exp: bool) -> Line<'static> {
    let color = ty_color(row.ty);
    let badge = format!("{:>8}", format!("[{}]", row.ty)); // 8 chars: "[Branch]", "     [R]" etc.
    let mne   = format!("{:<13}", row.mnemonic);

    let ops_len = row.operands.chars().count();
    let mut ops_spans = color_text(row.operands);
    if ops_len < OPS_W {
        ops_spans.push(Span::raw(" ".repeat(OPS_W - ops_len)));
    }

    let desc = pad_or_truncate(row.desc, desc_w);

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(badge, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(mne, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
    ];
    spans.extend(ops_spans);
    spans.push(Span::raw(" "));
    spans.push(Span::raw(desc));

    if show_exp && !row.expands.is_empty() {
        spans.push(Span::raw(" "));
        let exp_text = format!("\u{2192} {}", row.expands);   // → expands
        let exp = pad_or_truncate(&exp_text, EXP_W);
        spans.push(Span::styled(exp, Style::default().fg(Color::Rgb(100, 100, 120))));
    }

    Line::from(spans)
}

fn render_filter_bar(f: &mut Frame, area: Rect, app: &App) {
    // Record this row's y so mouse.rs can detect clicks on it
    app.docs.filter_bar_y.set(area.y);

    let type_filter = app.docs.type_filter;
    let cursor = app.docs.filter_cursor;
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (idx, &(label, bit, color)) in FILTER_ITEMS.iter().enumerate() {
        let is_cursor = idx == cursor;
        // Determine active state:
        // "All" (idx=0) is active when all bits are on
        let is_active = if idx == 0 {
            type_filter == ALL_MASK
        } else {
            (type_filter & bit) != 0
        };

        let bullet = if is_active { "\u{25CF}" } else { "\u{25CB}" }; // ● / ○
        let text = format!(" {bullet}{label} ");

        let fg = if is_active { color } else { theme::LABEL };
        let mut style = Style::default().fg(fg);
        if is_cursor {
            style = style.bg(Color::Rgb(50, 50, 80)).add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(text, style));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render a single-line page-tab bar (InstrRef | Syscalls | Memory Map).
/// Records tab_bar_y and tab_bar_xs in app.docs for mouse click handling.
fn render_page_tabs(f: &mut Frame, area: Rect, app: &App, extra_hint: &'static str) {
    app.docs.tab_bar_y.set(area.y);

    let pages = [DocsPage::InstrRef, DocsPage::Syscalls, DocsPage::MemoryMap, DocsPage::FcacheRef];
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut xs = [(0u16, 0u16); 4];
    let mut cursor_x = area.x;

    for (i, page) in pages.iter().enumerate() {
        let active = *page == app.docs.page;
        let label = format!(" {} ", page.label());
        let label_w = label.chars().count() as u16;
        let style = if active {
            Style::default()
                .fg(Color::Rgb(0, 0, 0))
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::LABEL)
        };
        xs[i] = (cursor_x, cursor_x + label_w);
        cursor_x += label_w;
        spans.push(Span::styled(label, style));

        // divider between tabs
        spans.push(Span::styled("│", Style::default().fg(Color::Rgb(60, 60, 80))));
        cursor_x += 1;
    }
    app.docs.tab_bar_xs.set(xs);

    if !extra_hint.is_empty() {
        spans.push(Span::styled(extra_hint, Style::default().fg(theme::LABEL)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Public rendering entry point ──────────────────────────────────────────────

pub(super) fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    match app.docs.page {
        DocsPage::InstrRef   => render_instr_ref(f, area, app),
        DocsPage::Syscalls   => render_free_page(f, area, app, syscall_lines(app.docs.lang)),
        DocsPage::MemoryMap  => render_free_page(f, area, app, memory_map_lines(app.docs.lang)),
        DocsPage::FcacheRef  => render_free_page(f, area, app, fcache_ref_lines(app.docs.lang)),
    }
}

fn render_instr_ref(f: &mut Frame, area: Rect, app: &App) {
    let search_bar_h: u16 = if app.docs.search_open { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),            // page tab bar (new)
            Constraint::Length(2),            // legend header
            Constraint::Length(search_bar_h), // search bar
            Constraint::Length(1),            // filter bar
            Constraint::Min(0),               // table area
        ])
        .split(area);

    let tab_area    = chunks[0];
    let meta_area   = chunks[1];
    let search_area = chunks[2];
    let filter_area = chunks[3];
    let table_area  = chunks[4];

    // ── Page tab bar ──
    let search_hint = if app.docs.search_open { "  Ctrl+F=search" } else { "" };
    let filter_hint = if !app.docs.search_open { "  ←/→=filter  Space=toggle" } else { "" };
    let tab_hint = format!("{search_hint}{filter_hint}  ↑/↓=scroll");
    // We can't use a &'static str here — pass empty and append hint spans manually.
    render_page_tabs(f, tab_area, app, "");
    // Append hints to the tab bar line without overwriting positions
    {
        let hint_x = {
            let xs = app.docs.tab_bar_xs.get();
            // after last tab (index 3): x_end + 1 for divider
            xs[3].1 + 1
        };
        let hint_area = Rect::new(hint_x.min(tab_area.x + tab_area.width), tab_area.y, tab_area.width.saturating_sub(hint_x.saturating_sub(tab_area.x)), 1);
        f.render_widget(
            Paragraph::new(Span::styled(tab_hint, Style::default().fg(theme::LABEL))),
            hint_area,
        );
    }

    // ── Legend header (2 lines) ──
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
        Line::styled(
            "─".repeat(area.width.min(300) as usize),
            Style::default().fg(Color::Rgb(60, 60, 80)),
        ),
    ];
    f.render_widget(Paragraph::new(meta_lines), meta_area);

    // ── Search bar ──
    if app.docs.search_open {
        let bar_style = Style::default().fg(theme::LABEL).bg(Color::Rgb(30, 30, 50));
        let bar_line = Line::from(vec![
            Span::styled(" Find: ", Style::default().fg(theme::ACCENT).bg(Color::Rgb(30, 30, 50))),
            Span::styled(app.docs.search_query.clone(), Style::default().fg(theme::LABEL_Y).bg(Color::Rgb(30, 30, 50))),
            Span::styled("  Esc=close", Style::default().fg(theme::LABEL).bg(Color::Rgb(30, 30, 50))),
        ]);
        f.render_widget(Paragraph::new(bar_line).style(bar_style), search_area);

        let prefix_len = " Find: ".len() as u16;
        let cursor_x = (search_area.x + prefix_len + app.docs.search_query.chars().count() as u16)
            .min(search_area.x + search_area.width.saturating_sub(1));
        if search_area.height > 0 {
            f.set_cursor_position((cursor_x, search_area.y));
        }
    }

    // ── Filter bar ──
    render_filter_bar(f, filter_area, app);

    if table_area.height == 0 || table_area.width == 0 { return; }

    // ── Table ──
    let w = table_area.width as usize;
    let (desc_w, show_exp) = col_widths(w);

    // Split table_area into col_header (1 line) + sep (1 line) + data rows
    let table_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // col header
            Constraint::Length(1),  // separator
            Constraint::Min(0),     // data rows
        ])
        .split(table_area);

    f.render_widget(Paragraph::new(render_col_header(w)), table_chunks[0]);
    f.render_widget(Paragraph::new(render_separator(w)), table_chunks[1]);

    let data_area = table_chunks[2];
    if data_area.height == 0 { return; }

    let rows = if app.docs.type_filter == 0 {
        vec![]
    } else {
        let q = if app.docs.search_open { app.docs.search_query.as_str() } else { "" };
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

// ── Free-form page renderer (Syscalls / MemoryMap) ────────────────────────────

/// Generic renderer for scrollable text pages with a 2-line header.
fn render_free_page(f: &mut Frame, area: Rect, app: &App, lines: Vec<Line<'static>>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header (tab bar + separator)
            Constraint::Min(0),    // content
        ])
        .split(area);

    // ── Tab bar (line 0 of header) ──
    let lang_hint = format!("  [{}] L=lang  ↑/↓=scroll", app.docs.lang.label());
    // render_page_tabs takes a &'static str; we pass a static stub and append a styled hint span
    let tab_area = Rect::new(chunks[0].x, chunks[0].y, chunks[0].width, 1);
    render_page_tabs(f, tab_area, app, "");
    {
        let hint_x = {
            let xs = app.docs.tab_bar_xs.get();
            xs[2].1 + 1
        };
        let hint_area = Rect::new(
            hint_x.min(tab_area.x + tab_area.width),
            tab_area.y,
            tab_area.width.saturating_sub(hint_x.saturating_sub(tab_area.x)),
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled(lang_hint, Style::default().fg(theme::LABEL))),
            hint_area,
        );
    }

    // ── Separator (line 1 of header) ──
    let sep_area = Rect::new(chunks[0].x, chunks[0].y + 1, chunks[0].width, 1);
    f.render_widget(
        Paragraph::new(Line::styled(
            "─".repeat(area.width.min(300) as usize),
            Style::default().fg(Color::Rgb(60, 60, 80)),
        )),
        sep_area,
    );

    // ── Scrollable content ──
    let content_area = chunks[1];
    if content_area.height == 0 { return; }
    let viewport_h = content_area.height as usize;
    let max_start = lines.len().saturating_sub(viewport_h);
    let start = app.docs.scroll.min(max_start);
    let end = (start + viewport_h).min(lines.len());
    f.render_widget(
        Paragraph::new(lines[start..end].to_vec()).wrap(Wrap { trim: false }),
        content_area,
    );
}

/// Total scrollable line count for free-form pages (used by clamp helpers).
pub(crate) fn free_page_line_count(page: DocsPage, lang: DocsLang) -> usize {
    match page {
        DocsPage::InstrRef   => 0,
        DocsPage::Syscalls   => syscall_lines(lang).len(),
        DocsPage::MemoryMap  => memory_map_lines(lang).len(),
        DocsPage::FcacheRef  => fcache_ref_lines(lang).len(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn h1(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(s, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
}
fn h2(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(s, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
}
fn kv(key: &'static str, val: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<12}"), Style::default().fg(Color::Yellow)),
        Span::styled(val, Style::default().fg(Color::White)),
    ])
}
fn note(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(format!("  {s}"), Style::default().fg(Color::DarkGray)))
}
fn blank() -> Line<'static> { Line::raw("") }
fn raw(s: &'static str) -> Line<'static> { Line::raw(s) }
fn mono(s: &'static str) -> Line<'static> {
    Line::from(Span::styled(s, Style::default().fg(Color::Rgb(180, 180, 200))))
}
fn trow(a7: &'static str, name: &'static str, args: &'static str, ret: &'static str, notes: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {a7:<6}"), Style::default().fg(Color::LightGreen)),
        Span::styled(format!("{name:<16}"), Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{args:<28}"), Style::default().fg(Color::White)),
        Span::styled(format!("{ret:<10}"), Style::default().fg(Color::Yellow)),
        Span::styled(notes, Style::default().fg(Color::DarkGray)),
    ])
}
fn thead() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "  a7    Name            Arguments                    Return    Notes",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
    ])
}
fn tsep() -> Line<'static> {
    Line::styled(
        "  ──────────────────────────────────────────────────────────────────────",
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )
}

// ── Syscall reference content ─────────────────────────────────────────────────

fn syscall_lines(lang: DocsLang) -> Vec<Line<'static>> {
    match lang {
        DocsLang::En  => syscall_lines_en(),
        DocsLang::PtBr => syscall_lines_ptbr(),
    }
}

fn syscall_lines_en() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Syscall Reference"),
        blank(),
        note("Calling convention: a7 = syscall number · a0..a5 = arguments · a0 = return value"),
        note("Negative return values signal errors (Linux errno convention, e.g. -9 = EBADF)."),
        blank(),

        // ── Linux ABI ──
        h2("Linux-compatible syscalls"),
        blank(),
        thead(),
        tsep(),
        trow("63",  "read",         "fd=a0, buf=a1, n=a2",   "bytes read",  "fd=0 (stdin) only; blocks until line ready"),
        trow("64",  "write",        "fd=a0, buf=a1, n=a2",   "bytes written","fd=1 (stdout) or 2 (stderr)"),
        trow("93",  "exit",         "code=a0",               "—",           "halts execution; sets exit code"),
        trow("94",  "exit_group",   "code=a0",               "—",           "alias of exit (93)"),
        trow("278", "getrandom",    "buf=a0, len=a1, flags=a2","len",        "fills buf with cryptographic random bytes"),
        trow("66",  "writev",       "fd=a0, iov=a1, n=a2",   "bytes written","scatter-write; iovec={u32 base, u32 len}; fd=1/2 only"),
        trow("172", "getpid",       "—",                     "1",           "always returns pid 1"),
        trow("174", "getuid",       "—",                     "0",           "always returns uid 0"),
        trow("176", "getgid",       "—",                     "0",           "always returns gid 0"),
        trow("215", "munmap",       "addr=a0, len=a1",       "0",           "no-op; memory is never freed in Raven"),
        trow("222", "mmap",         "0,len,prot,flags,fd=-1,0","ptr",       "anon heap alloc (MAP_ANONYMOUS=0x20 required)"),
        trow("403", "clock_gettime","clockid=a0, *tp=a1",    "0",           "writes {tv_sec,tv_nsec} based on instr_count; ~10ns/instr"),
        blank(),
        note("Supported getrandom flags: GRND_NONBLOCK (0x1), GRND_RANDOM (0x2)."),
        blank(),

        // ── RAVEN extensions ──
        h2("RAVEN teaching extensions  (a7 ≥ 1000)"),
        blank(),
        thead(),
        tsep(),
        trow("1000", "print_int",    "a0=integer",            "—",           "prints a0 as signed decimal to console"),
        trow("1001", "print_zstr",   "a0=addr",               "—",           "prints NUL-terminated string at addr"),
        trow("1002", "print_zstr_ln","a0=addr",               "—",           "same as 1001 + appends newline"),
        trow("1003", "read_line_z",  "a0=addr",               "—",           "reads console line into addr (NUL-terminated); blocks"),
        trow("1004", "print_uint",   "a0=u32",                "—",           "prints a0 as unsigned decimal"),
        trow("1005", "print_hex",    "a0=u32",                "—",           "prints a0 as hex  e.g. 0xDEADBEEF"),
        trow("1006", "print_char",   "a0=ascii",              "—",           "prints one ASCII character"),
        trow("1008", "print_newline","—",                     "—",           "prints a newline"),
        trow("1010", "read_u8",      "a0=addr",               "—",           "reads decimal from console; stores 1 byte at addr"),
        trow("1011", "read_u16",     "a0=addr",               "—",           "reads decimal from console; stores 2 bytes at addr"),
        trow("1012", "read_u32",     "a0=addr",               "—",           "reads decimal from console; stores 4 bytes at addr"),
        trow("1013", "read_int",     "a0=addr",               "—",           "reads signed int (accepts negatives); stores 4 bytes"),
        trow("1014", "read_float",   "a0=addr",               "—",           "reads f32 from console; stores 4 bytes (IEEE 754)"),
        trow("1015", "print_float",  "fa0=f32",               "—",           "prints fa0 as float (up to 6 significant digits)"),
        trow("1030", "get_instr_count","—",                   "a0=count",    "returns instructions executed since start (low 32 bits)"),
        trow("1031", "get_cycle_count","—",                   "a0=count",    "alias of 1030; cycle-accurate may differ in future"),
        blank(),
        h2("RAVEN memory utilities  (a7 ≥ 1050)"),
        blank(),
        thead(),
        tsep(),
        trow("1050", "memset",       "a0=dst, a1=byte, a2=len","—",         "fills len bytes at dst with byte value"),
        trow("1051", "memcpy",       "a0=dst, a1=src, a2=len","—",          "copies len bytes from src to dst"),
        trow("1052", "strlen",       "a0=addr",               "a0=len",     "returns length of NUL-terminated string"),
        trow("1053", "strcmp",       "a0=s1, a1=s2",          "a0=cmp",     "compares strings; <0 / 0 / >0"),
        blank(),

        // ── Usage example ──
        h2("Example — write(1, buf, 5) via raw ecall"),
        blank(),
        mono("  .data"),
        mono("  msg: .ascii \"hello\""),
        mono("  .text"),
        mono("      la   a1, msg      ; a1 = address of msg"),
        mono("      li   a0, 1        ; a0 = fd 1 (stdout)"),
        mono("      li   a2, 5        ; a2 = 5 bytes"),
        mono("      li   a7, 64       ; a7 = write"),
        mono("      ecall             ; a0 = bytes written (5)"),
        blank(),
        note("Pseudo-instructions like print, print_str, read, etc. expand to these ecalls automatically."),
    ]
}

fn syscall_lines_ptbr() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Referência de Syscalls"),
        blank(),
        note("Convenção: a7 = número da syscall · a0..a5 = argumentos · a0 = valor de retorno"),
        note("Retornos negativos indicam erros (convenção Linux errno, ex.: -9 = EBADF)."),
        blank(),

        // ── Linux ABI ──
        h2("Syscalls compatíveis com Linux"),
        blank(),
        thead(),
        tsep(),
        trow("63",  "read",         "fd=a0, buf=a1, n=a2",   "bytes lidos", "fd=0 (stdin); bloqueia até linha disponível"),
        trow("64",  "write",        "fd=a0, buf=a1, n=a2",   "bytes escritos","fd=1 (stdout) ou 2 (stderr)"),
        trow("93",  "exit",         "code=a0",               "—",           "encerra execução; define código de saída"),
        trow("94",  "exit_group",   "code=a0",               "—",           "alias de exit (93)"),
        trow("278", "getrandom",    "buf=a0, len=a1, flags=a2","len",        "preenche buf com bytes aleatórios criptográficos"),
        trow("66",  "writev",       "fd=a0, iov=a1, n=a2",   "bytes escritos","scatter-write; iovec={u32 base, u32 len}; fd=1/2"),
        trow("172", "getpid",       "—",                     "1",           "retorna pid fixo 1"),
        trow("174", "getuid",       "—",                     "0",           "retorna uid fixo 0"),
        trow("176", "getgid",       "—",                     "0",           "retorna gid fixo 0"),
        trow("215", "munmap",       "addr=a0, len=a1",       "0",           "nop; memória não é liberada no Raven"),
        trow("222", "mmap",         "0,len,prot,flags,fd=-1,0","ptr",       "aloca do heap anonimamente (MAP_ANONYMOUS=0x20)"),
        trow("403", "clock_gettime","clockid=a0, *tp=a1",    "0",           "escreve {tv_sec,tv_nsec} com base em instr_count"),
        blank(),
        note("Flags aceitas em getrandom: GRND_NONBLOCK (0x1), GRND_RANDOM (0x2)."),
        blank(),

        // ── RAVEN extensions ──
        h2("Extensões didáticas do RAVEN  (a7 ≥ 1000)"),
        blank(),
        thead(),
        tsep(),
        trow("1000", "print_int",    "a0=inteiro",            "—",           "imprime a0 como decimal com sinal no console"),
        trow("1001", "print_zstr",   "a0=endereço",           "—",           "imprime string terminada em NUL no endereço"),
        trow("1002", "print_zstr_ln","a0=endereço",           "—",           "igual a 1001 + adiciona nova linha"),
        trow("1003", "read_line_z",  "a0=endereço",           "—",           "lê linha do console em addr (NUL no fim); bloqueia"),
        trow("1004", "print_uint",   "a0=u32",                "—",           "imprime a0 como decimal sem sinal"),
        trow("1005", "print_hex",    "a0=u32",                "—",           "imprime a0 em hex, ex.: 0xDEADBEEF"),
        trow("1006", "print_char",   "a0=ascii",              "—",           "imprime um caractere ASCII"),
        trow("1008", "print_newline","—",                     "—",           "imprime nova linha"),
        trow("1010", "read_u8",      "a0=endereço",           "—",           "lê decimal do console; armazena 1 byte em addr"),
        trow("1011", "read_u16",     "a0=endereço",           "—",           "lê decimal do console; armazena 2 bytes em addr"),
        trow("1012", "read_u32",     "a0=endereço",           "—",           "lê decimal do console; armazena 4 bytes em addr"),
        trow("1013", "read_int",     "a0=endereço",           "—",           "lê inteiro com sinal (aceita negativos); armazena 4 bytes"),
        trow("1014", "read_float",   "a0=endereço",           "—",           "lê f32 do console; armazena 4 bytes (IEEE 754)"),
        trow("1015", "print_float",  "fa0=f32",               "—",           "imprime fa0 como float (até 6 dígitos significativos)"),
        trow("1030", "get_instr_count","—",                   "a0=count",    "retorna instruções executadas desde o início (32 bits)"),
        trow("1031", "get_cycle_count","—",                   "a0=count",    "alias de 1030; pode diferir em versão futura"),
        blank(),
        h2("Utilitários de memória do RAVEN  (a7 ≥ 1050)"),
        blank(),
        thead(),
        tsep(),
        trow("1050", "memset",       "a0=dst, a1=byte, a2=len","—",         "preenche len bytes em dst com o valor byte"),
        trow("1051", "memcpy",       "a0=dst, a1=src, a2=len","—",          "copia len bytes de src para dst"),
        trow("1052", "strlen",       "a0=endereço",           "a0=len",     "retorna comprimento de string terminada em NUL"),
        trow("1053", "strcmp",       "a0=s1, a1=s2",          "a0=cmp",     "compara strings; <0 / 0 / >0"),
        blank(),

        // ── Exemplo ──
        h2("Exemplo — write(1, buf, 5) via ecall direto"),
        blank(),
        mono("  .data"),
        mono("  msg: .ascii \"hello\""),
        mono("  .text"),
        mono("      la   a1, msg      ; a1 = endereço de msg"),
        mono("      li   a0, 1        ; a0 = fd 1 (stdout)"),
        mono("      li   a2, 5        ; a2 = 5 bytes"),
        mono("      li   a7, 64       ; a7 = write"),
        mono("      ecall             ; a0 = bytes escritos (5)"),
        blank(),
        note("Pseudo-instruções como print, print_str, read, etc. expandem para esses ecalls automaticamente."),
    ]
}

// ── Memory map content ────────────────────────────────────────────────────────

fn memory_map_lines(lang: DocsLang) -> Vec<Line<'static>> {
    match lang {
        DocsLang::En  => memory_map_lines_en(),
        DocsLang::PtBr => memory_map_lines_ptbr(),
    }
}

fn memory_map_lines_en() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Memory Map"),
        blank(),
        note("RAM = 128 KB · addresses 0x00000000 .. 0x0001FFFF · no MMU, no virtual memory"),
        blank(),

        // ── ASCII diagram ──
        h2("Layout"),
        blank(),
        mono("  0x00000000  ┌─────────────────────┐"),
        mono("              │  .text  (code)       │  ← instructions start at base_pc (default 0x0)"),
        mono("              ├─────────────────────┤"),
        mono("  0x00001000  │  .data              │  ← initialized data  (data_base = base_pc + 0x1000)"),
        mono("              │  .bss               │  ← zero-initialized; grows up after .data"),
        mono("              ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤"),
        mono("              │   (free space)       │  ← no allocator; read/write freely with lw/sw"),
        mono("              ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤"),
        mono("  0x0001FFFF  │  stack  (grows ↓)   │  ← sp = 0x20000 (one past end); push: sp-=4, sw rs,0(sp)"),
        mono("              └─────────────────────┘"),
        blank(),

        // ── Sections ──
        h2("Sections"),
        blank(),
        kv(".text",  "Assembled instructions, loaded at base_pc."),
        kv(".data",  "Initialized bytes (.byte/.half/.word/.float/.ascii/.asciz)."),
        kv(".bss",   "Zero-initialized reservation (.space N). No bytes in binary."),
        blank(),

        // ── Addresses ──
        h2("Key addresses"),
        blank(),
        kv("base_pc",    "Start of .text. Configurable; default 0x00000000."),
        kv("data_base",  "Start of .data / .bss = base_pc + 0x1000."),
        kv("sp (initial)","0x00020000 — one past end of RAM (RISC-V ABI). First push writes to 0x1FFFC."),
        blank(),

        // ── Free space note ──
        h2("Free space — no heap allocator"),
        blank(),
        raw("  The region between bss_end and the stack is ordinary RAM with no"),
        raw("  management. You can use it directly with sw/lw if you know the address."),
        raw("  There is no malloc/free — RAVEN has a flat, fixed 128 KB address space"),
        raw("  with no pagination or memory protection."),
        blank(),
        note("Tip: use .bss labels to reserve named buffers without wasting binary space."),
        blank(),

        // ── Access example ──
        h2("Example — using free space manually"),
        blank(),
        mono("  .bss"),
        mono("  buf: .space 64      ; reserve 64 bytes (address known at assemble time)"),
        mono("  .text"),
        mono("      la   t0, buf    ; t0 = &buf"),
        mono("      li   t1, 42"),
        mono("      sw   t1, 0(t0)  ; store 42 at buf[0]"),
        mono("      lw   t2, 0(t0)  ; load back → t2 = 42"),
        blank(),

        // ── Bump allocator ──
        h2("Manual heap — bump allocator"),
        blank(),
        raw("  To allocate dynamically at runtime, store a heap pointer in .data and"),
        raw("  advance it on each allocation. The heap grows upward; the stack grows"),
        raw("  downward — they will collide if combined use exceeds free space."),
        blank(),
        mono("  .data"),
        mono("  heap_ptr: .word 0x00004000   ; initial heap base (above .bss)"),
        blank(),
        mono("  ; alloc(a1 = size) → a0 = pointer to allocated block"),
        mono("  alloc:"),
        mono("      la   t0, heap_ptr"),
        mono("      lw   a0, 0(t0)      ; a0 = current heap_ptr (return value)"),
        mono("      add  t1, a0, a1     ; t1 = heap_ptr + size"),
        mono("      sw   t1, 0(t0)      ; heap_ptr += size"),
        mono("      ret"),
        blank(),
        note("There is no free() — allocations are permanent for the lifetime of the program."),
    ]
}

fn memory_map_lines_ptbr() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Mapa de Memória"),
        blank(),
        note("RAM = 128 KB · endereços 0x00000000 .. 0x0001FFFF · sem MMU, sem memória virtual"),
        blank(),

        // ── Diagrama ASCII ──
        h2("Layout"),
        blank(),
        mono("  0x00000000  ┌─────────────────────┐"),
        mono("              │  .text  (código)     │  ← instruções começam em base_pc (padrão 0x0)"),
        mono("              ├─────────────────────┤"),
        mono("  0x00001000  │  .data              │  ← dados inicializados  (data_base = base_pc + 0x1000)"),
        mono("              │  .bss               │  ← inicializada com zeros; cresce após .data"),
        mono("              ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤"),
        mono("              │   (espaço livre)     │  ← sem alocador; leitura/escrita livre com lw/sw"),
        mono("              ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤"),
        mono("  0x0001FFFF  │  pilha  (cresce ↓)  │  ← sp = 0x20000 (um além do fim); push: sp-=4, sw rs,0(sp)"),
        mono("              └─────────────────────┘"),
        blank(),

        // ── Seções ──
        h2("Seções"),
        blank(),
        kv(".text",  "Instruções montadas, carregadas em base_pc."),
        kv(".data",  "Bytes inicializados (.byte/.half/.word/.float/.ascii/.asciz)."),
        kv(".bss",   "Reserva zerada (.space N). Não ocupa espaço no binário."),
        blank(),

        // ── Endereços ──
        h2("Endereços importantes"),
        blank(),
        kv("base_pc",    "Início do .text. Configurável; padrão 0x00000000."),
        kv("data_base",  "Início do .data / .bss = base_pc + 0x1000."),
        kv("sp (inicial)","0x00020000 — um além do fim da RAM (ABI RISC-V). Primeiro push escreve em 0x1FFFC."),
        blank(),

        // ── Espaço livre ──
        h2("Espaço livre — sem alocador de heap"),
        blank(),
        raw("  A região entre o fim do .bss e a pilha é RAM comum, sem gerenciamento."),
        raw("  Você pode usá-la diretamente com sw/lw se souber o endereço."),
        raw("  Não existe malloc/free — o RAVEN tem um espaço de endereçamento"),
        raw("  plano e fixo de 128 KB, sem paginação nem proteção de memória."),
        blank(),
        note("Dica: use labels no .bss para reservar buffers nomeados sem desperdiçar espaço no binário."),
        blank(),

        // ── Exemplo ──
        h2("Exemplo — usando o espaço livre manualmente"),
        blank(),
        mono("  .bss"),
        mono("  buf: .space 64      ; reserva 64 bytes (endereço conhecido na montagem)"),
        mono("  .text"),
        mono("      la   t0, buf    ; t0 = &buf"),
        mono("      li   t1, 42"),
        mono("      sw   t1, 0(t0)  ; armazena 42 em buf[0]"),
        mono("      lw   t2, 0(t0)  ; lê de volta → t2 = 42"),
        blank(),

        // ── Alocador bump ──
        h2("Heap manual — alocador bump"),
        blank(),
        raw("  Para alocar dinamicamente em tempo de execução, armazene um ponteiro de"),
        raw("  heap no .data e avance-o a cada alocação. O heap cresce para cima; a"),
        raw("  pilha cresce para baixo — colidem se o uso combinado exceder o espaço livre."),
        blank(),
        mono("  .data"),
        mono("  heap_ptr: .word 0x00004000   ; base inicial do heap (acima do .bss)"),
        blank(),
        mono("  ; alloc(a1 = tamanho) → a0 = ponteiro para o bloco alocado"),
        mono("  alloc:"),
        mono("      la   t0, heap_ptr"),
        mono("      lw   a0, 0(t0)      ; a0 = heap_ptr atual (valor de retorno)"),
        mono("      add  t1, a0, a1     ; t1 = heap_ptr + tamanho"),
        mono("      sw   t1, 0(t0)      ; heap_ptr += tamanho"),
        mono("      ret"),
        blank(),
        note("Não existe free() — as alocações são permanentes durante a execução do programa."),
    ]
}

// ── .fcache Config Reference ──────────────────────────────────────────────────

fn fcache_ref_lines(lang: DocsLang) -> Vec<Line<'static>> {
    match lang {
        DocsLang::En    => fcache_ref_lines_en(),
        DocsLang::PtBr  => fcache_ref_lines_ptbr(),
    }
}

fn fcache_ref_lines_en() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — .fcache Config Reference"),
        blank(),
        note("A .fcache file stores cache hierarchy and CPI settings for sharing and reloading."),
        note("Use Ctrl+E to export and Ctrl+L to import on the Cache tab."),
        blank(),

        h2("Format Rules"),
        blank(),
        raw("  • Lines starting with # are comments and are ignored."),
        raw("  • Each setting is a key=value pair on its own line."),
        raw("  • Line order does not matter."),
        raw("  • Unknown keys are silently ignored (forward-compatible)."),
        raw("  • CPI keys are optional — missing keys use the default values shown below."),
        blank(),

        h2("Level Prefixes"),
        blank(),
        kv("icache",  "L1 Instruction Cache"),
        kv("dcache",  "L1 Data Cache"),
        kv("l2",      "Level 2 unified cache"),
        kv("l3",      "Level 3 unified cache"),
        kv("l4",      "Level 4 unified cache"),
        kv("levels=N","Number of extra levels beyond L1 (0 = L1 only, 1 = L1+L2, …)"),
        blank(),

        h2("Cache Level Keys  (prefix.key=value)"),
        blank(),
        Line::from(vec![
            Span::styled(format!("  {:<20}", "Key suffix"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Type"),  Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<28}", "Valid values / range"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Default"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled("Notes", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ]),
        tsep(),
        Line::from(vec![Span::styled(format!("  {:<20}", ".size"),           Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "64 – 1 048 576 bytes, pow2"),    Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Total cache size",          Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".line_size"),      Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "4 – 512 bytes, pow2"),          Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Cache line / block size",   Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".associativity"),  Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 16"),                        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("1=direct-mapped, N=N-way",  Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".replacement"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "Lru Mru Fifo Random Lfu Clock"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Eviction policy",           Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".write_policy"),   Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "WriteBack WriteThrough"),        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                          Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".write_alloc"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "WriteAllocate NoWriteAllocate"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                          Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".inclusion"),      Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "NonInclusive Inclusive Exclusive"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "NonInclusive"), Style::default().fg(Color::DarkGray)), Span::styled("optional",                 Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".hit_latency"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 999 cycles"),               Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                          Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".miss_penalty"),   Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "0 – 9999 cycles"),              Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Stall cycles on miss",      Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".assoc_penalty"),  Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "0 – 99"),                        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),               Style::default().fg(Color::DarkGray)), Span::styled("Extra cyc/way tag search", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".transfer_width"), Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 512 bytes"),                 Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "8"),               Style::default().fg(Color::DarkGray)), Span::styled("Bus width for line xfer",  Style::default().fg(Color::DarkGray))]),
        blank(),

        h2("CPI Config Keys"),
        blank(),
        Line::from(vec![
            Span::styled(format!("  {:<22}", "Key"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Type"),  Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Default"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled("Description", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ]),
        tsep(),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.alu"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),  Style::default().fg(Color::DarkGray)), Span::styled("add/sub/and/or/xor/shift/lui/auipc",       Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.mul"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "3"),  Style::default().fg(Color::DarkGray)), Span::styled("mul/mulh/mulhsu/mulhu",                    Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.div"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "20"), Style::default().fg(Color::DarkGray)), Span::styled("div/divu/rem/remu",                        Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.load"),             Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "0"),  Style::default().fg(Color::DarkGray)), Span::styled("extra load overhead (beyond cache latency)",Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.store"),            Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "0"),  Style::default().fg(Color::DarkGray)), Span::styled("extra store overhead",                     Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.branch_taken"),     Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "3"),  Style::default().fg(Color::DarkGray)), Span::styled("branch when taken (pipeline flush)",       Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.branch_not_taken"), Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),  Style::default().fg(Color::DarkGray)), Span::styled("branch when not taken",                    Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.jump"),             Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "2"),  Style::default().fg(Color::DarkGray)), Span::styled("jal / jalr",                               Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.system"),           Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "10"), Style::default().fg(Color::DarkGray)), Span::styled("ecall / ebreak / halt",                    Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.fp"),               Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "integer"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "5"),  Style::default().fg(Color::DarkGray)), Span::styled("RV32F float instructions",                 Style::default().fg(Color::White))]),
        blank(),

        h2("Annotated Example File"),
        blank(),
        mono("  # Raven Cache Config v2"),
        mono("  levels=1"),
        blank(),
        mono("  icache.size=4096"),
        mono("  icache.line_size=32"),
        mono("  icache.associativity=2"),
        mono("  icache.replacement=Lru"),
        mono("  icache.write_policy=WriteBack"),
        mono("  icache.write_alloc=WriteAllocate"),
        mono("  icache.hit_latency=1"),
        mono("  icache.miss_penalty=50"),
        mono("  icache.assoc_penalty=1"),
        mono("  icache.transfer_width=8"),
        blank(),
        mono("  dcache.size=4096"),
        mono("  dcache.line_size=32"),
        mono("  dcache.associativity=4"),
        mono("  dcache.replacement=Lru"),
        mono("  dcache.write_policy=WriteBack"),
        mono("  dcache.write_alloc=WriteAllocate"),
        mono("  dcache.hit_latency=2"),
        mono("  dcache.miss_penalty=50"),
        mono("  dcache.assoc_penalty=1"),
        mono("  dcache.transfer_width=8"),
        blank(),
        mono("  l2.size=131072"),
        mono("  l2.line_size=64"),
        mono("  l2.associativity=8"),
        mono("  l2.replacement=Lru"),
        mono("  l2.write_policy=WriteBack"),
        mono("  l2.write_alloc=WriteAllocate"),
        mono("  l2.inclusion=NonInclusive"),
        mono("  l2.hit_latency=10"),
        mono("  l2.miss_penalty=200"),
        mono("  l2.assoc_penalty=2"),
        mono("  l2.transfer_width=16"),
        blank(),
        mono("  # --- CPI Config ---"),
        mono("  cpi.alu=1"),
        mono("  cpi.mul=3"),
        mono("  cpi.div=20"),
        mono("  cpi.load=0"),
        mono("  cpi.store=0"),
        mono("  cpi.branch_taken=3"),
        mono("  cpi.branch_not_taken=1"),
        mono("  cpi.jump=2"),
        mono("  cpi.system=10"),
        mono("  cpi.fp=5"),
        blank(),
    ]
}

fn fcache_ref_lines_ptbr() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Referência de Configuração .fcache"),
        blank(),
        note("Um arquivo .fcache armazena configurações de hierarquia de cache e CPI para compartilhar e recarregar."),
        note("Use Ctrl+E para exportar e Ctrl+L para importar na aba Cache."),
        blank(),

        h2("Regras de Formato"),
        blank(),
        raw("  • Linhas começando com # são comentários e são ignoradas."),
        raw("  • Cada configuração é um par chave=valor em uma linha própria."),
        raw("  • A ordem das linhas não importa."),
        raw("  • Chaves desconhecidas são silenciosamente ignoradas (compatível com versões futuras)."),
        raw("  • Chaves CPI são opcionais — chaves ausentes usam os valores padrão mostrados abaixo."),
        blank(),

        h2("Prefixos de Nível"),
        blank(),
        kv("icache",  "Cache de Instruções L1"),
        kv("dcache",  "Cache de Dados L1"),
        kv("l2",      "Cache unificado nível 2"),
        kv("l3",      "Cache unificado nível 3"),
        kv("l4",      "Cache unificado nível 4"),
        kv("levels=N","Número de níveis extras além do L1 (0 = só L1, 1 = L1+L2, …)"),
        blank(),

        h2("Chaves de Nível de Cache  (prefixo.chave=valor)"),
        blank(),
        Line::from(vec![
            Span::styled(format!("  {:<20}", "Sufixo da chave"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Tipo"),    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<28}", "Valores válidos / intervalo"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Padrão"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled("Notas", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ]),
        tsep(),
        Line::from(vec![Span::styled(format!("  {:<20}", ".size"),           Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "64 – 1 048 576 bytes, pot2"),    Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Tamanho total do cache",      Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".line_size"),      Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "4 – 512 bytes, pot2"),          Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Tamanho da linha de cache",   Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".associativity"),  Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 16"),                        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("1=mapeamento direto, N=N-way",Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".replacement"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "Lru Mru Fifo Random Lfu Clock"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Política de substituição",   Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".write_policy"),   Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "WriteBack WriteThrough"),        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                            Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".write_alloc"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "WriteAllocate NoWriteAllocate"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                            Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".inclusion"),      Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "enum"),    Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "NonInclusive Inclusive Exclusive"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "NonInclusive"), Style::default().fg(Color::DarkGray)), Span::styled("opcional",                   Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".hit_latency"),    Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 999 ciclos"),               Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("",                            Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".miss_penalty"),   Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "0 – 9999 ciclos"),              Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "—"),               Style::default().fg(Color::DarkGray)), Span::styled("Ciclos de espera em miss",   Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".assoc_penalty"),  Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "0 – 99"),                        Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),               Style::default().fg(Color::DarkGray)), Span::styled("Ciclos extras/via na busca", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled(format!("  {:<20}", ".transfer_width"), Style::default().fg(Color::Yellow)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<28}", "1 – 512 bytes"),                 Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "8"),               Style::default().fg(Color::DarkGray)), Span::styled("Largura do barramento",      Style::default().fg(Color::DarkGray))]),
        blank(),

        h2("Chaves de Configuração CPI"),
        blank(),
        Line::from(vec![
            Span::styled(format!("  {:<22}", "Chave"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Tipo"),    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<10}", "Padrão"),  Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled("Descrição", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ]),
        tsep(),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.alu"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),  Style::default().fg(Color::DarkGray)), Span::styled("add/sub/and/or/xor/shift/lui/auipc",          Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.mul"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "3"),  Style::default().fg(Color::DarkGray)), Span::styled("mul/mulh/mulhsu/mulhu",                       Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.div"),              Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "20"), Style::default().fg(Color::DarkGray)), Span::styled("div/divu/rem/remu",                           Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.load"),             Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "0"),  Style::default().fg(Color::DarkGray)), Span::styled("overhead extra de load (além da latência de cache)", Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.store"),            Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "0"),  Style::default().fg(Color::DarkGray)), Span::styled("overhead extra de store",                     Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.branch_taken"),     Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "3"),  Style::default().fg(Color::DarkGray)), Span::styled("branch tomado (flush do pipeline)",           Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.branch_not_taken"), Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "1"),  Style::default().fg(Color::DarkGray)), Span::styled("branch não tomado",                          Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.jump"),             Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "2"),  Style::default().fg(Color::DarkGray)), Span::styled("jal / jalr",                                  Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.system"),           Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "10"), Style::default().fg(Color::DarkGray)), Span::styled("ecall / ebreak / halt",                       Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled(format!("  {:<22}", "cpi.fp"),               Style::default().fg(Color::LightCyan)), Span::styled(format!("{:<10}", "inteiro"), Style::default().fg(Color::White)), Span::styled(format!("{:<10}", "5"),  Style::default().fg(Color::DarkGray)), Span::styled("Instruções float RV32F",                      Style::default().fg(Color::White))]),
        blank(),

        h2("Exemplo Anotado de Arquivo"),
        blank(),
        mono("  # Raven Cache Config v2"),
        mono("  levels=1"),
        blank(),
        mono("  icache.size=4096"),
        mono("  icache.line_size=32"),
        mono("  icache.associativity=2"),
        mono("  icache.replacement=Lru"),
        mono("  icache.write_policy=WriteBack"),
        mono("  icache.write_alloc=WriteAllocate"),
        mono("  icache.hit_latency=1"),
        mono("  icache.miss_penalty=50"),
        mono("  icache.assoc_penalty=1"),
        mono("  icache.transfer_width=8"),
        blank(),
        mono("  dcache.size=4096"),
        mono("  dcache.line_size=32"),
        mono("  dcache.associativity=4"),
        mono("  dcache.replacement=Lru"),
        mono("  dcache.write_policy=WriteBack"),
        mono("  dcache.write_alloc=WriteAllocate"),
        mono("  dcache.hit_latency=2"),
        mono("  dcache.miss_penalty=50"),
        mono("  dcache.assoc_penalty=1"),
        mono("  dcache.transfer_width=8"),
        blank(),
        mono("  l2.size=131072"),
        mono("  l2.line_size=64"),
        mono("  l2.associativity=8"),
        mono("  l2.replacement=Lru"),
        mono("  l2.write_policy=WriteBack"),
        mono("  l2.write_alloc=WriteAllocate"),
        mono("  l2.inclusion=NonInclusive"),
        mono("  l2.hit_latency=10"),
        mono("  l2.miss_penalty=200"),
        mono("  l2.assoc_penalty=2"),
        mono("  l2.transfer_width=16"),
        blank(),
        mono("  # --- CPI Config ---"),
        mono("  cpi.alu=1"),
        mono("  cpi.mul=3"),
        mono("  cpi.div=20"),
        mono("  cpi.load=0"),
        mono("  cpi.store=0"),
        mono("  cpi.branch_taken=3"),
        mono("  cpi.branch_not_taken=1"),
        mono("  cpi.jump=2"),
        mono("  cpi.system=10"),
        mono("  cpi.fp=5"),
        blank(),
    ]
}
