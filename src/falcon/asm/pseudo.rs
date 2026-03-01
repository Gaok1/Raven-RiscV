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
        return Err("la: expected 'rd, label'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("la: invalid register")?;
    let addr = *labels
        .get(&ops[1])
        .ok_or_else(|| format!("la: label not found: '{}'", ops[1]))? as i32;

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
        return Err("push: expected 'rs'".into());
    }
    let rs = parse_reg(&ops[0]).ok_or("push: invalid register")?;
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
        return Err("pop: expected 'rd'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("pop: invalid register")?;
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

fn parse_rand_n(mnemonic: &str, s: &str, labels: &HashMap<String, u32>, count: i32) -> Result<Vec<Instruction>, String> {
    // "randByte/randHalf/randWord label"
    // Expands to 6 instructions:
    //   addi a7, x0, 278   (li a7, getrandom)
    //   lui  a0, hi(label) ]  la a0, label
    //   addi a0, a0, lo    ]
    //   addi a1, x0, count (li a1, 1/2/4)
    //   addi a2, x0, 0     (li a2, 0  — flags=0)
    //   ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err(format!("{mnemonic}: expected 'label'"));
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 278 },
        i1, i2,
        Instruction::Addi { rd: 11, rs1: 0, imm: count },
        Instruction::Addi { rd: 12, rs1: 0, imm: 0 },
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_rand_byte(s: &str, labels: &HashMap<String, u32>) -> Result<Vec<Instruction>, String> {
    parse_rand_n("randByte", s, labels, 1)
}
pub(crate) fn parse_rand_half(s: &str, labels: &HashMap<String, u32>) -> Result<Vec<Instruction>, String> {
    parse_rand_n("randHalf", s, labels, 2)
}
pub(crate) fn parse_rand_word(s: &str, labels: &HashMap<String, u32>) -> Result<Vec<Instruction>, String> {
    parse_rand_n("randWord", s, labels, 4)
}

pub(crate) fn parse_rand_bytes(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "randBytes label, len_reg"  or  "randBytes label, imm"
    // Expands to 6 instructions (Linux getrandom ABI):
    //   addi a7, x0, 278        (li a7, getrandom)
    //   lui  a0, hi(label)      ]  la a0, label
    //   addi a0, a0, lo(label)  ]
    //   addi a1, reg, 0         (mv a1, reg)  — or — addi a1, x0, imm  (li a1, imm)
    //   addi a2, x0, 0          (li a2, 0  — flags=0)
    //   ecall
    let mut parts = s.split_whitespace();
    parts.next(); // consume mnemonic
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 2 {
        return Err("randBytes: expected 'label, len'".into());
    }
    if parse_reg(&ops[0]).is_some() {
        return Err("randBytes: first operand must be a label, not a register".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (la_lui, la_addi) = parse_la(&la_line, labels)?;

    // Second operand: register or immediate for the byte count (a1)
    let len_inst = if let Some(reg) = parse_reg(&ops[1]) {
        Instruction::Addi { rd: 11, rs1: reg, imm: 0 }
    } else if let Some(imm) = parse_imm(&ops[1]) {
        if !(-2048..=2047).contains(&imm) {
            return Err(format!("randBytes: length immediate {imm} out of 12-bit signed range"));
        }
        Instruction::Addi { rd: 11, rs1: 0, imm }
    } else {
        return Err(format!("randBytes: invalid length operand '{}'", ops[1]));
    };

    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 278 },
        la_lui,
        la_addi,
        len_inst,
        Instruction::Addi { rd: 12, rs1: 0, imm: 0 },
        Instruction::Ecall,
    ])
}
