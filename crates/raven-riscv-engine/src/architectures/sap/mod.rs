//! Minimal 8-bit SAP (Simple-As-Possible) CPU and assembler.

use crate::capability::{
    CacheHierarchy, InstructionCodec, InstructionField, InstructionInfo, MemoryInspect,
    MemoryRegion, RegisterBank, RegisterFile, RegisterId,
};
use crate::cache_model::TeachingCache;
use crate::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, Assembler, Diagnostic,
    Endianness, Machine, MachineError, MachineSnapshot, MachineState, ProgramImage, ProgramSegment,
    RegisterValue, SourceMap, StepOutcome,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const ID: &str = "sap";
pub const MEMORY_SIZE: usize = 16;

static DESCRIPTOR: ArchitectureDescriptor = ArchitectureDescriptor {
    id: ID,
    display_name: "SAP",
    source_extension: "sap",
    address_bits: 4,
    instruction_alignment: 1,
    default_memory_size: MEMORY_SIZE,
    endianness: Endianness::Little,
    capabilities: ArchitectureCapabilities {
        elf: false,
        cache: true,
        virtual_memory: false,
        jit: false,
        pipeline: false,
        multicore: false,
        floating_point: false,
        guided_learning: false,
    },
};

#[derive(Default)]
pub struct Sap;

pub fn architecture() -> Arc<dyn Architecture> {
    static INSTANCE: OnceLock<Arc<Sap>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(Sap)).clone()
}

impl Architecture for Sap {
    fn descriptor(&self) -> &'static ArchitectureDescriptor {
        &DESCRIPTOR
    }

    fn assembler(&self) -> &'static dyn Assembler {
        &SapAssembler
    }

    fn default_source(&self) -> &'static str {
        "LDA first\nADD second\nOUT\nHLT\nfirst: DAT 10\nsecond: DAT 20\n"
    }

    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        if memory_size > MEMORY_SIZE {
            return Err(MachineError::new(
                "SAP memory size exceeds its 4-bit address space",
            ));
        }
        Ok(Box::new(SapMachine::new(memory_size)))
    }
}

pub struct SapAssembler;

#[derive(Clone)]
struct Line {
    source: usize,
    address: u8,
    text: String,
}

impl Assembler for SapAssembler {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn instruction_forms(&self, mnemonic: &str) -> &'static [&'static str] {
        match mnemonic.to_ascii_lowercase().as_str() {
            "nop" | "out" | "hlt" => &[""],
            "lda" | "add" | "sub" | "sta" => &["address"],
            "ldi" => &["imm"],
            "jmp" | "jc" | "jz" => &["label"],
            "dat" => &["byte"],
            _ => &[],
        }
    }

    fn is_register(&self, token: &str) -> bool {
        matches!(token.to_ascii_lowercase().as_str(), "a" | "b" | "out")
    }

    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic> {
        let base = u8::try_from(base_address)
            .ok()
            .filter(|address| *address < 16)
            .ok_or_else(|| Diagnostic::new(None, "SAP base address must be between 0 and 15"))?;

        let mut labels = HashMap::new();
        let mut labels_by_address = HashMap::<u64, Vec<String>>::new();
        let mut lines = Vec::new();
        let mut address = base;

        for (source_line, raw) in source.lines().enumerate() {
            let clean = raw.split(['#', ';']).next().unwrap_or("").trim();
            if clean.is_empty() {
                continue;
            }
            if address >= 16 {
                return Err(Diagnostic::new(
                    Some(source_line),
                    "SAP program exceeds its 16-byte memory",
                ));
            }
            let (label, text) = match clean.split_once(':') {
                Some((label, text)) => (Some(label.trim()), text.trim()),
                None => (None, clean),
            };
            if let Some(label) = label {
                if label.is_empty() {
                    return Err(Diagnostic::new(Some(source_line), "empty label"));
                }
                if labels.insert(label.to_string(), address).is_some() {
                    return Err(Diagnostic::new(
                        Some(source_line),
                        format!("duplicate label '{label}'"),
                    ));
                }
                labels_by_address
                    .entry(u64::from(address))
                    .or_default()
                    .push(label.to_string());
            }
            if text.is_empty() {
                continue;
            }
            lines.push(Line {
                source: source_line,
                address,
                text: text.to_string(),
            });
            address += 1;
        }

        let mut bytes = Vec::with_capacity(lines.len());
        let mut line_addresses = HashMap::new();
        let mut explicit_halts = Vec::new();
        for line in &lines {
            let byte = encode(line, &labels)?;
            bytes.push(byte);
            line_addresses.insert(line.source, u64::from(line.address));
            if byte == 0xf0 {
                explicit_halts.push(u64::from(line.address));
            }
        }

        let label_to_line = labels
            .iter()
            .filter_map(|(label, address)| {
                lines
                    .iter()
                    .find(|line| line.address >= *address)
                    .map(|line| (label.clone(), line.source))
            })
            .collect();

        Ok(ProgramImage {
            architecture: ID.into(),
            entry: base_address,
            segments: vec![ProgramSegment {
                address: base_address,
                bytes,
                executable: true,
                writable: true,
            }],
            zero_fill: vec![],
            source_map: SourceMap {
                labels: labels_by_address,
                label_to_line,
                line_addresses,
                explicit_halts,
                ..SourceMap::default()
            },
        })
    }
}

fn encode(line: &Line, labels: &HashMap<String, u8>) -> Result<u8, Diagnostic> {
    let parts: Vec<_> = line
        .text
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .collect();
    let mnemonic = parts.first().copied().unwrap_or("").to_ascii_uppercase();
    let error = |message| Diagnostic::new(Some(line.source), message);
    let operand = |name: &str| {
        labels
            .get(name)
            .copied()
            .or_else(|| number(name).filter(|value| *value < 16))
            .ok_or_else(|| error(format!("invalid SAP address '{name}'")))
    };

    match (mnemonic.as_str(), parts.as_slice()) {
        ("NOP", [_]) => Ok(0x00),
        ("OUT", [_]) => Ok(0xe0),
        ("HLT" | "HALT", [_]) => Ok(0xf0),
        ("LDA", [_, value]) => Ok(0x10 | operand(value)?),
        ("ADD", [_, value]) => Ok(0x20 | operand(value)?),
        ("SUB", [_, value]) => Ok(0x30 | operand(value)?),
        ("STA", [_, value]) => Ok(0x40 | operand(value)?),
        ("LDI", [_, value]) => Ok(0x50 | operand(value)?),
        ("JMP", [_, value]) => Ok(0x60 | operand(value)?),
        ("JC", [_, value]) => Ok(0x70 | operand(value)?),
        ("JZ", [_, value]) => Ok(0x80 | operand(value)?),
        ("DAT" | ".BYTE", [_, value]) => {
            number(value).ok_or_else(|| error(format!("invalid SAP byte '{value}'")))
        }
        _ => Err(error(format!("invalid SAP instruction '{}'", line.text))),
    }
}

fn number(text: &str) -> Option<u8> {
    if let Some(value) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u8::from_str_radix(value, 16).ok()
    } else if let Some(value) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        u8::from_str_radix(value, 2).ok()
    } else {
        text.parse().ok()
    }
}

pub struct SapMachine {
    accumulator: u8,
    b: u8,
    output: u8,
    pc: u8,
    carry: bool,
    zero: bool,
    memory: Vec<u8>,
    image: Option<ProgramImage>,
    state: MachineState,
    instructions: u64,
    stdout: Vec<u8>,
    fault: Option<String>,
    cache: TeachingCache,
}

impl SapMachine {
    fn new(memory_size: usize) -> Self {
        Self {
            accumulator: 0,
            b: 0,
            output: 0,
            pc: 0,
            carry: false,
            zero: false,
            memory: vec![0; memory_size],
            image: None,
            state: MachineState::Ready,
            instructions: 0,
            stdout: vec![],
            fault: None,
            cache: TeachingCache::new(4, 16, 4, 1),
        }
    }

    fn range(&self, address: u64, bytes: usize) -> Result<usize, MachineError> {
        let start = usize::try_from(address).map_err(|_| MachineError::new("address overflow"))?;
        let end = start
            .checked_add(bytes)
            .ok_or_else(|| MachineError::new("address overflow"))?;
        if end > self.memory.len() {
            return Err(MachineError::new("SAP memory access out of bounds"));
        }
        Ok(start)
    }

    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, MachineError> {
        let message = message.into();
        self.state = MachineState::Faulted;
        self.fault = Some(message.clone());
        Err(MachineError::new(message))
    }
}

impl Machine for SapMachine {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn reset(&mut self) {
        let image = self.image.clone();
        let size = self.memory.len();
        *self = Self::new(size);
        if let Some(image) = image
            && let Err(error) = self.load(&image)
        {
            self.state = MachineState::Faulted;
            self.fault = Some(error.to_string());
        }
    }

    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError> {
        image.expect_architecture(ID)?;
        let entry = u8::try_from(image.entry)
            .ok()
            .filter(|entry| *entry < 16)
            .ok_or_else(|| MachineError::new("entry exceeds SAP address space"))?;
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

        self.memory.fill(0);
        for segment in &image.segments {
            let start = self.range(segment.address, segment.bytes.len())?;
            self.memory[start..start + segment.bytes.len()].copy_from_slice(&segment.bytes);
        }
        for (start, size) in fills {
            self.memory[start..start + size].fill(0);
        }
        self.accumulator = 0;
        self.b = 0;
        self.output = 0;
        self.pc = entry;
        self.carry = false;
        self.zero = false;
        self.image = Some(image.clone());
        self.state = MachineState::Ready;
        self.instructions = 0;
        self.stdout.clear();
        self.fault = None;
        self.cache.reset();
        Ok(())
    }

    fn step(&mut self) -> Result<StepOutcome, MachineError> {
        if matches!(
            self.state,
            MachineState::Halted | MachineState::Exited(_) | MachineState::Faulted
        ) {
            return Err(MachineError::new("machine is not runnable; reset it first"));
        }
        let pc = usize::from(self.pc);
        let Some(&instruction) = self.memory.get(pc) else {
            return self.fail("SAP fetch out of bounds");
        };
        self.cache.fetch(pc, &self.memory);
        self.pc = self.pc.saturating_add(1);
        self.instructions += 1;
        self.state = MachineState::Running;
        let opcode = instruction >> 4;
        let address = usize::from(instruction & 0x0f);
        if matches!(opcode, 0x1..=0x4) && address >= self.memory.len() {
            return self.fail(format!("SAP data access out of bounds at 0x{address:X}"));
        }

        match opcode {
            0x0 => {}
            0x1 => {
                self.cache.read(address, &self.memory);
                self.accumulator = self.memory[address];
            }
            0x2 => {
                self.cache.read(address, &self.memory);
                self.b = self.memory[address];
                (self.accumulator, self.carry) = self.accumulator.overflowing_add(self.b);
                self.zero = self.accumulator == 0;
            }
            0x3 => {
                self.cache.read(address, &self.memory);
                self.b = self.memory[address];
                let (value, borrow) = self.accumulator.overflowing_sub(self.b);
                self.accumulator = value;
                self.carry = !borrow;
                self.zero = value == 0;
            }
            0x4 => {
                self.memory[address] = self.accumulator;
                self.cache.write(address, &[self.accumulator], &self.memory);
            }
            0x5 => self.accumulator = instruction & 0x0f,
            0x6 => self.pc = instruction & 0x0f,
            0x7 if self.carry => self.pc = instruction & 0x0f,
            0x7 => {}
            0x8 if self.zero => self.pc = instruction & 0x0f,
            0x8 => {}
            0xe => {
                self.output = self.accumulator;
                self.stdout
                    .extend_from_slice(format!("{}\n", self.output).as_bytes());
            }
            0xf => {
                self.pc = pc as u8;
                self.state = MachineState::Halted;
                return Ok(StepOutcome::Halted);
            }
            opcode => return self.fail(format!("invalid SAP opcode 0x{opcode:X} at 0x{pc:X}")),
        }
        Ok(StepOutcome::Stepped)
    }

    fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            architecture: ID,
            pc: u64::from(self.pc),
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
        if name.eq_ignore_ascii_case("pc") {
            return self.set_program_counter(value);
        }
        let id = RegisterFile::resolve(self, name)
            .ok_or_else(|| MachineError::new(format!("unknown SAP register '{name}'")))?;
        RegisterFile::write(self, id, value)
    }

    fn push_input(&mut self, _line: &str) {}

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
        Some(&SapCodec)
    }

    fn caches(&self) -> Option<&dyn CacheHierarchy> {
        Some(&self.cache)
    }
}

static BANKS: [RegisterBank; 2] = [
    RegisterBank::integer("", "Registers", 3, 8),
    RegisterBank::integer("", "Flags", 2, 1),
];

impl RegisterFile for SapMachine {
    fn banks(&self) -> &[RegisterBank] {
        &BANKS
    }

    fn read(&self, id: RegisterId) -> Option<u64> {
        match (id.bank, id.index) {
            (0, 0) => Some(u64::from(self.accumulator)),
            (0, 1) => Some(u64::from(self.b)),
            (0, 2) => Some(u64::from(self.output)),
            (1, 0) => Some(self.carry as u64),
            (1, 1) => Some(self.zero as u64),
            _ => None,
        }
    }

    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError> {
        match (id.bank, id.index) {
            (0, 0) => self.accumulator = u8::try_from(value)
                .map_err(|_| MachineError::new("register value exceeds 8 bits"))?,
            (0, 1) => self.b = u8::try_from(value)
                .map_err(|_| MachineError::new("register value exceeds 8 bits"))?,
            (0, 2) => self.output = u8::try_from(value)
                .map_err(|_| MachineError::new("register value exceeds 8 bits"))?,
            (1, 0) if value <= 1 => self.carry = value != 0,
            (1, 1) if value <= 1 => self.zero = value != 0,
            (1, _) => return Err(MachineError::new("flag value exceeds 1 bit")),
            _ => return Err(MachineError::new("no such SAP register")),
        }
        Ok(())
    }

    fn program_counter(&self) -> u64 {
        u64::from(self.pc)
    }

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError> {
        self.pc = u8::try_from(value)
            .ok()
            .filter(|pc| *pc < 16)
            .ok_or_else(|| MachineError::new("PC exceeds SAP address space"))?;
        Ok(())
    }

    fn name(&self, id: RegisterId) -> Option<String> {
        match id.bank {
            0 => ["a", "b", "out"].get(id.index).map(|name| (*name).into()),
            1 => ["carry", "zero"].get(id.index).map(|name| (*name).into()),
            _ => None,
        }
    }
}

impl MemoryInspect for SapMachine {
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
        vec![MemoryRegion::new("Memory", 0)]
    }
}

pub struct SapCodec;

impl InstructionCodec for SapCodec {
    fn instruction_width(&self, _address: u64, _bytes: &[u8]) -> usize {
        1
    }

    fn disassemble(&self, _address: u64, bytes: &[u8]) -> Option<String> {
        let byte = *bytes.first()?;
        let operand = byte & 0x0f;
        Some(match byte >> 4 {
            0x0 => "nop".into(),
            0x1 => format!("lda 0x{operand:X}"),
            0x2 => format!("add 0x{operand:X}"),
            0x3 => format!("sub 0x{operand:X}"),
            0x4 => format!("sta 0x{operand:X}"),
            0x5 => format!("ldi {operand}"),
            0x6 => format!("jmp 0x{operand:X}"),
            0x7 => format!("jc 0x{operand:X}"),
            0x8 => format!("jz 0x{operand:X}"),
            0xe => "out".into(),
            0xf => "hlt".into(),
            _ => return None,
        })
    }

    fn assemble(&self, address: u64, text: &str) -> Result<Vec<u8>, Diagnostic> {
        Ok(SapAssembler
            .assemble(text, address)?
            .executable_bytes()
            .map(<[u8]>::to_vec)
            .unwrap_or_default())
    }

    fn inspect(&self, address: u64, bytes: &[u8]) -> Option<InstructionInfo> {
        let byte = *bytes.first()?;
        let opcode = byte >> 4;
        let operand = byte & 0x0f;
        let mut fields = vec![InstructionField {
            name: "opcode",
            value: format!("0x{opcode:X}"),
        }];
        if matches!(opcode, 0x1..=0x8) {
            fields.push(InstructionField {
                name: if opcode == 0x5 { "immediate" } else { "address" },
                value: if opcode == 0x5 {
                    operand.to_string()
                } else {
                    format!("0x{operand:X}")
                },
            });
        }
        Some(InstructionInfo {
            mnemonic: self.disassemble(address, bytes)?,
            class: match opcode {
                0x1 => "Load",
                0x4 => "Store",
                0x6..=0x8 => "Control",
                0xe => "I/O",
                0xf => "System",
                _ => "ALU",
            },
            encoding: u64::from(byte),
            encoding_bits: 8,
            fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_labels_in_one_byte_instructions() {
        let image = SapAssembler
            .assemble("LDI 3\nSTA value\nOUT\nHLT\nvalue: DAT 42", 0)
            .unwrap();

        assert_eq!(
            image.executable_bytes(),
            Some(&[0x53, 0x44, 0xe0, 0xf0, 42][..])
        );
        assert_eq!(image.source_map.explicit_halts, [3]);
    }

    #[test]
    fn machine_runs_a_program() {
        let image = SapAssembler
            .assemble("LDA x\nADD y\nOUT\nHLT\nx: DAT 10\ny: DAT 20", 0)
            .unwrap();
        let mut machine = SapMachine::new(MEMORY_SIZE);
        machine.load(&image).unwrap();

        assert_eq!(machine.run(10).unwrap(), StepOutcome::Halted);
        assert_eq!(machine.snapshot().stdout, b"30\n");
    }
}
