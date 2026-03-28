use std::collections::HashMap;

use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;

use super::errors::AsmError;
use super::program::Program;
use super::pseudo::{
    parse_la, parse_li, parse_pop, parse_print, parse_print_str, parse_print_strln, parse_push,
    parse_random, parse_random_bytes, parse_read, parse_read_byte, parse_read_half,
    parse_read_word,
};
use super::utils::*;

#[derive(Debug)]
enum EquateEvalError {
    UnknownSymbol(String),
    InvalidExpr(String),
}

#[derive(Debug, Clone)]
enum ExprTok {
    Plus,
    Minus,
    Dot,
    Number(i64),
    Ident(String),
}

fn tokenize_expr(expr: &str) -> Result<Vec<ExprTok>, EquateEvalError> {
    let mut toks = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\r' | '\n' | ',' => {
                chars.next();
            }
            '+' => {
                chars.next();
                toks.push(ExprTok::Plus);
            }
            '-' => {
                chars.next();
                toks.push(ExprTok::Minus);
            }
            '(' | ')' | '*' | '/' | '%' | '&' | '|' | '^' | '<' | '>' | '~' => {
                return Err(EquateEvalError::InvalidExpr(format!(
                    "unsupported operator/paren '{c}' in expression"
                )));
            }
            '.' => {
                chars.next();
                // Bare '.' is the location counter. If it's immediately followed by an identifier
                // character, treat it as part of a symbol (e.g. '.L0', '.LC0').
                let is_ident_start = matches!(chars.peek(), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.' || *ch == '$');
                if is_ident_start {
                    let mut s = String::from(".");
                    while let Some(&ch) = chars.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '$' {
                            s.push(ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    toks.push(ExprTok::Ident(s));
                } else {
                    toks.push(ExprTok::Dot);
                }
            }
            '0'..='9' => {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_hexdigit() || ch == 'x' || ch == 'X' || ch == 'b' || ch == 'B' {
                        s.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let v = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    i64::from_str_radix(hex, 16).map_err(|_| {
                        EquateEvalError::InvalidExpr(format!("invalid hex number: {s}"))
                    })?
                } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
                    i64::from_str_radix(bin, 2).map_err(|_| {
                        EquateEvalError::InvalidExpr(format!("invalid binary number: {s}"))
                    })?
                } else {
                    s.parse::<i64>()
                        .map_err(|_| EquateEvalError::InvalidExpr(format!("invalid number: {s}")))?
                };
                toks.push(ExprTok::Number(v));
            }
            _ => {
                // identifier
                if c.is_ascii_alphabetic() || c == '_' || c == '$' {
                    let mut s = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '$' {
                            s.push(ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    toks.push(ExprTok::Ident(s));
                } else {
                    return Err(EquateEvalError::InvalidExpr(format!(
                        "unexpected character '{c}' in expression"
                    )));
                }
            }
        }
    }
    Ok(toks)
}

fn eval_expr(
    expr: &str,
    dot: i64,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> Result<i64, EquateEvalError> {
    let toks = tokenize_expr(expr)?;
    if toks.is_empty() {
        return Err(EquateEvalError::InvalidExpr("empty expression".into()));
    }

    let mut idx = 0usize;
    let parse_signed_term = |idx: &mut usize| -> Result<i64, EquateEvalError> {
        let mut sign = 1i64;
        while *idx < toks.len() {
            match toks[*idx] {
                ExprTok::Plus => {
                    *idx += 1;
                }
                ExprTok::Minus => {
                    sign = -sign;
                    *idx += 1;
                }
                _ => break,
            }
        }

        if *idx >= toks.len() {
            return Err(EquateEvalError::InvalidExpr(
                "expected term after operator".into(),
            ));
        }

        let v = match &toks[*idx] {
            ExprTok::Dot => dot,
            ExprTok::Number(n) => *n,
            ExprTok::Ident(name) => {
                if let Some(v) = consts.get(name) {
                    *v
                } else if let Some(v) = labels.get(name) {
                    *v as i64
                } else {
                    return Err(EquateEvalError::UnknownSymbol(name.clone()));
                }
            }
            ExprTok::Plus | ExprTok::Minus => unreachable!(),
        };
        *idx += 1;
        Ok(sign * v)
    };

    let mut acc = parse_signed_term(&mut idx)?;
    while idx < toks.len() {
        let op = match toks[idx] {
            ExprTok::Plus => 1i64,
            ExprTok::Minus => -1i64,
            _ => return Err(EquateEvalError::InvalidExpr("expected '+' or '-'".into())),
        };
        idx += 1;
        let rhs = parse_signed_term(&mut idx)?;
        acc += op * rhs;
    }
    Ok(acc)
}

// ---------- API ----------
pub fn assemble(text: &str, base_pc: u32) -> Result<Program, AsmError> {
    let line_comments = extract_visible_comments(text);
    let raw_block_comments = extract_block_comments(text);
    let lines = preprocess(text);
    let data_base = base_pc + 0x1000; // data region after code

    // 1st pass: symbol table
    #[derive(Clone, Copy)]
    enum Section {
        Text,
        Data,
        Bss,
    }
    struct EquateDef {
        name: String,
        expr: String,
        sec: Section,
        off: u32,
        line_no: usize,
    }
    let mut section = Section::Text;
    let mut pc_text = base_pc;
    let mut pc_data = 0u32; // offset from data_base
    let mut pc_bss = 0u32; // offset/size within .bss
    let mut items: Vec<(u32, LineKind, usize)> = Vec::new(); // (pc, LineKind, line number)
    let mut data_bytes = Vec::<u8>::new();
    // Collect label defs by section and offset; resolve absolute addresses after first pass
    let mut label_defs = HashMap::<String, (Section, u32)>::new();
    let mut label_source_lines = HashMap::<String, usize>::new(); // label → source line (0-based)
    let mut equates = Vec::<EquateDef>::new();
    // fixups for `.word label` — resolved after labels map is built
    let mut word_label_fixups: Vec<(usize, String, usize)> = Vec::new(); // (byte_offset, label_name, line_no)

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
            let rest = rest.trim();
            let name = rest
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap_or("");
            match name {
                ".text" | "text" => section = Section::Text,
                ".data" | "data" | ".rodata" | "rodata" | ".sdata" | "sdata" | ".srodata"
                | "srodata" => section = Section::Data,
                ".bss" | "bss" | ".sbss" | "sbss" => section = Section::Bss,
                ".note.GNU-stack" => continue, // no-op for this simulator
                "" => {
                    return Err(AsmError {
                        line: *line_no,
                        msg: "missing section name".into(),
                    });
                }
                _ => {
                    return Err(AsmError {
                        line: *line_no,
                        msg: format!("unknown section: {name}"),
                    });
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
            let lab_name = lab.trim().to_string();
            label_defs.insert(lab_name.clone(), (sec, off));
            label_source_lines.insert(lab_name, *line_no);
            line = rest[1..].trim();
            if line.is_empty() {
                // instruction label only
                continue;
            }
        }

        let ltrim = line.trim_start();
        if ltrim.is_empty() {
            continue;
        }

        // Common GAS directives that are irrelevant for this simulator (accepted as no-ops).
        if ltrim.starts_with(".globl") || ltrim.starts_with(".global") {
            let rest = ltrim
                .strip_prefix(".globl")
                .or_else(|| ltrim.strip_prefix(".global"))
                .unwrap_or("")
                .trim();
            if rest.is_empty() {
                return Err(AsmError {
                    line: *line_no,
                    msg: "missing symbol name in .globl/.global".into(),
                });
            }
            continue;
        }
        if ltrim.starts_with(".type")
            || ltrim.starts_with(".size")
            || ltrim.starts_with(".file")
            || ltrim.starts_with(".ident")
            || ltrim.starts_with(".option")
            || ltrim.starts_with(".attribute")
            || ltrim.starts_with(".cfi_")
        {
            continue;
        }

        // Equates / symbol assignments (e.g. `len = . - msg` or `.equ len, . - msg`)
        let (sec, off) = match section {
            Section::Text => (Section::Text, pc_text),
            Section::Data => (Section::Data, pc_data),
            Section::Bss => (Section::Bss, pc_bss),
        };
        if let Some(rest) = ltrim
            .strip_prefix(".equ")
            .or_else(|| ltrim.strip_prefix(".set"))
        {
            let rest = rest.trim();
            let (name, expr) = if let Some((n, e)) = rest.split_once(',') {
                (n.trim().to_string(), e.trim().to_string())
            } else {
                let mut it = rest.split_whitespace();
                let n = it.next().unwrap_or("").trim().to_string();
                let e = it.collect::<Vec<_>>().join(" ");
                (n, e.trim().to_string())
            };
            if name.is_empty() || expr.is_empty() {
                return Err(AsmError {
                    line: *line_no,
                    msg: "expected '.equ name, expr'".into(),
                });
            }
            equates.push(EquateDef {
                name,
                expr,
                sec,
                off,
                line_no: *line_no,
            });
            continue;
        }
        if !ltrim.starts_with('.') {
            if let Some((lhs, rhs)) = ltrim.split_once('=') {
                let name = lhs.trim();
                let expr = rhs.trim();
                if !name.is_empty() && !expr.is_empty() {
                    equates.push(EquateDef {
                        name: name.to_string(),
                        expr: expr.to_string(),
                        sec,
                        off,
                        line_no: *line_no,
                    });
                    continue;
                }
            }
        }

        match section {
            Section::Text => {
                if ltrim.starts_with("li ") {
                    let words = li_word_count(ltrim);
                    items.push((pc_text, LineKind::Li(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(words * 4);
                } else if ltrim.starts_with("la ") {
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
                } else if ltrim == "print_str"
                    || ltrim.starts_with("print_str ")
                    || ltrim == "printStr"
                    || ltrim.starts_with("printStr ")
                    || ltrim == "printString"
                    || ltrim.starts_with("printString ")
                {
                    items.push((pc_text, LineKind::PrintStr(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(44); // 11 instructions
                } else if ltrim == "print_str_ln"
                    || ltrim.starts_with("print_str_ln ")
                    || ltrim == "printStrLn"
                    || ltrim.starts_with("printStrLn ")
                {
                    items.push((pc_text, LineKind::PrintStrLn(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(80); // 20 instructions
                } else if ltrim == "read" || ltrim.starts_with("read ") {
                    items.push((pc_text, LineKind::Read(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(24); // 6 instructions
                } else if ltrim == "read_byte"
                    || ltrim.starts_with("read_byte ")
                    || ltrim == "readByte"
                    || ltrim.starts_with("readByte ")
                {
                    items.push((pc_text, LineKind::ReadByte(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "read_half"
                    || ltrim.starts_with("read_half ")
                    || ltrim == "readHalf"
                    || ltrim.starts_with("readHalf ")
                {
                    items.push((pc_text, LineKind::ReadHalf(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim == "read_word"
                    || ltrim.starts_with("read_word ")
                    || ltrim == "readWord"
                    || ltrim.starts_with("readWord ")
                {
                    items.push((pc_text, LineKind::ReadWord(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(16);
                } else if ltrim.starts_with("random ") || ltrim == "random" {
                    items.push((pc_text, LineKind::RandomByte(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(32); // 8 instructions
                } else if ltrim.starts_with("random_bytes ") || ltrim.starts_with("randomBytes ") {
                    items.push((pc_text, LineKind::RandomBytes(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(24); // 6 instructions
                } else {
                    items.push((pc_text, LineKind::Instr(ltrim.to_string()), *line_no));
                    pc_text = pc_text.wrapping_add(4);
                }
            }
            Section::Data => {
                if let Some(rest) = line.strip_prefix(".align") {
                    let n = parse_imm(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid .align: {rest}"),
                    })?;
                    if n <= 0 {
                        return Err(AsmError {
                            line: *line_no,
                            msg: format!("alignment must be positive: {n}"),
                        });
                    }
                    let n = n as u32;
                    let mask = n - 1;
                    let aligned = if (pc_data & mask) == 0 {
                        pc_data
                    } else {
                        (pc_data + mask) & !mask
                    };
                    let pad = (aligned - pc_data) as usize;
                    if pad != 0 {
                        data_bytes.extend(std::iter::repeat(0).take(pad));
                        pc_data = aligned;
                    }
                } else if let Some(rest) = line.strip_prefix(".byte") {
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
                        let w = w.trim();
                        if let Some(v) = parse_imm(w) {
                            let bytes = (v as u32).to_le_bytes();
                            data_bytes.extend_from_slice(&bytes);
                        } else if w
                            .chars()
                            .next()
                            .map_or(false, |c| c.is_ascii_alphabetic() || c == '_' || c == '.')
                            && w.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                        {
                            // label reference — emit placeholder, resolve after labels are built
                            word_label_fixups.push((data_bytes.len(), w.to_string(), *line_no));
                            data_bytes.extend_from_slice(&[0u8; 4]);
                        } else {
                            return Err(AsmError {
                                line: *line_no,
                                msg: format!("invalid .word: {w}"),
                            });
                        }
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
                } else if let Some(rest) = line.strip_prefix(".float") {
                    for f in rest.split(',') {
                        let f = f.trim();
                        let v: f32 = f.parse().map_err(|_| AsmError {
                            line: *line_no,
                            msg: format!("invalid .float: {f}"),
                        })?;
                        let bytes = v.to_le_bytes();
                        data_bytes.extend_from_slice(&bytes);
                        pc_data += 4;
                    }
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
                } else if let Some(rest) = line.strip_prefix(".ascii") {
                    let s = parse_str_lit(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid .ascii: {rest}"),
                    })?;
                    data_bytes.extend_from_slice(s.as_bytes());
                    pc_data += s.len() as u32;
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
                        return Err(AsmError {
                            line: *line_no,
                            msg: format!("size must be positive: {n}"),
                        });
                    }
                    pc_bss = pc_bss.wrapping_add(n as u32);
                } else if let Some(rest) = line.strip_prefix(".align") {
                    let n = parse_imm(rest).ok_or_else(|| AsmError {
                        line: *line_no,
                        msg: format!("invalid align: {rest}"),
                    })?;
                    if n <= 0 {
                        return Err(AsmError {
                            line: *line_no,
                            msg: format!("alignment must be positive: {n}"),
                        });
                    }
                    let n = n as u32;
                    let mask = n - 1;
                    let aligned = if (pc_bss & mask) == 0 {
                        pc_bss
                    } else {
                        (pc_bss + mask) & !mask
                    };
                    pc_bss = aligned;
                } else if line.starts_with(".byte")
                    || line.starts_with(".half")
                    || line.starts_with(".word")
                    || line.starts_with(".dword")
                    || line.starts_with(".float")
                    || line.starts_with(".ascii")
                    || line.starts_with(".asciz")
                    || line.starts_with(".string")
                {
                    return Err(AsmError {
                        line: *line_no,
                        msg: ".bss does not store explicit data; use .space/.zero/.skip/.align"
                            .into(),
                    });
                } else {
                    return Err(AsmError {
                        line: *line_no,
                        msg: format!("unknown .bss directive: {line}"),
                    });
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

    // Resolve .word label fixups now that all label addresses are known
    for (offset, name, line_no) in &word_label_fixups {
        let addr = labels.get(name).ok_or_else(|| AsmError {
            line: *line_no,
            msg: format!("undefined label in .word: {name}"),
        })?;
        let bytes = addr.to_le_bytes();
        data_bytes[*offset..*offset + 4].copy_from_slice(&bytes);
    }

    // Resolve equates now that we know final label addresses.
    let abs_of = |sec: Section, off: u32| -> i64 {
        match sec {
            Section::Text => off as i64,
            Section::Data => (data_base + off) as i64,
            Section::Bss => (data_base + data_size + off) as i64,
        }
    };
    let mut consts = HashMap::<String, i64>::new();
    if !equates.is_empty() {
        let mut pending = equates;
        // Try repeatedly to allow forward references between equates.
        loop {
            if pending.is_empty() {
                break;
            }
            let mut next = Vec::new();
            let mut progress = false;
            for def in pending.into_iter() {
                if labels.contains_key(&def.name) {
                    return Err(AsmError {
                        line: def.line_no,
                        msg: format!("equate redefines existing label: {}", def.name),
                    });
                }
                if consts.contains_key(&def.name) {
                    return Err(AsmError {
                        line: def.line_no,
                        msg: format!("duplicate equate: {}", def.name),
                    });
                }

                let dot = abs_of(def.sec, def.off);
                match eval_expr(&def.expr, dot, &labels, &consts) {
                    Ok(v) => {
                        consts.insert(def.name, v);
                        progress = true;
                    }
                    Err(EquateEvalError::UnknownSymbol(_)) => next.push(def),
                    Err(EquateEvalError::InvalidExpr(e)) => {
                        return Err(AsmError {
                            line: def.line_no,
                            msg: e,
                        });
                    }
                }
            }

            if next.is_empty() {
                break;
            }
            if !progress {
                // Produce a deterministic error for the first unresolved equate.
                let def = &next[0];
                let dot = abs_of(def.sec, def.off);
                let msg = match eval_expr(&def.expr, dot, &labels, &consts) {
                    Err(EquateEvalError::UnknownSymbol(sym)) => {
                        format!("unknown symbol in equate '{}': {}", def.name, sym)
                    }
                    Err(EquateEvalError::InvalidExpr(e)) => e,
                    Ok(_) => "failed to resolve equate".into(),
                };
                return Err(AsmError {
                    line: def.line_no,
                    msg,
                });
            }
            pending = next;
        }
    }

    // Build addr→labels reverse map and label→source-line map
    let mut addr_to_labels: HashMap<u32, Vec<String>> = HashMap::new();
    for (name, &addr) in &labels {
        addr_to_labels.entry(addr).or_default().push(name.clone());
    }
    for v in addr_to_labels.values_mut() {
        v.sort();
    }

    let label_to_line: HashMap<String, usize> = labels
        .iter()
        .filter_map(|(name, _addr)| label_source_lines.get(name).map(|&ln| (name.clone(), ln)))
        .collect();

    // 2nd pass: assemble
    let mut words = Vec::with_capacity(items.len());
    let mut halt_pcs = std::collections::HashSet::new();
    let mut comments: HashMap<u32, String> = HashMap::new();
    let mut block_comments: HashMap<u32, String> = HashMap::new();
    let mut line_addrs: HashMap<usize, u32> = HashMap::new();
    let mut prev_item_line: usize = 0;
    for (pc, kind, line_no) in items {
        line_addrs.entry(line_no).or_insert(pc);
        if let Some(c) = line_comments.get(&line_no) {
            comments.insert(pc, c.clone());
        }
        // Find the nearest ##! block comment for lines in range (prev_item_line+1)..=line_no
        let search_start = prev_item_line + 1;
        for search_line in search_start..=line_no {
            if let Some(bc) = raw_block_comments.get(&search_line) {
                block_comments.insert(pc, bc.clone());
                break;
            }
        }
        prev_item_line = line_no;
        match kind {
            LineKind::Instr(s) => {
                let inst = parse_instr(&s, pc, &labels, &consts).map_err(|e| AsmError {
                    line: line_no,
                    msg: e,
                })?;
                if matches!(inst, Instruction::Halt) {
                    halt_pcs.insert(pc);
                }
                let word = encode(inst).map_err(|e| AsmError {
                    line: line_no,
                    msg: e.to_string(),
                })?;
                words.push(word);
            }
            LineKind::Li(s) => {
                let insts = parse_li(&s, &consts).map_err(|e| AsmError {
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
                // normalise legacy camelCase mnemonics to the canonical snake_case form
                let s_norm = if s.starts_with("printString") {
                    s.replacen("printString", "print_str", 1)
                } else if s.starts_with("printStr") {
                    s.replacen("printStr", "print_str", 1)
                } else {
                    s.clone()
                };
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
                let s_norm = if s.starts_with("printStrLn") {
                    s.replacen("printStrLn", "print_str_ln", 1)
                } else {
                    s.clone()
                };
                let insts = parse_print_strln(&s_norm, &labels).map_err(|e| AsmError {
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
                let insts = parse_read_byte(&s, &labels).map_err(|e| AsmError {
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
            LineKind::ReadHalf(s) => {
                let insts = parse_read_half(&s, &labels).map_err(|e| AsmError {
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
            LineKind::ReadWord(s) => {
                let insts = parse_read_word(&s, &labels).map_err(|e| AsmError {
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
            LineKind::RandomByte(s) => {
                let insts = parse_random(&s).map_err(|e| AsmError {
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
            LineKind::RandomBytes(s) => {
                let insts = parse_random_bytes(&s, &labels).map_err(|e| AsmError {
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
        }
    }

    Ok(Program {
        text: words,
        data: data_bytes,
        data_base,
        bss_size: pc_bss,
        comments,
        block_comments,
        labels: addr_to_labels,
        line_addrs,
        label_to_line,
        halt_pcs,
    })
}

// ---------- Internals ----------
#[derive(Debug, Clone)]
enum LineKind {
    Instr(String),
    Li(String),
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
    RandomByte(String),
    RandomBytes(String),
}

/// Returns the number of words `li rd, imm` will emit (1 for 12-bit literals, 2 otherwise).
/// Called during the first pass before equates are resolved; equate names conservatively → 2 words.
fn li_word_count(s: &str) -> u32 {
    let rest = s.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);
    if ops.len() < 2 {
        return 1;
    }
    match parse_imm(&ops[1]) {
        Some(v) if v >= -2048 && v <= 2047 => 1,
        _ => 2,
    }
}

fn parse_instr(
    s: &str,
    pc: u32,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> Result<Instruction, String> {
    // ex: "addi x1, x0, 10"
    let mut parts = s.split_whitespace();
    let mnemonic = parts.next().ok_or("empty line")?.to_lowercase();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let ops = split_operands(&rest);

    use Instruction::*;

    let get_reg = |t: &str| parse_reg(t).ok_or_else(|| format!("invalid register: {t}"));
    let get_freg = |t: &str| parse_freg(t).ok_or_else(|| format!("invalid float register: {t}"));
    let get_imm = |t: &str| {
        if let Some(v) = parse_imm(t) {
            return Ok(v);
        }
        if t.starts_with('\'') {
            return Err(parse_char_lit(t).unwrap_err());
        }
        if let Some(v) = consts.get(t) {
            let v_i32 =
                i32::try_from(*v).map_err(|_| format!("immediate out of range for i32: {t}"))?;
            return Ok(v_i32);
        }
        Err(format!("invalid immediate: {t}"))
    };

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
        // li is handled as LineKind::Li (may expand to 2 words); reaching here is a bug
        "li" => Err("internal: li reached parse_instr unexpectedly".into()),
        "j" => {
            if ops.len() != 1 {
                return Err("j: expected label/immediate".into());
            }
            Ok(Jal {
                rd: 0,
                imm: branch_imm(&ops[0], pc, labels, 21, "j")?,
            })
        }
        "call" => {
            if ops.len() != 1 {
                return Err("call: expected label/immediate".into());
            }
            Ok(Jal {
                rd: 1,
                imm: branch_imm(&ops[0], pc, labels, 21, "call")?,
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
            let neg = -get_imm(&ops[2])?;
            let neg = check_signed(neg, 12, "subi")?;
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
            if ops.len() != 3 {
                return Err("expected 'rs1, rs2, label/imm'".into());
            }
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

        // Pseudo: bez / beqz rs, label  →  beq rs, x0, label
        //         bnez      rs, label  →  bne rs, x0, label
        "bez" | "beqz" => {
            if ops.len() != 2 {
                return Err(format!("{mnemonic}: expected 'rs, label/imm'"));
            }
            let rs1 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, mnemonic.as_str())?;
            Ok(Beq { rs1, rs2: 0, imm })
        }
        "bnez" => {
            if ops.len() != 2 {
                return Err("bnez: expected 'rs, label/imm'".into());
            }
            let rs1 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, "bnez")?;
            Ok(Bne { rs1, rs2: 0, imm })
        }

        // ---------- U-type ----------
        "lui" => {
            if ops.len() != 2 {
                return Err("lui: expected 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = check_u_imm(get_imm(&ops[1])?, "lui")?;
            Ok(Lui { rd, imm })
        }
        "auipc" => {
            if ops.len() != 2 {
                return Err("auipc: expected 'rd, imm'".into());
            }
            let rd = get_reg(&ops[0])?;
            let imm = check_u_imm(get_imm(&ops[1])?, "auipc")?;
            Ok(Auipc { rd, imm })
        }

        // jal: two formats: "jal rd,label" or "jal label" (rd=ra)
        "jal" => {
            if ops.is_empty() {
                return Err("jal: missing destination".into());
            }
            if ops.len() == 1 {
                let rd = 1; // ra
                let imm = branch_imm(&ops[0], pc, labels, 21, "jal")?;
                Ok(Jal { rd, imm })
            } else if ops.len() == 2 {
                Ok(Jal {
                    rd: get_reg(&ops[0])?,
                    imm: branch_imm(&ops[1], pc, labels, 21, "jal")?,
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
                imm: check_signed(get_imm(&ops[2])?, 12, "jalr")?,
            })
        }

        // ---------- Branch pseudos ----------
        // Two-register: bgt/ble/bgtu/bleu rs, rt, label  (swap operands → blt/bge/bltu/bgeu rt, rs)
        "bgt" | "ble" | "bgtu" | "bleu" => {
            if ops.len() != 3 {
                return Err(format!("{mnemonic}: expected 'rs, rt, label/imm'"));
            }
            let rs = get_reg(&ops[0])?;
            let rt = get_reg(&ops[1])?;
            let imm = branch_imm(&ops[2], pc, labels, 13, mnemonic.as_str())?;
            Ok(match mnemonic.as_str() {
                "bgt" => Blt {
                    rs1: rt,
                    rs2: rs,
                    imm,
                }, // blt rt, rs
                "ble" => Bge {
                    rs1: rt,
                    rs2: rs,
                    imm,
                }, // bge rt, rs
                "bgtu" => Bltu {
                    rs1: rt,
                    rs2: rs,
                    imm,
                }, // bltu rt, rs
                "bleu" => Bgeu {
                    rs1: rt,
                    rs2: rs,
                    imm,
                }, // bgeu rt, rs
                _ => unreachable!(),
            })
        }
        // Single-register vs zero: bltz/bgez/blez/bgtz rs, label
        "bltz" => {
            if ops.len() != 2 {
                return Err("bltz: expected 'rs, label/imm'".into());
            }
            let rs1 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, "bltz")?;
            Ok(Blt { rs1, rs2: 0, imm }) // blt rs, x0
        }
        "bgez" => {
            if ops.len() != 2 {
                return Err("bgez: expected 'rs, label/imm'".into());
            }
            let rs1 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, "bgez")?;
            Ok(Bge { rs1, rs2: 0, imm }) // bge rs, x0
        }
        "blez" => {
            if ops.len() != 2 {
                return Err("blez: expected 'rs, label/imm'".into());
            }
            let rs2 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, "blez")?;
            Ok(Bge { rs1: 0, rs2, imm }) // bge x0, rs  ⟺  rs ≤ 0
        }
        "bgtz" => {
            if ops.len() != 2 {
                return Err("bgtz: expected 'rs, label/imm'".into());
            }
            let rs2 = get_reg(&ops[0])?;
            let imm = branch_imm(&ops[1], pc, labels, 13, "bgtz")?;
            Ok(Blt { rs1: 0, rs2, imm }) // blt x0, rs  ⟺  rs > 0
        }

        // ---------- Set pseudos ----------
        "seqz" => {
            if ops.len() != 2 {
                return Err("seqz: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            Ok(Sltiu { rd, rs1, imm: 1 }) // sltiu rd, rs, 1
        }
        "snez" => {
            if ops.len() != 2 {
                return Err("snez: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs2 = get_reg(&ops[1])?;
            Ok(Sltu { rd, rs1: 0, rs2 }) // sltu rd, x0, rs
        }
        "sltz" => {
            if ops.len() != 2 {
                return Err("sltz: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            Ok(Slt { rd, rs1, rs2: 0 }) // slt rd, rs, x0
        }
        "sgtz" => {
            if ops.len() != 2 {
                return Err("sgtz: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs2 = get_reg(&ops[1])?;
            Ok(Slt { rd, rs1: 0, rs2 }) // slt rd, x0, rs
        }

        // ---------- Arithmetic pseudos ----------
        "neg" => {
            if ops.len() != 2 {
                return Err("neg: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs2 = get_reg(&ops[1])?;
            Ok(Sub { rd, rs1: 0, rs2 }) // sub rd, x0, rs
        }
        "not" => {
            if ops.len() != 2 {
                return Err("not: expected 'rd, rs'".into());
            }
            let rd = get_reg(&ops[0])?;
            let rs1 = get_reg(&ops[1])?;
            Ok(Xori { rd, rs1, imm: -1 }) // xori rd, rs, -1
        }

        // ---------- Memory ordering ----------
        "fence" => Ok(Fence), // nop in single-core simulator (RV32I base)

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
        "halt" => {
            if !ops.is_empty() {
                return Err("halt takes no operands".into());
            }
            Ok(Halt)
        }

        // ────────────────── RV32F ──────────────────

        // Load / Store
        "flw" => {
            let (rd, imm, rs1) = fp_load_like(&ops)?;
            Ok(Flw { rd, rs1, imm })
        }
        "fsw" => {
            let (rs2, imm, rs1) = fp_store_like(&ops)?;
            Ok(Fsw { rs2, rs1, imm })
        }

        // Arithmetic (3 float regs)
        "fadd.s" => {
            if ops.len() != 3 {
                return Err("fadd.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FaddS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fsub.s" => {
            if ops.len() != 3 {
                return Err("fsub.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FsubS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fmul.s" => {
            if ops.len() != 3 {
                return Err("fmul.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FmulS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fdiv.s" => {
            if ops.len() != 3 {
                return Err("fdiv.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FdivS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fsqrt.s" => {
            if ops.len() != 2 {
                return Err("fsqrt.s: expected 'frd, frs1'".into());
            }
            Ok(FsqrtS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
            })
        }
        "fmin.s" => {
            if ops.len() != 3 {
                return Err("fmin.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FminS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fmax.s" => {
            if ops.len() != 3 {
                return Err("fmax.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FmaxS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }

        // Sign injection
        "fsgnj.s" => {
            if ops.len() != 3 {
                return Err("fsgnj.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FsgnjS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fsgnjn.s" => {
            if ops.len() != 3 {
                return Err("fsgnjn.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FsgnjnS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fsgnjx.s" => {
            if ops.len() != 3 {
                return Err("fsgnjx.s: expected 'frd, frs1, frs2'".into());
            }
            Ok(FsgnjxS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }

        // Comparison (result → integer rd)
        "feq.s" => {
            if ops.len() != 3 {
                return Err("feq.s: expected 'rd, frs1, frs2'".into());
            }
            Ok(FeqS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "flt.s" => {
            if ops.len() != 3 {
                return Err("flt.s: expected 'rd, frs1, frs2'".into());
            }
            Ok(FltS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }
        "fle.s" => {
            if ops.len() != 3 {
                return Err("fle.s: expected 'rd, frs1, frs2'".into());
            }
            Ok(FleS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
            })
        }

        // Conversion
        "fcvt.w.s" => {
            if ops.len() < 2 || ops.len() > 3 {
                return Err("fcvt.w.s: expected 'rd, frs1[, rm]'".into());
            }
            let rm = if ops.len() == 3 {
                parse_rm(&ops[2])
                    .ok_or("fcvt.w.s: unknown rounding mode; expected rne|rtz|rdn|rup|rmm|dyn")?
            } else {
                0
            };
            Ok(FcvtWS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rm,
            })
        }
        "fcvt.wu.s" => {
            if ops.len() < 2 || ops.len() > 3 {
                return Err("fcvt.wu.s: expected 'rd, frs1[, rm]'".into());
            }
            let rm = if ops.len() == 3 {
                parse_rm(&ops[2])
                    .ok_or("fcvt.wu.s: unknown rounding mode; expected rne|rtz|rdn|rup|rmm|dyn")?
            } else {
                0
            };
            Ok(FcvtWuS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rm,
            })
        }
        "fcvt.s.w" => {
            if ops.len() != 2 {
                return Err("fcvt.s.w: expected 'frd, rs1'".into());
            }
            Ok(FcvtSW {
                rd: get_freg(&ops[0])?,
                rs1: get_reg(&ops[1])?,
            })
        }
        "fcvt.s.wu" => {
            if ops.len() != 2 {
                return Err("fcvt.s.wu: expected 'frd, rs1'".into());
            }
            Ok(FcvtSWu {
                rd: get_freg(&ops[0])?,
                rs1: get_reg(&ops[1])?,
            })
        }

        // Move (bit-pattern)
        "fmv.x.w" => {
            if ops.len() != 2 {
                return Err("fmv.x.w: expected 'rd, frs1'".into());
            }
            Ok(FmvXW {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
            })
        }
        "fmv.w.x" => {
            if ops.len() != 2 {
                return Err("fmv.w.x: expected 'frd, rs1'".into());
            }
            Ok(FmvWX {
                rd: get_freg(&ops[0])?,
                rs1: get_reg(&ops[1])?,
            })
        }

        // Classify
        "fclass.s" => {
            if ops.len() != 2 {
                return Err("fclass.s: expected 'rd, frs1'".into());
            }
            Ok(FclassS {
                rd: get_reg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
            })
        }

        // Fused multiply-add (R4-type): fmadd.s frd, frs1, frs2, frs3
        "fmadd.s" => {
            if ops.len() != 4 {
                return Err("fmadd.s: expected 'frd, frs1, frs2, frs3'".into());
            }
            Ok(FmaddS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
                rs3: get_freg(&ops[3])?,
            })
        }
        "fmsub.s" => {
            if ops.len() != 4 {
                return Err("fmsub.s: expected 'frd, frs1, frs2, frs3'".into());
            }
            Ok(FmsubS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
                rs3: get_freg(&ops[3])?,
            })
        }
        "fnmsub.s" => {
            if ops.len() != 4 {
                return Err("fnmsub.s: expected 'frd, frs1, frs2, frs3'".into());
            }
            Ok(FnmsubS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
                rs3: get_freg(&ops[3])?,
            })
        }
        "fnmadd.s" => {
            if ops.len() != 4 {
                return Err("fnmadd.s: expected 'frd, frs1, frs2, frs3'".into());
            }
            Ok(FnmaddS {
                rd: get_freg(&ops[0])?,
                rs1: get_freg(&ops[1])?,
                rs2: get_freg(&ops[2])?,
                rs3: get_freg(&ops[3])?,
            })
        }

        // Pseudos FP
        "fmv.s" => {
            // fmv.s frd, frs → fsgnj.s frd, frs, frs
            if ops.len() != 2 {
                return Err("fmv.s: expected 'frd, frs'".into());
            }
            let rd = get_freg(&ops[0])?;
            let rs = get_freg(&ops[1])?;
            Ok(FsgnjS {
                rd,
                rs1: rs,
                rs2: rs,
            })
        }
        "fneg.s" => {
            // fneg.s frd, frs → fsgnjn.s frd, frs, frs
            if ops.len() != 2 {
                return Err("fneg.s: expected 'frd, frs'".into());
            }
            let rd = get_freg(&ops[0])?;
            let rs = get_freg(&ops[1])?;
            Ok(FsgnjnS {
                rd,
                rs1: rs,
                rs2: rs,
            })
        }
        "fabs.s" => {
            // fabs.s frd, frs → fsgnjx.s frd, frs, frs
            if ops.len() != 2 {
                return Err("fabs.s: expected 'frd, frs'".into());
            }
            let rd = get_freg(&ops[0])?;
            let rs = get_freg(&ops[1])?;
            Ok(FsgnjxS {
                rd,
                rs1: rs,
                rs2: rs,
            })
        }

        _ => Err(format!("unsupported mnemonic: {mnemonic}")),
    }
}
