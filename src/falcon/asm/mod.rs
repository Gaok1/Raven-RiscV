// src/falcon/asm/mod.rs
use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;
use std::collections::HashMap;

// ---------- API ----------
pub fn assemble(text: &str, base_pc: u32) -> Result<Vec<u32>, String> {
    let lines = preprocess(text);
    // 1ª passada: tabela de símbolos
    let mut pc = base_pc;
    let mut items = Vec::new(); // (pc, LineKind)
    let mut labels = HashMap::<String, u32>::new();

    for raw in &lines {
        if raw.ends_with(':') {
            let name = raw.trim_end_matches(':').to_string();
            labels.insert(name, pc);
        } else if raw.is_empty() {
            // pass
        } else {
            items.push((pc, LineKind::Instr(raw.clone())));
            pc = pc.wrapping_add(4);
        }
    }

    // 2ª passada: monta
    let mut words = Vec::with_capacity(items.len());
    for (pc, kind) in items {
        match kind {
            LineKind::Instr(s) => {
                let inst = parse_instr(&s, pc, &labels)?;
                let word = encode(inst).map_err(|e| e.to_string())?;
                words.push(word);
            }
        }
    }
    Ok(words)
}

// ---------- Internals ----------
#[derive(Debug, Clone)]
enum LineKind {
    Instr(String),
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
    let mnemonic = parts.next().ok_or("linha vazia")?.to_lowercase();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);

    use Instruction::*;

    let get_reg = |t: &str| parse_reg(t).ok_or_else(|| format!("registrador inválido: {t}"));
    let get_imm = |t: &str| parse_imm(t).ok_or_else(|| format!("imediato inválido: {t}"));

    match mnemonic.as_str() {
        // ---------- R-type ----------
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" => {
            if ops.len() != 3 {
                return Err("esperado 'rd, rs1, rs2'".into());
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
                _ => unreachable!(),
            })
        }

        // ---------- I-type ----------
        "addi" | "andi" | "ori" | "xori" => {
            if ops.len() != 3 {
                return Err("esperado 'rd, rs1, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            let imm = get_imm(&ops[2])?;
            Ok(match mnemonic.as_str() {
                "addi" => Addi { rd, rs1, imm },
                "andi" => Andi { rd, rs1, imm },
                "ori" => Ori { rd, rs1, imm },
                "xori" => Xori { rd, rs1, imm },
                _ => unreachable!(),
            })
        }
        "slli" | "srli" | "srai" => {
            if ops.len() != 3 {
                return Err("esperado 'rd, rs1, shamt'".into());
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

        // ---------- Branches (label ou imediato) ----------
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            if ops.len() != 3 {
                return Err("esperado 'rs1, rs2, label/imediato'".into());
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
                return Err("esperado 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = get_imm(&ops[1])?;
            Ok(match mnemonic.as_str() {
                "lui" => Lui { rd, imm },
                "auipc" => Auipc { rd, imm },
                _ => unreachable!(),
            })
        }

        // jal: dois formatos: "jal rd,label" ou "jal label" (rd=ra)
        "jal" => {
            if ops.is_empty() {
                return Err("jal: faltou destino".into());
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
                Err("jal: argumentos demais".into())
            }
        }
        // jalr rd, rs1, imm
        "jalr" => {
            if ops.len() != 3 {
                return Err("jalr: esperado 'rd, rs1, imm'".into());
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
                return Err("ecall não leva operandos".into());
            }
            Ok(Ecall)
        }
        "ebreak" => {
            if !ops.is_empty() {
                return Err("ebreak não leva operandos".into());
            }
            Ok(Ebreak)
        }

        _ => Err(format!("mnemônico não suportado: {mnemonic}")),
    }
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
    let v = parse_imm(s).ok_or_else(|| format!("shamt inválido: {s}"))?;
    if (0..=31).contains(&v) {
        Ok(v as u8)
    } else {
        Err(format!("shamt fora de faixa: {v}"))
    }
}

// beq/bne/... e jal: token pode ser número ou label
fn branch_imm(tok: &str, pc: u32, labels: &HashMap<String, u32>) -> Result<i32, String> {
    if let Some(v) = parse_imm(tok) {
        return Ok(v);
    }
    let target = labels
        .get(&tok.to_string())
        .ok_or_else(|| format!("rótulo não encontrado: {tok}"))?;
    let imm = (*target as i64) - (pc as i64);
    // checagem básica de alcance (13 bits para B, 21 bits para J). Aqui só avisamos.
    Ok(imm as i32)
}

// lw rd, imm(rs1)   |  sw rs2, imm(rs1)
fn parse_memop(op: &str) -> Result<(i32, u8), String> {
    // "imm(rs1)"
    let (imm_s, rest) = op
        .split_once('(')
        .ok_or_else(|| format!("operand mem inválido: {op}"))?;
    let rs1_s = rest.strip_suffix(')').ok_or("faltou ')'")?;
    let imm = parse_imm(imm_s.trim()).ok_or_else(|| format!("imm inválido: {imm_s}"))?;
    let rs1 = parse_reg(rs1_s.trim()).ok_or_else(|| format!("rs1 inválido: {rs1_s}"))?;
    Ok((imm, rs1))
}
fn load_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("load: esperado 'rd, imm(rs1)'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("rd inválido")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rd, imm, rs1))
}
fn store_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("store: esperado 'rs2, imm(rs1)'".into());
    }
    let rs2 = parse_reg(&ops[0]).ok_or("rs2 inválido")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rs2, imm, rs1))
}
