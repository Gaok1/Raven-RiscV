//! Raven's production RV32IMAF backend.

pub use crate::falcon::*;

use crate::falcon::cache::CacheConfig;
use crate::falcon::jit::{ExecCtx, ExecOutcome, InterpreterBackend};
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
        let mut segments = vec![ProgramSegment {
            address: base_address,
            bytes: text_bytes,
            executable: true,
            writable: false,
        }];
        if !program.data.is_empty() {
            segments.push(ProgramSegment {
                address: u64::from(program.data_base),
                bytes: program.data.clone(),
                executable: false,
                writable: true,
            });
        }
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

pub struct RiscV32Machine {
    cpu: Cpu,
    mem: CacheController,
    console: Console,
    interpreter: InterpreterBackend,
    memory_size: usize,
    image: Option<ProgramImage>,
    state: MachineState,
    stdout: Vec<u8>,
    fault: Option<String>,
}

impl RiscV32Machine {
    fn new(memory_size: usize) -> Self {
        let mut cpu = Cpu::default();
        cpu.write(2, memory_size as u32);
        Self {
            cpu,
            mem: CacheController::new(
                CacheConfig::default(),
                CacheConfig::default(),
                vec![],
                memory_size,
            ),
            console: Console::default(),
            interpreter: InterpreterBackend::new(),
            memory_size,
            image: None,
            state: MachineState::Ready,
            stdout: Vec::new(),
            fault: None,
        }
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
            u32::try_from(image.entry).map_err(|_| MachineError::new("entry exceeds RV32"))?;
        for segment in &image.segments {
            self.checked_range(segment.address, segment.bytes.len())?;
        }
        let fills = image
            .zero_fill
            .iter()
            .map(|fill| {
                let size = usize::try_from(fill.size)
                    .map_err(|_| MachineError::new("zero-fill is too large"))?;
                let address = self.checked_range(fill.address, size)?;
                let size = u32::try_from(fill.size)
                    .map_err(|_| MachineError::new("zero-fill exceeds RV32"))?;
                Ok((address, size))
            })
            .collect::<Result<Vec<_>, MachineError>>()?;

        self.cpu = Cpu::default();
        self.cpu.pc = entry;
        self.cpu.write(2, self.memory_size as u32);
        self.mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            self.memory_size,
        );
        for segment in &image.segments {
            let address = self.checked_range(segment.address, segment.bytes.len())?;
            crate::falcon::program::load_bytes(&mut self.mem.ram, address, &segment.bytes)
                .map_err(|e| MachineError::new(e.to_string()))?;
        }
        for (address, size) in fills {
            crate::falcon::program::zero_bytes(&mut self.mem.ram, address, size)
                .map_err(|e| MachineError::new(e.to_string()))?;
        }
        self.mem.invalidate_all();
        self.mem.reset_stats();
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
        let outcome = {
            let mut ctx = ExecCtx::new(&mut self.cpu, &mut self.mem, &mut self.console);
            crate::falcon::jit::ExecutionBackend::run_until_yield(&mut self.interpreter, &mut ctx)
        };
        self.stdout.append(&mut self.cpu.stdout);
        match outcome {
            Ok(ExecOutcome::Stepped { .. }) => Ok(StepOutcome::Stepped),
            Ok(ExecOutcome::AwaitingInput) => {
                self.state = MachineState::AwaitingInput;
                Ok(StepOutcome::AwaitingInput)
            }
            Ok(ExecOutcome::Halted)
                if self.cpu.exit_code.is_some() || self.cpu.ebreak_hit || self.cpu.local_exit =>
            {
                let code = self.cpu.exit_code.unwrap_or(0) as i32;
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
        let mut registers = (0u8..32)
            .map(|i| RegisterValue {
                name: format!("x{i}"),
                value: u64::from(self.cpu.read(i)),
                bits: 32,
            })
            .collect::<Vec<_>>();
        registers.extend((0u8..32).map(|i| RegisterValue {
            name: format!("f{i}"),
            value: u64::from(self.cpu.fread_bits(i)),
            bits: 32,
        }));
        MachineSnapshot {
            architecture: ID,
            pc: u64::from(self.cpu.pc),
            registers,
            state: self.state,
            instructions: self.cpu.instr_count,
            stdout: self.stdout.clone(),
            fault: self.fault.clone(),
        }
    }

    fn read_memory(&self, address: u64, bytes: usize) -> Result<Vec<u8>, MachineError> {
        let start = self.checked_range(address, bytes)?;
        (0..bytes)
            .map(|offset| {
                crate::falcon::memory::Bus::load8(&self.mem, start + offset as u32)
                    .map_err(|e| MachineError::new(e.to_string()))
            })
            .collect()
    }

    fn write_memory(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        let start = self.checked_range(address, bytes.len())?;
        for (offset, value) in bytes.iter().copied().enumerate() {
            crate::falcon::memory::Bus::store8(&mut self.mem, start + offset as u32, value)
                .map_err(|e| MachineError::new(e.to_string()))?;
        }
        Ok(())
    }

    fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError> {
        let value = u32::try_from(value)
            .map_err(|_| MachineError::new("register value exceeds 32 bits"))?;
        if let Some(index) = crate::falcon::asm::utils::parse_reg(name) {
            if index == 0 {
                return Err(MachineError::new("x0 is immutable"));
            }
            self.cpu.write(index, value);
            return Ok(());
        }
        if let Some(index) = crate::falcon::asm::utils::parse_freg(name) {
            self.cpu.fwrite_bits(index, value);
            return Ok(());
        }
        if name.eq_ignore_ascii_case("pc") {
            self.cpu.pc = value;
            return Ok(());
        }
        Err(MachineError::new(format!("unknown RV32 register '{name}'")))
    }

    fn push_input(&mut self, line: &str) {
        self.console.push_input(line);
    }
}
