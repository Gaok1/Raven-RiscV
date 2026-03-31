use crate::falcon;
use crate::falcon::program::elf::ElfSection;
use std::collections::HashMap;
use std::fmt::Write as _;

pub fn disasm_word(w: u32) -> String {
    match falcon::decoder::decode(w) {
        Ok(ins) => pretty_instr(&ins),
        Err(_) => format!(".word 0x{w:08x}"),
    }
}

fn pretty_instr(i: &falcon::instruction::Instruction) -> String {
    use falcon::instruction::Instruction::*;
    match *i {
        Add { rd, rs1, rs2 } => format!(
            "add  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sub { rd, rs1, rs2 } => format!(
            "sub  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        And { rd, rs1, rs2 } => format!(
            "and  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Or { rd, rs1, rs2 } => format!(
            "or   {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Xor { rd, rs1, rs2 } => format!(
            "xor  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sll { rd, rs1, rs2 } => format!(
            "sll  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Srl { rd, rs1, rs2 } => format!(
            "srl  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sra { rd, rs1, rs2 } => format!(
            "sra  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Slt { rd, rs1, rs2 } => format!(
            "slt  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Sltu { rd, rs1, rs2 } => format!(
            "sltu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mul { rd, rs1, rs2 } => format!(
            "mul  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulh { rd, rs1, rs2 } => format!(
            "mulh {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulhsu { rd, rs1, rs2 } => format!(
            "mulhsu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Mulhu { rd, rs1, rs2 } => format!(
            "mulhu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Div { rd, rs1, rs2 } => format!(
            "div  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Divu { rd, rs1, rs2 } => format!(
            "divu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Rem { rd, rs1, rs2 } => format!(
            "rem  {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Remu { rd, rs1, rs2 } => format!(
            "remu {}, {}, {}",
            reg_name(rd),
            reg_name(rs1),
            reg_name(rs2)
        ),
        Addi { rd, rs1, imm } => format!("addi {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Andi { rd, rs1, imm } => format!("andi {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Ori { rd, rs1, imm } => format!("ori  {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Xori { rd, rs1, imm } => format!("xori {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Slti { rd, rs1, imm } => format!("slti {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Sltiu { rd, rs1, imm } => format!("sltiu {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Slli { rd, rs1, shamt } => format!("slli {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Srli { rd, rs1, shamt } => format!("srli {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Srai { rd, rs1, shamt } => format!("srai {}, {}, {shamt}", reg_name(rd), reg_name(rs1)),
        Lb { rd, rs1, imm } => format!("lb   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lh { rd, rs1, imm } => format!("lh   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lw { rd, rs1, imm } => format!("lw   {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lbu { rd, rs1, imm } => format!("lbu  {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Lhu { rd, rs1, imm } => format!("lhu  {}, {imm}({})", reg_name(rd), reg_name(rs1)),
        Sb { rs2, rs1, imm } => format!("sb   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Sh { rs2, rs1, imm } => format!("sh   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Sw { rs2, rs1, imm } => format!("sw   {}, {imm}({})", reg_name(rs2), reg_name(rs1)),
        Beq { rs1, rs2, imm } => format!("beq  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bne { rs1, rs2, imm } => format!("bne  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Blt { rs1, rs2, imm } => format!("blt  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bge { rs1, rs2, imm } => format!("bge  {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bltu { rs1, rs2, imm } => format!("bltu {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Bgeu { rs1, rs2, imm } => format!("bgeu {}, {}, {imm}", reg_name(rs1), reg_name(rs2)),
        Lui { rd, imm } => format!("lui  {}, 0x{:x}", reg_name(rd), (imm as u32) >> 12),
        Auipc { rd, imm } => format!("auipc {}, 0x{:x}", reg_name(rd), (imm as u32) >> 12),
        Jal { rd, imm } => format!("jal  {}, {imm}", reg_name(rd)),
        Jalr { rd, rs1, imm } => format!("jalr {}, {}, {imm}", reg_name(rd), reg_name(rs1)),
        Ecall => "ecall".to_string(),
        Ebreak => "ebreak".to_string(),
        Halt => "halt".to_string(),
        Fence => "fence".to_string(),
        // RV32F
        Flw { rd, rs1, imm } => format!("flw   {}, {imm}({})", freg_name(rd), reg_name(rs1)),
        Fsw { rs2, rs1, imm } => format!("fsw   {}, {imm}({})", freg_name(rs2), reg_name(rs1)),
        FaddS { rd, rs1, rs2 } => fmt3f("fadd.s", rd, rs1, rs2),
        FsubS { rd, rs1, rs2 } => fmt3f("fsub.s", rd, rs1, rs2),
        FmulS { rd, rs1, rs2 } => fmt3f("fmul.s", rd, rs1, rs2),
        FdivS { rd, rs1, rs2 } => fmt3f("fdiv.s", rd, rs1, rs2),
        FsqrtS { rd, rs1 } => format!("fsqrt.s {}, {}", freg_name(rd), freg_name(rs1)),
        FminS { rd, rs1, rs2 } => fmt3f("fmin.s", rd, rs1, rs2),
        FmaxS { rd, rs1, rs2 } => fmt3f("fmax.s", rd, rs1, rs2),
        FsgnjS { rd, rs1, rs2 } => fmt3f("fsgnj.s", rd, rs1, rs2),
        FsgnjnS { rd, rs1, rs2 } => fmt3f("fsgnjn.s", rd, rs1, rs2),
        FsgnjxS { rd, rs1, rs2 } => fmt3f("fsgnjx.s", rd, rs1, rs2),
        FeqS { rd, rs1, rs2 } => format!(
            "feq.s  {}, {}, {}",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FltS { rd, rs1, rs2 } => format!(
            "flt.s  {}, {}, {}",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FleS { rd, rs1, rs2 } => format!(
            "fle.s  {}, {}, {}",
            reg_name(rd),
            freg_name(rs1),
            freg_name(rs2)
        ),
        FcvtWS { rd, rs1, .. } => format!("fcvt.w.s  {}, {}", reg_name(rd), freg_name(rs1)),
        FcvtWuS { rd, rs1, .. } => format!("fcvt.wu.s {}, {}", reg_name(rd), freg_name(rs1)),
        FcvtSW { rd, rs1 } => format!("fcvt.s.w  {}, {}", freg_name(rd), reg_name(rs1)),
        FcvtSWu { rd, rs1 } => format!("fcvt.s.wu {}, {}", freg_name(rd), reg_name(rs1)),
        FmvXW { rd, rs1 } => format!("fmv.x.w {}, {}", reg_name(rd), freg_name(rs1)),
        FmvWX { rd, rs1 } => format!("fmv.w.x {}, {}", freg_name(rd), reg_name(rs1)),
        FclassS { rd, rs1 } => format!("fclass.s {}, {}", reg_name(rd), freg_name(rs1)),
        FmaddS { rd, rs1, rs2, rs3 } => fmt4f("fmadd.s", rd, rs1, rs2, rs3),
        FmsubS { rd, rs1, rs2, rs3 } => fmt4f("fmsub.s", rd, rs1, rs2, rs3),
        FnmsubS { rd, rs1, rs2, rs3 } => fmt4f("fnmsub.s", rd, rs1, rs2, rs3),
        FnmaddS { rd, rs1, rs2, rs3 } => fmt4f("fnmadd.s", rd, rs1, rs2, rs3),

        // RV32A
        LrW { rd, rs1 } => format!("{:<9} {}, ({})", "lr.w", reg_name(rd), reg_name(rs1)),
        ScW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "sc.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoswapW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amoswap.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoaddW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amoadd.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoxorW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amoxor.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoandW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amoand.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmoorW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amoor.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmomaxW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amomax.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmominW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amomin.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmomaxuW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amomaxu.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
        AmominuW { rd, rs1, rs2 } => format!(
            "{:<9} {}, {}, ({})",
            "amominu.w",
            reg_name(rd),
            reg_name(rs2),
            reg_name(rs1)
        ),
    }
}

fn fmt3f(m: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!(
        "{m:<9} {}, {}, {}",
        freg_name(rd),
        freg_name(rs1),
        freg_name(rs2)
    )
}
fn fmt4f(m: &str, rd: u8, rs1: u8, rs2: u8, rs3: u8) -> String {
    format!(
        "{m:<9} {}, {}, {}, {}",
        freg_name(rd),
        freg_name(rs1),
        freg_name(rs2),
        freg_name(rs3)
    )
}
fn freg_name(i: u8) -> &'static str {
    match i {
        0 => "ft0",
        1 => "ft1",
        2 => "ft2",
        3 => "ft3",
        4 => "ft4",
        5 => "ft5",
        6 => "ft6",
        7 => "ft7",
        8 => "fs0",
        9 => "fs1",
        10 => "fa0",
        11 => "fa1",
        12 => "fa2",
        13 => "fa3",
        14 => "fa4",
        15 => "fa5",
        16 => "fa6",
        17 => "fa7",
        18 => "fs2",
        19 => "fs3",
        20 => "fs4",
        21 => "fs5",
        22 => "fs6",
        23 => "fs7",
        24 => "fs8",
        25 => "fs9",
        26 => "fs10",
        27 => "fs11",
        28 => "ft8",
        29 => "ft9",
        30 => "ft10",
        31 => "ft11",
        _ => "f?",
    }
}
fn reg_name(i: u8) -> &'static str {
    match i {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "",
    }
}

// ── ELF → editable assembly source ───────────────────────────────────────────

/// Generate an assembly source text from an ELF binary that the Raven assembler
/// can re-assemble.  Branch/jal targets are replaced with label names where known;
/// synthetic labels (`_L_XXXXXXXX:`) are emitted for any unlabeled targets.
pub fn elf_to_asm_source(
    text_words: &[u32],
    text_base: u32,
    symbols: &HashMap<u32, Vec<String>>,
    sections: &[ElfSection],
) -> String {
    use falcon::instruction::Instruction::*;

    let n = text_words.len();
    let text_end = text_base.wrapping_add((n as u32) * 4);

    // ── Pass 1: find branch/jal targets that need synthetic labels ──────
    let mut need_label: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (i, &word) in text_words.iter().enumerate() {
        let pc = text_base.wrapping_add((i as u32) * 4);
        if let Ok(ins) = falcon::decoder::decode(word) {
            let target_imm: Option<i32> = match ins {
                Beq { imm, .. }
                | Bne { imm, .. }
                | Blt { imm, .. }
                | Bge { imm, .. }
                | Bltu { imm, .. }
                | Bgeu { imm, .. }
                | Jal { imm, .. } => Some(imm),
                _ => None,
            };
            if let Some(imm) = target_imm {
                let t = pc.wrapping_add(imm as u32);
                if t >= text_base && t < text_end && !symbols.contains_key(&t) {
                    need_label.insert(t);
                }
            }
        }
    }

    // ── Build combined label map: addr → first name to use in source ────
    let mut label_at: HashMap<u32, String> = HashMap::new();
    for (&addr, names) in symbols {
        if let Some(name) = names.first() {
            label_at.insert(addr, name.clone());
        }
    }
    for &addr in &need_label {
        label_at
            .entry(addr)
            .or_insert_with(|| format!("_L_{addr:08x}"));
    }

    // ── Pass 2: emit assembly text ───────────────────────────────────────
    let mut out = String::with_capacity(n * 40);
    let _ = writeln!(out, "# ELF disassembly — auto-generated by Raven");
    let _ = writeln!(out, "# text base: 0x{text_base:08x}  ({n} instructions)");
    let _ = writeln!(out);

    for (i, &word) in text_words.iter().enumerate() {
        let pc = text_base.wrapping_add((i as u32) * 4);

        // Blank line + label(s) before each labeled instruction
        if symbols.contains_key(&pc) || need_label.contains(&pc) {
            if i > 0 {
                let _ = writeln!(out);
            }
            if let Some(names) = symbols.get(&pc) {
                for name in names {
                    let _ = writeln!(out, "{name}:");
                }
            } else if let Some(name) = label_at.get(&pc) {
                let _ = writeln!(out, "{name}:");
            }
        }

        let line = disasm_with_labels(word, pc, &label_at);
        let _ = writeln!(out, "    {line}");
    }

    // ── Data sections ────────────────────────────────────────────────────
    // Group sections: .bss-like (no file bytes) need ".bss" context;
    // others need ".data" context.  Track which section we're currently in.
    let mut cur_section: Option<&str> = None;
    for sec in sections {
        let is_bss = sec.bytes.is_empty();
        let needed = if is_bss { ".bss" } else { ".data" };
        if cur_section != Some(needed) {
            let _ = writeln!(out);
            let _ = writeln!(out, "{needed}");
            cur_section = Some(needed);
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "# --- {} (0x{:08x}, {} bytes) ---",
            sec.name, sec.addr, sec.size
        );
        if is_bss {
            // Symbol labels at the bss base, then .skip for size
            if let Some(names) = symbols.get(&sec.addr) {
                for name in names {
                    let _ = writeln!(out, "{name}:");
                }
            }
            let _ = writeln!(out, "    .skip {}", sec.size);
        } else {
            emit_data_words(&mut out, &sec.bytes, sec.addr, symbols);
        }
    }

    out
}

/// Disassemble one word, replacing branch/jal targets with label names.
fn disasm_with_labels(word: u32, pc: u32, labels: &HashMap<u32, String>) -> String {
    use falcon::instruction::Instruction::*;

    let lbl = |imm: i32| -> String {
        let target = pc.wrapping_add(imm as u32);
        labels
            .get(&target)
            .cloned()
            .unwrap_or_else(|| imm.to_string())
    };

    match falcon::decoder::decode(word) {
        Ok(ins) => match ins {
            Beq { rs1, rs2, imm } => {
                format!("beq  {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Bne { rs1, rs2, imm } => {
                format!("bne  {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Blt { rs1, rs2, imm } => {
                format!("blt  {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Bge { rs1, rs2, imm } => {
                format!("bge  {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Bltu { rs1, rs2, imm } => {
                format!("bltu {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Bgeu { rs1, rs2, imm } => {
                format!("bgeu {}, {}, {}", reg_name(rs1), reg_name(rs2), lbl(imm))
            }
            Jal { rd, imm } => format!("jal  {}, {}", reg_name(rd), lbl(imm)),
            _ => disasm_word(word),
        },
        Err(_) => format!(".word 0x{word:08x}"),
    }
}

/// Emit data section bytes as `.word` / `.byte` directives, inserting symbol
/// labels at the appropriate addresses.
fn emit_data_words(out: &mut String, bytes: &[u8], base: u32, symbols: &HashMap<u32, Vec<String>>) {
    let mut i = 0usize;
    while i < bytes.len() {
        let addr = base.wrapping_add(i as u32);
        // Symbol label(s) at this address
        if let Some(names) = symbols.get(&addr) {
            for name in names {
                let _ = writeln!(out, "{name}:");
            }
        }
        // Emit 4 bytes as .word when 4-byte aligned and enough remain
        if i + 4 <= bytes.len() && addr % 4 == 0 {
            let w = u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
            let _ = writeln!(out, "    .word 0x{w:08x}");
            i += 4;
        } else {
            let _ = writeln!(out, "    .byte 0x{:02x}", bytes[i]);
            i += 1;
        }
    }
}
