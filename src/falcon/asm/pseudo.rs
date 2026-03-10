use std::collections::HashMap;

use crate::falcon::instruction::Instruction;

use super::utils::{check_signed, check_u_imm, parse_char_lit, parse_imm, parse_reg, split_operands};

/// `li rd, imm` — load any 32-bit immediate into `rd`.
/// For values in [-2048, 2047] emits a single `addi rd, x0, imm`.
/// For larger values emits `lui rd, hi20` + `addi rd, rd, lo12`.
pub(crate) fn parse_li(
    s: &str,
    consts: &HashMap<String, i64>,
) -> Result<Vec<Instruction>, String> {
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 2 {
        return Err("li: expected 'rd, imm'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("li: invalid rd")?;
    let imm: i32 = if ops[1].starts_with('\'') {
        parse_char_lit(&ops[1]).map_err(|e| format!("li: {e}"))?
    } else {
        parse_imm(&ops[1])
            .or_else(|| consts.get(&ops[1]).and_then(|&v| i32::try_from(v).ok()))
            .ok_or_else(|| format!("li: invalid immediate: {}", ops[1]))?
    };

    if imm >= -2048 && imm <= 2047 {
        return Ok(vec![Instruction::Addi { rd, rs1: 0, imm }]);
    }
    // Decompose: hi20 rounded so that sign-extending lo12 reconstructs imm.
    let hi20 = ((imm as i64 + 0x800) >> 12) as i32;
    let lo12 = imm.wrapping_sub(hi20 << 12);
    Ok(vec![
        Instruction::Lui  { rd, imm: hi20 << 12 }, // Lui stores pre-shifted value
        Instruction::Addi { rd, rs1: rd, imm: lo12 },
    ])
}

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
        }, // allocate: sp -= 4
        Instruction::Sw {
            rs2: rs,
            rs1: 2,
            imm: 0,
        }, // store at new sp (standard RISC-V convention)
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
        Instruction::Lw { rd, rs1: 2, imm: 0 }, // read from sp (standard RISC-V convention)
        Instruction::Addi {
            rd: 2,
            rs1: 2,
            imm: 4,
        }, // deallocate: sp += 4
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
    // "print_str label" — Linux write(1, buf, strlen(buf))  [syscall 64]
    // Expands to: la a1, label; strlen loop (t0=x5, a2=scan ptr); write(a0=1, a1=buf, a2=len)
    // Clobbers: a0, a1, a2, a7, t0
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("print_str: expected 'label'".into());
    }
    let la_line = format!("la a1, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        i1, i2,                                                  // la a1, label  (buf)
        Instruction::Addi { rd: 12, rs1: 11, imm: 0 },          // mv a2, a1  (scan ptr)
        // strlen loop (offsets relative to each instruction's own PC):
        Instruction::Lbu  { rd: 5,  rs1: 12, imm: 0 },          // lbu t0, 0(a2)
        Instruction::Beq  { rs1: 5, rs2: 0,  imm: 12 },         // beq t0, x0, +12 → exit
        Instruction::Addi { rd: 12, rs1: 12, imm: 1 },          // addi a2, a2, 1
        Instruction::Jal  { rd: 0,  imm: -12 },                  // jal x0, -12 → lbu
        // end loop: a2 = pointer to null byte
        Instruction::Sub  { rd: 12, rs1: 12, rs2: 11 },         // sub a2, a2, a1  (len)
        Instruction::Addi { rd: 10, rs1: 0,  imm: 1 },          // addi a0, x0, 1  (stdout)
        Instruction::Addi { rd: 17, rs1: 0,  imm: 64 },         // addi a7, x0, 64 (write)
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_print_strln(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "print_str_ln label" — same as print_str + write(1, "\n", 1)  [syscall 64]
    // '\n' is stored in a temporary stack slot.
    // Clobbers: a0, a1, a2, a7, t0, sp (temporarily)
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("print_str_ln: expected 'label'".into());
    }
    let la_line = format!("la a1, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        // --- strlen + write(1, buf, len) ---
        i1, i2,
        Instruction::Addi { rd: 12, rs1: 11, imm: 0 },
        Instruction::Lbu  { rd: 5,  rs1: 12, imm: 0 },
        Instruction::Beq  { rs1: 5, rs2: 0,  imm: 12 },
        Instruction::Addi { rd: 12, rs1: 12, imm: 1 },
        Instruction::Jal  { rd: 0,  imm: -12 },
        Instruction::Sub  { rd: 12, rs1: 12, rs2: 11 },
        Instruction::Addi { rd: 10, rs1: 0,  imm: 1 },
        Instruction::Addi { rd: 17, rs1: 0,  imm: 64 },
        Instruction::Ecall,
        // --- write(1, "\n", 1) via stack ---
        Instruction::Addi { rd: 2,  rs1: 2,  imm: -4 },         // sp -= 4
        Instruction::Addi { rd: 5,  rs1: 0,  imm: 10 },         // t0 = '\n'
        Instruction::Sb   { rs2: 5, rs1: 2,  imm: 0 },          // sb t0, 0(sp)
        Instruction::Addi { rd: 10, rs1: 0,  imm: 1 },          // a0 = 1 (stdout)
        Instruction::Addi { rd: 11, rs1: 2,  imm: 0 },          // a1 = sp (buf)
        Instruction::Addi { rd: 12, rs1: 0,  imm: 1 },          // a2 = 1 (len)
        Instruction::Addi { rd: 17, rs1: 0,  imm: 64 },         // a7 = write
        Instruction::Ecall,
        Instruction::Addi { rd: 2,  rs1: 2,  imm: 4 },          // sp += 4
    ])
}

pub(crate) fn parse_read(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "read label" — Linux read(0, buf, 256)  [syscall 63]
    // Reads up to 256 bytes from stdin into label's address.
    // a0 = bytes read after ecall. Clobbers: a0, a1, a2, a7
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("read: expected 'label'".into());
    }
    let la_line = format!("la a1, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 10, rs1: 0, imm: 0   }, // a0 = 0 (stdin)
        i1, i2,                                           // la a1, label (buf)
        Instruction::Addi { rd: 12, rs1: 0, imm: 256 }, // a2 = 256 (max bytes)
        Instruction::Addi { rd: 17, rs1: 0, imm: 63  }, // a7 = read
        Instruction::Ecall,
    ])
}

pub(crate) fn parse_read_byte(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "read_byte label" -> a7=1010; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("read_byte: expected 'label'".into());
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
    // "read_half label" -> a7=1011; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("read_half: expected 'label'".into());
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
    // "read_word label" -> a7=1012; a0=addr; ecall
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 || parse_reg(&ops[0]).is_some() {
        return Err("read_word: expected 'label'".into());
    }
    let la_line = format!("la a0, {}", ops[0]);
    let (i1, i2) = parse_la(&la_line, labels)?;
    Ok(vec![
        Instruction::Addi { rd: 17, rs1: 0, imm: 1012 },
        i1, i2, Instruction::Ecall,
    ])
}

pub(crate) fn parse_random(s: &str) -> Result<Vec<Instruction>, String> {
    // "random rd" — getrandom(syscall 278) into temp stack slot, then lw rd, 0(sp)
    // Fills 4 bytes (full 32-bit word). Clobbers: a0, a1, a2, a7
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 1 {
        return Err("random: expected 'rd'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("random: invalid rd")?;
    Ok(vec![
        Instruction::Addi { rd: 2,  rs1: 2, imm: -4 }, // sp -= 4 (temp slot)
        Instruction::Addi { rd: 17, rs1: 0, imm: 278 }, // a7 = getrandom
        Instruction::Addi { rd: 10, rs1: 2, imm: 0  }, // a0 = sp (buf)
        Instruction::Addi { rd: 11, rs1: 0, imm: 4  }, // a1 = 4 (len = 4 bytes)
        Instruction::Addi { rd: 12, rs1: 0, imm: 0  }, // a2 = 0 (flags)
        Instruction::Ecall,
        Instruction::Lw  { rd, rs1: 2, imm: 0 },       // rd = mem[sp] (full word)
        Instruction::Addi { rd: 2,  rs1: 2, imm: 4  }, // sp += 4 (restore)
    ])
}

pub(crate) fn parse_random_bytes(
    s: &str,
    labels: &HashMap<String, u32>,
) -> Result<Vec<Instruction>, String> {
    // "random_bytes label, n" — getrandom(buf=label, len=n, flags=0)
    // Clobbers: a0, a1, a2, a7
    use super::utils::parse_imm;
    let mut parts = s.split_whitespace();
    parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() != 2 {
        return Err("random_bytes: expected 'label, n'".into());
    }
    if parse_reg(&ops[0]).is_some() {
        return Err("random_bytes: first operand must be a label, not a register".into());
    }
    let n = parse_imm(&ops[1])
        .ok_or_else(|| format!("random_bytes: invalid byte count: {}", ops[1]))?;
    if n <= 0 {
        return Err(format!("random_bytes: byte count must be positive, got {n}"));
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
