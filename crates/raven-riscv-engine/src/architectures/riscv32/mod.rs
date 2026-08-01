//! Raven's production RV32IMAF backend.
//!
//! The complete ISA implementation lives under this module. [`falcon`] owns the
//! CPU, decoder, pipeline, cache, MMU, JIT, syscalls, and execution runtime;
//! this file exposes the ISA-neutral [`Architecture`] adapter.

pub mod falcon;

pub use crate::falcon::{CacheController, Cpu, jit::BackendKind};

use crate::capability::{
    CacheHierarchy, CacheLevelView, CacheRole, CacheSetView, InstructionCodec, MemoryInspect,
    MemoryRegion, PipelineInspect, RegisterBank, RegisterFile, RegisterId,
};
use crate::falcon::cache::CacheConfig;
use crate::falcon::jit::ExecOutcome;
use crate::falcon::machine::Machine as FalconMachine;
use crate::falcon::machine::types::MemWidth;
use crate::falcon::memory::Bus;
use crate::falcon::pipeline::PipelineSimState;
use crate::host::Console;
use crate::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, Assembler, Diagnostic,
    Endianness, Machine, MachineError, MachineSnapshot, MachineState, ProgramImage, ProgramSegment,
    RegisterValue, SourceMap, StepOutcome, ZeroFill,
};
use std::sync::{Arc, OnceLock};

pub const ID: &str = "riscv32";

static DESCRIPTOR: ArchitectureDescriptor = ArchitectureDescriptor {
    id: ID,
    display_name: "RISC-V 32 (RV32IMAF)",
    source_extension: "s",
    address_bits: 32,
    instruction_alignment: 4,
    default_memory_size: 16 * 1024 * 1024,
    endianness: Endianness::Little,
    capabilities: ArchitectureCapabilities {
        elf: true,
        cache: true,
        virtual_memory: true,
        jit: true,
        pipeline: true,
        multicore: true,
        floating_point: true,
        guided_learning: true,
    },
};

#[derive(Default)]
pub struct RiscV32;

pub fn architecture() -> Arc<dyn Architecture> {
    static INSTANCE: OnceLock<Arc<RiscV32>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(RiscV32)).clone()
}

impl Architecture for RiscV32 {
    fn descriptor(&self) -> &'static ArchitectureDescriptor {
        &DESCRIPTOR
    }
    fn assembler(&self) -> &'static dyn Assembler {
        &RiscV32Assembler
    }
    fn default_source(&self) -> &'static str {
        ".text\n    li a0, 42\n    print a0\n    halt\n"
    }
    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        if memory_size > u32::MAX as usize {
            return Err(MachineError::new(
                "RV32 memory size exceeds 32-bit address space",
            ));
        }
        Ok(Box::new(RiscV32Machine::new(memory_size)))
    }
}

pub struct RiscV32Assembler;

impl Assembler for RiscV32Assembler {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn instruction_forms(&self, mnemonic: &str) -> &'static [&'static str] {
        match mnemonic.to_ascii_lowercase().as_str() {
            "nop" | "ret" | "ecall" | "ebreak" | "halt" | "fence" => &[""],
            "mv" | "neg" | "not" | "seqz" | "snez" | "sltz" | "sgtz" => &["rd, rs"],
            "li" | "lui" | "auipc" => &["rd, imm"],
            "j" | "call" => &["label"],
            "jr" | "push" | "print" | "random" => &["rs"],
            "pop" => &["rd"],
            "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt"
            | "sltu" | "mul" | "mulh" | "mulhsu" | "mulhu" | "div" | "divu" | "rem"
            | "remu" => &["rd, rs1, rs2"],
            "addi" | "andi" | "ori" | "xori" | "slti" | "sltiu" | "subi" => &["rd, rs1, imm"],
            "slli" | "srli" | "srai" => &["rd, rs1, shamt"],
            "lb" | "lh" | "lw" | "lbu" | "lhu" => &["rd, imm(rs1)"],
            "sb" | "sh" | "sw" => &["rs2, imm(rs1)"],
            "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" | "bgt" | "ble" | "bgtu"
            | "bleu" => &["rs1, rs2, label"],
            "bez" | "beqz" | "bnez" | "bltz" | "bgez" | "blez" | "bgtz" => &["rs, label"],
            "jal" => &["label", "rd, label"],
            "jalr" => &["rd, rs1, imm"],
            "la" => &["rd, label"],
            "print_str" | "printstr" | "printstring" | "print_str_ln" | "println" | "read"
            | "read_byte" | "readbyte" | "read_half" | "readhalf" | "read_word" | "readword" => &["label"],
            "random_bytes" | "randombytes" => &["label, n"],
            "flw" => &["frd, imm(rs1)"],
            "fsw" => &["frs2, imm(rs1)"],
            "fadd.s" | "fsub.s" | "fmul.s" | "fdiv.s" | "fmin.s" | "fmax.s" | "fsgnj.s"
            | "fsgnjn.s" | "fsgnjx.s" => &["frd, frs1, frs2"],
            "fsqrt.s" | "fmv.s" | "fneg.s" | "fabs.s" => &["frd, frs"],
            "feq.s" | "flt.s" | "fle.s" => &["rd, frs1, frs2"],
            "fcvt.w.s" | "fcvt.wu.s" => &["rd, frs1", "rd, frs1, rm"],
            "fcvt.s.w" | "fcvt.s.wu" | "fmv.w.x" => &["frd, rs1"],
            "fmv.x.w" | "fclass.s" => &["rd, frs1"],
            "fmadd.s" | "fmsub.s" | "fnmsub.s" | "fnmadd.s" => &["frd, frs1, frs2, frs3"],
            _ => &[],
        }
    }

    fn is_register(&self, token: &str) -> bool {
        crate::falcon::asm::utils::parse_reg(token).is_some()
            || crate::falcon::asm::utils::parse_freg(token).is_some()
    }

    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic> {
        let base = u32::try_from(base_address)
            .map_err(|_| Diagnostic::new(None, "RV32 base address exceeds 32 bits"))?;
        let program = crate::falcon::asm::assemble(source, base)
            .map_err(|e| Diagnostic::new(Some(e.line), e.msg))?;
        let text_bytes = program
            .text
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        // The data segment is emitted even when empty: `data_base` is where
        // RV32 puts writable memory and where the guest heap starts, and an
        // image that dropped it would leave both to be guessed by whoever
        // loads it.
        let segments = vec![
            ProgramSegment {
                address: base_address,
                bytes: text_bytes,
                executable: true,
                writable: false,
            },
            ProgramSegment {
                address: u64::from(program.data_base),
                bytes: program.data.clone(),
                executable: false,
                writable: true,
            },
        ];
        let zero_fill = (program.bss_size > 0)
            .then(|| ZeroFill {
                address: u64::from(program.data_base) + program.data.len() as u64,
                size: u64::from(program.bss_size),
            })
            .into_iter()
            .collect();
        Ok(ProgramImage {
            architecture: ID.into(),
            entry: base_address,
            segments,
            zero_fill,
            source_map: SourceMap {
                comments: widen_map(program.comments),
                block_comments: widen_map(program.block_comments),
                labels: widen_map(program.labels),
                label_to_line: program.label_to_line,
                line_addresses: program
                    .line_addrs
                    .into_iter()
                    .map(|(k, v)| (k, u64::from(v)))
                    .collect(),
                explicit_halts: program.halt_pcs.into_iter().map(u64::from).collect(),
            },
        })
    }
}

fn widen_map<T>(map: std::collections::HashMap<u32, T>) -> std::collections::HashMap<u64, T> {
    map.into_iter().map(|(k, v)| (u64::from(k), v)).collect()
}

// ── Loading an image into RV32 memory ─────────────────────────────────────────
//
// The three hosts that run RV32 programs — [`RiscV32Machine`], the CLI, and the
// TUI's Run tab — used to each carry their own copy of "write the segments,
// zero the bss, place the stack pointer and the heap break". The helpers below
// are that copy, once: a host supplies the memory and gets identical placement,
// identical bounds checks, and identical error text.

/// Write `image`'s segments and zero-fill regions into RV32 memory.
///
/// Every address is checked against `mem`'s size *before* the first byte is
/// written, so an image that does not fit leaves memory untouched.
pub fn install_image(mem: &mut impl Bus, image: &ProgramImage) -> Result<(), MachineError> {
    image.expect_architecture(ID)?;
    let capacity = u64::from(mem.mem_len());
    let placement = |address: u64, len: u64, what: &str| -> Result<u32, MachineError> {
        if address.checked_add(len).is_none_or(|end| end > capacity) {
            return Err(MachineError::new(format!(
                "{what} at 0x{address:X} (+{len} bytes) does not fit in {capacity} bytes of RAM"
            )));
        }
        u32::try_from(address)
            .map_err(|_| MachineError::new(format!("{what} address exceeds RV32")))
    };

    let segments = image
        .segments
        .iter()
        .map(|segment| placement(segment.address, segment.bytes.len() as u64, "segment"))
        .collect::<Result<Vec<_>, _>>()?;
    let fills = image
        .zero_fill
        .iter()
        .map(|fill| {
            let address = placement(fill.address, fill.size, "zero-fill")?;
            let size = u32::try_from(fill.size)
                .map_err(|_| MachineError::new("zero-fill size exceeds RV32"))?;
            Ok((address, size))
        })
        .collect::<Result<Vec<_>, MachineError>>()?;

    for (address, segment) in segments.into_iter().zip(&image.segments) {
        crate::falcon::program::load_bytes(mem, address, &segment.bytes)
            .map_err(|e| MachineError::new(e.to_string()))?;
    }
    for (address, size) in fills {
        crate::falcon::program::zero_bytes(mem, address, size)
            .map_err(|e| MachineError::new(e.to_string()))?;
    }
    Ok(())
}

/// Where the guest heap starts once `image` is loaded: the first 16-byte
/// boundary past everything the image occupies.
pub fn heap_break_after(image: &ProgramImage) -> u32 {
    let end = u32::try_from(image.end_address()).unwrap_or(u32::MAX);
    end.wrapping_add(15) & !15
}

/// Decode a binary the user opened into an RV32 [`ProgramImage`].
///
/// Understands both FALC container versions and falls back to treating the
/// bytes as a flat block of machine code entered at `base`. ELF is deliberately
/// *not* handled here: it carries a symbol table and a section list that a
/// [`ProgramImage`] cannot express, so ELF hosts keep using
/// [`crate::falcon::program::load_elf`].
pub fn image_from_binary(bytes: &[u8], base: u64) -> Result<ProgramImage, MachineError> {
    if bytes.starts_with(b"FALC") {
        let image = ProgramImage::from_falc(bytes)?;
        image.expect_architecture(ID)?;
        return Ok(image);
    }
    Ok(ProgramImage {
        architecture: ID.into(),
        entry: base,
        segments: vec![ProgramSegment {
            address: base,
            bytes: bytes.to_vec(),
            executable: true,
            writable: false,
        }],
        zero_fill: Vec::new(),
        source_map: SourceMap::default(),
    })
}

/// RV32 as the rest of Raven sees it.
///
/// This owns the same [`FalconMachine`] the TUI drives directly, rather than a
/// reduced CPU of its own: everything the full runtime models — the pipeline,
/// the cache hierarchy, the MMU, the step-back journal — is reachable through
/// the [`Machine`] trait, so a host never has to special-case this backend to
/// get RV32's real behaviour.
pub struct RiscV32Machine {
    machine: FalconMachine<PipelineSimState>,
    console: Console,
    memory_size: usize,
    image: Option<ProgramImage>,
    state: MachineState,
    stdout: Vec<u8>,
    fault: Option<String>,
}

impl RiscV32Machine {
    fn new(memory_size: usize) -> Self {
        Self {
            machine: FalconMachine::new(
                Self::initial_cpu(memory_size),
                Self::initial_memory(memory_size),
                PipelineSimState::default(),
            ),
            console: Console::default(),
            memory_size,
            image: None,
            state: MachineState::Ready,
            stdout: Vec::new(),
            fault: None,
        }
    }

    /// `sp` starts one past the top of RAM, which is RV32's "empty stack".
    fn initial_cpu(memory_size: usize) -> Cpu {
        let mut cpu = Cpu::default();
        cpu.write(2, memory_size as u32);
        cpu
    }

    fn initial_memory(memory_size: usize) -> CacheController {
        CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            memory_size,
        )
    }

    fn cpu(&self) -> &Cpu {
        self.machine.cpu()
    }

    fn mem(&self) -> &CacheController {
        self.machine.mem()
    }

    fn checked_range(&self, address: u64, bytes: usize) -> Result<u32, MachineError> {
        let start =
            u32::try_from(address).map_err(|_| MachineError::new("address exceeds RV32"))?;
        let end = address
            .checked_add(bytes as u64)
            .ok_or_else(|| MachineError::new("address overflow"))?;
        if end > self.memory_size as u64 {
            return Err(MachineError::new("memory access out of bounds"));
        }
        Ok(start)
    }
}

impl Machine for RiscV32Machine {
    fn architecture_id(&self) -> &'static str {
        ID
    }

    fn reset(&mut self) {
        let image = self.image.clone();
        *self = Self::new(self.memory_size);
        if let Some(image) = image
            && let Err(error) = self.load(&image)
        {
            self.state = MachineState::Faulted;
            self.fault = Some(error.to_string());
        }
    }

    /// Load `image` atomically: memory is built and filled on the side, so a
    /// rejected image leaves the previous program running.
    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError> {
        image.expect_architecture(ID)?;
        let entry =
            u32::try_from(image.entry).map_err(|_| MachineError::new("entry exceeds RV32"))?;
        let mut mem = Self::initial_memory(self.memory_size);
        install_image(&mut mem.ram, image)?;
        mem.invalidate_all();
        mem.reset_stats();

        let mut cpu = Self::initial_cpu(self.memory_size);
        cpu.pc = entry;
        cpu.heap_break = heap_break_after(image);

        // A fresh runtime rather than an in-place edit: loading a program must
        // not leave the previous one's pipeline latches or step-back history
        // reachable behind the new image.
        self.machine = FalconMachine::new(cpu, mem, PipelineSimState::default());
        self.image = Some(image.clone());
        self.state = MachineState::Ready;
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
        self.state = MachineState::Running;
        // Journaled, so a host that offers step-back gets it for free; the
        // MMU is re-pointed at this hart's `satp` first.
        self.machine.sync_mmu();
        let outcome = self.machine.step_interpreted(&mut self.console);
        let emitted = std::mem::take(&mut self.machine.cpu_mut_unjournaled().stdout);
        self.stdout.extend(emitted);
        match outcome {
            Ok(ExecOutcome::Stepped { .. }) => Ok(StepOutcome::Stepped),
            Ok(ExecOutcome::AwaitingInput) => {
                self.state = MachineState::AwaitingInput;
                Ok(StepOutcome::AwaitingInput)
            }
            Ok(ExecOutcome::Halted)
                if self.cpu().exit_code.is_some()
                    || self.cpu().ebreak_hit
                    || self.cpu().local_exit =>
            {
                let code = self.cpu().exit_code.unwrap_or(0) as i32;
                self.state = MachineState::Exited(code);
                Ok(StepOutcome::Exited(code))
            }
            Ok(ExecOutcome::Halted) => {
                self.state = MachineState::Halted;
                Ok(StepOutcome::Halted)
            }
            Err(error) => {
                let message = error.to_string();
                self.state = MachineState::Faulted;
                self.fault = Some(message.clone());
                Err(MachineError::new(message))
            }
        }
    }

    fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            architecture: ID,
            pc: u64::from(self.cpu().pc),
            registers: RegisterFile::entries(self)
                .into_iter()
                .map(|entry| RegisterValue {
                    name: entry.name,
                    value: entry.value,
                    bits: entry.bits,
                })
                .collect(),
            state: self.state,
            instructions: self.cpu().instr_count,
            stdout: self.stdout.clone(),
            fault: self.fault.clone(),
        }
    }

    fn read_memory(&self, address: u64, bytes: usize) -> Result<Vec<u8>, MachineError> {
        let start = self.checked_range(address, bytes)?;
        (0..bytes)
            .map(|offset| {
                crate::falcon::memory::Bus::load8(self.mem(), start + offset as u32)
                    .map_err(|e| MachineError::new(e.to_string()))
            })
            .collect()
    }

    fn write_memory(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        let start = self.checked_range(address, bytes.len())?;
        for (offset, value) in bytes.iter().copied().enumerate() {
            self.machine
                .write_mem(start + offset as u32, MemWidth::B1, u64::from(value))
                .map_err(|e| MachineError::new(e.to_string()))?;
        }
        Ok(())
    }

    fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError> {
        if name.eq_ignore_ascii_case("pc") {
            return self.set_program_counter(value);
        }
        let id = RegisterFile::resolve(self, name)
            .ok_or_else(|| MachineError::new(format!("unknown RV32 register '{name}'")))?;
        RegisterFile::write(self, id, value)
    }

    fn push_input(&mut self, line: &str) {
        self.console.push_input(line);
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
        Some(&RiscV32Codec)
    }
    fn caches(&self) -> Option<&dyn CacheHierarchy> {
        Some(self)
    }
    /// `None` while the pipeline model is disabled, which is how a host knows
    /// to hide the tab rather than draw five empty stages.
    fn pipeline(&self) -> Option<&dyn PipelineInspect> {
        self.machine.pipeline_inspect()
    }
}

// ── Capabilities ──────────────────────────────────────────────────────────────

/// Registers delegate wholesale to the runtime: the same banks, aliases and
/// journaled writes a host sees when it drives the [`FalconMachine`] directly.
impl RegisterFile for RiscV32Machine {
    fn banks(&self) -> &[RegisterBank] {
        self.machine.banks()
    }

    fn read(&self, id: RegisterId) -> Option<u64> {
        self.machine.read(id)
    }

    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError> {
        self.machine.write(id, value)
    }

    fn program_counter(&self) -> u64 {
        self.machine.program_counter()
    }

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError> {
        self.machine.set_program_counter(value)
    }

    fn alias(&self, id: RegisterId) -> Option<&'static str> {
        self.machine.alias(id)
    }

    fn resolve(&self, name: &str) -> Option<RegisterId> {
        self.machine.resolve(name)
    }
}

impl MemoryInspect for RiscV32Machine {
    /// The size the machine was built with, which is authoritative even before
    /// an image is loaded.
    fn size(&self) -> u64 {
        self.memory_size as u64
    }

    fn peek(&self, address: u64, bytes: usize) -> Vec<u8> {
        self.machine.peek(address, bytes)
    }

    fn poke(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        self.write_memory(address, bytes)
    }

    /// Same heap and stack the runtime reports, but `.data` comes from the
    /// loaded image rather than standing in the PC for it — this adapter is the
    /// layer that keeps the image around.
    fn regions(&self) -> Vec<MemoryRegion> {
        let top = MemoryInspect::size(self).saturating_sub(1);
        let mut regions = self.machine.regions();
        let data = self
            .image
            .as_ref()
            .and_then(|image| image.data_segment())
            .map_or_else(|| u64::from(self.cpu().pc), |segment| segment.address);
        if let Some(first) = regions.first_mut() {
            *first = MemoryRegion::new("Data", data.min(top));
        }
        regions
    }
}

impl CacheHierarchy for RiscV32Machine {
    fn level_count(&self) -> usize {
        self.mem().level_count()
    }

    fn cache(&self, level: usize, role: CacheRole) -> Option<CacheLevelView<'_>> {
        self.mem().cache(level, role)
    }

    fn set(&self, level: usize, role: CacheRole, set: usize) -> Option<CacheSetView<'_>> {
        self.mem().set(level, role, set)
    }
}

/// Stateless: decoding an RV32 word depends only on the word.
pub struct RiscV32Codec;

impl InstructionCodec for RiscV32Codec {
    /// RV32 without the C extension is fixed-width, so the bytes are not
    /// consulted — a compressed-instruction backend would look at them.
    fn instruction_width(&self, _address: u64, _bytes: &[u8]) -> usize {
        4
    }

    fn disassemble(&self, _address: u64, bytes: &[u8]) -> Option<String> {
        let word = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        crate::falcon::decoder::decode(word).ok()?;
        Some(crate::falcon::decoder::disasm(word))
    }

    fn assemble(&self, address: u64, text: &str) -> Result<Vec<u8>, Diagnostic> {
        let base =
            u32::try_from(address).map_err(|_| Diagnostic::new(None, "address exceeds RV32"))?;
        let program = crate::falcon::asm::assemble(text, base)
            .map_err(|e| Diagnostic::new(Some(e.line), e.msg))?;
        Ok(program
            .text
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect())
    }
}
