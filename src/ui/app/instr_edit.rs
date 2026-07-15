//! Instruction-word field editing: encoding formats, per-field seeds, parsing
//! and bit-splicing. Backs the editable Instruction Details panel — the pure
//! logic lives here so the commit path and the renderer share one source of
//! truth for where each field sits in the word.

use crate::falcon::asm::utils::{parse_imm, parse_reg};
use ratatui::style::Color;

// ── Encoding formats ─────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum EncFormat {
    R,
    I,
    S,
    B,
    U,
    J,
}

impl EncFormat {
    pub(crate) fn name(self) -> &'static str {
        match self {
            EncFormat::R => "R-type",
            EncFormat::I => "I-type",
            EncFormat::S => "S-type",
            EncFormat::B => "B-type",
            EncFormat::U => "U-type",
            EncFormat::J => "J-type",
        }
    }
    pub(crate) fn segments(self) -> Vec<Seg> {
        seg_list(self)
    }
}

pub(crate) fn detect_format(word: u32) -> EncFormat {
    match word & 0x7f {
        0x03 | 0x13 | 0x1b | 0x67 | 0x73 => EncFormat::I,
        0x23 => EncFormat::S,
        0x63 => EncFormat::B,
        0x37 | 0x17 => EncFormat::U,
        0x6f => EncFormat::J,
        _ => EncFormat::R,
    }
}

pub(crate) struct Seg {
    pub(crate) label: &'static str,
    pub(crate) width: u8,
    pub(crate) color: Color,
}

pub(crate) fn seg_list(format: EncFormat) -> Vec<Seg> {
    macro_rules! s {
        ($l:expr, $w:expr, $c:expr) => {
            Seg {
                label: $l,
                width: $w,
                color: $c,
            }
        };
    }
    use Color::*;
    match format {
        EncFormat::R => vec![
            s!("funct7", 7, Red),
            s!("rs2", 5, LightRed),
            s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow),
            s!("rd", 5, LightGreen),
            s!("opcode", 7, Cyan),
        ],
        EncFormat::I => vec![
            s!("imm[11:0]", 12, Blue),
            s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow),
            s!("rd", 5, LightGreen),
            s!("opcode", 7, Cyan),
        ],
        EncFormat::S => vec![
            s!("imm[11:5]", 7, Blue),
            s!("rs2", 5, LightRed),
            s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow),
            s!("imm[4:0]", 5, Blue),
            s!("opcode", 7, Cyan),
        ],
        EncFormat::B => vec![
            s!("i12", 1, Blue),
            s!("i10:5", 6, Blue),
            s!("rs2", 5, LightRed),
            s!("rs1", 5, LightMagenta),
            s!("fn3", 3, Yellow),
            s!("i4:1", 4, Blue),
            s!("i11", 1, Blue),
            s!("opcode", 7, Cyan),
        ],
        EncFormat::U => vec![
            s!("imm[31:12]", 20, Blue),
            s!("rd", 5, LightGreen),
            s!("opcode", 7, Cyan),
        ],
        EncFormat::J => vec![
            s!("i20", 1, Blue),
            s!("i10:1", 10, Blue),
            s!("i11", 1, Blue),
            s!("i19:12", 8, Blue),
            s!("rd", 5, LightGreen),
            s!("opcode", 7, Cyan),
        ],
    }
}

// ── Editable fields ──────────────────────────────────────────────────────────

/// One editable spot in the Instruction Details panel. `Word` is only a hitbox
/// tag (the full hex word edits through `RunEditTarget::Instr`); every other
/// kind edits through `RunEditTarget::InstrField`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum InstrFieldKind {
    /// The full word in hex (header line 1). Hitbox tag only.
    Word,
    /// The mnemonic line, edited as one line of assembly.
    Asm,
    /// The full word in binary (header line 1).
    Bin,
    Rd,
    Rs1,
    Rs2,
    /// The format-dependent immediate (I/S/B/U/J bit layouts).
    Imm,
    Opcode,
    Funct3,
    Funct7,
    Shamt,
}

/// Whether `field` exists in the encoding of `word`. `Funct7`/`Shamt` on
/// I-type are only meaningful for the shift instructions (funct3 1 or 5).
pub(crate) fn field_available(word: u32, field: InstrFieldKind) -> bool {
    use InstrFieldKind::*;
    let format = detect_format(word);
    let i_shift = format == EncFormat::I && matches!((word >> 12) & 0x7, 0x1 | 0x5);
    match field {
        Word | Asm | Bin | Opcode => true,
        Rd => matches!(format, EncFormat::R | EncFormat::I | EncFormat::U | EncFormat::J),
        Rs1 => matches!(format, EncFormat::R | EncFormat::I | EncFormat::S | EncFormat::B),
        Rs2 => matches!(format, EncFormat::R | EncFormat::S | EncFormat::B),
        Funct3 => matches!(format, EncFormat::R | EncFormat::I | EncFormat::S | EncFormat::B),
        Imm => !matches!(format, EncFormat::R),
        Funct7 => format == EncFormat::R || i_shift,
        Shamt => i_shift,
    }
}

/// Decode the immediate of `word` exactly as the Decoded section displays it
/// (U-type as the unshifted `imm[31:12]` value).
pub(crate) fn decode_imm(word: u32, format: EncFormat) -> i32 {
    match format {
        EncFormat::R => 0,
        EncFormat::I => (((word >> 20) as i32) << 20) >> 20,
        EncFormat::S => {
            let imm_lo = (word >> 7) & 0x1f;
            let imm_hi = (word >> 25) & 0x7f;
            ((((imm_hi << 5) | imm_lo) as i32) << 20) >> 20
        }
        EncFormat::B => {
            let b12 = (word >> 31) & 1;
            let b10_5 = (word >> 25) & 0x3f;
            let b4_1 = (word >> 8) & 0xf;
            let b11 = (word >> 7) & 1;
            ((((b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1)) as i32) << 19) >> 19
        }
        EncFormat::U => ((word & 0xfffff000) as i32) >> 12,
        EncFormat::J => {
            let b20 = (word >> 31) & 1;
            let b10_1 = (word >> 21) & 0x3ff;
            let b11 = (word >> 20) & 1;
            let b19_12 = (word >> 12) & 0xff;
            ((((b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1)) as i32) << 11) >> 11
        }
    }
}

/// The text an editor on `field` starts from — the value exactly as the
/// details panel renders it (registers as `x{n}`, immediates in decimal,
/// functs in hex). `Asm` seeds with the disassembly so a small tweak doesn't
/// require retyping the whole line; `Word` never reaches here.
pub(crate) fn seed_field(word: u32, field: InstrFieldKind) -> String {
    use InstrFieldKind::*;
    match field {
        Word => String::new(),
        Asm => match crate::falcon::decoder::decode(word) {
            Ok(_) => crate::ui::view::disasm::disasm_word(word),
            Err(_) => String::new(),
        },
        Bin => format!("{word:032b}"),
        Rd => format!("x{}", (word >> 7) & 0x1f),
        Rs1 => format!("x{}", (word >> 15) & 0x1f),
        Rs2 => format!("x{}", (word >> 20) & 0x1f),
        Imm => format!("{}", decode_imm(word, detect_format(word))),
        Opcode => format!("0x{:02x}", word & 0x7f),
        Funct3 => format!("0x{:01x}", (word >> 12) & 0x7),
        Funct7 => format!("0x{:02x}", (word >> 25) & 0x7f),
        Shamt => format!("{}", (word >> 20) & 0x1f),
    }
}

/// Parse the typed text for a numeric field. Registers accept `x11`, ABI names
/// and bare `11`; immediates accept decimal and `0x`/`0b` prefixes; binary is
/// the full 32-bit word. `Word`/`Asm` are handled by their own commit paths.
pub(crate) fn parse_field_value(field: InstrFieldKind, text: &str) -> Result<i64, String> {
    use InstrFieldKind::*;
    let t = text.trim();
    match field {
        Word | Asm => Err("not a numeric field".into()),
        Rd | Rs1 | Rs2 => {
            if let Ok(n) = t.parse::<u8>() {
                if n < 32 {
                    return Ok(n as i64);
                }
                return Err(format!("register {n} out of range (0..31)"));
            }
            parse_reg(t)
                .map(|r| r as i64)
                .ok_or_else(|| format!("\"{t}\" is not a register (x0..x31 or ABI name)"))
        }
        Imm | Shamt | Opcode | Funct3 | Funct7 => parse_imm(t)
            .map(|v| v as i64)
            .ok_or_else(|| format!("cannot parse \"{t}\" as a number")),
        Bin => {
            let digits: String = t
                .trim_start_matches("0b")
                .trim_start_matches("0B")
                .chars()
                .filter(|c| *c != '_')
                .collect();
            if digits.is_empty() || digits.len() > 32 {
                return Err("binary value must have 1..=32 bits".into());
            }
            u32::from_str_radix(&digits, 2)
                .map(|v| v as i64)
                .map_err(|_| format!("cannot parse \"{t}\" as binary"))
        }
    }
}

fn set_bits(word: u32, lo: u32, width: u32, value: u32) -> u32 {
    let mask = ((1u64 << width) - 1) as u32;
    (word & !(mask << lo)) | ((value & mask) << lo)
}

fn check_range(value: i64, min: i64, max: i64, what: &str) -> Result<(), String> {
    if value < min || value > max {
        Err(format!("{what} {value} out of range ({min}..{max})"))
    } else {
        Ok(())
    }
}

/// Splice `value` into `field` of `word`, range-checked against the field's
/// width (immediates against the format's signed range, branch/jump offsets
/// must be even). Returns the rewritten word.
pub(crate) fn splice_field(
    word: u32,
    format: EncFormat,
    field: InstrFieldKind,
    value: i64,
) -> Result<u32, String> {
    use InstrFieldKind::*;
    match field {
        Word | Asm => Err("not a spliceable field".into()),
        Bin => {
            check_range(value, 0, u32::MAX as i64, "word")?;
            Ok(value as u32)
        }
        Rd => {
            check_range(value, 0, 31, "rd")?;
            Ok(set_bits(word, 7, 5, value as u32))
        }
        Rs1 => {
            check_range(value, 0, 31, "rs1")?;
            Ok(set_bits(word, 15, 5, value as u32))
        }
        Rs2 => {
            check_range(value, 0, 31, "rs2")?;
            Ok(set_bits(word, 20, 5, value as u32))
        }
        Opcode => {
            check_range(value, 0, 0x7f, "opcode")?;
            Ok(set_bits(word, 0, 7, value as u32))
        }
        Funct3 => {
            check_range(value, 0, 0x7, "funct3")?;
            Ok(set_bits(word, 12, 3, value as u32))
        }
        Funct7 => {
            check_range(value, 0, 0x7f, "funct7")?;
            Ok(set_bits(word, 25, 7, value as u32))
        }
        Shamt => {
            check_range(value, 0, 31, "shamt")?;
            Ok(set_bits(word, 20, 5, value as u32))
        }
        Imm => splice_imm(word, format, value),
    }
}

fn splice_imm(word: u32, format: EncFormat, value: i64) -> Result<u32, String> {
    match format {
        EncFormat::R => Err("R-type has no immediate".into()),
        EncFormat::I => {
            check_range(value, -(1 << 11), (1 << 11) - 1, "imm")?;
            Ok(set_bits(word, 20, 12, value as u32))
        }
        EncFormat::S => {
            check_range(value, -(1 << 11), (1 << 11) - 1, "offset")?;
            let v = value as u32;
            let word = set_bits(word, 7, 5, v & 0x1f);
            Ok(set_bits(word, 25, 7, (v >> 5) & 0x7f))
        }
        EncFormat::B => {
            check_range(value, -(1 << 12), (1 << 12) - 1, "offset")?;
            if value % 2 != 0 {
                return Err(format!("branch offset {value} must be even"));
            }
            let v = value as u32;
            let word = set_bits(word, 8, 4, (v >> 1) & 0xf);
            let word = set_bits(word, 25, 6, (v >> 5) & 0x3f);
            let word = set_bits(word, 7, 1, (v >> 11) & 1);
            Ok(set_bits(word, 31, 1, (v >> 12) & 1))
        }
        EncFormat::U => {
            // Accept signed 20-bit or the full unsigned 20-bit range, like the
            // assembler's `check_u_imm`.
            check_range(value, -(1 << 19), 0xFFFFF, "imm[31:12]")?;
            Ok(set_bits(word, 12, 20, value as u32))
        }
        EncFormat::J => {
            check_range(value, -(1 << 20), (1 << 20) - 1, "offset")?;
            if value % 2 != 0 {
                return Err(format!("jump offset {value} must be even"));
            }
            let v = value as u32;
            let word = set_bits(word, 21, 10, (v >> 1) & 0x3ff);
            let word = set_bits(word, 20, 1, (v >> 11) & 1);
            let word = set_bits(word, 12, 8, (v >> 12) & 0xff);
            Ok(set_bits(word, 31, 1, (v >> 20) & 1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use InstrFieldKind::*;

    /// `addi a0, zero, 1` — I-type.
    const ADDI: u32 = 0x00100513;
    /// `add a0, a1, a2` — R-type.
    const ADD: u32 = 0x00c58533;
    /// `beq a0, a1, 8` — B-type.
    const BEQ: u32 = 0x00b50463;
    /// `lui a1, 0x1` — U-type.
    const LUI: u32 = 0x000015b7;
    /// `jal ra, 16` — J-type.
    const JAL: u32 = 0x010000ef;
    /// `sw a0, 4(sp)` — S-type.
    const SW: u32 = 0x00a12223;

    #[test]
    fn seed_matches_decoded_fields() {
        assert_eq!(seed_field(ADDI, Rd), "x10");
        assert_eq!(seed_field(ADDI, Rs1), "x0");
        assert_eq!(seed_field(ADDI, Imm), "1");
        assert_eq!(seed_field(ADDI, Opcode), "0x13");
        assert_eq!(seed_field(ADDI, Bin), format!("{ADDI:032b}"));
        assert_eq!(seed_field(LUI, Imm), "1");
        assert_eq!(seed_field(BEQ, Imm), "8");
        assert_eq!(seed_field(JAL, Imm), "16");
        assert_eq!(seed_field(SW, Imm), "4");
    }

    #[test]
    fn splice_same_value_round_trips() {
        for &word in &[ADDI, ADD, BEQ, LUI, JAL, SW] {
            let format = detect_format(word);
            for field in [Rd, Rs1, Rs2, Imm, Opcode, Funct3, Funct7, Shamt, Bin] {
                if !field_available(word, field) {
                    continue;
                }
                let seed = seed_field(word, field);
                let v = parse_field_value(field, &seed).unwrap();
                assert_eq!(
                    splice_field(word, format, field, v).unwrap(),
                    word,
                    "round-trip of {field:?} on {word:#010x}"
                );
            }
        }
    }

    #[test]
    fn splice_rd_rewrites_only_rd_bits() {
        let v = parse_field_value(Rd, "a1").unwrap();
        assert_eq!(v, 11);
        let out = splice_field(ADDI, EncFormat::I, Rd, v).unwrap();
        assert_eq!((out >> 7) & 0x1f, 11);
        assert_eq!(out & !(0x1f << 7), ADDI & !(0x1f << 7));
    }

    #[test]
    fn registers_accept_bare_numbers_and_abi_names() {
        assert_eq!(parse_field_value(Rs1, "11").unwrap(), 11);
        assert_eq!(parse_field_value(Rs1, "x11").unwrap(), 11);
        assert_eq!(parse_field_value(Rs1, "a1").unwrap(), 11);
        assert!(parse_field_value(Rs1, "32").is_err());
        assert!(parse_field_value(Rs1, "q7").is_err());
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        assert!(splice_field(ADDI, EncFormat::I, Funct3, 9).is_err());
        assert!(splice_field(ADDI, EncFormat::I, Imm, 4096).is_err());
        assert!(splice_field(BEQ, EncFormat::B, Imm, 7).is_err()); // odd offset
        assert!(splice_field(ADD, EncFormat::R, Rs2, 32).is_err());
    }

    #[test]
    fn branch_offset_splices_all_bit_groups() {
        // beq a0, a1, -2: imm[12|11|10:5|4:1] scattered encoding.
        let out = splice_field(BEQ, EncFormat::B, Imm, -2).unwrap();
        assert_eq!(decode_imm(out, EncFormat::B), -2);
        // Untouched fields survive.
        assert_eq!((out >> 15) & 0x1f, (BEQ >> 15) & 0x1f);
        assert_eq!((out >> 20) & 0x1f, (BEQ >> 20) & 0x1f);
        assert_eq!(out & 0x7f, BEQ & 0x7f);
    }

    #[test]
    fn jump_offset_round_trips_negative() {
        let out = splice_field(JAL, EncFormat::J, Imm, -4).unwrap();
        assert_eq!(decode_imm(out, EncFormat::J), -4);
    }

    #[test]
    fn shift_fields_only_on_shift_instructions() {
        // slli a0, a0, 3
        let slli: u32 = 0x00351513;
        assert!(field_available(slli, Shamt));
        assert!(field_available(slli, Funct7));
        assert!(!field_available(ADDI, Shamt));
        assert!(!field_available(ADDI, Funct7));
        let out = splice_field(slli, EncFormat::I, Shamt, 5).unwrap();
        assert_eq!((out >> 20) & 0x1f, 5);
    }
}
