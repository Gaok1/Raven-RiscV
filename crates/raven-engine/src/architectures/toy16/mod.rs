//! A tiny, intentionally different 16-bit architecture.
//!
//! Toy16 is public and selectable so architecture-neutral integrations can be
//! tested without pretending that every CPU looks like RISC-V.

use crate::cache_model::TeachingCache;
use crate::capability::{
    BitRole, CacheHierarchy, InstructionBitField, InstructionCodec, InstructionField,
    InstructionInfo, MemoryInspect, MemoryRegion, PipelineControl, PipelineDynamicInspect,
    PipelineInspect, PipelineTuning,
    PipelineInstructionClass, RegisterBank, RegisterFile, RegisterId,
};
use crate::pipeline::{PipelineEngine, PipelineOp, PipelineShape, StageSpec, UnitSpec};
use crate::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, Assembler, CycleResult,
    Diagnostic, Endianness, InstructionDoc, Machine, MachineError, MachineSnapshot, MachineState,
    ProgramImage, ProgramSegment, RegisterValue, SourceMap, StepOutcome,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock};

pub const ID: &str = "toy16";
pub const MEMORY_SIZE: usize = 64 * 1024;

/// Toy16's whole instruction set. Eight registers, one ALU operation and a
/// byte-wide address space for data — deliberately small, so the reference
/// fits on one screen next to the encoding it produces.
#[rustfmt::skip]
static INSTRUCTION_DOCS: &[InstructionDoc] = &[
    InstructionDoc::new("ALU", "li", "rd, imm", "rd = imm (0-255)"),
    InstructionDoc::new("ALU", "add", "rd, rs1, rs2", "rd = rs1 + rs2"),
    InstructionDoc::new("ALU", "nop", "", "Do nothing for one instruction"),
    InstructionDoc::new("Load", "load", "rd, [addr]", "rd = memory[addr], addr is 0-255"),
    InstructionDoc::new("Store", "store", "rs, [addr]", "memory[addr] = rs, addr is 0-255"),
    InstructionDoc::new("Jump", "jmp", "label", "Jump to label unconditionally"),
    InstructionDoc::new("Branch", "jnz", "rs, label", "Jump to label when rs is not zero"),
    InstructionDoc::new("I/O", "print", "rs", "Print rs as a decimal number"),
    InstructionDoc::new("I/O", "putc", "ascii", "Print one character by ASCII code"),
    InstructionDoc::new("SYS", "halt", "", "Stop the machine"),
    InstructionDoc::new("SYS", "syscall", "", "Call read, write, or exit through the host"),
];

static DESCRIPTOR: ArchitectureDescriptor = ArchitectureDescriptor {
    id: ID,
    display_name: "Toy16",
    source_extension: "toy",
    address_bits: 16,
    instruction_alignment: 2,
    default_memory_size: MEMORY_SIZE,
    endianness: Endianness::Little,
    capabilities: ArchitectureCapabilities {
        elf: false,
        cache: true,
        virtual_memory: false,
        jit: false,
        pipeline: true,
        multicore: false,
        floating_point: false,
        syscalls: true,
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
        "li r0, 72\nputc 72\nputc 101\nputc 108\nputc 108\nputc 111\nputc 44\nputc 32\nputc 87\nputc 111\nputc 114\nputc 108\nputc 100\nputc 33\nputc 10\nhalt\n"
    }
    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        if memory_size > MEMORY_SIZE {
            return Err(MachineError::new(
                "Toy16 memory size exceeds 16-bit address space",
            ));
        }
        Ok(Box::new(Toy16Machine::new(memory_size)))
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

    fn instruction_forms(&self, mnemonic: &str) -> &'static [&'static str] {
        match mnemonic.to_ascii_lowercase().as_str() {
            "nop" | "halt" | "syscall" => &[""],
            "li" => &["rd, imm"],
            "add" => &["rd, rs1, rs2"],
            "load" | "store" => &["rd, [addr]"],
            "jmp" => &["label"],
            "jnz" => &["rs, label"],
            "print" => &["rs"],
            "putc" => &["ascii"],
            _ => &[],
        }
    }

    fn is_register(&self, token: &str) -> bool {
        parse_register(token).is_some()
    }

    fn documented_instructions(&self) -> &'static [InstructionDoc] {
        INSTRUCTION_DOCS
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
        ("syscall", [_]) => Ok(0xA000),
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
        ("putc", [_, value]) => {
            let value = immediate(value)?;
            if !(0..=255).contains(&value) {
                return Err(error("putc value must be between 0 and 255".into()));
            }
            Ok(0x9000 | value as u16)
        }
        _ => Err(error(format!(
            "invalid Toy16 instruction '{}'; expected nop, halt, li, add, load, store, jmp, jnz, print, or putc",
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
    input: VecDeque<u8>,
    fault: Option<String>,
    cache: TeachingCache,
    pipeline: PipelineEngine<u16>,
}

/// Toy16's whole datapath declaration: the classic five stages, two bytes per
/// instruction. Everything the pipeline pane shows follows from these two facts.
static PIPELINE_STAGES: [StageSpec; 5] = crate::pipeline::RISC_FIVE_STAGE;

/// Execute dispatches into these. Toy16 has no instruction that runs long, so
/// the bank never holds work — but the utilization strip still shows which unit
/// each instruction went through, and the model is the same one a machine with
/// a slow multiplier would use.
static PIPELINE_UNITS: [UnitSpec; 3] = [
    UnitSpec::new(
        "ALU",
        &[
            PipelineInstructionClass::Alu,
            PipelineInstructionClass::Branch,
            PipelineInstructionClass::Jump,
        ],
        PipelineInstructionClass::Alu,
    ),
    UnitSpec::new(
        "LSU",
        &[
            PipelineInstructionClass::Load,
            PipelineInstructionClass::Store,
        ],
        PipelineInstructionClass::Load,
    ),
    UnitSpec::new(
        "SYS",
        &[PipelineInstructionClass::System],
        PipelineInstructionClass::System,
    ),
];

fn pipeline_shape() -> PipelineShape {
    PipelineShape::scalar(&PIPELINE_STAGES, Some(2)).with_parallel_units(&PIPELINE_UNITS)
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
            input: VecDeque::new(),
            fault: None,
            cache: TeachingCache::new(16, 256, 8, 2),
            pipeline: PipelineEngine::new(pipeline_shape(), 0),
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

    fn execute_instruction(&mut self, pc: usize, word: u16) -> Result<StepOutcome, MachineError> {
        self.pc = (pc as u16).wrapping_add(2);
        self.instructions += 1;
        self.state = MachineState::Running;
        let reg = |shift: u16| ((word >> shift) & 7) as usize;
        match word >> 12 {
            0 => {}
            1 => {
                self.pc = pc as u16;
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
                self.cache.read(addr, &self.memory);
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
                self.cache.write(addr, &bytes, &self.memory);
            }
            6 => self.pc = (word & 0x0fff) * 2,
            7 if self.registers[reg(9)] != 0 => self.pc = (word & 0xff) * 2,
            7 => {}
            8 => self
                .stdout
                .extend_from_slice(format!("{}\n", self.registers[reg(9)]).as_bytes()),
            9 => self.stdout.push((word & 0xff) as u8),
            0xA => return self.syscall(pc),
            opcode => {
                return self.fail(format!("invalid Toy16 opcode 0x{opcode:X} at 0x{pc:04X}"));
            }
        }
        Ok(StepOutcome::Stepped)
    }

    /// Toy16's minimal syscall surface, mirroring the x86-64 ABI: `r0` selects
    /// the call (`0` read, `1` write, `60` exit), with the same operand roles
    /// the Linux convention gives them. Only stdin/stdout descriptors are
    /// answered; anything else returns `-1` in `r0` rather than faking a host
    /// device that is not there.
    fn syscall(&mut self, pc: usize) -> Result<StepOutcome, MachineError> {
        match self.registers[0] {
            0 => {
                let count = usize::from(self.registers[3]);
                if self.registers[1] != 0 {
                    self.registers[0] = u16::MAX;
                    return Ok(StepOutcome::Stepped);
                }
                if self.input.is_empty() {
                    self.pc = pc as u16;
                    self.instructions = self.instructions.saturating_sub(1);
                    self.state = MachineState::AwaitingInput;
                    return Ok(StepOutcome::AwaitingInput);
                }
                let count = count.min(self.input.len());
                let start = usize::from(self.registers[2]);
                let end = start
                    .checked_add(count)
                    .ok_or_else(|| MachineError::new("Toy16 read buffer overflow"))?;
                if end > self.memory.len() {
                    return self.fail("Toy16 read buffer out of bounds");
                }
                for slot in &mut self.memory[start..end] {
                    *slot = self.input.pop_front().unwrap();
                }
                self.registers[0] = count as u16;
                Ok(StepOutcome::Stepped)
            }
            1 => {
                let count = usize::from(self.registers[3]);
                let start = usize::from(self.registers[2]);
                let end = start
                    .checked_add(count)
                    .ok_or_else(|| MachineError::new("Toy16 write buffer overflow"))?;
                if end > self.memory.len() {
                    return self.fail("Toy16 write buffer out of bounds");
                }
                if matches!(self.registers[1], 1 | 2) {
                    self.stdout.extend_from_slice(&self.memory[start..end]);
                    self.registers[0] = count as u16;
                } else {
                    self.registers[0] = u16::MAX;
                }
                Ok(StepOutcome::Stepped)
            }
            60 => {
                let code = self.registers[1] as i32;
                self.state = MachineState::Exited(code);
                Ok(StepOutcome::Exited(code))
            }
            number => self.fail(format!("unsupported Toy16 syscall {number}")),
        }
    }

    fn pipeline_instruction(&mut self, address: u64) -> PipelineOp<u16> {
        let Some(pc) = usize::try_from(address).ok() else {
            return Self::pipeline_fetch_fault(address);
        };
        if pc + 2 > self.memory.len() || pc % 2 != 0 {
            return Self::pipeline_fetch_fault(address);
        }
        self.cache.fetch(pc, &self.memory);
        let bytes = [self.memory[pc], self.memory[pc + 1]];
        let word = u16::from_le_bytes(bytes);
        let opcode = word >> 12;
        let register = |shift: u16| format!("r{}", (word >> shift) & 7);
        let class = match opcode {
            0 | 2 | 3 => PipelineInstructionClass::Alu,
            4 => PipelineInstructionClass::Load,
            5 => PipelineInstructionClass::Store,
            6 => PipelineInstructionClass::Jump,
            7 => PipelineInstructionClass::Branch,
            1 | 8 | 9 | 0xA => PipelineInstructionClass::System,
            _ => PipelineInstructionClass::Unknown,
        };
        let mut instruction = PipelineOp::new(
            address,
            Toy16Codec
                .disassemble(address, &bytes)
                .unwrap_or_else(|| format!(".word 0x{word:04X}")),
            class,
            word,
        );
        match opcode {
            2 | 4 => {
                let destination = register(9);
                instruction.destination = Some(destination.clone());
                instruction.writes.push(destination);
            }
            3 => {
                let destination = register(9);
                instruction.destination = Some(destination.clone());
                instruction.writes.push(destination);
                instruction.sources.extend([register(6), register(3)]);
            }
            5 | 7 | 8 => instruction.sources.push(register(9)),
            0xA => {
                instruction.destination = Some("r0".into());
                instruction.sources.extend(["r0", "r1", "r2", "r3"].map(str::to_string));
                instruction.writes.push("r0".into());
            }
            _ => {}
        }
        // Both control transfers encode their target, so the front end can
        // speculate rather than always assuming the fall-through path.
        match opcode {
            6 => {
                instruction.branch = true;
                instruction.branch_target = Some(u64::from((word & 0x0fff) * 2));
            }
            7 => {
                instruction.branch = true;
                instruction.branch_target = Some(u64::from((word & 0xff) * 2));
            }
            _ => {}
        }
        instruction
    }

    fn pipeline_fetch_fault(address: u64) -> PipelineOp<u16> {
        PipelineOp::faulted(address, "Toy16 fetch out of bounds or unaligned", 0)
    }
}

impl Machine for Toy16Machine {
    fn architecture_id(&self) -> &'static str {
        ID
    }
    fn reset(&mut self) {
        let image = self.image.clone();
        let size = self.memory.len();
        // The datapath the user tuned is not part of the program being
        // reloaded. Rebuilding the machine from defaults used to throw away
        // every stage, unit and latency setting on every reset, carrying only
        // `enabled` across — so carry the whole pipeline instead and clear just
        // its execution state, which is the part a run does own.
        let mut fresh = Self::new(size);
        std::mem::swap(&mut fresh.pipeline, &mut self.pipeline);
        *self = fresh;
        self.pipeline.reset(u64::from(self.pc));
        if let Some(image) = image
            && let Err(error) = self.load(&image)
        {
            self.state = MachineState::Faulted;
            self.fault = Some(error.to_string());
        }
    }

    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError> {
        image.expect_architecture(ID)?;
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
        self.input.clear();
        self.fault = None;
        self.cache.reset();
        self.pipeline.reset(u64::from(entry));
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
        let pc = self.pc as usize;
        if pc + 2 > self.memory.len() {
            return self.fail("Toy16 fetch out of bounds");
        }
        self.cache.fetch(pc, &self.memory);
        let word = u16::from_le_bytes([self.memory[pc], self.memory[pc + 1]]);
        let outcome = self.execute_instruction(pc, word)?;
        if outcome == StepOutcome::Halted {
            self.pipeline.halt();
        }
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
            let outcome =
                match self.execute_instruction(instruction.address as usize, instruction.payload) {
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
                retired_address: Some(instruction.address),
                outcome,
            };
            if outcome == StepOutcome::Halted {
                self.pipeline.halt();
                return Ok(result);
            }
            redirect = self.pipeline.resolve(&instruction, u64::from(self.pc));
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
            .ok_or_else(|| MachineError::new(format!("unknown Toy16 register '{name}'")))?;
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
        Some(&Toy16Codec)
    }

    fn caches(&self) -> Option<&dyn CacheHierarchy> {
        Some(&self.cache)
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

    fn pipeline_dynamic(&self) -> Option<&dyn PipelineDynamicInspect> {
        self.pipeline.dynamic()
    }
}

// ── Capabilities ──────────────────────────────────────────────────────────────
//
// Toy16 exists to keep the capability surface honest: one bank instead of two,
// eight registers instead of thirty-two, sixteen bits instead of thirty-two,
// two-byte instructions instead of four, and no calling convention at all. If
// anything here needs a special case in a host, the surface leaked RISC-V.

static BANKS: [RegisterBank; 1] = [RegisterBank::integer("r", "Registers", 8, 16)];

impl RegisterFile for Toy16Machine {
    fn banks(&self) -> &[RegisterBank] {
        &BANKS
    }

    fn read(&self, id: RegisterId) -> Option<u64> {
        (id.bank == 0)
            .then(|| self.registers.get(id.index).map(|v| u64::from(*v)))
            .flatten()
    }

    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError> {
        let value = u16::try_from(value)
            .map_err(|_| MachineError::new("register value exceeds 16 bits"))?;
        let slot = (id.bank == 0)
            .then(|| self.registers.get_mut(id.index))
            .flatten()
            .ok_or_else(|| MachineError::new("no such Toy16 register"))?;
        *slot = value;
        Ok(())
    }

    fn program_counter(&self) -> u64 {
        u64::from(self.pc)
    }

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError> {
        self.pc = u16::try_from(value).map_err(|_| MachineError::new("PC exceeds 16 bits"))?;
        Ok(())
    }
}

impl MemoryInspect for Toy16Machine {
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

    /// Toy16 has no stack pointer and no heap, so it names one place rather
    /// than three: where the loaded image put its data, or the bottom of memory
    /// when the program has no data at all.
    fn regions(&self) -> Vec<MemoryRegion> {
        let data = self
            .image
            .as_ref()
            .and_then(|image| image.data_segment())
            .map(|segment| segment.address);
        vec![match data {
            Some(address) => MemoryRegion::new("Data", address),
            None => MemoryRegion::new("Memory", 0),
        }]
    }
}

pub struct Toy16Codec;

impl InstructionCodec for Toy16Codec {
    fn instruction_width(&self, _address: u64, _bytes: &[u8]) -> usize {
        2
    }

    fn disassemble(&self, _address: u64, bytes: &[u8]) -> Option<String> {
        let word = u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?);
        let reg = |shift: u16| (word >> shift) & 7;
        Some(match word >> 12 {
            0 => "nop".to_string(),
            1 => "halt".to_string(),
            2 => format!("li r{}, {}", reg(9), word & 0xff),
            3 => format!("add r{}, r{}, r{}", reg(9), reg(6), reg(3)),
            4 => format!("load r{}, [0x{:02X}]", reg(9), word & 0xff),
            5 => format!("store r{}, [0x{:02X}]", reg(9), word & 0xff),
            6 => format!("jmp 0x{:04X}", (word & 0x0fff) * 2),
            7 => format!("jnz r{}, 0x{:04X}", reg(9), (word & 0xff) * 2),
            8 => format!("print r{}", reg(9)),
            9 => format!("putc {}", word & 0xff),
            0xA => "syscall".to_string(),
            _ => return None,
        })
    }

    fn assemble(&self, address: u64, text: &str) -> Result<Vec<u8>, Diagnostic> {
        let image = Toy16Assembler.assemble(text, address)?;
        Ok(image
            .executable_bytes()
            .map(<[u8]>::to_vec)
            .unwrap_or_default())
    }

    fn inspect(&self, address: u64, bytes: &[u8]) -> Option<InstructionInfo> {
        let word = u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?);
        let opcode = word >> 12;
        let mut fields = vec![InstructionField {
            name: "opcode",
            value: format!("0x{opcode:X}"),
        }];
        match opcode {
            2 => {
                fields.push(InstructionField {
                    name: "rd",
                    value: format!("r{}", (word >> 9) & 7),
                });
                fields.push(InstructionField {
                    name: "imm",
                    value: (word & 0xff).to_string(),
                });
            }
            3 => {
                fields.push(InstructionField {
                    name: "rd",
                    value: format!("r{}", (word >> 9) & 7),
                });
                fields.push(InstructionField {
                    name: "rs1",
                    value: format!("r{}", (word >> 6) & 7),
                });
                fields.push(InstructionField {
                    name: "rs2",
                    value: format!("r{}", (word >> 3) & 7),
                });
            }
            4 | 5 | 7 => {
                fields.push(InstructionField {
                    name: "reg",
                    value: format!("r{}", (word >> 9) & 7),
                });
                fields.push(InstructionField {
                    name: "address",
                    value: format!("0x{:04X}", (word & 0xff) * if opcode == 7 { 2 } else { 1 }),
                });
            }
            6 => fields.push(InstructionField {
                name: "target",
                value: format!("0x{:04X}", (word & 0x0fff) * 2),
            }),
            8 => fields.push(InstructionField {
                name: "rs",
                value: format!("r{}", (word >> 9) & 7),
            }),
            9 => fields.push(InstructionField {
                name: "ascii",
                value: (word & 0xff).to_string(),
            }),
            0 | 1 | 0xA => {}
            _ => return None,
        }
        Some(InstructionInfo {
            mnemonic: self.disassemble(address, bytes)?,
            class: match opcode {
                4 => "Load",
                5 => "Store",
                6 | 7 => "Control",
                8 | 9 => "I/O",
                1 | 0xA => "System",
                _ => "ALU",
            },
            encoding: u64::from(word),
            encoding_bits: 16,
            fields,
            layout: encoding_layout(opcode),
        })
    }
}

/// Where each opcode puts its operands in the 16-bit word, most-significant
/// segment first. A host draws this as a field map, exactly as it does for a
/// 32-bit ISA — the widths differ, nothing else does.
fn encoding_layout(opcode: u16) -> Vec<InstructionBitField> {
    use BitRole::*;
    let seg = InstructionBitField::new;
    let op = seg("opcode", 4, Opcode);
    match opcode {
        // li rd, imm — the immediate is 8 bits, so bit 8 is unused, as in the
        // load/store forms below.
        2 => vec![
            op,
            seg("rd", 3, Destination),
            seg("—", 1, Other),
            seg("imm", 8, Immediate),
        ],
        // alu rd, rs1, rs2
        3 => vec![
            op,
            seg("rd", 3, Destination),
            seg("rs1", 3, Source),
            seg("rs2", 3, Source),
            seg("—", 3, Other),
        ],
        // load / store / jump-through-register
        4 | 5 | 7 => vec![
            op,
            seg("reg", 3, Source),
            seg("—", 1, Other),
            seg("address", 8, Immediate),
        ],
        // jump
        6 => vec![op, seg("target", 12, Immediate)],
        // print rs
        8 => vec![op, seg("rs", 3, Source), seg("—", 9, Other)],
        // putc ascii
        9 => vec![op, seg("—", 4, Other), seg("ascii", 8, Immediate)],
        // nop / halt carry no operands
        _ => vec![op, seg("—", 12, Other)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_writes_memory_to_stdout_and_exits() {
        let source = "
            li r0, 1
            li r1, 1
            li r2, 0x80
            li r3, 2
            syscall
            li r0, 60
            li r1, 7
            syscall
        ";
        let image = Toy16Assembler.assemble(source, 0).unwrap();
        let mut machine = Toy16Machine::new(MEMORY_SIZE);
        machine.load(&image).unwrap();
        machine.write_memory(0x80, b"hi").unwrap();
        assert_eq!(machine.run(64).unwrap(), StepOutcome::Exited(7));
        assert_eq!(machine.snapshot().stdout, b"hi");
    }

    #[test]
    fn syscall_reads_host_input_into_memory() {
        let source = "
            li r0, 0
            li r1, 0
            li r2, 0x90
            li r3, 3
            syscall
            li r0, 60
            li r1, 0
            syscall
        ";
        let image = Toy16Assembler.assemble(source, 0).unwrap();
        let mut machine = Toy16Machine::new(MEMORY_SIZE);
        machine.load(&image).unwrap();
        machine.push_input("ab");
        assert_eq!(machine.run(64).unwrap(), StepOutcome::Exited(0));
        assert_eq!(machine.read_memory(0x90, 3).unwrap(), b"ab\n");
    }

    #[test]
    fn syscall_round_trips_through_the_codec() {
        let bytes = Toy16Codec.assemble(0, "syscall").unwrap();
        assert_eq!(bytes, [0x00, 0xA0]);
        assert_eq!(Toy16Codec.disassemble(0, &bytes).as_deref(), Some("syscall"));
    }
}
