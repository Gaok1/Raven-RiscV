use std::collections::HashMap;

// Public(crate) helpers reused by submodules

/// Extracts `##!` block comments from raw source lines.
/// Returns map from 0-based line number → comment text.
pub(crate) fn extract_block_comments(text: &str) -> HashMap<usize, String> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim();
            let rest = t.strip_prefix("##!")?;
            let c = rest.trim().to_string();
            if c.is_empty() { None } else { Some((i, c)) }
        })
        .collect()
}

/// Extracts `#!` visible comments from raw source lines.
/// Returns a map from 0-based line number to the comment text (trimmed, without `#!`).
pub(crate) fn extract_visible_comments(text: &str) -> HashMap<usize, String> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let pos = l.find("#!")?;
            let comment = l[pos + 2..].trim().to_string();
            if comment.is_empty() { None } else { Some((i, comment)) }
        })
        .collect()
}

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
    // LUI/AUIPC take an **unshifted** 20-bit immediate that populates bits [31:12].
    // The encoder/CPU model stores this immediate as the final shifted value (imm20 << 12).
    //
    // Accept both signed 20-bit values ([-524288..524287]) and the full unsigned
    // 20-bit range ([0..0xFFFFF]), matching common assembler behavior.
    let min = -(1i32 << 19);
    let max = 0xFFFFF;
    if imm < min || imm > max {
        return Err(format!(
            "{ctx}: immediate {imm} out of 20-bit range ({min}..{max})"
        ));
    }

    let imm20 = (imm as u32) & 0xFFFFF;
    Ok((imm20 << 12) as i32)
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

/// Parse a float register name: f0–f31, ft0–ft11, fs0–fs11, fa0–fa7
pub(crate) fn parse_freg(s: &str) -> Option<u8> {
    let s = s.trim().to_lowercase();
    // f0..f31
    if let Some(n) = s.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        if n < 32 { return Some(n); }
    }
    // ABI names (RISC-V F calling convention)
    match s.as_str() {
        "ft0"  => Some(0),  "ft1"  => Some(1),  "ft2"  => Some(2),  "ft3"  => Some(3),
        "ft4"  => Some(4),  "ft5"  => Some(5),  "ft6"  => Some(6),  "ft7"  => Some(7),
        "fs0"  => Some(8),  "fs1"  => Some(9),
        "fa0"  => Some(10), "fa1"  => Some(11), "fa2"  => Some(12), "fa3"  => Some(13),
        "fa4"  => Some(14), "fa5"  => Some(15), "fa6"  => Some(16), "fa7"  => Some(17),
        "fs2"  => Some(18), "fs3"  => Some(19), "fs4"  => Some(20), "fs5"  => Some(21),
        "fs6"  => Some(22), "fs7"  => Some(23), "fs8"  => Some(24), "fs9"  => Some(25),
        "fs10" => Some(26), "fs11" => Some(27),
        "ft8"  => Some(28), "ft9"  => Some(29), "ft10" => Some(30), "ft11" => Some(31),
        _ => None,
    }
}

/// Parse `imm(freg)` for FP loads: returns (imm, base_int_reg)
pub(crate) fn fp_load_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("flw: expected 'frd, imm(rs1)'".into());
    }
    let rd = parse_freg(&ops[0]).ok_or_else(|| format!("invalid float rd: {}", ops[0]))?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rd, imm, rs1))
}

/// Parse `frs2, imm(rs1)` for FP stores
pub(crate) fn fp_store_like(ops: &[String]) -> Result<(u8, i32, u8), String> {
    if ops.len() != 2 {
        return Err("fsw: expected 'frs2, imm(rs1)'".into());
    }
    let rs2 = parse_freg(&ops[0]).ok_or_else(|| format!("invalid float rs2: {}", ops[0]))?;
    let (imm, rs1) = parse_memop(&ops[1])?;
    Ok((rs2, imm, rs1))
}

pub(crate) fn get_freg(s: &str) -> Result<u8, String> {
    parse_freg(s).ok_or_else(|| format!("invalid float register: {s}"))
}
