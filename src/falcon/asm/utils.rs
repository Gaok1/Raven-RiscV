use std::collections::HashMap;

// Public(crate) helpers reused by submodules
pub(crate) fn preprocess(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            let l = l.split(';').next().unwrap_or(l);
            let l = l.split('#').next().unwrap_or(l);
            (i, l.trim().to_string())
        })
        .filter(|(_, l)| !l.is_empty())
        .collect()
}

pub(crate) fn split_operands(rest: &str) -> Vec<String> {
    rest.split(',')
        .map(|t| t.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn parse_reg(s: &str) -> Option<u8> {
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

pub(crate) fn parse_imm(s: &str) -> Option<i32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x") {
        i32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i32>().ok()
    }
}

pub(crate) fn parse_imm64(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

pub(crate) fn parse_str_lit(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        Some(s[1..s.len() - 1].to_string())
    } else {
        None
    }
}

// Parse shift amount (shamt) for SLLI, SRLI, SRAI
pub(crate) fn parse_shamt(s: &str) -> Result<u8, String> {
    let v = parse_imm(s).ok_or_else(|| format!("invalid shamt: {s}"))?;
    if (0..=31).contains(&v) {
        Ok(v as u8)
    } else {
        Err(format!("shamt out of range: {v}"))
    }
}

pub(crate) fn check_signed(imm: i32, bits: u32, ctx: &str) -> Result<i32, String> {
    let max = (1i32 << (bits - 1)) - 1;
    let min = -(1i32 << (bits - 1));
    if imm < min || imm > max {
        Err(format!(
            "{ctx}: immediate {imm} out of {bits}-bit signed range ({min}..{max})"
        ))
    } else {
        Ok(imm)
    }
}

pub(crate) fn check_u_imm(imm: i32, ctx: &str) -> Result<i32, String> {
    if imm & 0xfff != 0 {
        return Err(format!("{ctx}: immediate {imm} has non-zero lower 12 bits"));
    }
    let imm64 = imm as i64;
    let min = -(1i64 << 31);
    let max = (1i64 << 31) - (1i64 << 12);
    if imm64 < min || imm64 > max {
        Err(format!(
            "{ctx}: immediate {imm} out of 20-bit signed range ({min}..{max})"
        ))
    } else {
        Ok(imm)
    }
}

// beq/bne/... and jal: token can be a number or label
pub(crate) fn branch_imm(
    tok: &str,
    pc: u32,
    labels: &HashMap<String, u32>,
    bits: u32,
    ctx: &str,
) -> Result<i32, String> {
    let imm = if let Some(v) = parse_imm(tok) {
        v
    } else {
        let target = labels
            .get(&tok.to_string())
            .ok_or_else(|| format!("label not found: {tok}"))?;
        (*target as i64 - pc as i64) as i32
    };
    if imm % 2 != 0 {
        return Err(format!("{ctx}: offset {imm} must be even"));
    }
    check_signed(imm, bits, ctx)
}

// lw rd, imm(rs1)   |  sw rs2, imm(rs1)
pub(crate) fn parse_memop(op: &str) -> Result<(i32, u8), String> {
    // "imm(rs1)"
    let (imm_s, rest) = op
        .split_once('(')
        .ok_or_else(|| format!("invalid mem operand: {op}"))?;
    let rs1_s = rest.strip_suffix(')').ok_or("missing ')'")?;
    let imm = parse_imm(imm_s.trim()).ok_or_else(|| format!("invalid imm: {imm_s}"))?;
    let rs1 = parse_reg(rs1_s.trim()).ok_or_else(|| format!("invalid rs1: {rs1_s}"))?;
    Ok((imm, rs1))
}

pub(crate) fn load_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("load: expected 'rd, imm(rs1)'".into());
    }
    let rd = parse_reg(&ops[0]).ok_or("invalid rd")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rd, imm, rs1))
}

pub(crate) fn store_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("store: expected 'rs2, imm(rs1)'".into());
    }
    let rs2 = parse_reg(&ops[0]).ok_or("invalid rs2")?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rs2, imm, rs1))
}

