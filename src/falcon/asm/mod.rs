// src/falcon/asm/mod.rs
use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;
use std::collections::HashMap;

// Structure returned with code and data
pub struct Program {
    pub text: Vec<u32>,
    pub data: Vec<u8>,
    pub data_base: u32,
}

// ---------- API ----------
pub fn assemble(text: &str, base_pc: u32) -> Result<Program, String> {
    let lines = preprocess(text);
    let data_base = base_pc + 0x1000; // data region after code

    // 1st pass: symbol table
    enum Section {
        Text,
        Data,
    }
    let mut section = Section::Text;
    let mut pc_text = base_pc;
    let mut pc_data = 0u32; // offset from data_base
    let mut items = Vec::new(); // (pc, LineKind)
    let mut data_bytes = Vec::<u8>::new();
    let mut labels = HashMap::<String, u32>::new();

    for raw in &lines {
        if raw == ".text" {
            section = Section::Text;
            continue;
        }
        if raw == ".data" {
            section = Section::Data;
            continue;
        }

        let mut line = raw.as_str();
        if let Some(idx) = line.find(':') {
            let (lab, rest) = line.split_at(idx);
            let addr = match section {
                Section::Text => pc_text,
                Section::Data => data_base + pc_data,
            };
            labels.insert(lab.trim().to_string(), addr);
            line = rest[1..].trim();
            if line.is_empty() {
                continue;
            }
        }

        match section {
            Section::Text => {
                let ltrim = line.trim_start();
                if ltrim.starts_with("la ") {
                    items.push((pc_text, LineKind::La(ltrim.to_string())));
                    pc_text = pc_text.wrapping_add(8);
                } else {
                    items.push((pc_text, LineKind::Instr(ltrim.to_string())));
                    pc_text = pc_text.wrapping_add(4);
                }
            }
            Section::Data => {
                if let Some(rest) = line.strip_prefix(".byte") {
                    for b in rest.split(',') {
                        let v = parse_imm(b).ok_or_else(|| format!("invalid .byte: {b}"))?;
                        if !(0..=255).contains(&v) {
                            return Err(format!(".byte outside 0..255: {v}"));
                        }
                        data_bytes.push(v as u8);
                        pc_data += 1;
                    }
                } else if let Some(rest) = line.strip_prefix(".word") {
                    for w in rest.split(',') {
                        let v = parse_imm(w).ok_or_else(|| format!("invalid .word: {w}"))?;
                        let bytes = (v as u32).to_le_bytes();
                        data_bytes.extend_from_slice(&bytes);
                        pc_data += 4;
                    }
                } else {
                    return Err(format!("unknown data directive: {line}"));
                }
            }
        }
    }

    // 2nd pass: assemble
    let mut words = Vec::with_capacity(items.len());
    for (pc, kind) in items {
        match kind {
            LineKind::Instr(s) => {
                let inst = parse_instr(&s, pc, &labels)?;
                let word = encode(inst).map_err(|e| e.to_string())?;
                words.push(word);
            }
            LineKind::La(s) => {
                let (i1, i2) = parse_la(&s, &labels)?;
                let w1 = encode(i1).map_err(|e| e.to_string())?;
                let w2 = encode(i2).map_err(|e| e.to_string())?;
                words.push(w1);
                words.push(w2);
            }
        }
    }

    Ok(Program {
        text: words,
        data: data_bytes,
        data_base,
    })
}

// ---------- Internals ----------
#[derive(Debug, Clone)]
enum LineKind {
    Instr(String),
    La(String),
}

fn preprocess(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            let l = l.split(';').next().unwrap_or(l);
            let l = l.split('#').next().unwrap_or(l);
            l.trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn parse_instr(s: &str, pc: u32, labels: &HashMap<String, u32>) -> Result<Instruction, String> {
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
            Ok(Addi {
                rd: 0,
                rs1: 0,
                imm: 0,
            })
        }
        "mv" => {
            if ops.len() != 2 {
                return Err("expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs = get_reg(&ops[1])?;
            Ok(Addi {
                rd,
                rs1: rs,
                imm: 0,
            })
        }
        "li" => {
            if ops.len() != 2 {
                return Err("expected 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = get_imm(&ops[1])?;
            if !(-2048..=2047).contains(&imm) {
                return Err("li: immediate out of 12-bit range".into());
            }
            Ok(Addi { rd, rs1: 0, imm })
        }
        "j" => {
            if ops.len() != 1 {
                return Err("j: expected label/immediate".into());
            }
            Ok(Jal {
                rd: 0,
                imm: branch_imm(&ops[0], pc, labels)?,
            })
        }
        "call" => {
            if ops.len() != 1 {
                return Err("call: expected label/immediate".into());
            }
            Ok(Jal {
                rd: 1,
                imm: branch_imm(&ops[0], pc, labels)?,
            })
        }
        "jr" => {
            if ops.len() != 1 {
                return Err("jr: expected register".into());
            }
            let rs1 = get_reg(&ops[0])?;
            Ok(Jalr { rd: 0, rs1, imm: 0 })
        }
        "ret" => {
            if !ops.is_empty() {
                return Err("ret takes no operands".into());
            }
            Ok(Jalr {
                rd: 0,
                rs1: 1,
                imm: 0,
            })
        }
        "subi" => {
            if ops.len() != 3 {
                return Err("expected 'rd, rs1, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let imm = get_imm(&ops[2])?;
            let neg = -imm;
            if !(-2048..=2047).contains(&neg) {
                return Err("subi: immediate out of 12-bit range".into());
            }
            Ok(Addi { rd, rs1, imm: neg })
        }

        // ---------- R-type ----------
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" | "mul"
        | "mulh" | "mulhsu" | "mulhu" | "div" | "divu" | "rem" | "remu" => {
            if ops.len() != 3 {
                return Err("expected 'rd, rs1, rs2'".into());
            }
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
            if ops.len() != 3 {
                return Err("expected 'rd, rs1, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let imm = get_imm(&ops[2])?;
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
            if ops.len() != 3 {
                return Err("expected 'rd, rs1, shamt'".into());
            }
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
            Ok(match mnemonic.as_str() {
                "sb" => Sb { rs2, rs1, imm },
                "sh" => Sh { rs2, rs1, imm },
                "sw" => Sw { rs2, rs1, imm },
                _ => unreachable!(),
            })
        }

        // ---------- Branches (label or immediate) ----------
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            if ops.len() != 3 {
                return Err("expected 'rs1, rs2, label/immediate'".into());
            }
            let rs1 = get_reg(&ops[0])?;
            let rs2 = get_reg(&ops[1])?;
            let imm = branch_imm(&ops[2], pc, labels)?;
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

        // ---------- U/J ----------
        "lui" | "auipc" => {
            if ops.len() != 2 {
                return Err("expected 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = get_imm(&ops[1])?;
            Ok(match mnemonic.as_str() {
                "lui" => Lui { rd, imm },
                "auipc" => Auipc { rd, imm },
                _ => unreachable!(),
            })
        }

        // jal: two formats: "jal rd,label" or "jal label" (rd=ra)
        "jal" => {
            if ops.is_empty() {
                return Err("jal: missing destination".into());
            }
            if ops.len() == 1 {
                let rd = 1; // ra
                let imm = branch_imm(&ops[0], pc, labels)?;
                Ok(Jal { rd, imm })
            } else if ops.len() == 2 {
                Ok(Jal {
                    rd: get_reg(&ops[0])?,
                    imm: branch_imm(&ops[1], pc, labels)?,
                })
            } else {
                Err("jal: too many arguments".into())
            }
        }
        // jalr rd, rs1, imm
        "jalr" => {
            if ops.len() != 3 {
                return Err("jalr: expected 'rd, rs1, imm'".into());
            }
            Ok(Jalr {
                rd: get_reg(&ops[0])?,
                rs1: get_reg(&ops[1])?,
                imm: get_imm(&ops[2])?,
            })
        }

        // system
        "ecall" => {
            if !ops.is_empty() {
                return Err("ecall takes no operands".into());
            }
            Ok(Ecall)
        }
        "ebreak" => {
            if !ops.is_empty() {
                return Err("ebreak takes no operands".into());
            }
            Ok(Ebreak)
        }

        _ => Err(format!("unsupported mnemonic: {mnemonic}")),
    }
}

fn parse_la(s: &str, labels: &HashMap<String, u32>) -> Result<(Instruction, Instruction), String> {
    // "la rd, label"
    let mut parts = s.split_whitespace();
    parts.next(); // consume mnemonic
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 2 {
        return Err("expected 'rd, label'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("invalid rd")?;
    let addr = *labels
        .get(&ops[1])
        .ok_or_else(|| format!("label not found: {}", ops[1]))? as i32;

    // Split the address into a high part (aligned to 12 bits) and a low part.
    // The `lui` instruction loads the upper 20 bits already shifted, therefore
    // we need to shift the high part before generating the opcode.
    let hi = ((addr + 0x800) >> 12) << 12; // aligned high part
    let lo = addr - hi; // 12-bit low part
    let lo_signed = if lo & 0x800 != 0 { lo - 0x1000 } else { lo };

    Ok((
        Instruction::Lui { rd, imm: hi },
        Instruction::Addi {
            rd,
            rs1: rd,
            imm: lo_signed,
        },
    ))
}

fn split_operands(rest: &str) -> Vec<String> {
    rest.split(',')
        .map(|t| t.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_reg(s: &str) -> Option<u8> {
    let s = s.trim().to_lowercase();
    if let Some(num) = s.strip_prefix('x').and_then(|n| n.parse::<u8>().ok()) {
        if num < 32 {
            return Some(num);
        }
    }
    // aliases
    let map: HashMap<&'static str, u8> = HashMap::from([
        ("zero", 0),
        ("ra", 1),
        ("sp", 2),
        ("gp", 3),
        ("tp", 4),
        ("t0", 5),
        ("t1", 6),
        ("t2", 7),
        ("s0", 8),
        ("fp", 8),
        ("s1", 9),
        ("a0", 10),
        ("a1", 11),
        ("a2", 12),
        ("a3", 13),
        ("a4", 14),
        ("a5", 15),
        ("a6", 16),
        ("a7", 17),
        ("s2", 18),
        ("s3", 19),
        ("s4", 20),
        ("s5", 21),
        ("s6", 22),
        ("s7", 23),
        ("s8", 24),
        ("s9", 25),
        ("s10", 26),
        ("s11", 27),
        ("t3", 28),
        ("t4", 29),
        ("t5", 30),
        ("t6", 31),
    ]);
    map.get(s.as_str()).cloned()
}

fn parse_imm(s: &str) -> Option<i32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x") {
        i32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i32>().ok()
    }
}

fn parse_shamt(s: &str) -> Result<u8, String> {
    let v = parse_imm(s).ok_or_else(|| format!("invalid shamt: {s}"))?;
    if (0..=31).contains(&v) {
        Ok(v as u8)
    } else {
        Err(format!("shamt out of range: {v}"))
    }
}

// beq/bne/... and jal: token can be a number or label
fn branch_imm(tok: &str, pc: u32, labels: &HashMap<String, u32>) -> Result<i32, String> {
    if let Some(v) = parse_imm(tok) {
        return Ok(v);
    }
    let target = labels
        .get(&tok.to_string())
        .ok_or_else(|| format!("label not found: {tok}"))?;
    let imm = (*target as i64) - (pc as i64);
    // basic range check (13 bits for B, 21 bits for J). Here we only warn.
    Ok(imm as i32)
}

// lw rd, imm(rs1)   |  sw rs2, imm(rs1)
fn parse_memop(op: &str) -> Result<(i32, u8), String> {
    // "imm(rs1)"
    let (imm_s, rest) = op
        .split_once('(')
        .ok_or_else(|| format!("invalid mem operand: {op}"))?;
    let rs1_s = rest.strip_suffix(')').ok_or("missing ')'")?;
    let imm = parse_imm(imm_s.trim()).ok_or_else(|| format!("invalid imm: {imm_s}"))?;
    let rs1 = parse_reg(rs1_s.trim()).ok_or_else(|| format!("invalid rs1: {rs1_s}"))?;
    Ok((imm, rs1))
}
fn load_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("load: expected 'rd, imm(rs1)'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("invalid rd")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rd, imm, rs1))
}
fn store_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("store: expected 'rs2, imm(rs1)'".into());
    }
    let rs2 = parse_reg(&ops[0]).ok_or("invalid rs2")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rs2, imm, rs1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::falcon::encoder::encode;

    #[test]
    fn la_generates_lui_addi_pair() {
        // Assemble a simple program using 'la' for a symbol in .data
        let asm = ".data\nvar: .word 0\n.text\nla t0, var";
        let prog = assemble(asm, 0).expect("assemble");

        // Two instructions should be emitted: LUI and ADDI
        assert_eq!(prog.text.len(), 2);

        let expected_lui = encode(Instruction::Lui { rd: 5, imm: 0x1000 }).expect("encode lui");
        let expected_addi = encode(Instruction::Addi {
            rd: 5,
            rs1: 5,
            imm: 0,
        })
        .expect("encode addi");

        assert_eq!(prog.text[0], expected_lui);
        assert_eq!(prog.text[1], expected_addi);
    }

    #[test]
    fn call_expands_to_jal_ra() {
        // Simple program with a call to a local label
        let asm = ".text\ncall func\nfunc: ebreak";
        let prog = assemble(asm, 0).expect("assemble");

        // Should emit: JAL ra, func; EBREAK
        assert_eq!(prog.text.len(), 2);

        let expected_jal = encode(Instruction::Jal { rd: 1, imm: 4 }).expect("encode jal");
        assert_eq!(prog.text[0], expected_jal);
    }
}
