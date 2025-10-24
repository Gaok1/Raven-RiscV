use std::collections::HashMap;

use crate::falcon::instruction::Instruction;

use super::utils::{check_signed, check_u_imm, parse_imm, parse_reg, split_operands};

pub(crate) fn parse_la(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<(Instruction, Instruction), String> {
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
    let hi = check_u_imm(hi, "la")?;
    let lo_signed = check_signed(lo_signed, 12, "la")?;

    Ok((
        Instruction::Lui { rd, imm: hi },
        Instruction::Addi {
            rd,
            rs1: rd,
            imm: lo_signed,
        },
    ))
}

pub(crate) fn parse_push(s: &str) -> Result<(Instruction, Instruction), String> {
    // "push rs"
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 {
        return Err("expected 'rs'".into());
    }
    let rs = parse_reg(&ops[0]).ok_or("invalid rs")?;
    Ok((
        Instruction::Addi {
            rd: 2,
            rs1: 2,
            imm: -4,
        }, // alocate stack space
        Instruction::Sw {
            rs2: rs,
            rs1: 2,
            imm: 4,
        }, //write into sp+4
    ))
}

pub(crate) fn parse_pop(s: &str) -> Result<(Instruction, Instruction), String> {
    // "pop rd"
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 {
        return Err("expected 'rd'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("invalid rd")?;
    Ok((
        Instruction::Lw { rd, rs1: 2, imm: 4 }, // read from sp+4
        Instruction::Addi {
            rd: 2,
            rs1: 2,
            imm: 4,
        }, // deallocate stack space (sp += 4)
    ))
}

pub(crate) fn parse_print(s: &str) -> Result<Vec<Instruction>, String> {
    // "print rd"
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 {
        return Err("print: expected 'rd'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("print: invalid rd")?;
    Ok(vec![
        Instruction::Addi {
            rd: 17,
            rs1: 0,
            imm: 1,
        },
        Instruction::Addi {
            rd: 10,
            rs1: rd,
            imm: 0,
        },
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_print_str(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "printStr label" (no newline)
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("printStr: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi {
            rd: 17,
            rs1: 0,
            imm: 2,
        },
        i1,
        i2,
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_print_strln(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "printStrLn label" (append + newline)
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("printStrLn: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 4 },
        i1,
        i2,
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_read(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "read label" (data written to memory pointed by label)
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("read: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi {
            rd: 17,
            rs1: 0,
            imm: 3,
        },
        i1,
        i2,
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_byte(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "readByte label" -> a7=64; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("readByte: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 64 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_half(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "readHalf label" -> a7=65; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("readHalf: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 65 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_word(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "readWord label" -> a7=66; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("readWord: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 66 },
        i1, i2, Instruction::Ecall,
    ])
}
