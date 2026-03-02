use std::collections::HashMap;

use crate::falcon::instruction::Instruction;

use super::utils::{check_signed, check_u_imm, parse_reg, split_operands};

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

    // Split the address into a high U-immediate (imm20) and a signed low 12-bit immediate.
    // Use rounding (+0x800) so the low part fits a signed 12-bit ADDI.
    let hi20 = (addr + 0x800) >> 12;
    let hi = check_u_imm(hi20, "la")?;
    let lo = addr - hi;
    let lo = check_signed(lo, 12, "la")?;

    Ok((
        Instruction::Lui { rd, imm: hi },
        Instruction::Addi {
            rd,
            rs1: rd,
            imm: lo,
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
            imm: 1000,
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
            imm: 1001,
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
        Instruction::Addi { rd: 17, rs1: 0, imm: 1002 },
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
            imm: 1003,
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
    // "readByte label" -> a7=1010; a0=addr; ecall
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
        Instruction::Addi { rd: 17, rs1: 0, imm: 1010 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_half(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "readHalf label" -> a7=1011; a0=addr; ecall
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
        Instruction::Addi { rd: 17, rs1: 0, imm: 1011 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_word(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "readWord label" -> a7=1012; a0=addr; ecall
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
        Instruction::Addi { rd: 17, rs1: 0, imm: 1012 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_random_byte(s: &str) -> Result<Vec<Instruction>, String> {
    // "randomByte rd" — getrandom(syscall 278) into temp stack slot, then lbu rd, 0(sp)
    // Clobbers: a0, a1, a2, a7
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 {
        return Err("randomByte: expected 'rd'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("randomByte: invalid rd")?;
    Ok(vec![
        Instruction::Addi { rd: 2,  rs1: 2, imm: -4 }, // sp -= 4 (temp slot)
        Instruction::Addi { rd: 17, rs1: 0, imm: 278 }, // a7 = getrandom
        Instruction::Addi { rd: 10, rs1: 2, imm: 0  }, // a0 = sp (buf)
        Instruction::Addi { rd: 11, rs1: 0, imm: 1  }, // a1 = 1 (len)
        Instruction::Addi { rd: 12, rs1: 0, imm: 0  }, // a2 = 0 (flags)
        Instruction::Ecall,
        Instruction::Lbu { rd, rs1: 2, imm: 0 },       // rd = mem[sp]
        Instruction::Addi { rd: 2,  rs1: 2, imm: 4  }, // sp += 4 (restore)
    ])
}

pub(crate) fn parse_random_bytes(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "randomBytes label, n" — getrandom(buf=label, len=n, flags=0)
    // Clobbers: a0, a1, a2, a7
    use super::utils::parse_imm;
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 2 {
        return Err("randomBytes: expected 'label, n'".into());
    }
    if parse_reg(&ops[0]).is_some() {
        return Err("randomBytes: first operand must be a label, not a register".into());
    }
    let n = parse_imm(&ops[1])
        .ok_or_else(|| format!("randomBytes: invalid byte count: {}", ops[1]))?;
    if n <= 0 {
        return Err(format!("randomBytes: byte count must be positive, got {n}"));
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 278 }, // a7 = getrandom
        i1, i2,                                          // la a0, label
        Instruction::Addi { rd: 11, rs1: 0, imm: n  },  // a1 = n (len)
        Instruction::Addi { rd: 12, rs1: 0, imm: 0  },  // a2 = 0 (flags)
        Instruction::Ecall,
    ])
}
