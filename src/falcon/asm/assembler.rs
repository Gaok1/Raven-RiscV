use std::collections::HashMap;

use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;

use super::errors::AsmError;
use super::program::Program;
use super::pseudo::{parse_la, parse_pop, parse_print, parse_print_str, parse_print_strln, parse_push, parse_read, parse_read_byte, parse_read_half, parse_read_word};
use super::utils::*;

// ---------- API ----------
pub fn assemble(text: &str, base_pc: u32) -> Result<Program, AsmError> {
    let lines = preprocess(text);
    let data_base = base_pc + 0x1000; // data region after code

    // 1st pass: symbol table
    enum Section {
        Text,
        Data,
        Bss,
    }
    let mut section = Section::Text;
    let mut pc_text = base_pc;
    let mut pc_data = 0u32; // offset from data_base
    let mut pc_bss = 0u32;  // offset/size within .bss
    let mut items: Vec<(u32, LineKind, usize)> = Vec::new(); // (pc, LineKind, line number)
    let mut data_bytes = Vec::<u8>::new();
    // Collect label defs by section and offset; resolve absolute addresses after first pass
    let mut label_defs = HashMap::<String, (Section, u32)>::new();

    // Iterate over lines and collect labels and instructions
    for (line_no, raw) in &lines {
        if raw == ".text" {
            section = Section::Text;
            continue;
        }
        if raw == ".data" {
            section = Section::Data;
            continue;
        }
        if raw == ".bss" {
            section = Section::Bss;
            continue;
        }
        if let Some(rest) = raw.strip_prefix(".section") {
            let name = rest.trim();
            match name {
                ".text" | "text" => section = Section::Text,
                ".data" | "data" => section = Section::Data,
                ".bss" | "bss" => section = Section::Bss,
                "" => {
                    return Err(AsmError {
                        line: *line_no,
                        msg: "missing section name".into(),
                    })
                }
                _ => {
                    return Err(AsmError {
                        line: *line_no,
                        msg: format!("unknown section: {name}"),
                    })
                }
            }
            continue;
        }

        let mut line = raw.as_str();
        if let Some(idx) = line.find(':') {
            let (lab, rest) = line.split_at(idx);
            let (sec, off) = match section {
                Section::Text => (Section::Text, pc_text),
                Section::Data => (Section::Data, pc_data),
                Section::Bss => (Section::Bss, pc_bss),
            };
            label_defs.insert(lab.trim().to_string(), (sec, off));
            line = rest[1..].trim();
            if line.is_empty() {
                // instruction label only
                continue;
            }
        }

        match section {
            Section::Text => {
                let ltrim = line.trim_start();
                if ltrim.starts_with("la ") {
                    items.push((pc_text, LineKind::La(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(8);
                } else if ltrim.starts_with("push ") {
                    items.push((pc_text, LineKind::Push(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(8);
                } else if ltrim.starts_with("pop ") {
                    items.push((pc_text, LineKind::Pop(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(8);
                } else if ltrim == "print" || ltrim.starts_with("print ") {
                    items.push((pc_text, LineKind::Print(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(12);
                } else if ltrim == "printStr" || ltrim.starts_with("printStr ")
                    || ltrim == "printString" || ltrim.starts_with("printString ")
                {
                    items.push((pc_text, LineKind::PrintStr(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "printStrLn" || ltrim.starts_with("printStrLn ") {
                    items.push((pc_text, LineKind::PrintStrLn(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "read" || ltrim.starts_with("read ") {
                    items.push((pc_text, LineKind::Read(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "readByte" || ltrim.starts_with("readByte ") {
                    items.push((pc_text, LineKind::ReadByte(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "readHalf" || ltrim.starts_with("readHalf ") {
                    items.push((pc_text, LineKind::ReadHalf(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "readWord" || ltrim.starts_with("readWord ") {
                    items.push((pc_text, LineKind::ReadWord(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else {
                    items.push((pc_text, LineKind::Instr(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(4);
                }
            }
            Section::Data => {
                if let Some(rest) = line.strip_prefix(".byte") {
                    for b in rest.split(',') {
                        let v = parse_imm(b).ok_or_else(|| AsmError {
                            line: *line_no,
                            msg: format!("invalid .byte: {b}"),
                        })?;
                        if !(0..=255).contains(&v) {
                            return Err(AsmError {
                                line: *line_no,
                                msg: format!(".byte outside 0..255: {v}"),
                            });
                        }
                        data_bytes.push(v as u8);
                        pc_data += 1;
                    }
                } else if let Some(rest) = line.strip_prefix(".half") {
                    for h in rest.split(',') {
                        let v = parse_imm(h).ok_or_else(|| AsmError {
                            line: *line_no,
                            msg: format!("invalid .half: {h}"),
                        })?;
                        if !(0..=65535).contains(&v) {
                            return Err(AsmError {
                                line: *line_no,
                                msg: format!(".half outside 0..65535: {v}"),
                            });
                        }
                        let bytes = (v as u16).to_le_bytes();
                        data_bytes.extend_from_slice(&bytes);
                        pc_data += 2;
                    }
                } else if let Some(rest) = line.strip_prefix(".word") {
                    for w in rest.split(',') {
                        let v = parse_imm(w).ok_or_else(|| AsmError {
                            line: *line_no,
                            msg: format!("invalid .word: {w}"),
                        })?;
                        let bytes = (v as u32).to_le_bytes();
                        data_bytes.extend_from_slice(&bytes);
                        pc_data += 4;
                    }
                } else if let Some(rest) = line.strip_prefix(".dword") {
                    for d in rest.split(',') {
                        let v = parse_imm64(d).ok_or_else(|| AsmError {
                            line: *line_no,
                            msg: format!("invalid .dword: {d}"),
                        })?;
                        let bytes = (v as i64 as u64).to_le_bytes();
                        data_bytes.extend_from_slice(&bytes);
                        pc_data += 8;
                    }
                } else if let Some(rest) = line.strip_prefix(".ascii") {
                    let s = parse_str_lit(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid .ascii: {rest}"),
                    })?;
                    data_bytes.extend_from_slice(s.as_bytes());
                    pc_data += s.len() as u32;
                } else if let Some(rest) = line
                    .strip_prefix(".asciz")
                    .or_else(|| line.strip_prefix(".string"))
                {
                    let s = parse_str_lit(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid string: {rest}"),
                    })?;
                    data_bytes.extend_from_slice(s.as_bytes());
                    data_bytes.push(0);
                    pc_data += (s.len() + 1) as u32;
                } else if let Some(rest) = line
                    .strip_prefix(".space")
                    .or_else(|| line.strip_prefix(".zero"))
                {
                    let n = parse_imm(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid size: {rest}"),
                    })?;
                    if n < 0 {
                        return Err(AsmError {
                            line: *line_no,
                            msg: format!("size must be positive: {n}"),
                        });
                    }
                    let n = n as usize;
                    data_bytes.extend(std::iter::repeat(0).take(n));
                    pc_data += n as u32;
                } else {
                    return Err(AsmError {
                        line: *line_no,
                        msg: format!("unknown data directive: {line}"),
                    });
                }
            }
            Section::Bss => {
                // .bss accepts size/alignment directives but no explicit data
                if let Some(rest) = line
                    .strip_prefix(".space")
                    .or_else(|| line.strip_prefix(".zero"))
                    .or_else(|| line.strip_prefix(".skip"))
                {
                    let n = parse_imm(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid size: {rest}"),
                    })?;
                    if n < 0 {
                        return Err(AsmError { line: *line_no, msg: format!("size must be positive: {n}") });
                    }
                    pc_bss = pc_bss.wrapping_add(n as u32);
                } else if let Some(rest) = line.strip_prefix(".align") {
                    let n = parse_imm(rest).ok_or_else(|| AsmError { line: *line_no, msg: format!("invalid align: {rest}") })?;
                    if n <= 0 { return Err(AsmError { line: *line_no, msg: format!("alignment must be positive: {n}") }); }
                    let n = n as u32;
                    let mask = n - 1;
                    let aligned = if (pc_bss & mask) == 0 { pc_bss } else { (pc_bss + mask) & !mask };
                    pc_bss = aligned;
                } else if line.starts_with(".byte")
                    || line.starts_with(".half")
                    || line.starts_with(".word")
                    || line.starts_with(".dword")
                    || line.starts_with(".ascii")
                    || line.starts_with(".asciz")
                    || line.starts_with(".string")
                {
                    return Err(AsmError { line: *line_no, msg: ".bss does not store explicit data; use .space/.zero/.skip/.align".into() });
                } else {
                    return Err(AsmError { line: *line_no, msg: format!("unknown .bss directive: {line}") });
                }
            }
        }
    }

    // Build final labels map with absolute addresses
    let mut labels = HashMap::<String, u32>::new();
    let data_size = pc_data;
    for (name, (sec, off)) in label_defs.into_iter() {
        let addr = match sec {
            Section::Text => off, // pc_text was absolute when captured
            Section::Data => data_base + off,
            Section::Bss => data_base + data_size + off,
        };
        labels.insert(name, addr);
    }

    // 2nd pass: assemble
    let mut words = Vec::with_capacity(items.len());
    for (pc, kind, line_no) in items {
        match kind {
            LineKind::Instr(s) => {
                let inst = parse_instr(&s, pc, &labels).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                let word = encode(inst).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                words.push(word);
            }
            LineKind::La(s) => {
                let (i1, i2) = parse_la(&s, &labels).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                let w1 = encode(i1).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                let w2 = encode(i2).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                words.push(w1);
                words.push(w2);
            }
            LineKind::Push(s) => {
                let (i1, i2) = parse_push(&s).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                let w1 = encode(i1).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                let w2 = encode(i2).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                words.push(w1);
                words.push(w2);
            }
            LineKind::Pop(s) => {
                let (i1, i2) = parse_pop(&s).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                let w1 = encode(i1).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                let w2 = encode(i2).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                words.push(w1);
                words.push(w2);
            }
            LineKind::Print(s) => {
                let insts = parse_print(&s).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                for inst in insts {
                    let w = encode(inst).map_err(|e| AsmError {
                        line: line_no,
                        msg: e.to_string(),
                    })?;
                    words.push(w);
                }
            }
            LineKind::PrintStr(s) => {
                // accept both 'printStr' and legacy 'printString' mnemonics
                let s_norm = if s.starts_with("printString") {
                    s.replacen("printString", "printStr", 1)
                } else { s.clone() };
                let insts = parse_print_str(&s_norm, &labels).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                for inst in insts {
                    let w = encode(inst).map_err(|e| AsmError {
                        line: line_no,
                        msg: e.to_string(),
                    })?;
                    words.push(w);
                }
            }
            LineKind::PrintStrLn(s) => {
                let insts = parse_print_strln(&s, &labels).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                for inst in insts {
                    let w = encode(inst).map_err(|e| AsmError {
                        line: line_no,
                        msg: e.to_string(),
                    })?;
                    words.push(w);
                }
            }
            LineKind::Read(s) => {
                let insts = parse_read(&s, &labels).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                for inst in insts {
                    let w = encode(inst).map_err(|e| AsmError {
                        line: line_no,
                        msg: e.to_string(),
                    })?;
                    words.push(w);
                }
            }
            LineKind::ReadByte(s) => {
                let insts = parse_read_byte(&s, &labels).map_err(|e| AsmError { line: line_no, msg: e })?;
                for inst in insts { let w = encode(inst).map_err(|e| AsmError { line: line_no, msg: e.to_string() })?; words.push(w); }
            }
            LineKind::ReadHalf(s) => {
                let insts = parse_read_half(&s, &labels).map_err(|e| AsmError { line: line_no, msg: e })?;
                for inst in insts { let w = encode(inst).map_err(|e| AsmError { line: line_no, msg: e.to_string() })?; words.push(w); }
            }
            LineKind::ReadWord(s) => {
                let insts = parse_read_word(&s, &labels).map_err(|e| AsmError { line: line_no, msg: e })?;
                for inst in insts { let w = encode(inst).map_err(|e| AsmError { line: line_no, msg: e.to_string() })?; words.push(w); }
            }
        }
    }

    Ok(Program {
        text: words,
        data: data_bytes,
        data_base,
        bss_size: pc_bss,
    })
}

// ---------- Internals ----------
#[derive(Debug, Clone)]
enum LineKind {
    Instr(String),
    La(String),
    Push(String),
    Pop(String),
    Print(String),
    PrintStr(String),
    PrintStrLn(String),
    Read(String),
    ReadByte(String),
    ReadHalf(String),
    ReadWord(String),
}

fn parse_instr(
    s: &str,
    pc: u32,
    labels: &HashMap<String, u32>,
) -> Result<Instruction, String> {
    // ex: "addi x1, x0, 10"
    let mut parts = s.split_whitespace();
    let mnemonic = parts.next().ok_or("empty line")?.to_lowercase();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);

    use Instruction::*;

    let get_reg = |t: &str| parse_reg(t).ok_or_else(|| format!("invalid register: {t}"));
    let get_imm = |t: &str| parse_imm(t).ok_or_else(|| format!("invalid immediate: {t}"));

    match mnemonic.as_str() {
        // ---------- Pseudo-instructions ----------
        "nop" => {
            if !ops.is_empty() {
                return Err("nop takes no operands".into());
            }
            Ok(Addi { rd: 0, rs1: 0, imm: 0 })
        }
        "mv" => {
            if ops.len() != 2 {
                return Err("expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs = get_reg(&ops[1])?;
            Ok(Addi { rd, rs1: rs, imm: 0 })
        }
        "li" => {
            if ops.len() != 2 {
                return Err("expected 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = check_signed(get_imm(&ops[1])?, 12, "li")?;
            Ok(Addi { rd, rs1: 0, imm })
        }
        "j" => {
            if ops.len() != 1 {
                return Err("j: expected label/immediate".into());
            }
            Ok(Jal { rd: 0, imm: branch_imm(&ops[0], pc, labels, 21, "j")? })
        }
        "call" => {
            if ops.len() != 1 {
                return Err("call: expected label/immediate".into());
            }
            Ok(Jal { rd: 1, imm: branch_imm(&ops[0], pc, labels, 21, "call")? })
        }
        "jr" => {
            if ops.len() != 1 { return Err("jr: expected register".into()); }
            let rs1 = get_reg(&ops[0])?;
            Ok(Jalr { rd: 0, rs1, imm: 0 })
        }
        "ret" => {
            if !ops.is_empty() { return Err("ret takes no operands".into()); }
            Ok(Jalr { rd: 0, rs1: 1, imm: 0 })
        }
        "subi" => {
            if ops.len() != 3 { return Err("expected 'rd, rs1, imm'".into()); }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let neg = -get_imm(&ops[2])?;
            let neg = check_signed(neg, 12, "subi")?;
            Ok(Addi { rd, rs1, imm: neg })
        }

        // ---------- R-type ----------
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" | "mul"
        | "mulh" | "mulhsu" | "mulhu" | "div" | "divu" | "rem" | "remu" => {
            if ops.len() != 3 { return Err("expected 'rd, rs1, rs2'".into()); }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let rs2 = get_reg(&ops[2])?;
            Ok(match mnemonic.as_str() {
                "add" => Add { rd, rs1, rs2 },
                "sub" => Sub { rd, rs1, rs2 },
                "and" => And { rd, rs1, rs2 },
                "or" => Or { rd, rs1, rs2 },
                "xor" => Xor { rd, rs1, rs2 },
                "sll" => Sll { rd, rs1, rs2 },
                "srl" => Srl { rd, rs1, rs2 },
                "sra" => Sra { rd, rs1, rs2 },
                "slt" => Slt { rd, rs1, rs2 },
                "sltu" => Sltu { rd, rs1, rs2 },
                "mul" => Mul { rd, rs1, rs2 },
                "mulh" => Mulh { rd, rs1, rs2 },
                "mulhsu" => Mulhsu { rd, rs1, rs2 },
                "mulhu" => Mulhu { rd, rs1, rs2 },
                "div" => Div { rd, rs1, rs2 },
                "divu" => Divu { rd, rs1, rs2 },
                "rem" => Rem { rd, rs1, rs2 },
                "remu" => Remu { rd, rs1, rs2 },
                _ => unreachable!(),
            })
        }

        // ---------- I-type ----------
        "addi" | "andi" | "ori" | "xori" | "slti" | "sltiu" => {
            if ops.len() != 3 { return Err("expected 'rd, rs1, imm'".into()); }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let imm = check_signed(get_imm(&ops[2])?, 12, mnemonic.as_str())?;
            Ok(match mnemonic.as_str() {
                "addi" => Addi { rd, rs1, imm },
                "andi" => Andi { rd, rs1, imm },
                "ori" => Ori { rd, rs1, imm },
                "xori" => Xori { rd, rs1, imm },
                "slti" => Slti { rd, rs1, imm },
                "sltiu" => Sltiu { rd, rs1, imm },
                _ => unreachable!(),
            })
        }
        "slli" | "srli" | "srai" => {
            if ops.len() != 3 { return Err("expected 'rd, rs1, shamt'".into()); }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let shamt = parse_shamt(&ops[2])?;
            Ok(match mnemonic.as_str() {
                "slli" => Slli { rd, rs1, shamt },
                "srli" => Srli { rd, rs1, shamt },
                "srai" => Srai { rd, rs1, shamt },
                _ => unreachable!(),
            })
        }

        // ---------- Loads (imm(rs1)) ----------
        "lb" | "lh" | "lw" | "lbu" | "lhu" => {
            let (rd, imm, rs1) = load_like(&ops)?;
            let imm = check_signed(imm, 12, mnemonic.as_str())?;
            Ok(match mnemonic.as_str() {
                "lb" => Lb { rd, rs1, imm },
                "lh" => Lh { rd, rs1, imm },
                "lw" => Lw { rd, rs1, imm },
                "lbu" => Lbu { rd, rs1, imm },
                "lhu" => Lhu { rd, rs1, imm },
                _ => unreachable!(),
            })
        }

        // ---------- Stores (rs2, imm(rs1)) ----------
        "sb" | "sh" | "sw" => {
            let (rs2, imm, rs1) = store_like(&ops)?;
            let imm = check_signed(imm, 12, mnemonic.as_str())?;
            Ok(match mnemonic.as_str() {
                "sb" => Sb { rs2, rs1, imm },
                "sh" => Sh { rs2, rs1, imm },
                "sw" => Sw { rs2, rs1, imm },
                _ => unreachable!(),
            })
        }

        // ---------- Branches (rs1, rs2, label/imm) ----------
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            if ops.len() != 3 { return Err("expected 'rs1, rs2, label/imm'".into()); }
            let rs1 = get_reg(&ops[0])?;
            let rs2 = get_reg(&ops[1])?;
            let imm = branch_imm(&ops[2], pc, labels, 13, mnemonic.as_str())?;
            Ok(match mnemonic.as_str() {
                "beq" => Beq { rs1, rs2, imm },
                "bne" => Bne { rs1, rs2, imm },
                "blt" => Blt { rs1, rs2, imm },
                "bge" => Bge { rs1, rs2, imm },
                "bltu" => Bltu { rs1, rs2, imm },
                "bgeu" => Bgeu { rs1, rs2, imm },
                _ => unreachable!(),
            })
        }

        // ---------- U-type ----------
        "lui" => {
            if ops.len() != 2 { return Err("lui: expected 'rd, imm'".into()); }
            let rd = get_reg(&ops[0])?;
            let imm = check_u_imm(get_imm(&ops[1])?, "lui")?;
            Ok(Lui { rd, imm })
        }
        "auipc" => {
            if ops.len() != 2 { return Err("auipc: expected 'rd, imm'".into()); }
            let rd = get_reg(&ops[0])?;
            let imm = check_u_imm(get_imm(&ops[1])?, "auipc")?;
            Ok(Auipc { rd, imm })
        }

        // jal: two formats: "jal rd,label" or "jal label" (rd=ra)
        "jal" => {
            if ops.is_empty() { return Err("jal: missing destination".into()); }
            if ops.len() == 1 {
                let rd = 1; // ra
                let imm = branch_imm(&ops[0], pc, labels, 21, "jal")?;
                Ok(Jal { rd, imm })
            } else if ops.len() == 2 {
                Ok(Jal { rd: get_reg(&ops[0])?, imm: branch_imm(&ops[1], pc, labels, 21, "jal")? })
            } else {
                Err("jal: too many arguments".into())
            }
        }
        // jalr rd, rs1, imm
        "jalr" => {
            if ops.len() != 3 { return Err("jalr: expected 'rd, rs1, imm'".into()); }
            Ok(Jalr { rd: get_reg(&ops[0])?, rs1: get_reg(&ops[1])?, imm: check_signed(get_imm(&ops[2])?, 12, "jalr")? })
        }

        // system
        "ecall" => {
            if !ops.is_empty() { return Err("ecall takes no operands".into()); }
            Ok(Ecall)
        }
        "halt" => {
            if !ops.is_empty() { return Err("halt takes no operands".into()); }
            Ok(Halt)
        }

        _ => Err(format!("unsupported mnemonic: {mnemonic}")),
    }
}

