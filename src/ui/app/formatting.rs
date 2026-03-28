use super::*;

pub(super) fn word_at(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return String::new();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    if !is_word(chars[col]) {
        return String::new();
    }
    let start = (0..=col)
        .rev()
        .take_while(|&i| i < chars.len() && is_word(chars[i]))
        .last()
        .unwrap_or(col);
    let end = (col..chars.len())
        .take_while(|&i| is_word(chars[i]))
        .last()
        .map(|i| i + 1)
        .unwrap_or(col + 1);
    chars[start..end].iter().collect()
}

pub(super) fn format_asm_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|l| {
            let formatted = format_asm_line(l);
            formatted
                .split('\n')
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn format_asm_line(line: &str) -> String {
    let trimmed_end = line.trim_end();
    if trimmed_end.is_empty() {
        return String::new();
    }

    let comment_byte = find_comment_byte(trimmed_end);
    let (code, comment) = if let Some(ci) = comment_byte {
        (&trimmed_end[..ci], &trimmed_end[ci..])
    } else {
        (trimmed_end, "")
    };

    let code_trimmed = code.trim();
    if code_trimmed.is_empty() {
        return trimmed_end.to_string();
    }

    if code_trimmed.ends_with(':') && !code_trimmed.contains(' ') {
        return if comment.is_empty() {
            code_trimmed.to_string()
        } else {
            format!("{}  {}", code_trimmed, comment.trim_start())
        };
    }

    if code_trimmed.starts_with('.') {
        let formatted = format!("    {}", normalize_operand_spacing(code_trimmed));
        return if comment.is_empty() {
            formatted
        } else {
            format!("{formatted}  {}", comment.trim_start())
        };
    }

    let (prefix, instr_code) = if let Some((lab, rest)) = code_trimmed.split_once(':') {
        let rest = rest.trim();
        if !rest.is_empty() {
            (Some(lab.trim()), rest)
        } else {
            (None, code_trimmed)
        }
    } else {
        (None, code_trimmed)
    };

    let formatted_instr = format!("    {}", normalize_operand_spacing(instr_code));
    let body = if let Some(lab) = prefix {
        format!("{lab}:\n{formatted_instr}")
    } else {
        formatted_instr
    };

    if comment.is_empty() {
        body
    } else {
        format!("{body}  {}", comment.trim_start())
    }
}

fn normalize_operand_spacing(code: &str) -> String {
    let mut it = code.splitn(2, |c: char| c.is_whitespace());
    let mnemonic = it.next().unwrap_or("");
    let ops_raw = it.next().unwrap_or("").trim();
    if ops_raw.is_empty() {
        return mnemonic.to_string();
    }
    let mut result = String::new();
    let mut tok = String::new();
    let mut depth = 0i32;
    let mut first = true;
    for ch in ops_raw.chars() {
        match ch {
            '(' => {
                depth += 1;
                tok.push(ch);
            }
            ')' => {
                depth -= 1;
                tok.push(ch);
            }
            ',' if depth == 0 => {
                if !first {
                    result.push_str(", ");
                }
                result.push_str(tok.trim());
                tok.clear();
                first = false;
            }
            _ => tok.push(ch),
        }
    }
    if !first {
        result.push_str(", ");
    }
    result.push_str(tok.trim());
    format!("{mnemonic} {result}")
}

fn find_comment_byte(s: &str) -> Option<usize> {
    let mut in_str = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_str = !in_str,
            '#' | ';' if !in_str => return Some(i),
            _ => {}
        }
    }
    None
}

impl App {
    pub(in crate::ui) fn format_code(&mut self) {
        let formatted = format_asm_lines(&self.editor.buf.lines);
        if formatted == self.editor.buf.lines {
            return;
        }
        self.editor.buf.snapshot();
        self.editor.buf.lines = formatted;
        let max_row = self.editor.buf.lines.len().saturating_sub(1);
        if self.editor.buf.cursor_row > max_row {
            self.editor.buf.cursor_row = max_row;
        }
    }
}

pub(super) fn classify_mem_access(word: u32, cpu: &crate::falcon::Cpu) -> Option<(u32, u32, bool)> {
    let opcode = word & 0x7F;
    let funct3 = (word >> 12) & 0x7;
    let rs1 = ((word >> 15) & 0x1F) as usize;

    match opcode {
        0x03 | 0x07 => {
            let imm = ((word as i32) >> 20) as u32;
            let addr = cpu.x[rs1].wrapping_add(imm);
            let size: u32 = match funct3 {
                0 | 4 => 1,
                1 | 5 => 2,
                2 => 4,
                _ => return None,
            };
            Some((addr, size, false))
        }
        0x23 | 0x27 => {
            let imm_lo = (word >> 7) & 0x1F;
            let imm_hi = (word >> 25) & 0x7F;
            let imm = (((imm_hi << 5) | imm_lo) as i32)
                .wrapping_shl(20)
                .wrapping_shr(20) as u32;
            let addr = cpu.x[rs1].wrapping_add(imm);
            let size: u32 = match funct3 {
                0 => 1,
                1 => 2,
                2 => 4,
                _ => return None,
            };
            Some((addr, size, true))
        }
        _ => None,
    }
}
