//! A tiny, intentionally different 16-bit architecture.
//!
//! Toy16 is public and selectable so architecture-neutral integrations can be
//! tested without pretending that every CPU looks like RISC-V.

use crate::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, Assembler, Diagnostic,
    Endianness, Machine, MachineError, MachineSnapshot, MachineState, ProgramImage, ProgramSegment,
    RegisterValue, SourceMap, StepOutcome,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const ID: &str = "toy16";
pub const MEMORY_SIZE: usize = 64 * 1024;

static DESCRIPTOR: ArchitectureDescriptor = ArchitectureDescriptor {
    id: ID,
    display_name: "Toy16",
    source_extension: "toy",
    address_bits: 16,
    instruction_alignment: 2,
    endianness: Endianness::Little,
    capabilities: ArchitectureCapabilities {
        elf: false,
        cache: false,
        virtual_memory: false,
        jit: false,
        pipeline: false,
        multicore: false,
        floating_point: false,
        guided_learning: false,
    },
};

#[derive(Default)]
pub struct Toy16;

pub fn architecture() -> Arc<dyn Architecture> {
    static INSTANCE: OnceLock<Arc<Toy16>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(Toy16)).clone()
}

impl Architecture for Toy16 {
    fn descriptor(&self) -> &'static ArchitectureDescriptor {
        &DESCRIPTOR
    }
    fn assembler(&self) -> &'static dyn Assembler {
        &Toy16Assembler
    }
    fn default_source(&self) -> &'static str {
        "li r0, 20\nli r1, 22\nadd r2, r0, r1\nprint r2\nhalt\n"
    }
    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        let size = if memory_size == 0 {
            MEMORY_SIZE
        } else {
            memory_size.min(MEMORY_SIZE)
        };
        Ok(Box::new(Toy16Machine::new(size)))
    }
}

pub struct Toy16Assembler;

#[derive(Clone)]
struct ParsedLine {
    source_line: usize,
    address: u16,
    text: String,
}

impl Assembler for Toy16Assembler {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic> {
        let base = u16::try_from(base_address)
            .map_err(|_| Diagnostic::new(None, "Toy16 base address exceeds 16 bits"))?;
        if base % 2 != 0 {
            return Err(Diagnostic::new(
                None,
                "Toy16 base address must be 2-byte aligned",
            ));
        }

        let mut labels = HashMap::<String, u16>::new();
        let mut labels_by_address = HashMap::<u64, Vec<String>>::new();
        let mut parsed = Vec::new();
        let mut pc = base;
        for (line_number, raw) in source.lines().enumerate() {
            let clean = raw.split(['#', ';']).next().unwrap_or("").trim();
            if clean.is_empty() {
                continue;
            }
            let (label, instruction) = match clean.split_once(':') {
                Some((label, instruction)) => (Some(label.trim()), instruction.trim()),
                None => (None, clean),
            };
            if let Some(label) = label {
                if label.is_empty() {
                    return Err(Diagnostic::new(Some(line_number), "empty label"));
                }
                if labels.insert(label.to_string(), pc).is_some() {
                    return Err(Diagnostic::new(
                        Some(line_number),
                        format!("duplicate label '{label}'"),
                    ));
                }
                labels_by_address
                    .entry(u64::from(pc))
                    .or_default()
                    .push(label.to_string());
            }
            if instruction.is_empty() {
                continue;
            }
            parsed.push(ParsedLine {
                source_line: line_number,
                address: pc,
                text: instruction.to_string(),
            });
            pc = pc.checked_add(2).ok_or_else(|| {
                Diagnostic::new(Some(line_number), "program exceeds Toy16 address space")
            })?;
        }

        let mut bytes = Vec::with_capacity(parsed.len() * 2);
        let mut line_addresses = HashMap::new();
        let mut explicit_halts = Vec::new();
        for line in &parsed {
            let word = encode_line(line, &labels)?;
            if word >> 12 == 1 {
                explicit_halts.push(u64::from(line.address));
            }
            bytes.extend_from_slice(&word.to_le_bytes());
            line_addresses.insert(line.source_line, u64::from(line.address));
        }

        let label_to_line = labels
            .keys()
            .filter_map(|name| {
                parsed
                    .iter()
                    .find(|line| labels[name] <= line.address)
                    .map(|line| (name.clone(), line.source_line))
            })
            .collect();
        Ok(ProgramImage {
            architecture: ID.into(),
            entry: base_address,
            segments: vec![ProgramSegment {
                address: base_address,
                bytes,
                executable: true,
                writable: false,
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

fn encode_line(line: &ParsedLine, labels: &HashMap<String, u16>) -> Result<u16, Diagnostic> {
    let normalized = line.text.replace([',', '[', ']'], " ");
    let parts: Vec<_> = normalized.split_whitespace().collect();
    let error = |message: String| Diagnostic::new(Some(line.source_line), message);
    let mnemonic = parts.first().copied().unwrap_or("").to_ascii_lowercase();
    let register = |text: &str| {
        parse_register(text).ok_or_else(|| error(format!("invalid Toy16 register '{text}'")))
    };
    let immediate =
        |text: &str| parse_number(text).ok_or_else(|| error(format!("invalid immediate '{text}'")));
    let target = |text: &str| {
        labels
            .get(text)
            .copied()
            .or_else(|| parse_number(text).and_then(|v| u16::try_from(v).ok()))
            .ok_or_else(|| error(format!("unknown label or address '{text}'")))
    };
    match (mnemonic.as_str(), parts.as_slice()) {
        ("nop", [_]) => Ok(0),
        ("halt", [_]) => Ok(0x1000),
        ("li", [_, rd, imm]) => {
            let imm = immediate(imm)?;
            if !(0..=255).contains(&imm) {
                return Err(error("li immediate must be between 0 and 255".into()));
            }
            Ok(0x2000 | (u16::from(register(rd)?) << 9) | (imm as u16 & 0xff))
        }
        ("add", [_, rd, ra, rb]) => Ok(0x3000
            | (u16::from(register(rd)?) << 9)
            | (u16::from(register(ra)?) << 6)
            | (u16::from(register(rb)?) << 3)),
        ("load", [_, rd, addr]) | ("store", [_, rd, addr]) => {
            let address = target(addr)?;
            if address > 0xff {
                return Err(error("Toy16 load/store address must be <= 255".into()));
            }
            let opcode = if mnemonic == "load" { 0x4000 } else { 0x5000 };
            Ok(opcode | (u16::from(register(rd)?) << 9) | address)
        }
        ("jmp", [_, addr]) => {
            let address = target(addr)?;
            if address % 2 != 0 || address / 2 > 0x0fff {
                return Err(error("jump target is out of range or unaligned".into()));
            }
            Ok(0x6000 | (address / 2))
        }
        ("jnz", [_, reg, addr]) => {
            let address = target(addr)?;
            if address % 2 != 0 || address / 2 > 0xff {
                return Err(error(
                    "jnz target must be an aligned address below 512".into(),
                ));
            }
            Ok(0x7000 | (u16::from(register(reg)?) << 9) | (address / 2))
        }
        ("print", [_, reg]) => Ok(0x8000 | (u16::from(register(reg)?) << 9)),
        _ => Err(error(format!(
            "invalid Toy16 instruction '{}'; expected nop, halt, li, add, load, store, jmp, jnz, or print",
            line.text
        ))),
    }
}

fn parse_register(text: &str) -> Option<u8> {
    text.strip_prefix('r')
        .or_else(|| text.strip_prefix('R'))?
        .parse::<u8>()
        .ok()
        .filter(|r| *r < 8)
}

fn parse_number(text: &str) -> Option<i32> {
    let (negative, raw) = text
        .strip_prefix('-')
        .map_or((false, text), |raw| (true, raw));
    let value = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        i32::from_str_radix(hex, 16).ok()?
    } else {
        raw.parse().ok()?
    };
    Some(if negative { -value } else { value })
}

pub struct Toy16Machine {
    registers: [u16; 8],
    pc: u16,
    memory: Vec<u8>,
    image: Option<ProgramImage>,
    state: MachineState,
    instructions: u64,
    stdout: Vec<u8>,
    fault: Option<String>,
}

impl Toy16Machine {
    fn new(memory_size: usize) -> Self {
        Self {
            registers: [0; 8],
            pc: 0,
            memory: vec![0; memory_size],
            image: None,
            state: MachineState::Ready,
            instructions: 0,
            stdout: vec![],
            fault: None,
        }
    }

    fn range(&self, address: u64, bytes: usize) -> Result<usize, MachineError> {
        let start = usize::try_from(address).map_err(|_| MachineError::new("address overflow"))?;
        let end = start
            .checked_add(bytes)
            .ok_or_else(|| MachineError::new("address overflow"))?;
        if end > self.memory.len() {
            return Err(MachineError::new("Toy16 memory access out of bounds"));
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

impl Machine for Toy16Machine {
    fn architecture_id(&self) -> &'static str {
        ID
    }
    fn reset(&mut self) {
        let image = self.image.clone();
        let size = self.memory.len();
        *self = Self::new(size);
        if let Some(image) = image {
            let _ = self.load(&image);
        }
    }

    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError> {
        if image.architecture != ID {
            return Err(MachineError::new(format!(
                "cannot load {} image into {ID}",
                image.architecture
            )));
        }
        let entry =
            u16::try_from(image.entry).map_err(|_| MachineError::new("entry exceeds Toy16"))?;
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
        self.registers = [0; 8];
        self.pc = entry;
        self.image = Some(image.clone());
        self.state = MachineState::Ready;
        self.instructions = 0;
        self.stdout.clear();
        self.fault = None;
        Ok(())
    }

    fn step(&mut self) -> Result<StepOutcome, MachineError> {
        if matches!(
            self.state,
            MachineState::Halted | MachineState::Exited(_) | MachineState::Faulted
        ) {
            return Err(MachineError::new("machine is not runnable; reset it first"));
        }
        let pc = self.pc as usize;
        if pc + 2 > self.memory.len() {
            return self.fail("Toy16 fetch out of bounds");
        }
        let word = u16::from_le_bytes([self.memory[pc], self.memory[pc + 1]]);
        self.pc = self.pc.wrapping_add(2);
        self.instructions += 1;
        self.state = MachineState::Running;
        let reg = |shift: u16| ((word >> shift) & 7) as usize;
        match word >> 12 {
            0 => {}
            1 => {
                self.state = MachineState::Halted;
                return Ok(StepOutcome::Halted);
            }
            2 => self.registers[reg(9)] = word & 0xff,
            3 => {
                self.registers[reg(9)] = self.registers[reg(6)].wrapping_add(self.registers[reg(3)])
            }
            4 => {
                let addr = (word & 0xff) as usize;
                if addr + 2 > self.memory.len() {
                    return self.fail("Toy16 load out of bounds");
                }
                self.registers[reg(9)] =
                    u16::from_le_bytes([self.memory[addr], self.memory[addr + 1]]);
            }
            5 => {
                let addr = (word & 0xff) as usize;
                if addr + 2 > self.memory.len() {
                    return self.fail("Toy16 store out of bounds");
                }
                let bytes = self.registers[reg(9)].to_le_bytes();
                self.memory[addr..addr + 2].copy_from_slice(&bytes);
            }
            6 => self.pc = (word & 0x0fff) * 2,
            7 if self.registers[reg(9)] != 0 => self.pc = (word & 0xff) * 2,
            7 => {}
            8 => self
                .stdout
                .extend_from_slice(format!("{}\n", self.registers[reg(9)]).as_bytes()),
            opcode => return self.fail(format!("invalid Toy16 opcode 0x{opcode:X} at 0x{pc:04X}")),
        }
        Ok(StepOutcome::Stepped)
    }

    fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            architecture: ID,
            pc: u64::from(self.pc),
            registers: self
                .registers
                .iter()
                .enumerate()
                .map(|(index, value)| RegisterValue {
                    name: format!("r{index}"),
                    value: u64::from(*value),
                    bits: 16,
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
            self.pc = u16::try_from(value).map_err(|_| MachineError::new("PC exceeds 16 bits"))?;
            return Ok(());
        }
        let index = parse_register(name)
            .ok_or_else(|| MachineError::new(format!("unknown Toy16 register '{name}'")))?;
        self.registers[index as usize] = u16::try_from(value)
            .map_err(|_| MachineError::new("register value exceeds 16 bits"))?;
        Ok(())
    }

    fn push_input(&mut self, _line: &str) {}
}
