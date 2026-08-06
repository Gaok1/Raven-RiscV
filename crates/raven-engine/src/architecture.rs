use crate::capability::{
    AddressTranslation, CacheHierarchy, InstructionCodec, MemoryInspect, PipelineControl,
    PipelineDynamicInspect, PipelineInspect, PipelineTuning, RegisterFile,
};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureCapabilities {
    pub elf: bool,
    pub cache: bool,
    pub virtual_memory: bool,
    pub jit: bool,
    pub pipeline: bool,
    pub multicore: bool,
    pub floating_point: bool,
    /// A program can call into the host through a syscall interface. A backend
    /// without one has no syscall reference for a host to show.
    pub syscalls: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchitectureDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source_extension: &'static str,
    pub address_bits: u8,
    pub instruction_alignment: u8,
    /// RAM handed to [`Architecture::create_machine`] when the caller has no
    /// preference of its own.
    pub default_memory_size: usize,
    pub endianness: Endianness,
    pub capabilities: ArchitectureCapabilities,
}

impl ArchitectureDescriptor {
    /// Number of distinct byte addresses this architecture can name.
    ///
    /// Wider than any address so the count is exact: a 64-bit space holds
    /// 2^64 addresses, one more than a `u64` can express. In a `u64` the shift
    /// does not happen at all — the shift count is taken modulo 64, the same
    /// masking x86 does to `CL`, so `1 << 64` yields 1 and every caller would
    /// clamp a 64-bit machine down to a single byte.
    pub fn address_space_bytes(&self) -> u128 {
        1u128 << self.address_bits
    }

    /// Reduce a requested RAM size to something this architecture can address.
    ///
    /// Callers that let a user pick a memory size should route it through here
    /// rather than hand [`Architecture::create_machine`] a size the backend has
    /// to reject.
    pub fn clamp_memory_size(&self, requested: usize) -> usize {
        if requested == 0 {
            return self.default_memory_size;
        }
        usize::try_from(self.address_space_bytes()).map_or(requested, |max| requested.min(max))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Zero-based source line, when the error belongs to a source line.
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    pub fn new(line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {}: {}", line + 1, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSegment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub executable: bool,
    pub writable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZeroFill {
    pub address: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub comments: HashMap<u64, String>,
    pub block_comments: HashMap<u64, String>,
    pub labels: HashMap<u64, Vec<String>>,
    pub label_to_line: HashMap<String, usize>,
    pub line_addresses: HashMap<usize, u64>,
    pub explicit_halts: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramImage {
    pub architecture: String,
    pub entry: u64,
    pub segments: Vec<ProgramSegment>,
    pub zero_fill: Vec<ZeroFill>,
    pub source_map: SourceMap,
}

impl ProgramImage {
    pub fn executable_bytes(&self) -> Option<&[u8]> {
        self.segments
            .iter()
            .find(|s| s.executable)
            .map(|s| s.bytes.as_slice())
    }

    /// The image's data segment — the first non-executable one — if it has any.
    ///
    /// Hosts that show a memory pane open it here, so they do not have to guess
    /// where an architecture puts `.data`.
    pub fn data_segment(&self) -> Option<&ProgramSegment> {
        self.segments.iter().find(|s| !s.executable)
    }

    /// How many instructions the executable segment holds, at the alignment the
    /// architecture's descriptor reports.
    pub fn instruction_count(&self, instruction_alignment: u8) -> usize {
        let stride = usize::from(instruction_alignment).max(1);
        self.executable_bytes().map_or(0, |b| b.len() / stride)
    }

    /// Total initialized bytes outside the executable segment.
    pub fn data_bytes(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| !s.executable)
            .map(|s| s.bytes.len())
            .sum()
    }

    /// Total bytes the image asks the loader to zero (`.bss` and friends).
    pub fn zero_filled_bytes(&self) -> u64 {
        self.zero_fill.iter().map(|f| f.size).sum()
    }

    /// One byte past the highest address the image occupies.
    ///
    /// Loaders use this to place the initial program break: everything above it
    /// is free for the guest heap.
    pub fn end_address(&self) -> u64 {
        let segments = self
            .segments
            .iter()
            .map(|s| s.address.saturating_add(s.bytes.len() as u64));
        let fills = self
            .zero_fill
            .iter()
            .map(|f| f.address.saturating_add(f.size));
        segments.chain(fills).max().unwrap_or(0)
    }

    /// Serialize the architecture-neutral FALC v2 container.
    ///
    /// The container carries segments and zero-fill regions only; the
    /// [`SourceMap`] is not round-tripped.
    pub fn to_falc_v2(&self) -> Result<Vec<u8>, MachineError> {
        crate::falc::encode_v2(self)
    }

    /// Read FALC v2, or map the legacy RV32-only FALC v1 layout to an image.
    pub fn from_falc(bytes: &[u8]) -> Result<Self, MachineError> {
        crate::falc::decode(bytes)
    }

    /// Reject an image that was built for a different backend.
    ///
    /// Every [`Machine::load`] implementation starts with this check, so the
    /// error message stays identical across backends.
    pub fn expect_architecture(&self, id: &str) -> Result<(), MachineError> {
        if self.architecture == id {
            return Ok(());
        }
        Err(MachineError::new(format!(
            "cannot load {} image into {id}",
            self.architecture
        )))
    }
}

/// One row of an architecture's instruction reference.
///
/// This is documentation, not decoding: it describes what a programmer writes,
/// which is why it sits with the assembler rather than the codec. A host draws
/// the rows it is given and never has an opinion about which ISA they describe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstructionDoc {
    /// The ISA's own grouping — `"R"`, `"Load"`, `"Pseudo"`. A host filters and
    /// colours by it without knowing what the names mean, so an ISA is free to
    /// group its instructions however it teaches them.
    pub kind: &'static str,
    pub mnemonic: &'static str,
    /// Operands as written, such as `"rd, rs1, rs2"`. Empty when there are none.
    pub operands: &'static str,
    pub summary: &'static str,
    /// What a pseudo-instruction assembles into; empty for a real one.
    pub expands_to: &'static str,
}

impl InstructionDoc {
    pub const fn new(
        kind: &'static str,
        mnemonic: &'static str,
        operands: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            kind,
            mnemonic,
            operands,
            summary,
            expands_to: "",
        }
    }

    pub const fn expanding(
        kind: &'static str,
        mnemonic: &'static str,
        operands: &'static str,
        summary: &'static str,
        expands_to: &'static str,
    ) -> Self {
        Self {
            kind,
            mnemonic,
            operands,
            summary,
            expands_to,
        }
    }
}

pub trait Assembler: Send + Sync {
    fn architecture_id(&self) -> &'static str;
    fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic>;

    /// Operand forms shown by editor mnemonic helpers, such as
    /// `&["rd, rs1, rs2"]`. The metadata belongs to the ISA, not the host.
    fn instruction_forms(&self, _mnemonic: &str) -> &'static [&'static str] {
        &[]
    }

    /// Whether `token` names a register in this ISA, for editor highlighting.
    fn is_register(&self, _token: &str) -> bool {
        false
    }

    /// Every instruction this ISA documents, for a host's reference pane.
    ///
    /// Empty when a backend ships no reference — a host then shows one fewer
    /// page rather than another architecture's instructions.
    fn documented_instructions(&self) -> &'static [InstructionDoc] {
        &[]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterValue {
    pub name: String,
    pub value: u64,
    pub bits: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineState {
    Ready,
    Running,
    AwaitingInput,
    Halted,
    Exited(i32),
    Faulted,
}

impl fmt::Display for MachineState {
    /// Stable, lower-case spelling — this reaches users through run reports and
    /// the TUI, so it must not drift with the `Debug` derive.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => f.write_str("ready"),
            Self::Running => f.write_str("running"),
            Self::AwaitingInput => f.write_str("awaiting-input"),
            Self::Halted => f.write_str("halted"),
            Self::Exited(code) => write!(f, "exited({code})"),
            Self::Faulted => f.write_str("faulted"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineSnapshot {
    pub architecture: &'static str,
    pub pc: u64,
    pub registers: Vec<RegisterValue>,
    pub state: MachineState,
    pub instructions: u64,
    pub stdout: Vec<u8>,
    pub fault: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Stepped,
    AwaitingInput,
    Halted,
    Exited(i32),
}

/// Result of advancing a machine by one pipeline clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CycleResult {
    pub retired_address: Option<u64>,
    pub outcome: StepOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError(String);

impl MachineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MachineError {}

/// A backend a host can load, step and inspect.
///
/// `Any` is a supertrait so a host holding `dyn Machine` can ask for the
/// concrete backend back:
///
/// ```ignore
/// let rv32 = (machine as &dyn std::any::Any).downcast_ref::<RiscV32Machine>();
/// ```
///
/// That is for *driving* a backend whose execution controls this trait does
/// not model — breakpoints, step-back, a JIT, several harts. Everything a host
/// merely *observes* is a capability below, and asking for a concrete type to
/// read state is a bug: it is the one thing that would put an architecture's
/// name back into the view layer.
pub trait Machine: Send + std::any::Any {
    fn architecture_id(&self) -> &'static str;
    fn reset(&mut self);
    fn load(&mut self, image: &ProgramImage) -> Result<(), MachineError>;
    fn step(&mut self) -> Result<StepOutcome, MachineError>;
    fn snapshot(&self) -> MachineSnapshot;
    fn read_memory(&self, address: u64, bytes: usize) -> Result<Vec<u8>, MachineError>;
    fn write_memory(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError>;
    fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError>;
    fn push_input(&mut self, line: &str);

    // ── Optional capabilities ────────────────────────────────────────────
    //
    // Everything above is the floor every backend implements. These say what
    // else this machine can do; see [`crate::capability`] for the contract and
    // for how to add one. Answering `None` is always valid and always safe —
    // hosts show one pane fewer, nothing breaks.

    /// Named register banks, for a host that wants to draw or edit registers
    /// without assuming this ISA's shape.
    fn registers(&self) -> Option<&dyn RegisterFile> {
        None
    }

    fn registers_mut(&mut self) -> Option<&mut dyn RegisterFile> {
        None
    }

    /// Guest memory, readable without disturbing the machine.
    fn memory(&self) -> Option<&dyn MemoryInspect> {
        None
    }

    fn memory_mut(&mut self) -> Option<&mut dyn MemoryInspect> {
        None
    }

    /// Disassembling and assembling a single instruction, for a listing pane
    /// and for editing an instruction in place.
    fn code(&self) -> Option<&dyn InstructionCodec> {
        None
    }

    /// Cache levels and counters, borrowed without warming a line or changing
    /// replacement state while a host draws them.
    fn caches(&self) -> Option<&dyn CacheHierarchy> {
        None
    }

    /// Cycle-level pipeline state: stage occupancy, hazards, and the timing
    /// history behind a Gantt view.
    ///
    /// A backend with no pipeline model answers `None`; a switched-off model
    /// remains inspectable and reports that state through `PipelineStatus`.
    fn pipeline(&self) -> Option<&dyn PipelineInspect> {
        None
    }

    fn pipeline_control(&mut self) -> Option<&mut dyn PipelineControl> {
        None
    }

    /// The datapath's adjustable properties, for a settings screen.
    ///
    /// A backend with a fixed datapath answers `None` and a host shows nothing;
    /// one that declares a shape can answer `Some` and let a user rewire the
    /// bypasses, change the branch predictor or add a functional unit, and then
    /// watch what it does to the Gantt view.
    fn pipeline_tuning(&self) -> Option<&dyn PipelineTuning> {
        None
    }

    fn pipeline_tuning_mut(&mut self) -> Option<&mut dyn PipelineTuning> {
        None
    }

    /// The structures a dynamically scheduled model runs on — reservation
    /// stations, the reorder buffer, the register alias table.
    ///
    /// `None` from a backend running an in-order model, which is what tells a
    /// host to draw stages instead of a workbench. The answer changes with the
    /// model, so a host asks every frame rather than once.
    fn pipeline_dynamic(&self) -> Option<&dyn PipelineDynamicInspect> {
        None
    }

    /// Advance one pipeline clock. The default retires one whole instruction.
    fn cycle(&mut self) -> Result<CycleResult, MachineError> {
        let address = self.snapshot().pc;
        self.step().map(|outcome| CycleResult {
            retired_address: Some(address),
            outcome,
        })
    }

    /// Virtual-to-physical translation: the TLB and the page walk behind it.
    ///
    /// A backend that runs on physical addresses answers `None`; one whose
    /// translation is merely switched off still answers `Some`, reporting a
    /// bare scheme, so a host can show the pane in its "off" state.
    fn translation(&self) -> Option<&dyn AddressTranslation> {
        None
    }

    fn run(&mut self, max_steps: u64) -> Result<StepOutcome, MachineError> {
        for _ in 0..max_steps {
            let outcome = self.step()?;
            if outcome != StepOutcome::Stepped {
                return Ok(outcome);
            }
        }
        Err(MachineError::new(format!(
            "execution limit of {max_steps} steps reached"
        )))
    }
}

pub trait Architecture: Send + Sync {
    fn descriptor(&self) -> &'static ArchitectureDescriptor;
    fn assembler(&self) -> &'static dyn Assembler;
    fn default_source(&self) -> &'static str;

    /// Build a machine with `memory_size` bytes of RAM.
    ///
    /// Backends reject a size their address space cannot cover instead of
    /// silently shrinking it; use
    /// [`ArchitectureDescriptor::clamp_memory_size`] on user-supplied values.
    fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError>;
}

#[derive(Default, Clone)]
pub struct ArchitectureRegistry {
    entries: HashMap<&'static str, Arc<dyn Architecture>>,
}

impl ArchitectureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The shared registry of built-in backends.
    ///
    /// Prefer this over [`Self::with_builtins`] unless you intend to
    /// [`register`](Self::register) extra architectures into your own copy.
    pub fn builtin() -> &'static Self {
        static BUILTIN: OnceLock<ArchitectureRegistry> = OnceLock::new();
        BUILTIN.get_or_init(Self::with_builtins)
    }

    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(crate::architectures::riscv32::architecture());
        registry.register(crate::architectures::sap::architecture());
        registry.register(crate::architectures::toy16::architecture());
        registry.register(crate::architectures::x86_64::architecture());
        registry
    }

    /// Every registered ID, sorted — for error messages and UI listings.
    pub fn ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.entries.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The architecture following `id` in registration order, wrapping around.
    pub fn next_after(&self, id: &str) -> Option<Arc<dyn Architecture>> {
        let all = self.architectures();
        let current = all.iter().position(|a| a.descriptor().id == id)?;
        all.get((current + 1) % all.len()).cloned()
    }

    pub fn register(
        &mut self,
        architecture: Arc<dyn Architecture>,
    ) -> Option<Arc<dyn Architecture>> {
        self.entries
            .insert(architecture.descriptor().id, architecture)
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Architecture>> {
        self.entries.get(id).cloned()
    }

    pub fn architectures(&self) -> Vec<Arc<dyn Architecture>> {
        let mut values: Vec<_> = self.entries.values().cloned().collect();
        values.sort_by_key(|a| a.descriptor().id);
        values
    }
}

