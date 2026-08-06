//! Minimal x86-64 backend using Intel syntax.
//!
//! The backend intentionally models the small, useful integer/system subset
//! Raven can teach today: moves, integer ALU operations, stack/control flow,
//! `hlt`, and Linux-style `read`, `write`, and `exit` syscalls. Unsupported
//! opcodes fault instead of being approximated.

mod elf;
mod pipeline;

pub use elf::image_from_elf;

use crate::capability::{
    BitRole, InstructionBitField, InstructionCodec, InstructionField, InstructionInfo,
    MemoryInspect, MemoryRegion, PipelineControl, PipelineInspect, PipelineTuning, RegisterBank, RegisterFile,
    RegisterId,
};
use crate::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, Assembler, CycleResult,
    Diagnostic, Endianness, InstructionDoc, Machine, MachineError, MachineSnapshot, MachineState,
    ProgramImage, ProgramSegment, RegisterValue, SourceMap, StepOutcome, ZeroFill,
};
use pipeline::{X86Op, X86Pipeline};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock};

pub const ID: &str = "x86_64";
pub const DEFAULT_MEMORY_SIZE: usize = 16 * 1024 * 1024;

static INSTRUCTION_DOCS: &[InstructionDoc] = &[
    InstructionDoc::new(
        "Move",
        "mov",
        "dst, src",
        "Copy a register, immediate, or memory value",
    ),
    InstructionDoc::new("Address", "lea", "reg, [mem]", "Load an effective address"),
    InstructionDoc::new("ALU", "add", "reg, reg|imm", "Add and update status flags"),
    InstructionDoc::new(
        "ALU",
        "sub",
        "reg, reg|imm",
        "Subtract and update status flags",
    ),
    InstructionDoc::new("ALU", "xor", "reg, reg", "Bitwise exclusive-or"),
    InstructionDoc::new("ALU", "and", "reg, reg", "Bitwise and"),
    InstructionDoc::new("ALU", "or", "reg, reg", "Bitwise or"),
    InstructionDoc::new("ALU", "imul", "reg, reg", "Signed integer multiply"),
    InstructionDoc::new("ALU", "mul", "reg", "Unsigned multiply of rax into rdx:rax"),
    InstructionDoc::new(
        "ALU",
        "div",
        "reg",
        "Unsigned divide rdx:rax, quotient in rax and remainder in rdx",
    ),
    InstructionDoc::new(
        "ALU",
        "idiv",
        "reg",
        "Signed divide rdx:rax, quotient in rax and remainder in rdx",
    ),
    InstructionDoc::new("ALU", "inc", "reg", "Increment a register"),
    InstructionDoc::new("ALU", "dec", "reg", "Decrement a register"),
    InstructionDoc::new("ALU", "neg", "reg", "Replace a register with its negation"),
    InstructionDoc::new("ALU", "not", "reg", "Flip every bit, leaving flags alone"),
    InstructionDoc::new(
        "Shift",
        "shl",
        "reg, imm|cl",
        "Shift left, filling with zeros",
    ),
    InstructionDoc::new(
        "Shift",
        "sal",
        "reg, imm|cl",
        "Arithmetic left shift; same as shl",
    ),
    InstructionDoc::new(
        "Shift",
        "shr",
        "reg, imm|cl",
        "Shift right, filling with zeros",
    ),
    InstructionDoc::new(
        "Shift",
        "sar",
        "reg, imm|cl",
        "Shift right, keeping the sign bit",
    ),
    InstructionDoc::new(
        "Compare",
        "cmp",
        "reg, reg|imm",
        "Subtract into flags without storing",
    ),
    InstructionDoc::new("Compare", "test", "reg, reg", "Bitwise test into flags"),
    InstructionDoc::new("Stack", "push", "reg", "Push a 64-bit register"),
    InstructionDoc::new("Stack", "pop", "reg", "Pop a 64-bit register"),
    InstructionDoc::new("Jump", "jmp", "label", "Jump unconditionally"),
    InstructionDoc::new("Branch", "je", "label", "Jump when equal/zero"),
    InstructionDoc::new("Branch", "jne", "label", "Jump when not equal/non-zero"),
    InstructionDoc::new("Branch", "jl", "label", "Jump on signed less"),
    InstructionDoc::new("Branch", "jle", "label", "Jump on signed less-or-equal"),
    InstructionDoc::new("Branch", "jg", "label", "Jump on signed greater"),
    InstructionDoc::new("Branch", "jge", "label", "Jump on signed greater-or-equal"),
    InstructionDoc::new("Branch", "jb", "label", "Jump on unsigned below (CF=1)"),
    InstructionDoc::new(
        "Branch",
        "jbe",
        "label",
        "Jump on unsigned below-or-equal (CF=1 or ZF=1)",
    ),
    InstructionDoc::new("Branch", "ja", "label", "Jump on unsigned above"),
    InstructionDoc::new(
        "Branch",
        "jae",
        "label",
        "Jump on unsigned above-or-equal (CF=0)",
    ),
    InstructionDoc::new("Branch", "js", "label", "Jump when the sign flag is set"),
    InstructionDoc::new("Branch", "jns", "label", "Jump when the sign flag is clear"),
    InstructionDoc::new("Branch", "jo", "label", "Jump on signed overflow"),
    InstructionDoc::new("Branch", "jno", "label", "Jump when no signed overflow"),
    InstructionDoc::new(
        "Call",
        "call",
        "label",
        "Push RIP and call a relative target",
    ),
    InstructionDoc::new("Call", "ret", "", "Return to the address on the stack"),
    InstructionDoc::new("System", "syscall", "", "Invoke read, write, or exit"),
    InstructionDoc::new("System", "hlt", "", "Stop the machine"),
    InstructionDoc::new("System", "nop", "", "Do nothing for one instruction"),
    // One row per directive the assembler answers to, rather than a grouped
    // `db/dw/dd/dq`: the editor looks a mnemonic up by what was typed, so a
    // grouped row documents a word nobody can write.
    InstructionDoc::new("Directive", "db", "values", "Emit initialized bytes"),
    InstructionDoc::new("Directive", "dw", "values", "Emit initialized 16-bit words"),
    InstructionDoc::new("Directive", "dd", "values", "Emit initialized 32-bit words"),
    InstructionDoc::new("Directive", "dq", "values", "Emit initialized 64-bit words"),
    InstructionDoc::new("Directive", "resb", "count", "Reserve zero-filled bytes"),
    InstructionDoc::new("Directive", "resw", "count", "Reserve zero-filled 16-bit words"),
    InstructionDoc::new("Directive", "resd", "count", "Reserve zero-filled 32-bit words"),
    InstructionDoc::new("Directive", "resq", "count", "Reserve zero-filled 64-bit words"),
];

static DESCRIPTOR: ArchitectureDescriptor = ArchitectureDescriptor {
    id: ID,
    display_name: "x86-64",
    source_extension: "asm",
    address_bits: 64,
    instruction_alignment: 1,
    default_memory_size: DEFAULT_MEMORY_SIZE,
    endianness: Endianness::Little,
    capabilities: ArchitectureCapabilities {
        elf: true,
        cache: false,
        virtual_memory: false,
        jit: false,
        pipeline: true,
        multicore: false,
        floating_point: false,
        syscalls: true,
    },
};

#[derive(Default)]
pub struct X86_64;

pub fn architecture() -> Arc<dyn Architecture> {
    static INSTANCE: OnceLock<Arc<X86_64>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(X86_64)).clone()
}

impl Architecture for X86_64 {
    fn descriptor(&self) -> &'static ArchitectureDescriptor {
        &DESCRIPTOR
    }

    fn assembler(&self) -> &'static dyn Assembler {
        &X86_64Assembler
    }

    fn default_source(&self) -> &'static str {
        r#".text
_start:
    mov rax, 1
    mov rdi, 1
    lea rsi, [rel message]
    mov rdx, 14
    syscall
    mov rax, 60
    xor rdi, rdi
    syscall
.data
message: db "Hello, World!", 10
"#
    }

    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        if memory_size == 0 {
            return Err(MachineError::new("x86-64 memory size must be non-zero"));
        }
        Ok(Box::new(X86_64Machine::new(memory_size)))
    }
}

// ---- Assembler ------------------------------------------------------------

pub struct X86_64Assembler;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Text,
    Data,
    Bss,
}

impl Section {
    fn executable(self) -> bool {
        self == Self::Text
    }

    fn writable(self) -> bool {
        self != Self::Text
    }
}

#[derive(Clone, Debug)]
enum LineKind {
    Bytes(Vec<u8>),
    Instruction(String, usize),
    Zero(usize),
}

#[derive(Clone, Debug)]
struct ParsedLine {
    source_line: usize,
    address: u64,
    section: Section,
    kind: LineKind,
}

impl Assembler for X86_64Assembler {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn instruction_forms(&self, mnemonic: &str) -> &'static [&'static str] {
        let mnemonic = mnemonic.to_ascii_lowercase();
        // Reading the branch list off the encoder keeps completion from
        // drifting behind the mnemonics the assembler really takes.
        if relative_opcode(&mnemonic).is_some() {
            return &["label"];
        }
        match mnemonic.as_str() {
            "nop" | "hlt" | "syscall" | "ret" => &[""],
            "mov" => &["reg, reg", "reg, imm", "reg, [mem]", "[mem], reg"],
            "lea" => &["reg, [mem]"],
            "add" | "sub" | "cmp" | "and" | "or" | "xor" | "test" => &["reg, reg", "reg, imm"],
            "imul" => &["reg, reg", "reg"],
            "mul" | "div" | "idiv" => &["reg"],
            "shl" | "sal" | "shr" | "sar" => &["reg, imm", "reg, cl"],
            "inc" | "dec" | "neg" | "not" | "push" | "pop" => &["reg"],
            "db" | "dw" | "dd" | "dq" => &["values"],
            "resb" | "resw" | "resd" | "resq" => &["count"],
            _ => &[],
        }
    }

    fn is_register(&self, token: &str) -> bool {
        parse_register(token).is_some() || token.eq_ignore_ascii_case("rip")
    }

    fn documented_instructions(&self) -> &'static [InstructionDoc] {
        INSTRUCTION_DOCS
    }

    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic> {
        let mut section = Section::Text;
        let mut pc = base_address;
        let mut labels = HashMap::<String, u64>::new();
        let mut labels_by_address = HashMap::<u64, Vec<String>>::new();
        let mut label_lines = HashMap::<String, usize>::new();
        let mut parsed = Vec::<ParsedLine>::new();
        let mut comments = HashMap::new();

        for (line_number, raw) in source.lines().enumerate() {
            let (code, comment) = split_comment(raw);
            let mut code = code.trim();
            if code.is_empty() {
                continue;
            }

            let lower = code.to_ascii_lowercase();
            if let Some(next) = parse_section(&lower) {
                section = next;
                continue;
            }
            if lower == "bits 64"
                || lower == "default rel"
                || lower.starts_with("global ")
                || lower.starts_with("extern ")
            {
                continue;
            }

            if let Some((label, rest)) = split_label(code) {
                validate_label(label, line_number)?;
                if labels.insert(label.to_string(), pc).is_some() {
                    return Err(Diagnostic::new(
                        Some(line_number),
                        format!("duplicate label '{label}'"),
                    ));
                }
                label_lines.insert(label.to_string(), line_number);
                labels_by_address
                    .entry(pc)
                    .or_default()
                    .push(label.to_string());
                code = rest.trim();
                if code.is_empty() {
                    continue;
                }
            }

            let kind = parse_line_kind(code, section, pc, line_number, true, &HashMap::new())?;
            let size = line_size(&kind);
            if size == 0 {
                return Err(Diagnostic::new(Some(line_number), "empty directive"));
            }
            if let Some(comment) = comment.filter(|text| !text.trim().is_empty()) {
                comments.insert(pc, comment.trim().to_string());
            }
            parsed.push(ParsedLine {
                source_line: line_number,
                address: pc,
                section,
                kind,
            });
            pc = pc.checked_add(size as u64).ok_or_else(|| {
                Diagnostic::new(Some(line_number), "program exceeds x86-64 address space")
            })?;
        }

        let mut resolved = Vec::with_capacity(parsed.len());
        let mut line_addresses = HashMap::new();
        let mut explicit_halts = Vec::new();
        for line in parsed {
            let kind = match line.kind {
                LineKind::Instruction(text, _) => {
                    let bytes =
                        encode_instruction(&text, line.address, line.source_line, false, &labels)?;
                    if bytes == [0xF4] {
                        explicit_halts.push(line.address);
                    }
                    LineKind::Bytes(bytes)
                }
                kind => kind,
            };
            line_addresses.insert(line.source_line, line.address);
            resolved.push(ParsedLine { kind, ..line });
        }

        let mut segments = Vec::<ProgramSegment>::new();
        let mut zero_fill = Vec::<ZeroFill>::new();
        for line in &resolved {
            match &line.kind {
                LineKind::Bytes(bytes) => append_segment(
                    &mut segments,
                    line.address,
                    bytes,
                    line.section.executable(),
                    line.section.writable(),
                ),
                LineKind::Zero(size) => zero_fill.push(ZeroFill {
                    address: line.address,
                    size: *size as u64,
                }),
                LineKind::Instruction(_, _) => unreachable!(),
            }
        }
        let entry = labels.get("_start").copied().unwrap_or_else(|| {
            resolved
                .iter()
                .find(|line| line.section == Section::Text)
                .map_or(base_address, |line| line.address)
        });
        let label_to_line = labels
            .keys()
            .filter_map(|label| label_lines.get(label).map(|line| (label.clone(), *line)))
            .collect();

        Ok(ProgramImage {
            architecture: ID.into(),
            entry,
            segments,
            zero_fill,
            source_map: SourceMap {
                comments,
                labels: labels_by_address,
                label_to_line,
                line_addresses,
                explicit_halts,
                ..SourceMap::default()
            },
        })
    }
}

fn parse_section(line: &str) -> Option<Section> {
    let name = line.strip_prefix("section ").unwrap_or(line).trim();
    match name {
        ".text" | "text" => Some(Section::Text),
        ".data" | "data" => Some(Section::Data),
        ".bss" | "bss" => Some(Section::Bss),
        _ => None,
    }
}

fn validate_label(label: &str, line: usize) -> Result<(), Diagnostic> {
    let mut chars = label.chars();
    let valid_first = chars
        .next()
        .is_some_and(|ch| ch == '_' || ch == '.' || ch.is_ascii_alphabetic());
    if !valid_first || !chars.all(|ch| ch == '_' || ch == '.' || ch.is_ascii_alphanumeric()) {
        return Err(Diagnostic::new(
            Some(line),
            format!("invalid x86-64 label '{label}'"),
        ));
    }
    Ok(())
}

fn split_label(line: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    for (index, ch) in line.char_indices() {
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            ':' if quote.is_none() => return Some((line[..index].trim(), &line[index + 1..])),
            _ => {}
        }
    }
    None
}

fn split_comment(line: &str) -> (&str, Option<&str>) {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            ';' | '#' if quote.is_none() => return (&line[..index], Some(&line[index + 1..])),
            _ => {}
        }
    }
    (line, None)
}

fn parse_line_kind(
    code: &str,
    section: Section,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<LineKind, Diagnostic> {
    let (mnemonic, operands) = split_mnemonic(code);
    let lower = mnemonic.to_ascii_lowercase();
    match lower.as_str() {
        "db" | ".byte" => Ok(LineKind::Bytes(parse_data(operands, 1, line)?)),
        "dw" | ".word" => Ok(LineKind::Bytes(parse_data(operands, 2, line)?)),
        "dd" | ".long" => Ok(LineKind::Bytes(parse_data(operands, 4, line)?)),
        "dq" | ".quad" => Ok(LineKind::Bytes(parse_data(operands, 8, line)?)),
        "resb" | "resw" | "resd" | "resq" => {
            let unit = match lower.as_str() {
                "resb" => 1,
                "resw" => 2,
                "resd" => 4,
                _ => 8,
            };
            let count = parse_u64(operands.trim()).ok_or_else(|| {
                Diagnostic::new(Some(line), format!("invalid reserve count '{operands}'"))
            })?;
            let size = usize::try_from(count)
                .ok()
                .and_then(|count| count.checked_mul(unit))
                .ok_or_else(|| Diagnostic::new(Some(line), "reserve size is too large"))?;
            Ok(LineKind::Zero(size))
        }
        _ if section == Section::Bss => Err(Diagnostic::new(
            Some(line),
            "only resb/resw/resd/resq are valid in .bss",
        )),
        _ => {
            let bytes = encode_instruction(code, address, line, unresolved, labels)?;
            if unresolved {
                Ok(LineKind::Instruction(code.to_string(), bytes.len()))
            } else {
                Ok(LineKind::Bytes(bytes))
            }
        }
    }
}

fn line_size(kind: &LineKind) -> usize {
    match kind {
        LineKind::Bytes(bytes) => bytes.len(),
        LineKind::Instruction(_, size) => *size,
        LineKind::Zero(size) => *size,
    }
}

fn append_segment(
    segments: &mut Vec<ProgramSegment>,
    address: u64,
    bytes: &[u8],
    executable: bool,
    writable: bool,
) {
    if let Some(last) = segments.last_mut()
        && last.executable == executable
        && last.writable == writable
        && last.address.saturating_add(last.bytes.len() as u64) == address
    {
        last.bytes.extend_from_slice(bytes);
        return;
    }
    segments.push(ProgramSegment {
        address,
        bytes: bytes.to_vec(),
        executable,
        writable,
    });
}

fn parse_data(operands: &str, width: usize, line: usize) -> Result<Vec<u8>, Diagnostic> {
    let mut bytes = Vec::new();
    for value in split_operands(operands) {
        let value = value.trim();
        if let Some(string) = parse_string(value) {
            let string = string.map_err(|message| Diagnostic::new(Some(line), message))?;
            if width != 1 {
                return Err(Diagnostic::new(
                    Some(line),
                    "strings are only valid with db/.byte",
                ));
            }
            bytes.extend_from_slice(&string);
            continue;
        }
        let number = parse_i128(value)
            .ok_or_else(|| Diagnostic::new(Some(line), format!("invalid data value '{value}'")))?;
        let max = 1i128 << (width * 8);
        if number < -(max / 2) || number >= max {
            return Err(Diagnostic::new(
                Some(line),
                format!("data value '{value}' does not fit in {} bits", width * 8),
            ));
        }
        bytes.extend_from_slice(&(number as u128).to_le_bytes()[..width]);
    }
    Ok(bytes)
}

fn parse_string(value: &str) -> Option<Result<Vec<u8>, String>> {
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') || !value.ends_with(quote) || value.len() < 2 {
        return None;
    }
    let mut bytes = Vec::new();
    let mut chars = value[1..value.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            if !ch.is_ascii() {
                return Some(Err("x86-64 string literals must be ASCII".into()));
            }
            bytes.push(ch as u8);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Some(Err("unterminated escape sequence".into()));
        };
        bytes.push(match escaped {
            'n' => b'\n',
            'r' => b'\r',
            't' => b'\t',
            '0' => 0,
            '\\' => b'\\',
            '\'' => b'\'',
            '"' => b'"',
            _ => return Some(Err(format!("unsupported escape '\\{escaped}'"))),
        });
    }
    Some(Ok(bytes))
}

fn split_mnemonic(line: &str) -> (&str, &str) {
    line.find(char::is_whitespace)
        .map_or((line, ""), |index| (&line[..index], line[index..].trim()))
}

fn split_operands(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut brackets = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            '[' if quote.is_none() => brackets += 1,
            ']' if quote.is_none() => brackets -= 1,
            ',' if quote.is_none() && brackets == 0 => {
                result.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if !text[start..].trim().is_empty() {
        result.push(text[start..].trim());
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reg {
    index: u8,
    bits: u8,
}

const REG64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];
const REG32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d",
];
const REG8: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b",
    "r13b", "r14b", "r15b",
];

fn parse_register(text: &str) -> Option<Reg> {
    let name = text.trim().to_ascii_lowercase();
    for (bits, names) in [(64, &REG64), (32, &REG32), (8, &REG8)] {
        if let Some(index) = names.iter().position(|candidate| *candidate == name) {
            return Some(Reg {
                index: index as u8,
                bits,
            });
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AddressTerm {
    Number(i64),
    Label(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MemoryOperand {
    base: Option<u8>,
    displacement: AddressTerm,
    relative: bool,
}

fn parse_memory(text: &str) -> Option<Result<MemoryOperand, String>> {
    let text = text.trim();
    let inner = text.strip_prefix('[')?.strip_suffix(']')?.trim();
    let (relative, inner) = inner
        .strip_prefix("rel ")
        .or_else(|| inner.strip_prefix("REL "))
        .map_or((false, inner), |rest| (true, rest.trim()));

    let mut split = None;
    for (index, ch) in inner.char_indices().skip(1) {
        if matches!(ch, '+' | '-') {
            split = Some((index, ch));
            break;
        }
    }
    let (head, tail) = split.map_or((inner, None), |(index, sign)| {
        (&inner[..index], Some((sign, inner[index + 1..].trim())))
    });
    let head = head.trim();
    if let Some(base) = parse_register(head) {
        if base.bits != 64 {
            return Some(Err("memory base register must be 64-bit".into()));
        }
        let displacement = match tail {
            None => 0,
            Some((sign, value)) => {
                let Some(value) = parse_i64(value) else {
                    return Some(Err(format!("invalid displacement '{value}'")));
                };
                if sign == '-' { -value } else { value }
            }
        };
        return Some(Ok(MemoryOperand {
            base: Some(base.index),
            displacement: AddressTerm::Number(displacement),
            relative: false,
        }));
    }
    if tail.is_some() {
        return Some(Err(
            "indexed/scaled memory operands are not supported".into()
        ));
    }
    let displacement = parse_i64(head)
        .map(AddressTerm::Number)
        .unwrap_or_else(|| AddressTerm::Label(head.to_string()));
    Some(Ok(MemoryOperand {
        base: None,
        displacement,
        relative: relative || !head.starts_with(|ch: char| ch.is_ascii_digit()),
    }))
}

fn parse_i128(text: &str) -> Option<i128> {
    let text = text.trim().replace('_', "");
    let (negative, raw) = text
        .strip_prefix('-')
        .map_or((false, text.as_str()), |raw| (true, raw));
    let value = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        i128::from_str_radix(hex, 16).ok()?
    } else if let Some(bin) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        i128::from_str_radix(bin, 2).ok()?
    } else {
        raw.parse().ok()?
    };
    Some(if negative { -value } else { value })
}

fn parse_i64(text: &str) -> Option<i64> {
    i64::try_from(parse_i128(text)?).ok()
}

fn parse_u64(text: &str) -> Option<u64> {
    u64::try_from(parse_i128(text)?).ok()
}

fn resolve_value(
    text: &str,
    labels: &HashMap<String, u64>,
    unresolved: bool,
    line: usize,
) -> Result<u64, Diagnostic> {
    parse_i128(text)
        .and_then(|value| {
            if value < 0 {
                i64::try_from(value).ok().map(|value| value as u64)
            } else {
                u64::try_from(value).ok()
            }
        })
        .or_else(|| labels.get(text.trim()).copied())
        .or_else(|| unresolved.then_some(0))
        .ok_or_else(|| {
            Diagnostic::new(
                Some(line),
                format!("unknown x86-64 label or value '{}'", text.trim()),
            )
        })
}

fn encode_instruction(
    text: &str,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    let error = |message: String| Diagnostic::new(Some(line), message);
    let (mnemonic, operand_text) = split_mnemonic(text);
    let mnemonic = mnemonic.to_ascii_lowercase();
    let operands = split_operands(operand_text);
    let register = |operand: &str| {
        parse_register(operand).ok_or_else(|| error(format!("invalid x86-64 register '{operand}'")))
    };
    let memory = |operand: &str| {
        parse_memory(operand)
            .ok_or_else(|| error(format!("expected memory operand, got '{operand}'")))?
            .map_err(error)
    };

    if let (Some(opcode), [target]) = (relative_opcode(&mnemonic), operands.as_slice()) {
        return encode_relative(opcode, target, address, line, unresolved, labels);
    }

    match (mnemonic.as_str(), operands.as_slice()) {
        ("nop", []) => Ok(vec![0x90]),
        ("hlt", []) | ("halt", []) => Ok(vec![0xF4]),
        ("syscall", []) => Ok(vec![0x0F, 0x05]),
        ("ret", []) => Ok(vec![0xC3]),
        ("push", [reg]) | ("pop", [reg]) => {
            let reg = register(reg)?;
            if reg.bits != 64 {
                return Err(error("push/pop require a 64-bit register".into()));
            }
            let mut out = Vec::new();
            if reg.index >= 8 {
                out.push(0x41);
            }
            out.push(if mnemonic == "push" { 0x50 } else { 0x58 } | (reg.index & 7));
            Ok(out)
        }
        ("inc", [reg]) | ("dec", [reg]) => {
            let reg = register(reg)?;
            if !matches!(reg.bits, 32 | 64) {
                return Err(error("inc/dec support 32-bit or 64-bit registers".into()));
            }
            let rex = rex(reg.bits == 64, false, false, reg.index >= 8);
            let mut out = prefix(rex);
            out.extend([
                0xFF,
                0xC0 | (u8::from(mnemonic == "dec") << 3) | (reg.index & 7),
            ]);
            Ok(out)
        }
        ("mov", [dst, src]) => encode_mov(dst, src, address, line, unresolved, labels),
        ("lea", [dst, src]) => {
            let dst = register(dst)?;
            if dst.bits != 64 {
                return Err(error("lea destination must be a 64-bit register".into()));
            }
            encode_reg_memory(0x8D, dst, memory(src)?, address, line, unresolved, labels)
        }
        (op @ ("add" | "sub" | "cmp" | "and" | "or" | "xor"), [dst, src])
            if parse_register(dst).is_some() =>
        {
            let dst = register(dst)?;
            if let Some(src) = parse_register(src) {
                if dst.bits != src.bits || !matches!(dst.bits, 32 | 64) {
                    return Err(error(
                        "binary registers must have the same 32/64-bit width".into(),
                    ));
                }
                let opcode = match op {
                    "add" => 0x01,
                    "or" => 0x09,
                    "and" => 0x21,
                    "sub" => 0x29,
                    "xor" => 0x31,
                    _ => 0x39,
                };
                Ok(encode_reg_reg(opcode, dst, src))
            } else {
                // Without this the 8-bit form would fall through to the 32-bit
                // `0x81` encoding and silently operate on the full register.
                if !matches!(dst.bits, 32 | 64) {
                    return Err(error(
                        "arithmetic immediates need a 32-bit or 64-bit register".into(),
                    ));
                }
                let immediate =
                    parse_i64(src).ok_or_else(|| error(format!("invalid immediate '{src}'")))?;
                let immediate = i32::try_from(immediate)
                    .map_err(|_| error("arithmetic immediate must fit in 32 bits".into()))?;
                // Group-1 `/digit` extensions, in the manual's order.
                let extension = match op {
                    "add" => 0,
                    "or" => 1,
                    "and" => 4,
                    "sub" => 5,
                    "xor" => 6,
                    _ => 7,
                };
                Ok(encode_reg_imm32(dst, extension, immediate))
            }
        }
        ("test", [dst, src]) => {
            let dst = register(dst)?;
            if !matches!(dst.bits, 32 | 64) {
                return Err(error("test needs a 32-bit or 64-bit register".into()));
            }
            match parse_register(src) {
                Some(src) if src.bits == dst.bits => Ok(encode_reg_reg(0x85, dst, src)),
                Some(_) => Err(error("test registers must have the same width".into())),
                None => {
                    let immediate = parse_i64(src)
                        .ok_or_else(|| error(format!("invalid immediate '{src}'")))?;
                    let immediate = i32::try_from(immediate)
                        .map_err(|_| error("test immediate must fit in 32 bits".into()))?;
                    Ok(encode_group3_imm32(dst, 0, immediate))
                }
            }
        }
        ("imul", [dst, src]) => {
            let dst = register(dst)?;
            let src = register(src)?;
            if dst.bits != src.bits || !matches!(dst.bits, 32 | 64) {
                return Err(error(
                    "binary registers must have the same 32/64-bit width".into(),
                ));
            }
            Ok(encode_imul(dst, src))
        }
        (op @ ("shl" | "sal" | "shr" | "sar"), [dst, amount]) => {
            let dst = register(dst)?;
            if !matches!(dst.bits, 32 | 64) {
                return Err(error("shifts need a 32-bit or 64-bit register".into()));
            }
            // `sal` and `shl` are the same instruction; the manual keeps both
            // names because arithmetic and logical left shifts are identical.
            let extension = match op {
                "shl" | "sal" => 4,
                "shr" => 5,
                _ => 7,
            };
            let mut out = prefix(rex(dst.bits == 64, false, false, dst.index >= 8));
            if amount.eq_ignore_ascii_case("cl") {
                out.extend([0xD3, 0xC0 | (extension << 3) | (dst.index & 7)]);
                return Ok(out);
            }
            let count = parse_i64(amount)
                .filter(|count| (0..=63).contains(count))
                .ok_or_else(|| error(format!("shift count must be cl or 0-63, got '{amount}'")))?;
            out.extend([0xC1, 0xC0 | (extension << 3) | (dst.index & 7), count as u8]);
            Ok(out)
        }
        (op @ ("neg" | "not"), [dst]) => {
            let dst = register(dst)?;
            if !matches!(dst.bits, 32 | 64) {
                return Err(error("neg/not need a 32-bit or 64-bit register".into()));
            }
            Ok(encode_group3(dst, if op == "neg" { 3 } else { 2 }))
        }
        (op @ ("mul" | "imul" | "div" | "idiv"), [src]) => {
            let src = register(src)?;
            if !matches!(src.bits, 32 | 64) {
                return Err(error("mul/div need a 32-bit or 64-bit register".into()));
            }
            let extension = match op {
                "mul" => 4,
                "imul" => 5,
                "div" => 6,
                _ => 7,
            };
            Ok(encode_group3(src, extension))
        }
        _ => Err(error(format!(
            "unsupported or invalid x86-64 instruction '{text}'"
        ))),
    }
}

/// Opcode bytes for every mnemonic that takes a single relative target.
/// Conditions list all their manual spellings, so `jc`, `jb`, and `jnae`
/// assemble to the same branch a student would find in a reference.
fn relative_opcode(mnemonic: &str) -> Option<&'static [u8]> {
    Some(match mnemonic {
        "jmp" => &[0xE9],
        "call" => &[0xE8],
        "jo" => &[0x0F, 0x80],
        "jno" => &[0x0F, 0x81],
        "jb" | "jc" | "jnae" => &[0x0F, 0x82],
        "jae" | "jnb" | "jnc" => &[0x0F, 0x83],
        "je" | "jz" => &[0x0F, 0x84],
        "jne" | "jnz" => &[0x0F, 0x85],
        "jbe" | "jna" => &[0x0F, 0x86],
        "ja" | "jnbe" => &[0x0F, 0x87],
        "js" => &[0x0F, 0x88],
        "jns" => &[0x0F, 0x89],
        "jl" | "jnge" => &[0x0F, 0x8C],
        "jge" | "jnl" => &[0x0F, 0x8D],
        "jle" | "jng" => &[0x0F, 0x8E],
        "jg" | "jnle" => &[0x0F, 0x8F],
        _ => return None,
    })
}

fn encode_relative(
    opcode: &[u8],
    target: &str,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    let next = address.wrapping_add((opcode.len() + 4) as u64);
    // On the sizing pass an unknown label is still ahead of us; branching to
    // the next instruction keeps the encoding the same length it will be once
    // the label resolves.
    let target =
        if unresolved && !labels.contains_key(target.trim()) && parse_i128(target).is_none() {
            next
        } else {
            resolve_value(target, labels, unresolved, line)?
        };
    let displacement = (target as i128) - (next as i128);
    let displacement = i32::try_from(displacement).map_err(|_| {
        Diagnostic::new(Some(line), "relative branch target is out of 32-bit range")
    })?;
    let mut out = opcode.to_vec();
    out.extend_from_slice(&displacement.to_le_bytes());
    Ok(out)
}

fn encode_mov(
    dst: &str,
    src: &str,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    let error = |message: String| Diagnostic::new(Some(line), message);
    match (
        parse_register(dst),
        parse_register(src),
        parse_memory(dst),
        parse_memory(src),
    ) {
        (Some(dst), Some(src), _, _) => {
            if dst.bits != src.bits {
                return Err(error("mov registers must have the same width".into()));
            }
            let opcode = if dst.bits == 8 { 0x88 } else { 0x89 };
            Ok(encode_reg_reg(opcode, dst, src))
        }
        (Some(dst), None, _, Some(memory)) => encode_reg_memory(
            if dst.bits == 8 { 0x8A } else { 0x8B },
            dst,
            memory.map_err(error)?,
            address,
            line,
            unresolved,
            labels,
        ),
        (None, Some(src), Some(memory), _) => encode_memory_reg(
            if src.bits == 8 { 0x88 } else { 0x89 },
            memory.map_err(error)?,
            src,
            address,
            line,
            unresolved,
            labels,
        ),
        (Some(dst), None, _, None) => {
            let value = resolve_value(src, labels, unresolved, line)?;
            let mut out = prefix(force_rex(
                rex(dst.bits == 64, false, false, dst.index >= 8),
                dst.bits == 8 && dst.index >= 4,
            ));
            out.push(if dst.bits == 8 { 0xB0 } else { 0xB8 } | (dst.index & 7));
            match dst.bits {
                8 if value <= u8::MAX as u64 || value >= i8::MIN as i64 as u64 => {
                    out.push(value as u8)
                }
                8 => return Err(error(format!("immediate {value} does not fit in 8 bits"))),
                32 if value <= u32::MAX as u64 || value >= i32::MIN as i64 as u64 => {
                    out.extend_from_slice(&(value as u32).to_le_bytes())
                }
                32 => return Err(error(format!("immediate {value} does not fit in 32 bits"))),
                64 => out.extend_from_slice(&value.to_le_bytes()),
                _ => unreachable!(),
            }
            Ok(out)
        }
        _ => Err(error(format!("invalid mov operands '{dst}, {src}'"))),
    }
}

fn rex(w: bool, r: bool, x: bool, b: bool) -> Option<u8> {
    let value = 0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b);
    (value != 0x40).then_some(value)
}

fn force_rex(rex: Option<u8>, force: bool) -> Option<u8> {
    rex.or(force.then_some(0x40))
}

fn prefix(rex: Option<u8>) -> Vec<u8> {
    rex.into_iter().collect()
}

fn encode_reg_reg(opcode: u8, dst: Reg, src: Reg) -> Vec<u8> {
    let byte_register_rex = dst.bits == 8 && (dst.index >= 4 || src.index >= 4);
    let rex = force_rex(
        rex(dst.bits == 64, src.index >= 8, false, dst.index >= 8),
        byte_register_rex,
    );
    let mut out = prefix(rex);
    out.extend([opcode, 0xC0 | ((src.index & 7) << 3) | (dst.index & 7)]);
    out
}

fn encode_reg_imm32(dst: Reg, extension: u8, immediate: i32) -> Vec<u8> {
    let mut out = prefix(rex(dst.bits == 64, false, false, dst.index >= 8));
    out.extend([0x81, 0xC0 | (extension << 3) | (dst.index & 7)]);
    out.extend_from_slice(&immediate.to_le_bytes());
    out
}

/// Group 3 (`0xF7 /digit`), which holds `test imm`, `not`, `neg`, and the
/// widening multiply/divide family.
fn encode_group3(dst: Reg, extension: u8) -> Vec<u8> {
    let mut out = prefix(rex(dst.bits == 64, false, false, dst.index >= 8));
    out.extend([0xF7, 0xC0 | (extension << 3) | (dst.index & 7)]);
    out
}

fn encode_group3_imm32(dst: Reg, extension: u8, immediate: i32) -> Vec<u8> {
    let mut out = encode_group3(dst, extension);
    out.extend_from_slice(&immediate.to_le_bytes());
    out
}

fn encode_imul(dst: Reg, src: Reg) -> Vec<u8> {
    let mut out = prefix(rex(dst.bits == 64, dst.index >= 8, false, src.index >= 8));
    out.extend([0x0F, 0xAF, 0xC0 | ((dst.index & 7) << 3) | (src.index & 7)]);
    out
}

fn encode_reg_memory(
    opcode: u8,
    reg: Reg,
    memory: MemoryOperand,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    encode_memory(opcode, reg, memory, address, line, unresolved, labels)
}

fn encode_memory_reg(
    opcode: u8,
    memory: MemoryOperand,
    reg: Reg,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    encode_memory(opcode, reg, memory, address, line, unresolved, labels)
}

fn encode_memory(
    opcode: u8,
    reg: Reg,
    memory: MemoryOperand,
    address: u64,
    line: usize,
    unresolved: bool,
    labels: &HashMap<String, u64>,
) -> Result<Vec<u8>, Diagnostic> {
    let error = |message: String| Diagnostic::new(Some(line), message);
    let b = memory.base.is_some_and(|base| base >= 8);
    let rex = force_rex(
        rex(reg.bits == 64, reg.index >= 8, false, b),
        reg.bits == 8 && reg.index >= 4,
    );
    let mut out = prefix(rex);
    out.push(opcode);

    if memory.relative {
        out.push(((reg.index & 7) << 3) | 0x05);
        let next = address.wrapping_add((out.len() + 4) as u64);
        let target = match &memory.displacement {
            AddressTerm::Number(value) => *value as u64,
            AddressTerm::Label(label) if unresolved && !labels.contains_key(label) => next,
            AddressTerm::Label(label) => resolve_value(label, labels, false, line)?,
        };
        let displacement = (target as i128) - (next as i128);
        let displacement = i32::try_from(displacement)
            .map_err(|_| error("RIP-relative target is out of 32-bit range".into()))?;
        out.extend_from_slice(&displacement.to_le_bytes());
        return Ok(out);
    }

    let displacement = match &memory.displacement {
        AddressTerm::Number(value) => *value,
        AddressTerm::Label(label) => resolve_value(label, labels, unresolved, line)? as i64,
    };
    let Some(base) = memory.base else {
        let displacement = i32::try_from(displacement)
            .map_err(|_| error("absolute memory address must fit in 32 bits".into()))?;
        out.push(((reg.index & 7) << 3) | 0x04);
        out.push(0x25);
        out.extend_from_slice(&displacement.to_le_bytes());
        return Ok(out);
    };

    let base_low = base & 7;
    let force_displacement = matches!(base_low, 5);
    let (mode, displacement_bytes): (u8, Vec<u8>) = if displacement == 0 && !force_displacement {
        (0, Vec::new())
    } else if let Ok(value) = i8::try_from(displacement) {
        (1, vec![value as u8])
    } else {
        let value = i32::try_from(displacement)
            .map_err(|_| error("memory displacement must fit in 32 bits".into()))?;
        (2, value.to_le_bytes().to_vec())
    };
    let rm = if base_low == 4 { 4 } else { base_low };
    out.push((mode << 6) | ((reg.index & 7) << 3) | rm);
    if base_low == 4 {
        out.push(0x24);
    }
    out.extend(displacement_bytes);
    Ok(out)
}

// ---- Decoder --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operand {
    Register(Reg),
    Memory(EffectiveAddress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectiveAddress {
    base: Option<u8>,
    displacement: i64,
    rip_relative: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Sub,
    Xor,
    And,
    Or,
    Cmp,
    Test,
    Imul,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShiftOp {
    Left,
    Right,
    ArithmeticRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnaryOp {
    Neg,
    Not,
}

/// The one-operand multiply and divide family, which is the reason `rax` and
/// `rdx` are special: the product is twice as wide as its inputs, and the
/// dividend is twice as wide as the divisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WideningOp {
    Mul,
    Imul,
    Div,
    Idiv,
}

/// Branch conditions, grouped the way the manual groups them: the signed
/// family reads SF/OF, the unsigned family reads CF, and the two families
/// disagree on the same `cmp` — which is the whole reason to teach both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Condition {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Below,
    BelowEqual,
    Above,
    AboveEqual,
    Sign,
    NotSign,
    Overflow,
    NotOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodedOp {
    Nop,
    Halt,
    Syscall,
    Mov {
        dst: Operand,
        src: Operand,
    },
    MovImmediate {
        dst: Reg,
        value: u64,
    },
    Lea {
        dst: Reg,
        address: EffectiveAddress,
    },
    Binary {
        op: BinaryOp,
        dst: Operand,
        src: Operand,
    },
    BinaryImmediate {
        op: BinaryOp,
        dst: Reg,
        value: i32,
    },
    Inc {
        reg: Reg,
        decrement: bool,
    },
    /// `amount: None` is the `cl` form — the count lives in a register, which
    /// is why a shift can be a data hazard on `rcx`.
    Shift {
        op: ShiftOp,
        dst: Reg,
        amount: Option<u8>,
    },
    Unary {
        op: UnaryOp,
        dst: Reg,
    },
    Widening {
        op: WideningOp,
        src: Reg,
    },
    Push(Reg),
    Pop(Reg),
    Jump(i32),
    JumpIf {
        condition: Condition,
        displacement: i32,
    },
    Call(i32),
    Ret,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Decoded {
    len: usize,
    text: String,
    class: &'static str,
    op: DecodedOp,
}

fn decode(bytes: &[u8], address: u64) -> Option<Decoded> {
    let mut cursor = 0;
    let rex = bytes
        .first()
        .copied()
        .filter(|byte| (0x40..=0x4F).contains(byte));
    if rex.is_some() {
        cursor += 1;
    }
    let opcode = *bytes.get(cursor)?;
    cursor += 1;
    let w = rex.is_some_and(|value| value & 8 != 0);
    let r = rex.is_some_and(|value| value & 4 != 0);
    let b = rex.is_some_and(|value| value & 1 != 0);

    let finish = |len, text: String, class, op| {
        Some(Decoded {
            len,
            text,
            class,
            op,
        })
    };
    match opcode {
        0x90 => finish(cursor, "nop".into(), "System", DecodedOp::Nop),
        0xF4 => finish(cursor, "hlt".into(), "System", DecodedOp::Halt),
        0xC3 => finish(cursor, "ret".into(), "Control", DecodedOp::Ret),
        0x50..=0x57 => {
            let reg = Reg {
                index: (opcode - 0x50) | (u8::from(b) << 3),
                bits: 64,
            };
            finish(
                cursor,
                format!("push {}", reg_name(reg)),
                "Stack",
                DecodedOp::Push(reg),
            )
        }
        0x58..=0x5F => {
            let reg = Reg {
                index: (opcode - 0x58) | (u8::from(b) << 3),
                bits: 64,
            };
            finish(
                cursor,
                format!("pop {}", reg_name(reg)),
                "Stack",
                DecodedOp::Pop(reg),
            )
        }
        0xB0..=0xB7 => {
            let reg = Reg {
                index: (opcode - 0xB0) | (u8::from(b) << 3),
                bits: 8,
            };
            let value = u64::from(*bytes.get(cursor)?);
            cursor += 1;
            finish(
                cursor,
                format!("mov {}, {value}", reg_name(reg)),
                "Move",
                DecodedOp::MovImmediate { dst: reg, value },
            )
        }
        0xB8..=0xBF => {
            let reg = Reg {
                index: (opcode - 0xB8) | (u8::from(b) << 3),
                bits: if w { 64 } else { 32 },
            };
            let value = if w {
                let value = u64::from_le_bytes(bytes.get(cursor..cursor + 8)?.try_into().ok()?);
                cursor += 8;
                value
            } else {
                let value = u32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
                cursor += 4;
                u64::from(value)
            };
            finish(
                cursor,
                format!("mov {}, 0x{value:X}", reg_name(reg)),
                "Move",
                DecodedOp::MovImmediate { dst: reg, value },
            )
        }
        0x88 | 0x89 | 0x8A | 0x8B | 0x8D | 0x01 | 0x09 | 0x21 | 0x29 | 0x31 | 0x39 | 0x85 => {
            let bits = if matches!(opcode, 0x88 | 0x8A) {
                8
            } else if w {
                64
            } else {
                32
            };
            let (reg, rm, next) = decode_modrm(bytes, cursor, bits, r, b)?;
            cursor = next;
            let (dst, src) = if matches!(opcode, 0x8A | 0x8B | 0x8D) {
                (Operand::Register(reg), rm)
            } else {
                (rm, Operand::Register(reg))
            };
            if opcode == 0x8D {
                let Operand::Memory(memory) = src else {
                    return None;
                };
                return finish(
                    cursor,
                    format!(
                        "lea {}, {}",
                        operand_name(dst, address, cursor),
                        memory_name(memory, address, cursor)
                    ),
                    "Address",
                    DecodedOp::Lea {
                        dst: reg,
                        address: memory,
                    },
                );
            }
            if matches!(opcode, 0x88..=0x8B) {
                return finish(
                    cursor,
                    format!(
                        "mov {}, {}",
                        operand_name(dst, address, cursor),
                        operand_name(src, address, cursor)
                    ),
                    "Move",
                    DecodedOp::Mov { dst, src },
                );
            }
            let op = match opcode {
                0x01 => BinaryOp::Add,
                0x09 => BinaryOp::Or,
                0x21 => BinaryOp::And,
                0x29 => BinaryOp::Sub,
                0x31 => BinaryOp::Xor,
                0x39 => BinaryOp::Cmp,
                0x85 => BinaryOp::Test,
                _ => unreachable!(),
            };
            finish(
                cursor,
                format!(
                    "{} {}, {}",
                    binary_name(op),
                    operand_name(dst, address, cursor),
                    operand_name(src, address, cursor)
                ),
                binary_class(op),
                DecodedOp::Binary { op, dst, src },
            )
        }
        0x81 => {
            let modrm = *bytes.get(cursor)?;
            let extension = (modrm >> 3) & 7;
            let op = match extension {
                0 => BinaryOp::Add,
                1 => BinaryOp::Or,
                4 => BinaryOp::And,
                5 => BinaryOp::Sub,
                6 => BinaryOp::Xor,
                7 => BinaryOp::Cmp,
                _ => return None,
            };
            let bits = if w { 64 } else { 32 };
            let (_, rm, next) = decode_modrm(bytes, cursor, bits, false, b)?;
            let Operand::Register(dst) = rm else {
                return None;
            };
            cursor = next;
            let value = i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
            cursor += 4;
            finish(
                cursor,
                format!("{} {}, {value}", binary_name(op), reg_name(dst)),
                binary_class(op),
                DecodedOp::BinaryImmediate { op, dst, value },
            )
        }
        0xC1 | 0xD3 => {
            let modrm = *bytes.get(cursor)?;
            let op = match (modrm >> 3) & 7 {
                4 => ShiftOp::Left,
                5 => ShiftOp::Right,
                7 => ShiftOp::ArithmeticRight,
                _ => return None,
            };
            let bits = if w { 64 } else { 32 };
            let (_, rm, next) = decode_modrm(bytes, cursor, bits, false, b)?;
            let Operand::Register(dst) = rm else {
                return None;
            };
            cursor = next;
            let amount = if opcode == 0xC1 {
                let count = *bytes.get(cursor)?;
                cursor += 1;
                Some(count)
            } else {
                None
            };
            let name = shift_name(op);
            let text = match amount {
                Some(count) => format!("{name} {}, {count}", reg_name(dst)),
                None => format!("{name} {}, cl", reg_name(dst)),
            };
            finish(cursor, text, "ALU", DecodedOp::Shift { op, dst, amount })
        }
        0xF7 => {
            let modrm = *bytes.get(cursor)?;
            let extension = (modrm >> 3) & 7;
            let bits = if w { 64 } else { 32 };
            let (_, rm, next) = decode_modrm(bytes, cursor, bits, false, b)?;
            let Operand::Register(dst) = rm else {
                return None;
            };
            cursor = next;
            match extension {
                0 => {
                    let value = i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
                    cursor += 4;
                    finish(
                        cursor,
                        format!("test {}, {value}", reg_name(dst)),
                        "Compare",
                        DecodedOp::BinaryImmediate {
                            op: BinaryOp::Test,
                            dst,
                            value,
                        },
                    )
                }
                2 | 3 => {
                    let op = if extension == 2 {
                        UnaryOp::Not
                    } else {
                        UnaryOp::Neg
                    };
                    finish(
                        cursor,
                        format!("{} {}", unary_name(op), reg_name(dst)),
                        "ALU",
                        DecodedOp::Unary { op, dst },
                    )
                }
                4..=7 => {
                    let op = match extension {
                        4 => WideningOp::Mul,
                        5 => WideningOp::Imul,
                        6 => WideningOp::Div,
                        _ => WideningOp::Idiv,
                    };
                    finish(
                        cursor,
                        format!("{} {}", widening_name(op), reg_name(dst)),
                        if matches!(op, WideningOp::Mul | WideningOp::Imul) {
                            "ALU"
                        } else {
                            "Divide"
                        },
                        DecodedOp::Widening { op, src: dst },
                    )
                }
                _ => None,
            }
        }
        0xFF => {
            let modrm = *bytes.get(cursor)?;
            let decrement = match (modrm >> 3) & 7 {
                0 => false,
                1 => true,
                _ => return None,
            };
            let bits = if w { 64 } else { 32 };
            let (_, operand, next) = decode_modrm(bytes, cursor, bits, false, b)?;
            let Operand::Register(reg) = operand else {
                return None;
            };
            cursor = next;
            finish(
                cursor,
                format!(
                    "{} {}",
                    if decrement { "dec" } else { "inc" },
                    reg_name(reg)
                ),
                "ALU",
                DecodedOp::Inc { reg, decrement },
            )
        }
        0xE8 | 0xE9 => {
            let displacement = i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
            cursor += 4;
            let target = relative_target(address, cursor, displacement);
            finish(
                cursor,
                format!(
                    "{} 0x{target:X}",
                    if opcode == 0xE8 { "call" } else { "jmp" }
                ),
                "Control",
                if opcode == 0xE8 {
                    DecodedOp::Call(displacement)
                } else {
                    DecodedOp::Jump(displacement)
                },
            )
        }
        0xEB | 0x70..=0x7F => {
            let displacement = i32::from(*bytes.get(cursor)? as i8);
            cursor += 1;
            if opcode == 0xEB {
                let target = relative_target(address, cursor, displacement);
                return finish(
                    cursor,
                    format!("jmp 0x{target:X}"),
                    "Control",
                    DecodedOp::Jump(displacement),
                );
            }
            let condition = short_condition(opcode)?;
            let target = relative_target(address, cursor, displacement);
            finish(
                cursor,
                format!("{} 0x{target:X}", condition_name(condition)),
                "Control",
                DecodedOp::JumpIf {
                    condition,
                    displacement,
                },
            )
        }
        0x0F => {
            let second = *bytes.get(cursor)?;
            cursor += 1;
            match second {
                0x05 => finish(cursor, "syscall".into(), "System", DecodedOp::Syscall),
                0xAF => {
                    let bits = if w { 64 } else { 32 };
                    let (reg, rm, next) = decode_modrm(bytes, cursor, bits, r, b)?;
                    cursor = next;
                    let dst = Operand::Register(reg);
                    finish(
                        cursor,
                        format!(
                            "imul {}, {}",
                            reg_name(reg),
                            operand_name(rm, address, cursor)
                        ),
                        "ALU",
                        DecodedOp::Binary {
                            op: BinaryOp::Imul,
                            dst,
                            src: rm,
                        },
                    )
                }
                0x80..=0x8F => {
                    let condition = near_condition(second)?;
                    let displacement =
                        i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
                    cursor += 4;
                    let target = relative_target(address, cursor, displacement);
                    finish(
                        cursor,
                        format!("{} 0x{target:X}", condition_name(condition)),
                        "Control",
                        DecodedOp::JumpIf {
                            condition,
                            displacement,
                        },
                    )
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn decode_modrm(
    bytes: &[u8],
    mut cursor: usize,
    bits: u8,
    rex_r: bool,
    rex_b: bool,
) -> Option<(Reg, Operand, usize)> {
    let modrm = *bytes.get(cursor)?;
    cursor += 1;
    let mode = modrm >> 6;
    let reg = Reg {
        index: ((modrm >> 3) & 7) | (u8::from(rex_r) << 3),
        bits,
    };
    let rm_low = modrm & 7;
    if mode == 3 {
        return Some((
            reg,
            Operand::Register(Reg {
                index: rm_low | (u8::from(rex_b) << 3),
                bits,
            }),
            cursor,
        ));
    }

    let mut base = Some(rm_low | (u8::from(rex_b) << 3));
    let mut rip_relative = false;
    if rm_low == 4 {
        let sib = *bytes.get(cursor)?;
        cursor += 1;
        if (sib >> 3) & 7 != 4 || sib >> 6 != 0 {
            return None; // scaled/indexed addressing is outside this backend's subset
        }
        let sib_base = sib & 7;
        if mode == 0 && sib_base == 5 {
            base = None;
        } else {
            base = Some(sib_base | (u8::from(rex_b) << 3));
        }
    } else if mode == 0 && rm_low == 5 {
        base = None;
        rip_relative = true;
    }
    let displacement = match mode {
        0 if base.is_none() => {
            let value = i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
            cursor += 4;
            i64::from(value)
        }
        0 => 0,
        1 => {
            let value = i64::from(*bytes.get(cursor)? as i8);
            cursor += 1;
            value
        }
        2 => {
            let value = i32::from_le_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
            cursor += 4;
            i64::from(value)
        }
        _ => return None,
    };
    Some((
        reg,
        Operand::Memory(EffectiveAddress {
            base,
            displacement,
            rip_relative,
        }),
        cursor,
    ))
}

fn reg_name(reg: Reg) -> &'static str {
    match reg.bits {
        8 => REG8[reg.index as usize],
        32 => REG32[reg.index as usize],
        _ => REG64[reg.index as usize],
    }
}

fn operand_name(operand: Operand, address: u64, len: usize) -> String {
    match operand {
        Operand::Register(reg) => reg_name(reg).into(),
        Operand::Memory(memory) => memory_name(memory, address, len),
    }
}

fn memory_name(memory: EffectiveAddress, address: u64, len: usize) -> String {
    if memory.rip_relative {
        return format!("[rel 0x{:X}]", effective_address(memory, &[], address, len));
    }
    match memory.base {
        Some(base) if memory.displacement == 0 => format!("[{}]", REG64[base as usize]),
        Some(base) if memory.displacement > 0 => {
            format!("[{}+0x{:X}]", REG64[base as usize], memory.displacement)
        }
        Some(base) => format!("[{}-0x{:X}]", REG64[base as usize], -memory.displacement),
        None => format!("[0x{:X}]", memory.displacement as u64),
    }
}

fn effective_address(memory: EffectiveAddress, registers: &[u64], address: u64, len: usize) -> u64 {
    let base = if memory.rip_relative {
        address.wrapping_add(len as u64)
    } else {
        memory
            .base
            .and_then(|index| registers.get(index as usize).copied())
            .unwrap_or(0)
    };
    base.wrapping_add_signed(memory.displacement)
}

fn relative_target(address: u64, len: usize, displacement: i32) -> u64 {
    address
        .wrapping_add(len as u64)
        .wrapping_add_signed(i64::from(displacement))
}

fn binary_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Xor => "xor",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Cmp => "cmp",
        BinaryOp::Test => "test",
        BinaryOp::Imul => "imul",
    }
}

fn shift_name(op: ShiftOp) -> &'static str {
    match op {
        ShiftOp::Left => "shl",
        ShiftOp::Right => "shr",
        ShiftOp::ArithmeticRight => "sar",
    }
}

fn unary_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
    }
}

fn widening_name(op: WideningOp) -> &'static str {
    match op {
        WideningOp::Mul => "mul",
        WideningOp::Imul => "imul",
        WideningOp::Div => "div",
        WideningOp::Idiv => "idiv",
    }
}

/// Reads `value` as a signed number `bits` wide.
fn sign_extend(value: u64, bits: u8) -> i64 {
    let shift = 64 - u32::from(bits);
    ((value << shift) as i64) >> shift
}

fn binary_class(op: BinaryOp) -> &'static str {
    if matches!(op, BinaryOp::Cmp | BinaryOp::Test) {
        "Compare"
    } else {
        "ALU"
    }
}

fn short_condition(opcode: u8) -> Option<Condition> {
    near_condition(opcode.wrapping_add(0x10))
}

fn near_condition(opcode: u8) -> Option<Condition> {
    Some(match opcode {
        0x80 => Condition::Overflow,
        0x81 => Condition::NotOverflow,
        0x82 => Condition::Below,
        0x83 => Condition::AboveEqual,
        0x84 => Condition::Equal,
        0x85 => Condition::NotEqual,
        0x86 => Condition::BelowEqual,
        0x87 => Condition::Above,
        0x88 => Condition::Sign,
        0x89 => Condition::NotSign,
        0x8C => Condition::Less,
        0x8D => Condition::GreaterEqual,
        0x8E => Condition::LessEqual,
        0x8F => Condition::Greater,
        _ => return None,
    })
}

/// The canonical spelling of a condition, used when disassembling. Aliases
/// (`jz`, `jc`, `jnae`, …) assemble but read back as the primary mnemonic.
fn condition_name(condition: Condition) -> &'static str {
    match condition {
        Condition::Equal => "je",
        Condition::NotEqual => "jne",
        Condition::Less => "jl",
        Condition::LessEqual => "jle",
        Condition::Greater => "jg",
        Condition::GreaterEqual => "jge",
        Condition::Below => "jb",
        Condition::BelowEqual => "jbe",
        Condition::Above => "ja",
        Condition::AboveEqual => "jae",
        Condition::Sign => "js",
        Condition::NotSign => "jns",
        Condition::Overflow => "jo",
        Condition::NotOverflow => "jno",
    }
}

// ---- Machine --------------------------------------------------------------

const FLAG_CF: u64 = 1 << 0;
const FLAG_ZF: u64 = 1 << 6;
const FLAG_SF: u64 = 1 << 7;
const FLAG_OF: u64 = 1 << 11;

pub struct X86_64Machine {
    registers: [u64; 16],
    rip: u64,
    rflags: u64,
    memory: Vec<u8>,
    image: Option<ProgramImage>,
    state: MachineState,
    instructions: u64,
    stdout: Vec<u8>,
    input: VecDeque<u8>,
    fault: Option<String>,
    pipeline: X86Pipeline,
}

impl X86_64Machine {
    pub fn new(memory_size: usize) -> Self {
        let mut registers = [0; 16];
        registers[4] = initial_stack_pointer(memory_size);
        Self {
            registers,
            rip: 0,
            rflags: 2,
            memory: vec![0; memory_size],
            image: None,
            state: MachineState::Ready,
            instructions: 0,
            stdout: Vec::new(),
            input: VecDeque::new(),
            fault: None,
            pipeline: X86Pipeline::new(pipeline::shape(), 0),
        }
    }

    fn range(&self, address: u64, bytes: usize) -> Result<usize, MachineError> {
        let start = usize::try_from(address).map_err(|_| MachineError::new("address overflow"))?;
        let end = start
            .checked_add(bytes)
            .ok_or_else(|| MachineError::new("address overflow"))?;
        if end > self.memory.len() {
            return Err(MachineError::new("x86-64 memory access out of bounds"));
        }
        Ok(start)
    }

    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, MachineError> {
        let message = message.into();
        self.state = MachineState::Faulted;
        self.fault = Some(message.clone());
        Err(MachineError::new(message))
    }

    fn read_reg(&self, reg: Reg) -> u64 {
        let value = self.registers[reg.index as usize];
        value & width_mask(reg.bits)
    }

    fn write_reg(&mut self, reg: Reg, value: u64) {
        let slot = &mut self.registers[reg.index as usize];
        match reg.bits {
            8 => *slot = (*slot & !0xFF) | (value & 0xFF),
            32 => *slot = value & 0xFFFF_FFFF,
            64 => *slot = value,
            _ => unreachable!(),
        }
    }

    fn read_operand_width(
        &self,
        operand: Operand,
        bits: u8,
        address: u64,
        len: usize,
    ) -> Result<u64, MachineError> {
        match operand {
            Operand::Register(reg) => Ok(self.read_reg(reg)),
            Operand::Memory(memory) => {
                let address = effective_address(memory, &self.registers, address, len);
                let bytes = usize::from(bits / 8);
                let start = self.range(address, bytes)?;
                Ok(self.memory[start..start + bytes]
                    .iter()
                    .enumerate()
                    .fold(0, |value, (index, byte)| {
                        value | (u64::from(*byte) << (index * 8))
                    }))
            }
        }
    }

    fn write_operand(
        &mut self,
        operand: Operand,
        bits: u8,
        value: u64,
        address: u64,
        len: usize,
    ) -> Result<(), MachineError> {
        match operand {
            Operand::Register(reg) => {
                self.write_reg(reg, value);
                Ok(())
            }
            Operand::Memory(memory) => {
                let address = effective_address(memory, &self.registers, address, len);
                let bytes = usize::from(bits / 8);
                let start = self.range(address, bytes)?;
                self.memory[start..start + bytes].copy_from_slice(&value.to_le_bytes()[..bytes]);
                Ok(())
            }
        }
    }

    fn push(&mut self, value: u64) -> Result<(), MachineError> {
        let rsp = self.registers[4]
            .checked_sub(8)
            .ok_or_else(|| MachineError::new("x86-64 stack overflow"))?;
        let start = self.range(rsp, 8)?;
        self.memory[start..start + 8].copy_from_slice(&value.to_le_bytes());
        self.registers[4] = rsp;
        Ok(())
    }

    fn pop(&mut self) -> Result<u64, MachineError> {
        let rsp = self.registers[4];
        let start = self.range(rsp, 8)?;
        let value = u64::from_le_bytes(self.memory[start..start + 8].try_into().unwrap());
        self.registers[4] = rsp
            .checked_add(8)
            .ok_or_else(|| MachineError::new("x86-64 stack pointer overflow"))?;
        Ok(value)
    }

    fn set_logic_flags(&mut self, value: u64, bits: u8) {
        self.rflags &= !(FLAG_CF | FLAG_ZF | FLAG_SF | FLAG_OF);
        let value = value & width_mask(bits);
        if value == 0 {
            self.rflags |= FLAG_ZF;
        }
        if value & sign_bit(bits) != 0 {
            self.rflags |= FLAG_SF;
        }
    }

    fn set_add_flags(&mut self, lhs: u64, rhs: u64, result: u64, bits: u8) {
        self.set_logic_flags(result, bits);
        let mask = width_mask(bits);
        if (lhs as u128 & mask as u128) + (rhs as u128 & mask as u128) > mask as u128 {
            self.rflags |= FLAG_CF;
        }
        let sign = sign_bit(bits);
        if (!(lhs ^ rhs) & (lhs ^ result) & sign) != 0 {
            self.rflags |= FLAG_OF;
        }
    }

    fn set_sub_flags(&mut self, lhs: u64, rhs: u64, result: u64, bits: u8) {
        self.set_logic_flags(result, bits);
        let mask = width_mask(bits);
        if lhs & mask < rhs & mask {
            self.rflags |= FLAG_CF;
        }
        let sign = sign_bit(bits);
        if ((lhs ^ rhs) & (lhs ^ result) & sign) != 0 {
            self.rflags |= FLAG_OF;
        }
    }

    fn condition(&self, condition: Condition) -> bool {
        let cf = self.rflags & FLAG_CF != 0;
        let zf = self.rflags & FLAG_ZF != 0;
        let sf = self.rflags & FLAG_SF != 0;
        let of = self.rflags & FLAG_OF != 0;
        match condition {
            Condition::Equal => zf,
            Condition::NotEqual => !zf,
            Condition::Less => sf != of,
            Condition::LessEqual => zf || sf != of,
            Condition::Greater => !zf && sf == of,
            Condition::GreaterEqual => sf == of,
            Condition::Below => cf,
            Condition::BelowEqual => cf || zf,
            Condition::Above => !cf && !zf,
            Condition::AboveEqual => !cf,
            Condition::Sign => sf,
            Condition::NotSign => !sf,
            Condition::Overflow => of,
            Condition::NotOverflow => !of,
        }
    }

    fn syscall(&mut self, instruction_address: u64) -> Result<StepOutcome, MachineError> {
        match self.registers[0] {
            0 => {
                let count = usize::try_from(self.registers[2]).unwrap_or(usize::MAX);
                if self.registers[7] != 0 {
                    self.registers[0] = u64::MAX; // -EPERM
                    return Ok(StepOutcome::Stepped);
                }
                if self.input.is_empty() {
                    self.rip = instruction_address;
                    self.instructions = self.instructions.saturating_sub(1);
                    self.state = MachineState::AwaitingInput;
                    return Ok(StepOutcome::AwaitingInput);
                }
                let count = count.min(self.input.len());
                let start = self.range(self.registers[6], count)?;
                for slot in &mut self.memory[start..start + count] {
                    *slot = self.input.pop_front().unwrap();
                }
                self.registers[0] = count as u64;
                Ok(StepOutcome::Stepped)
            }
            1 => {
                let count = usize::try_from(self.registers[2]).unwrap_or(usize::MAX);
                let start = self.range(self.registers[6], count)?;
                if matches!(self.registers[7], 1 | 2) {
                    self.stdout
                        .extend_from_slice(&self.memory[start..start + count]);
                    self.registers[0] = count as u64;
                } else {
                    self.registers[0] = u64::MAX; // -EPERM
                }
                Ok(StepOutcome::Stepped)
            }
            60 => {
                let code = self.registers[7] as i32;
                self.state = MachineState::Exited(code);
                Ok(StepOutcome::Exited(code))
            }
            number => self.fail(format!("unsupported x86-64 syscall {number}")),
        }
    }

    fn pipeline_instruction(&self, address: u64) -> X86Op {
        let Ok(start) = usize::try_from(address) else {
            return pipeline::fault_op(address, "x86-64 fetch address overflow");
        };
        let Some(bytes) = self
            .memory
            .get(start..start.saturating_add(15).min(self.memory.len()))
        else {
            return pipeline::fault_op(address, "x86-64 fetch out of bounds");
        };
        decode(bytes, address).map_or_else(
            || {
                pipeline::fault_op(
                    address,
                    format!(
                        "unsupported x86-64 opcode 0x{:02X} at 0x{address:016X}",
                        bytes.first().copied().unwrap_or(0)
                    ),
                )
            },
            |decoded| pipeline::op(address, decoded),
        )
    }
}

fn width_mask(bits: u8) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn initial_stack_pointer(memory_size: usize) -> u64 {
    (memory_size.saturating_sub(32) as u64) & !0xF
}

fn sign_bit(bits: u8) -> u64 {
    1u64 << (bits - 1)
}

impl Machine for X86_64Machine {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn reset(&mut self) {
        let image = self.image.clone();
        let size = self.memory.len();
        let pipeline_enabled = self.pipeline.enabled();
        *self = Self::new(size);
        self.pipeline.set_enabled(pipeline_enabled);
        if let Some(image) = image
            && let Err(error) = self.load(&image)
        {
            self.state = MachineState::Faulted;
            self.fault = Some(error.to_string());
        }
    }

    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError> {
        image.expect_architecture(ID)?;
        for segment in &image.segments {
            self.range(segment.address, segment.bytes.len())?;
        }
        let fills = image
            .zero_fill
            .iter()
            .map(|fill| {
                let size = usize::try_from(fill.size)
                    .map_err(|_| MachineError::new("zero-fill is too large"))?;
                Ok((self.range(fill.address, size)?, size))
            })
            .collect::<Result<Vec<_>, MachineError>>()?;
        self.range(image.entry, 1)?;

        self.memory.fill(0);
        for segment in &image.segments {
            let start = self.range(segment.address, segment.bytes.len())?;
            self.memory[start..start + segment.bytes.len()].copy_from_slice(&segment.bytes);
        }
        for (start, size) in fills {
            self.memory[start..start + size].fill(0);
        }
        self.registers = [0; 16];
        self.registers[4] = initial_stack_pointer(self.memory.len());
        self.rip = image.entry;
        self.rflags = 2;
        self.image = Some(image.clone());
        self.state = MachineState::Ready;
        self.instructions = 0;
        self.stdout.clear();
        self.input.clear();
        self.fault = None;
        self.pipeline.reset(image.entry);
        Ok(())
    }

    fn step(&mut self) -> Result<StepOutcome, MachineError> {
        if self.pipeline.enabled() {
            loop {
                let cycle = self.cycle()?;
                if cycle.retired_address.is_some() || cycle.outcome != StepOutcome::Stepped {
                    return Ok(cycle.outcome);
                }
            }
        }
        if matches!(
            self.state,
            MachineState::Halted | MachineState::Exited(_) | MachineState::Faulted
        ) {
            return Err(MachineError::new("machine is not runnable; reset it first"));
        }
        let instruction_address = self.rip;
        let start = self.range(instruction_address, 1)?;
        let end = start.saturating_add(15).min(self.memory.len());
        let Some(instruction) = decode(&self.memory[start..end], instruction_address) else {
            return self.fail(format!(
                "unsupported x86-64 opcode 0x{:02X} at 0x{instruction_address:016X}",
                self.memory[start]
            ));
        };
        self.rip = self.rip.wrapping_add(instruction.len as u64);
        self.instructions += 1;
        self.state = MachineState::Running;

        let outcome = match instruction.op {
            DecodedOp::Nop => StepOutcome::Stepped,
            DecodedOp::Halt => {
                self.rip = instruction_address;
                self.state = MachineState::Halted;
                StepOutcome::Halted
            }
            DecodedOp::Syscall => return self.syscall(instruction_address),
            DecodedOp::MovImmediate { dst, value } => {
                self.write_reg(dst, value);
                StepOutcome::Stepped
            }
            DecodedOp::Mov { dst, src } => {
                let bits = match (dst, src) {
                    (Operand::Register(reg), _) | (_, Operand::Register(reg)) => reg.bits,
                    _ => unreachable!(),
                };
                let value =
                    self.read_operand_width(src, bits, instruction_address, instruction.len)?;
                self.write_operand(dst, bits, value, instruction_address, instruction.len)?;
                StepOutcome::Stepped
            }
            DecodedOp::Lea { dst, address } => {
                let value = effective_address(
                    address,
                    &self.registers,
                    instruction_address,
                    instruction.len,
                );
                self.write_reg(dst, value);
                StepOutcome::Stepped
            }
            DecodedOp::Binary { op, dst, src } => {
                let bits = match dst {
                    Operand::Register(reg) => reg.bits,
                    Operand::Memory(_) => match src {
                        Operand::Register(reg) => reg.bits,
                        _ => unreachable!(),
                    },
                };
                let lhs =
                    self.read_operand_width(dst, bits, instruction_address, instruction.len)?;
                let rhs =
                    self.read_operand_width(src, bits, instruction_address, instruction.len)?;
                let result = match op {
                    BinaryOp::Add => lhs.wrapping_add(rhs),
                    BinaryOp::Sub | BinaryOp::Cmp => lhs.wrapping_sub(rhs),
                    BinaryOp::Xor => lhs ^ rhs,
                    BinaryOp::And | BinaryOp::Test => lhs & rhs,
                    BinaryOp::Or => lhs | rhs,
                    BinaryOp::Imul => lhs.wrapping_mul(rhs),
                } & width_mask(bits);
                match op {
                    BinaryOp::Add => self.set_add_flags(lhs, rhs, result, bits),
                    BinaryOp::Sub | BinaryOp::Cmp => self.set_sub_flags(lhs, rhs, result, bits),
                    _ => self.set_logic_flags(result, bits),
                }
                if !matches!(op, BinaryOp::Cmp | BinaryOp::Test) {
                    self.write_operand(dst, bits, result, instruction_address, instruction.len)?;
                }
                StepOutcome::Stepped
            }
            DecodedOp::BinaryImmediate { op, dst, value } => {
                let lhs = self.read_reg(dst);
                let rhs = if dst.bits == 64 {
                    (i64::from(value)) as u64
                } else {
                    value as u32 as u64
                };
                let result = match op {
                    BinaryOp::Add => lhs.wrapping_add(rhs),
                    BinaryOp::Sub | BinaryOp::Cmp => lhs.wrapping_sub(rhs),
                    BinaryOp::And | BinaryOp::Test => lhs & rhs,
                    BinaryOp::Or => lhs | rhs,
                    BinaryOp::Xor => lhs ^ rhs,
                    BinaryOp::Imul => lhs.wrapping_mul(rhs),
                } & width_mask(dst.bits);
                match op {
                    BinaryOp::Add => self.set_add_flags(lhs, rhs, result, dst.bits),
                    BinaryOp::Sub | BinaryOp::Cmp => self.set_sub_flags(lhs, rhs, result, dst.bits),
                    _ => self.set_logic_flags(result, dst.bits),
                }
                if !matches!(op, BinaryOp::Cmp | BinaryOp::Test) {
                    self.write_reg(dst, result);
                }
                StepOutcome::Stepped
            }
            DecodedOp::Shift { op, dst, amount } => {
                // The count is masked to the operand width — 5 bits for a
                // 32-bit shift, 6 for a 64-bit one — and a masked count of
                // zero leaves every flag untouched.
                let mask = if dst.bits == 64 { 63 } else { 31 };
                let count = u32::from(match amount {
                    Some(count) => count,
                    None => self.registers[1] as u8,
                }) & mask;
                if count == 0 {
                    return Ok(StepOutcome::Stepped);
                }
                let bits = dst.bits;
                let value = self.read_reg(dst);
                let width = u32::from(bits);
                let (result, carry) = match op {
                    ShiftOp::Left => (value << count, (value >> (width - count)) & 1),
                    ShiftOp::Right => (value >> count, (value >> (count - 1)) & 1),
                    ShiftOp::ArithmeticRight => {
                        let signed = sign_extend(value, bits);
                        (
                            (signed >> count) as u64,
                            ((signed >> (count - 1)) & 1) as u64,
                        )
                    }
                };
                let result = result & width_mask(bits);
                // Overflow is architecturally defined for single-bit shifts
                // only; wider counts leave it cleared here.
                let overflow = count == 1
                    && match op {
                        ShiftOp::Left => (result & sign_bit(bits) != 0) != (carry != 0),
                        ShiftOp::Right => value & sign_bit(bits) != 0,
                        ShiftOp::ArithmeticRight => false,
                    };
                self.set_logic_flags(result, bits);
                if carry != 0 {
                    self.rflags |= FLAG_CF;
                }
                if overflow {
                    self.rflags |= FLAG_OF;
                }
                self.write_reg(dst, result);
                StepOutcome::Stepped
            }
            DecodedOp::Widening { op, src } => {
                let bits = src.bits;
                let mask = width_mask(bits);
                let half = u32::from(bits);
                let low = Reg { index: 0, bits };
                let high = Reg { index: 2, bits };
                let accumulator = self.registers[0] & mask;
                let value = self.read_reg(src);
                match op {
                    WideningOp::Mul | WideningOp::Imul => {
                        // The product is twice as wide as its operands, so it
                        // lands in rdx:rax rather than in one register.
                        let (lower, upper, overflow) = if op == WideningOp::Mul {
                            let product = u128::from(accumulator) * u128::from(value);
                            let upper = ((product >> half) as u64) & mask;
                            ((product as u64) & mask, upper, upper != 0)
                        } else {
                            let product = i128::from(sign_extend(accumulator, bits))
                                * i128::from(sign_extend(value, bits));
                            let lower = (product as u64) & mask;
                            // Signed overflow means the high half held more
                            // than a sign extension of the low half.
                            let overflow = i128::from(sign_extend(lower, bits)) != product;
                            (lower, ((product >> half) as u64) & mask, overflow)
                        };
                        self.write_reg(low, lower);
                        self.write_reg(high, upper);
                        // CF and OF report "the answer needed the high half",
                        // which is how you check for overflow after a multiply.
                        self.rflags &= !(FLAG_CF | FLAG_OF);
                        if overflow {
                            self.rflags |= FLAG_CF | FLAG_OF;
                        }
                    }
                    WideningOp::Div | WideningOp::Idiv => {
                        if value == 0 {
                            return self.fail("x86-64 divide by zero");
                        }
                        let dividend = (u128::from(self.registers[2] & mask) << half)
                            | u128::from(accumulator);
                        let (quotient, remainder) = if op == WideningOp::Div {
                            let divisor = u128::from(value);
                            (dividend / divisor, dividend % divisor)
                        } else {
                            let dividend = if bits == 64 {
                                dividend as i128
                            } else {
                                i128::from(dividend as u64 as i64)
                            };
                            let divisor = i128::from(sign_extend(value, bits));
                            let (Some(quotient), Some(remainder)) =
                                (dividend.checked_div(divisor), dividend.checked_rem(divisor))
                            else {
                                return self.fail("x86-64 divide overflow");
                            };
                            (quotient as u128, remainder as u128)
                        };
                        // A quotient too wide for one register is a fault on
                        // real hardware, not a truncated answer.
                        let fits = if op == WideningOp::Div {
                            quotient <= u128::from(mask)
                        } else {
                            let quotient = quotient as i128;
                            let limit = 1i128 << (bits - 1);
                            (-limit..limit).contains(&quotient)
                        };
                        if !fits {
                            return self.fail("x86-64 divide overflow: quotient does not fit");
                        }
                        self.write_reg(low, quotient as u64 & mask);
                        self.write_reg(high, remainder as u64 & mask);
                    }
                }
                StepOutcome::Stepped
            }
            DecodedOp::Unary { op, dst } => {
                let value = self.read_reg(dst);
                let result = match op {
                    // `not` is the one ALU operation that touches no flags.
                    UnaryOp::Not => !value & width_mask(dst.bits),
                    UnaryOp::Neg => {
                        let result = 0u64.wrapping_sub(value) & width_mask(dst.bits);
                        self.set_sub_flags(0, value, result, dst.bits);
                        result
                    }
                };
                self.write_reg(dst, result);
                StepOutcome::Stepped
            }
            DecodedOp::Inc { reg, decrement } => {
                let old_cf = self.rflags & FLAG_CF;
                let lhs = self.read_reg(reg);
                let result = if decrement {
                    lhs.wrapping_sub(1)
                } else {
                    lhs.wrapping_add(1)
                } & width_mask(reg.bits);
                if decrement {
                    self.set_sub_flags(lhs, 1, result, reg.bits);
                } else {
                    self.set_add_flags(lhs, 1, result, reg.bits);
                }
                self.rflags = (self.rflags & !FLAG_CF) | old_cf;
                self.write_reg(reg, result);
                StepOutcome::Stepped
            }
            DecodedOp::Push(reg) => {
                self.push(self.read_reg(reg))?;
                StepOutcome::Stepped
            }
            DecodedOp::Pop(reg) => {
                let value = self.pop()?;
                self.write_reg(reg, value);
                StepOutcome::Stepped
            }
            DecodedOp::Jump(displacement) => {
                self.rip = self.rip.wrapping_add_signed(i64::from(displacement));
                StepOutcome::Stepped
            }
            DecodedOp::JumpIf {
                condition,
                displacement,
            } => {
                if self.condition(condition) {
                    self.rip = self.rip.wrapping_add_signed(i64::from(displacement));
                }
                StepOutcome::Stepped
            }
            DecodedOp::Call(displacement) => {
                self.push(self.rip)?;
                self.rip = self.rip.wrapping_add_signed(i64::from(displacement));
                StepOutcome::Stepped
            }
            DecodedOp::Ret => {
                self.rip = self.pop()?;
                StepOutcome::Stepped
            }
        };
        Ok(outcome)
    }

    fn cycle(&mut self) -> Result<CycleResult, MachineError> {
        if matches!(
            self.state,
            MachineState::Halted | MachineState::Exited(_) | MachineState::Faulted
        ) {
            return Err(MachineError::new("machine is not runnable; reset it first"));
        }
        self.state = MachineState::Running;
        let retiring = self.pipeline.start_cycle();
        let mut result = CycleResult {
            retired_address: None,
            outcome: StepOutcome::Stepped,
        };
        let mut redirect = None;

        if let Some(instruction) = retiring {
            if let Some(message) = instruction.fault.clone() {
                self.pipeline.fault(message.clone());
                return self.fail(message);
            }
            let address = instruction.address;
            let pipeline_enabled = self.pipeline.enabled();
            self.pipeline.set_enabled(false);
            self.rip = address;
            let execution = self.step();
            self.pipeline.set_enabled(pipeline_enabled);
            let outcome = match execution {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.pipeline.fault(error.to_string());
                    return Err(error);
                }
            };
            if outcome == StepOutcome::AwaitingInput {
                self.pipeline.retry(instruction, "awaiting input");
                return Ok(CycleResult {
                    retired_address: None,
                    outcome,
                });
            }
            self.pipeline.retire(&instruction);
            result = CycleResult {
                retired_address: Some(address),
                outcome,
            };
            if matches!(outcome, StepOutcome::Halted | StepOutcome::Exited(_)) {
                self.pipeline.halt();
                return Ok(result);
            }
            redirect = self.pipeline.resolve(&instruction, self.rip);
        }

        if let Some(address) = self.pipeline.advance(redirect) {
            let instruction = self.pipeline_instruction(address);
            self.pipeline.fetched(instruction);
        }
        Ok(result)
    }

    fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            architecture: ID,
            pc: self.rip,
            registers: RegisterFile::entries(self)
                .into_iter()
                .map(|entry| RegisterValue {
                    name: entry.name,
                    value: entry.value,
                    bits: entry.bits,
                })
                .collect(),
            state: self.state,
            instructions: self.instructions,
            stdout: self.stdout.clone(),
            fault: self.fault.clone(),
        }
    }

    fn read_memory(&self, address: u64, bytes: usize) -> Result<Vec<u8>, MachineError> {
        let start = self.range(address, bytes)?;
        Ok(self.memory[start..start + bytes].to_vec())
    }

    fn write_memory(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        let start = self.range(address, bytes.len())?;
        self.memory[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError> {
        if name.eq_ignore_ascii_case("rip") || name.eq_ignore_ascii_case("pc") {
            return self.set_program_counter(value);
        }
        let id = RegisterFile::resolve(self, name)
            .ok_or_else(|| MachineError::new(format!("unknown x86-64 register '{name}'")))?;
        RegisterFile::write(self, id, value)
    }

    fn push_input(&mut self, line: &str) {
        self.input.extend(line.as_bytes());
        self.input.push_back(b'\n');
        if self.state == MachineState::AwaitingInput {
            self.state = MachineState::Ready;
        }
    }

    fn registers(&self) -> Option<&dyn RegisterFile> {
        Some(self)
    }

    fn registers_mut(&mut self) -> Option<&mut dyn RegisterFile> {
        Some(self)
    }

    fn memory(&self) -> Option<&dyn MemoryInspect> {
        Some(self)
    }

    fn memory_mut(&mut self) -> Option<&mut dyn MemoryInspect> {
        Some(self)
    }

    fn code(&self) -> Option<&dyn InstructionCodec> {
        Some(&X86_64Codec)
    }

    fn pipeline(&self) -> Option<&dyn PipelineInspect> {
        Some(&self.pipeline)
    }

    fn pipeline_control(&mut self) -> Option<&mut dyn PipelineControl> {
        Some(&mut self.pipeline)
    }

    fn pipeline_tuning(&self) -> Option<&dyn PipelineTuning> {
        Some(&self.pipeline)
    }

    fn pipeline_tuning_mut(&mut self) -> Option<&mut dyn PipelineTuning> {
        Some(&mut self.pipeline)
    }
}

static BANKS: [RegisterBank; 2] = [
    RegisterBank::integer("r", "General purpose", 16, 64),
    RegisterBank::integer("rflags", "Flags", 1, 64),
];

impl RegisterFile for X86_64Machine {
    fn banks(&self) -> &[RegisterBank] {
        &BANKS
    }

    fn read(&self, id: RegisterId) -> Option<u64> {
        match (id.bank, id.index) {
            (0, index) => self.registers.get(index).copied(),
            (1, 0) => Some(self.rflags),
            _ => None,
        }
    }

    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError> {
        match (id.bank, id.index) {
            (0, index) if index < self.registers.len() => {
                self.registers[index] = value;
                Ok(())
            }
            // Bit 1 of RFLAGS reads as one on real hardware, so a value
            // without it is not a state this machine can be in. Refuse it
            // rather than store something other than what was asked for —
            // the same stance RV32 takes on its hard-wired `x0`.
            (1, 0) if value & 2 == 0 => Err(MachineError::new(format!(
                "rflags bit 1 is always set; write 0x{:X} instead",
                value | 2
            ))),
            (1, 0) => {
                self.rflags = value;
                Ok(())
            }
            _ => Err(MachineError::new("no such x86-64 register")),
        }
    }

    fn program_counter(&self) -> u64 {
        self.rip
    }

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError> {
        self.range(value, 1)?;
        self.rip = value;
        Ok(())
    }

    fn resolve(&self, name: &str) -> Option<RegisterId> {
        let name = name.trim();
        REG64
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
            .map(|index| RegisterId::new(0, index))
            .or_else(|| {
                name.eq_ignore_ascii_case("rflags")
                    .then(|| RegisterId::new(1, 0))
            })
    }

    fn name(&self, id: RegisterId) -> Option<String> {
        match (id.bank, id.index) {
            (0, index) => REG64.get(index).map(|name| (*name).to_string()),
            (1, 0) => Some("rflags".into()),
            _ => None,
        }
    }
}

impl MemoryInspect for X86_64Machine {
    fn size(&self) -> u64 {
        self.memory.len() as u64
    }

    fn peek(&self, address: u64, bytes: usize) -> Vec<u8> {
        let Ok(start) = usize::try_from(address) else {
            return Vec::new();
        };
        let end = start.saturating_add(bytes).min(self.memory.len());
        self.memory.get(start..end).unwrap_or_default().to_vec()
    }

    fn poke(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        self.write_memory(address, bytes)
    }

    fn regions(&self) -> Vec<MemoryRegion> {
        let mut regions = vec![MemoryRegion::new(
            "Code",
            self.image.as_ref().map_or(0, |image| image.entry),
        )];
        if let Some(data) = self.image.as_ref().and_then(ProgramImage::data_segment) {
            regions.push(MemoryRegion::new("Data", data.address));
        }
        regions.push(MemoryRegion::new(
            "Stack",
            self.memory.len().saturating_sub(1) as u64,
        ));
        regions
    }
}

// ---- Codec ----------------------------------------------------------------

pub struct X86_64Codec;

impl InstructionCodec for X86_64Codec {
    /// x86-64 caps an instruction at fifteen bytes; the longest this backend
    /// emits is the ten-byte `movabs`.
    fn max_instruction_bytes(&self) -> usize {
        15
    }

    fn instruction_width(&self, address: u64, bytes: &[u8]) -> usize {
        decode(bytes, address).map_or(1, |instruction| instruction.len)
    }

    fn disassemble(&self, address: u64, bytes: &[u8]) -> Option<String> {
        decode(bytes, address).map(|instruction| instruction.text)
    }

    /// Route through the full assembler, as every other backend does.
    ///
    /// Encoding the line on its own cannot see labels or directives, so an
    /// inline edit of `jmp done` failed where the same text assembles fine in
    /// the editor.
    fn assemble(&self, address: u64, text: &str) -> Result<Vec<u8>, Diagnostic> {
        Ok(X86_64Assembler
            .assemble(text, address)?
            .executable_bytes()
            .map(<[u8]>::to_vec)
            .unwrap_or_default())
    }

    fn inspect(&self, address: u64, bytes: &[u8]) -> Option<InstructionInfo> {
        let instruction = decode(bytes, address)?;
        let shown = instruction.len.min(8);
        let encoding = bytes
            .get(..shown)?
            .iter()
            .enumerate()
            .fold(0u64, |word, (index, byte)| {
                word | (u64::from(*byte) << (index * 8))
            });
        let encoding_bits = (shown * 8) as u8;
        Some(InstructionInfo {
            mnemonic: instruction.text,
            class: instruction.class,
            encoding,
            encoding_bits,
            fields: vec![InstructionField {
                name: "length",
                value: format!(
                    "{} byte{}",
                    instruction.len,
                    if instruction.len == 1 { "" } else { "s" }
                ),
            }],
            layout: vec![InstructionBitField::new(
                "encoding",
                encoding_bits,
                BitRole::Opcode,
            )],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_program_prints_and_exits() {
        let architecture = architecture();
        let image = architecture
            .assembler()
            .assemble(architecture.default_source(), 0)
            .unwrap();
        let mut machine = architecture.create_machine(DEFAULT_MEMORY_SIZE).unwrap();
        machine.load(&image).unwrap();
        assert_eq!(machine.run(32).unwrap(), StepOutcome::Exited(0));
        assert_eq!(machine.snapshot().stdout, b"Hello, World!\n");
    }

    #[test]
    fn arithmetic_branches_and_stack_execute() {
        let source = r#"
            mov rax, 40
            mov rbx, 2
            add rax, rbx
            push rax
            pop rcx
            cmp rcx, 42
            jne bad
            hlt
        bad:
            mov rcx, 0
            hlt
        "#;
        let image = X86_64Assembler.assemble(source, 0).unwrap();
        let mut machine = X86_64Machine::new(4096);
        machine.load(&image).unwrap();
        assert_eq!(machine.run(16).unwrap(), StepOutcome::Halted);
        assert_eq!(machine.snapshot().registers[1].value, 42);
    }

    /// The point of shipping both branch families: `-1` is below `1` when read
    /// as signed and above it when read as unsigned, off the very same `cmp`.
    #[test]
    fn signed_and_unsigned_branches_disagree_on_one_compare() {
        let run = |branch: &str| {
            let source = format!(
                "
                mov rax, 0
                mov rbx, -1
                cmp rbx, 1
                {branch} taken
                hlt
            taken:
                mov rax, 1
                hlt
            "
            );
            let image = X86_64Assembler.assemble(&source, 0).unwrap();
            let mut machine = X86_64Machine::new(4096);
            machine.load(&image).unwrap();
            assert_eq!(machine.run(16).unwrap(), StepOutcome::Halted);
            machine.snapshot().registers[0].value
        };

        assert_eq!(run("jl"), 1, "-1 is signed-less than 1");
        assert_eq!(run("ja"), 1, "0xFFFF... is unsigned-above 1");
        assert_eq!(run("jb"), 0, "unsigned -1 is not below 1");
        assert_eq!(run("jge"), 0, "-1 is not signed-greater-or-equal to 1");
        assert_eq!(run("js"), 1, "the difference is negative");
        assert_eq!(run("jo"), 0, "-1 minus 1 does not overflow");
    }

    #[test]
    fn branch_aliases_share_an_encoding_and_read_back_canonically() {
        for (alias, canonical) in [
            ("jc", "jb"),
            ("jnae", "jb"),
            ("jnb", "jae"),
            ("jna", "jbe"),
            ("jnbe", "ja"),
            ("jz", "je"),
            ("jnge", "jl"),
            ("jnle", "jg"),
        ] {
            let bytes = X86_64Codec.assemble(0, &format!("{alias} 0x40")).unwrap();
            assert_eq!(
                bytes,
                X86_64Codec
                    .assemble(0, &format!("{canonical} 0x40"))
                    .unwrap(),
                "{alias} should encode like {canonical}"
            );
            assert_eq!(
                X86_64Codec.disassemble(0, &bytes).as_deref(),
                Some(format!("{canonical} 0x40").as_str())
            );
        }
    }

    /// Assembles a snippet, runs it to `hlt`, and hands back the machine so a
    /// test can look at both registers and flags.
    fn run_snippet(source: &str) -> X86_64Machine {
        let image = X86_64Assembler.assemble(source, 0).unwrap();
        let mut machine = X86_64Machine::new(4096);
        machine.load(&image).unwrap();
        assert_eq!(machine.run(64).unwrap(), StepOutcome::Halted);
        machine
    }

    /// `shr` and `sar` differ on exactly one thing — what fills the vacated
    /// high bits — and that difference is the reason both exist.
    #[test]
    fn logical_and_arithmetic_right_shifts_disagree_on_sign() {
        let machine = run_snippet(
            "
            mov rax, -16
            mov rbx, -16
            shr rax, 2
            sar rbx, 2
            hlt
            ",
        );
        assert_eq!(
            machine.registers[0], 0x3FFF_FFFF_FFFF_FFFC,
            "shr zero-fills"
        );
        assert_eq!(machine.registers[3] as i64, -4, "sar keeps the sign");
    }

    #[test]
    fn shifts_carry_the_last_bit_out_and_mask_their_count() {
        // Shifting 0b1001 right by one drops a set bit into CF.
        let machine = run_snippet("mov eax, 9\nshr eax, 1\nhlt\n");
        assert_eq!(machine.registers[0], 4);
        assert!(machine.rflags & FLAG_CF != 0, "the dropped 1 lands in CF");

        // A 32-bit shift masks its count to five bits, so 33 shifts by one.
        let machine = run_snippet("mov eax, 8\nshl eax, 33\nhlt\n");
        assert_eq!(machine.registers[0], 16, "count masked to 5 bits");

        // A masked count of zero is a true no-op, flags included.
        let machine = run_snippet("mov eax, 9\nshr eax, 1\nshl eax, 32\nhlt\n");
        assert!(
            machine.rflags & FLAG_CF != 0,
            "a zero-count shift leaves CF from the previous shift alone"
        );

        // The cl form reads rcx rather than an encoded immediate.
        let machine = run_snippet("mov rax, 1\nmov rcx, 5\nshl rax, cl\nhlt\n");
        assert_eq!(machine.registers[0], 32);
    }

    #[test]
    fn neg_sets_carry_unless_the_operand_is_zero_and_not_leaves_flags_alone() {
        let machine = run_snippet("mov rax, 5\nneg rax\nhlt\n");
        assert_eq!(machine.registers[0] as i64, -5);
        assert!(machine.rflags & FLAG_CF != 0, "negating non-zero sets CF");

        let machine = run_snippet("mov rax, 0\nneg rax\nhlt\n");
        assert_eq!(machine.registers[0], 0);
        assert!(machine.rflags & FLAG_CF == 0, "negating zero clears CF");
        assert!(machine.rflags & FLAG_ZF != 0);

        // `cmp` leaves ZF set; `not` must not disturb it.
        let machine = run_snippet("mov rax, 7\ncmp rax, 7\nnot rax\nhlt\n");
        assert_eq!(machine.registers[0], !7u64);
        assert!(machine.rflags & FLAG_ZF != 0, "not touches no flags");
    }

    #[test]
    fn logic_immediates_round_trip_and_execute() {
        for (source, expected) in [("and rax, 12", 8u64), ("or rax, 3", 11), ("xor rax, 10", 0)] {
            let machine = run_snippet(&format!("mov rax, 10\n{source}\nhlt\n"));
            assert_eq!(machine.registers[0], expected, "{source}");
        }

        // `test` compares without storing, exactly like `cmp`.
        let machine = run_snippet("mov rax, 8\ntest rax, 4\nhlt\n");
        assert_eq!(machine.registers[0], 8, "test does not write back");
        assert!(machine.rflags & FLAG_ZF != 0, "8 and 4 share no bits");

        for text in ["and eax, 12", "or eax, 3", "xor eax, 10", "test eax, 4"] {
            let bytes = X86_64Codec.assemble(0, text).unwrap();
            assert_eq!(
                X86_64Codec.disassemble(0, &bytes).as_deref(),
                Some(text),
                "{text} should survive a round trip"
            );
        }
    }

    /// Walks a byte bit by bit — the exercise shifts exist for. Worth one
    /// end-to-end test because it leans on shifts, immediate `and`, `dec`,
    /// a byte store, and a syscall all at once.
    #[test]
    fn shifting_a_byte_out_bit_by_bit_prints_its_binary_form() {
        let source = r#".text
_start:
    mov rbx, 0xA5
    mov rcx, 8
bit_loop:
    mov rax, rbx
    shr rax, 7
    and rax, 1
    add rax, 48
    mov [rel slot], al
    mov rax, 1
    mov rdi, 1
    lea rsi, [rel slot]
    mov rdx, 1
    syscall
    shl rbx, 1
    dec rcx
    jne bit_loop
    mov rax, 60
    xor rdi, rdi
    syscall
.data
slot: db 0
"#;
        let image = X86_64Assembler.assemble(source, 0).unwrap();
        let mut machine = X86_64Machine::new(DEFAULT_MEMORY_SIZE);
        machine.load(&image).unwrap();
        assert_eq!(machine.run(256).unwrap(), StepOutcome::Exited(0));
        assert_eq!(machine.snapshot().stdout, b"10100101");
    }

    #[test]
    fn widening_multiply_spills_into_rdx_and_flags_report_it() {
        // A product that fits leaves rdx clear and CF/OF clear with it.
        let machine = run_snippet("mov rax, 6\nmov rbx, 7\nmul rbx\nhlt\n");
        assert_eq!(machine.registers[0], 42);
        assert_eq!(machine.registers[2], 0);
        assert!(machine.rflags & (FLAG_CF | FLAG_OF) == 0);

        // 2^63 * 2 needs 65 bits, so the answer is only complete across both.
        let machine = run_snippet(
            "
            mov rax, 0x8000000000000000
            mov rbx, 2
            mul rbx
            hlt
            ",
        );
        assert_eq!(machine.registers[0], 0, "low half wraps to zero");
        assert_eq!(machine.registers[2], 1, "the missing bit is in rdx");
        assert!(
            machine.rflags & (FLAG_CF | FLAG_OF) == FLAG_CF | FLAG_OF,
            "CF and OF say the high half matters"
        );

        // Signed multiply sign-extends into rdx instead of zero-filling.
        let machine = run_snippet("mov rax, -3\nmov rbx, 5\nimul rbx\nhlt\n");
        assert_eq!(machine.registers[0] as i64, -15);
        assert_eq!(machine.registers[2], u64::MAX, "rdx holds the sign");
        assert!(
            machine.rflags & (FLAG_CF | FLAG_OF) == 0,
            "-15 fits, so this is not an overflow"
        );
    }

    /// `imul` is two instructions wearing one mnemonic: the one-operand form
    /// writes rdx:rax, the two-operand form writes only its destination.
    #[test]
    fn one_and_two_operand_imul_stay_distinct() {
        for text in [
            "imul rbx",
            "imul rax, rbx",
            "mul rbx",
            "div rbx",
            "idiv rbx",
        ] {
            let bytes = X86_64Codec.assemble(0, text).unwrap();
            assert_eq!(
                X86_64Codec.disassemble(0, &bytes).as_deref(),
                Some(text),
                "{text} should survive a round trip"
            );
        }

        // The two-operand form leaves rdx alone even when the product is big.
        let machine = run_snippet("mov rdx, 99\nmov rax, -3\nmov rbx, 5\nimul rax, rbx\nhlt\n");
        assert_eq!(machine.registers[0] as i64, -15);
        assert_eq!(machine.registers[2], 99, "two-operand imul spares rdx");
    }

    #[test]
    fn divide_splits_quotient_and_remainder_across_rax_and_rdx() {
        let machine = run_snippet("mov rdx, 0\nmov rax, 17\nmov rbx, 5\ndiv rbx\nhlt\n");
        assert_eq!(machine.registers[0], 3, "quotient in rax");
        assert_eq!(machine.registers[2], 2, "remainder in rdx");

        // Signed division truncates toward zero, so -17/5 is -3 remainder -2.
        let machine = run_snippet("mov rax, -17\nmov rdx, -1\nmov rbx, 5\nidiv rbx\nhlt\n");
        assert_eq!(machine.registers[0] as i64, -3);
        assert_eq!(machine.registers[2] as i64, -2);
    }

    /// Dividing by zero and overflowing the quotient are faults on real
    /// hardware, not quietly wrong answers.
    #[test]
    fn divide_faults_rather_than_inventing_an_answer() {
        for (source, expected) in [
            (
                "mov rdx, 0\nmov rax, 1\nmov rbx, 0\ndiv rbx\nhlt\n",
                "divide by zero",
            ),
            // rdx:rax = 2^64, so the quotient needs 65 bits.
            (
                "mov rdx, 1\nmov rax, 0\nmov rbx, 1\ndiv rbx\nhlt\n",
                "does not fit",
            ),
        ] {
            let image = X86_64Assembler.assemble(source, 0).unwrap();
            let mut machine = X86_64Machine::new(4096);
            machine.load(&image).unwrap();
            let error = machine.run(16).unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "expected {expected:?}, got {error}"
            );
            assert!(matches!(machine.state, MachineState::Faulted));
        }
    }

    #[test]
    fn byte_registers_reject_arithmetic_immediates() {
        // `add al, 5` used to fall through to the 32-bit encoding and quietly
        // clobber all of eax.
        let error = X86_64Assembler.assemble("add al, 5\n", 0).unwrap_err();
        assert!(
            error.message.contains("32-bit or 64-bit register"),
            "unexpected diagnostic: {}",
            error.message
        );
    }

    #[test]
    fn codec_handles_variable_width_instructions() {
        let mov = X86_64Codec.assemble(0, "mov r8, 0x1234").unwrap();
        assert_eq!(mov.len(), 10);
        assert_eq!(X86_64Codec.instruction_width(0, &mov), 10);
        assert_eq!(
            X86_64Codec.disassemble(0, &mov).as_deref(),
            Some("mov r8, 0x1234")
        );
        assert!(X86_64Codec.inspect(0, &mov).unwrap().layout_is_complete());

        assert_eq!(
            X86_64Codec.assemble(0, "mov sil, [rax]").unwrap(),
            [0x40, 0x8A, 0x30]
        );
        assert_eq!(
            X86_64Codec.assemble(0, "mov al, sil").unwrap(),
            [0x40, 0x88, 0xF0]
        );
        assert!(X86_64Codec.assemble(0, "mov eax, -1").is_ok());
    }

    #[test]
    fn relative_labels_work_at_canonical_high_addresses() {
        let base = 0xFFFF_8000_0000_0000;
        let image = X86_64Assembler
            .assemble("start: jmp start\nlea rax, [rel start]\n", base)
            .unwrap();
        assert_eq!(image.entry, base);
    }

    #[test]
    fn pipeline_tracks_raw_hazards_and_branch_flushes() {
        let source = r#"
            mov rax, 1
            add rbx, rax
            jmp target
            mov rbx, 99
        target:
            add rbx, rax
            hlt
        "#;
        let image = X86_64Assembler.assemble(source, 0).unwrap();
        let mut machine = X86_64Machine::new(4096);
        machine.pipeline_control().unwrap().set_enabled(true);
        machine.load(&image).unwrap();
        assert_eq!(machine.run(200).unwrap(), StepOutcome::Halted);
        assert_eq!(machine.snapshot().registers[3].value, 2);

        let pipeline = machine.pipeline().unwrap();
        assert_eq!(pipeline.stage_count(), 6);
        assert!(pipeline.stats().raw_stalls > 0);
        assert!(pipeline.stats().flushes > 0);
        assert!(
            pipeline
                .edges()
                .iter()
                .any(|edge| edge.kind == crate::capability::PipelineEdgeKind::Feedback)
        );
    }
}
