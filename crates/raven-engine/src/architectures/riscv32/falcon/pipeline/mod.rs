//! RV32 pipeline execution core. Presentation and interaction live in the host TUI.
//!
//! # What is here, and what is not
//!
//! The *vocabulary* of a pipeline is not RISC-V's, so it is not defined here:
//! instruction classes, hazard kinds, trace kinds, bypass configuration, branch
//! resolution and prediction policy, the timing table, the two-bit predictor
//! and the stage and unit declarations all come from [`crate::pipeline`] and
//! are re-exported below. Any architecture gets the same set by declaring a
//! [`PipelineShape`](crate::pipeline::PipelineShape), and three already do.
//!
//! What stays is the part that needs an ISA to mean anything. This model does
//! not merely *time* instructions the way
//! [`ScalarPipeline`](crate::pipeline::ScalarPipeline) does — it executes them
//! stage by stage, so [`PipeSlot`] carries live operand values, an address
//! computed in EX, a value loaded in MEM. That is what lets RV32 forward real
//! values along a configurable set of bypasses, stall on a cache miss it can
//! measure, and take an MMU trap from the middle of the datapath. A generic
//! engine cannot do any of it without knowing what a register holds.
//!
//! The [`FuState`] bank and the Gantt recorder stay for the same reason: they
//! track values and cache latency in flight, not just occupancy. Their
//! occupancy-only equivalents already live in the shared package, which is what
//! every other backend uses.

pub mod forwarding;
mod inspect;
pub mod predictor;
pub mod sim;

use crate::falcon::instruction::Instruction;
use crate::falcon::registers::ExecRegion;
use std::collections::VecDeque;

// ── Instruction class ────────────────────────────────────────────────────────
//
// RV32 kept its own ten-variant class enum next to the ten-variant one every
// host already reads. They were the same list — an instruction is a multiply or
// a load regardless of who encodes it — so the enum is the shared one and only
// the decoding stayed here, where the ISA is.

pub use crate::capability::PipelineInstructionClass as InstrClass;

/// Classify an instruction word.
pub fn classify(word: u32) -> InstrClass {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        Ok(
            Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Sll { .. }
            | Srl { .. }
            | Sra { .. }
            | Slt { .. }
            | Sltu { .. }
            | Addi { .. }
            | Andi { .. }
            | Ori { .. }
            | Xori { .. }
            | Slti { .. }
            | Sltiu { .. }
            | Slli { .. }
            | Srli { .. }
            | Srai { .. }
            | Lui { .. }
            | Auipc { .. },
        ) => InstrClass::Alu,
        Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => InstrClass::Multiply,
        Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => InstrClass::Divide,
        Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. }) => InstrClass::Load,
        Ok(LrW { .. }) => InstrClass::Load,
        Ok(
            Sb { .. }
            | Sh { .. }
            | Sw { .. }
            | ScW { .. }
            | AmoswapW { .. }
            | AmoaddW { .. }
            | AmoxorW { .. }
            | AmoandW { .. }
            | AmoorW { .. }
            | AmomaxW { .. }
            | AmominW { .. }
            | AmomaxuW { .. }
            | AmominuW { .. },
        ) => InstrClass::Store,
        Ok(Beq { .. } | Bne { .. } | Blt { .. } | Bge { .. } | Bltu { .. } | Bgeu { .. }) => {
            InstrClass::Branch
        }
        Ok(Jal { .. } | Jalr { .. }) => InstrClass::Jump,
        Ok(Ecall | Ebreak | Halt | Fence | FenceI) => InstrClass::System,
        Ok(
            Flw { .. }
            | Fsw { .. }
            | FaddS { .. }
            | FsubS { .. }
            | FmulS { .. }
            | FdivS { .. }
            | FsqrtS { .. }
            | FminS { .. }
            | FmaxS { .. }
            | FsgnjS { .. }
            | FsgnjnS { .. }
            | FsgnjxS { .. }
            | FeqS { .. }
            | FltS { .. }
            | FleS { .. }
            | FcvtWS { .. }
            | FcvtWuS { .. }
            | FcvtSW { .. }
            | FcvtSWu { .. }
            | FmvXW { .. }
            | FmvWX { .. }
            | FclassS { .. }
            | FmaddS { .. }
            | FmsubS { .. }
            | FnmsubS { .. }
            | FnmaddS { .. },
        ) => InstrClass::FloatingPoint,
        _ => InstrClass::Unknown,
    }
}

/// Extract (rd, rs1, rs2) from an instruction word.
pub fn operands(word: u32) -> (Option<u8>, Option<u8>, Option<u8>) {
    use crate::falcon::instruction::Instruction::*;
    match crate::falcon::decoder::decode(word) {
        // R-type
        Ok(
            Add { rd, rs1, rs2 }
            | Sub { rd, rs1, rs2 }
            | And { rd, rs1, rs2 }
            | Or { rd, rs1, rs2 }
            | Xor { rd, rs1, rs2 }
            | Sll { rd, rs1, rs2 }
            | Srl { rd, rs1, rs2 }
            | Sra { rd, rs1, rs2 }
            | Slt { rd, rs1, rs2 }
            | Sltu { rd, rs1, rs2 }
            | Mul { rd, rs1, rs2 }
            | Mulh { rd, rs1, rs2 }
            | Mulhsu { rd, rs1, rs2 }
            | Mulhu { rd, rs1, rs2 }
            | Div { rd, rs1, rs2 }
            | Divu { rd, rs1, rs2 }
            | Rem { rd, rs1, rs2 }
            | Remu { rd, rs1, rs2 },
        ) => (Some(rd), Some(rs1), Some(rs2)),
        // I-type (rd + rs1)
        Ok(
            Addi { rd, rs1, .. }
            | Andi { rd, rs1, .. }
            | Ori { rd, rs1, .. }
            | Xori { rd, rs1, .. }
            | Slti { rd, rs1, .. }
            | Sltiu { rd, rs1, .. }
            | Slli { rd, rs1, .. }
            | Srli { rd, rs1, .. }
            | Srai { rd, rs1, .. }
            | Lb { rd, rs1, .. }
            | Lh { rd, rs1, .. }
            | Lw { rd, rs1, .. }
            | Lbu { rd, rs1, .. }
            | Lhu { rd, rs1, .. }
            | Jalr { rd, rs1, .. }
            | Flw { rd, rs1, .. }
            | LrW { rd, rs1, .. },
        ) => (Some(rd), Some(rs1), None),
        // U-type / J-type (only rd)
        Ok(Lui { rd, .. } | Auipc { rd, .. } | Jal { rd, .. }) => (Some(rd), None, None),
        // S-type (no rd, has rs1+rs2)
        Ok(
            Sb { rs1, rs2, .. }
            | Sh { rs1, rs2, .. }
            | Sw { rs1, rs2, .. }
            | Fsw { rs1, rs2, .. },
        ) => (None, Some(rs1), Some(rs2)),
        Ok(
            ScW { rd, rs1, rs2, .. }
            | AmoswapW { rd, rs1, rs2, .. }
            | AmoaddW { rd, rs1, rs2, .. }
            | AmoxorW { rd, rs1, rs2, .. }
            | AmoandW { rd, rs1, rs2, .. }
            | AmoorW { rd, rs1, rs2, .. }
            | AmomaxW { rd, rs1, rs2, .. }
            | AmominW { rd, rs1, rs2, .. }
            | AmomaxuW { rd, rs1, rs2, .. }
            | AmominuW { rd, rs1, rs2, .. },
        ) => (Some(rd), Some(rs1), Some(rs2)),
        // B-type (no rd, has rs1+rs2)
        Ok(
            Beq { rs1, rs2, .. }
            | Bne { rs1, rs2, .. }
            | Blt { rs1, rs2, .. }
            | Bge { rs1, rs2, .. }
            | Bltu { rs1, rs2, .. }
            | Bgeu { rs1, rs2, .. },
        ) => (None, Some(rs1), Some(rs2)),
        // FP R-type
        Ok(
            FaddS { rd, rs1, rs2, .. }
            | FsubS { rd, rs1, rs2, .. }
            | FmulS { rd, rs1, rs2, .. }
            | FdivS { rd, rs1, rs2, .. }
            | FminS { rd, rs1, rs2, .. }
            | FmaxS { rd, rs1, rs2, .. }
            | FsgnjS { rd, rs1, rs2, .. }
            | FsgnjnS { rd, rs1, rs2, .. }
            | FsgnjxS { rd, rs1, rs2, .. }
            | FmaddS { rd, rs1, rs2, .. }
            | FmsubS { rd, rs1, rs2, .. }
            | FnmsubS { rd, rs1, rs2, .. }
            | FnmaddS { rd, rs1, rs2, .. },
        ) => (Some(rd), Some(rs1), Some(rs2)),
        // FP compare: rd + rs1 + rs2
        Ok(
            FeqS { rd, rs1, rs2, .. } | FltS { rd, rs1, rs2, .. } | FleS { rd, rs1, rs2, .. },
        ) => (Some(rd), Some(rs1), Some(rs2)),
        // FP I-type with rd
        Ok(
            FsqrtS { rd, rs1, .. }
            | FcvtWS { rd, rs1, .. }
            | FcvtWuS { rd, rs1, .. }
            | FcvtSW { rd, rs1, .. }
            | FcvtSWu { rd, rs1, .. }
            | FmvXW { rd, rs1, .. }
            | FmvWX { rd, rs1, .. }
            | FclassS { rd, rs1, .. },
        ) => (Some(rd), Some(rs1), None),
        _ => (None, None, None),
    }
}

// ── Pipeline config ──────────────────────────────────────────────────────────
//
// None of this is RISC-V's. Where a branch resolves, which bypasses are wired,
// how long each class of work takes — every pipelined machine has an answer, so
// the definitions live in `crate::pipeline` and RV32 merely declares its own.
// They are re-exported here because `falcon::pipeline` is the established path.

pub use crate::pipeline::{
    BranchPredict, BranchResolve, PipelineBypassConfig, PipelineMode, PipelineTiming, UnitSpec,
};

// ── Hazard type ───────────────────────────────────────────────────────────────
//
// RV32 used to keep its own copy of these and a pair of functions translating
// to the ones a host reads. The two enums always had the same variants — they
// describe hazards, not an ISA — so there is one now, and no translation to
// keep in step.

pub use crate::capability::PipelineHazardKind as HazardType;
pub use crate::capability::PipelineTraceKind as TraceKind;

#[derive(Clone, Debug)]
pub struct HazardTrace {
    pub kind: TraceKind,
    pub from_stage: usize,
    pub to_stage: usize,
    pub detail: String,
}

// ── Stage names ───────────────────────────────────────────────────────────────

/// RV32's datapath declaration. The names and roles a host draws come from
/// here, so [`Stage`] stays what it is useful for — an index into the stage
/// array — instead of being a second place stage names are spelled out.
pub static STAGES: [crate::pipeline::StageSpec; 5] = crate::pipeline::RISC_FIVE_STAGE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    IF = 0,
    ID = 1,
    EX = 2,
    MEM = 3,
    WB = 4,
}

impl Stage {
    pub fn label(self) -> &'static str {
        STAGES[self as usize].name
    }

    pub fn role(self) -> crate::capability::PipelineStageRole {
        STAGES[self as usize].role
    }

    pub fn all() -> [Stage; 5] {
        [Stage::IF, Stage::ID, Stage::EX, Stage::MEM, Stage::WB]
    }
}

// ── Functional units ──────────────────────────────────────────────────────────

/// RV32's execution units, declared in the shared vocabulary.
///
/// [`FuKind`] stays what it is good for — a name for an index into this array —
/// while the labels, the classes each unit claims and the colour a host paints
/// an idle one all come from one declaration instead of three matches that had
/// to agree.
pub static UNITS: [UnitSpec; 6] = [
    UnitSpec::new(
        "ALU",
        &[
            InstrClass::Alu,
            InstrClass::Branch,
            InstrClass::Jump,
        ],
        InstrClass::Alu,
    ),
    UnitSpec::new("MUL", &[InstrClass::Multiply], InstrClass::Multiply),
    UnitSpec::new("DIV", &[InstrClass::Divide], InstrClass::Divide),
    UnitSpec::new(
        "FPU",
        &[InstrClass::FloatingPoint],
        InstrClass::FloatingPoint,
    ),
    UnitSpec::new(
        "LSU",
        &[InstrClass::Load, InstrClass::Store],
        InstrClass::Load,
    ),
    UnitSpec::new("SYS", &[InstrClass::System], InstrClass::System),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FuKind {
    Alu,
    Mul,
    Div,
    Fpu,
    Lsu,
    Sys,
}

impl FuKind {
    pub const COUNT: usize = UNITS.len();

    pub fn spec(self) -> &'static UnitSpec {
        &UNITS[self as usize]
    }

    pub fn label(self) -> &'static str {
        self.spec().name
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn all() -> [FuKind; Self::COUNT] {
        [
            Self::Alu,
            Self::Mul,
            Self::Div,
            Self::Fpu,
            Self::Lsu,
            Self::Sys,
        ]
    }

    /// The unit that runs `class`. An unrecognised instruction belongs to no
    /// unit, which is what keeps it from being dispatched at all.
    pub fn from_class(class: InstrClass) -> Option<Self> {
        (class != InstrClass::Unknown)
            .then(|| UNITS.iter().position(|unit| unit.handles(class)))
            .flatten()
            .map(|index| Self::all()[index])
    }
}

#[derive(Clone, Default)]
pub struct FuState {
    pub kind: Option<FuKind>,
    pub slot: Option<PipeSlot>,
    pub busy_cycles_left: u8,
}

pub type FuBank = [Vec<FuState>; FuKind::COUNT];

// ── Pipeline slot ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PipeSlot {
    pub seq: u64,
    pub gantt_id: u64,
    pub pc: u32,
    pub word: u32,
    pub disasm: String,
    pub rd: Option<u8>,
    pub rs1: Option<u8>,
    pub rs2: Option<u8>,
    pub class: InstrClass,
    pub is_bubble: bool,
    pub is_speculative: bool,
    pub hazard: Option<HazardType>,
    pub fu_cycles_left: u8,
    pub if_stall_cycles: u8,
    pub mem_stall_cycles: u8,

    // ── Per-stage data ───────────────────────────────────────────────────
    /// Decoded instruction (set at ID stage). Instruction is Copy.
    pub instr: Option<Instruction>,
    /// Register operand values read at ID stage
    pub rs1_val: u32,
    pub rs2_val: u32,
    /// ALU/computation result (set at EX stage)
    pub alu_result: u32,
    /// Computed memory address for loads/stores (set at EX)
    pub mem_addr: Option<u32>,
    /// Value loaded from memory (set at MEM stage, for loads)
    pub mem_result: Option<u32>,
    /// Branch target PC (set at EX)
    pub branch_target: Option<u32>,
    /// Whether branch was taken (set at EX)
    pub branch_taken: bool,
    /// Static prediction chosen when the instruction first reached ID.
    pub predicted_taken: bool,
    pub predicted_target: Option<u32>,
}

impl PipeSlot {
    pub fn bubble() -> Self {
        Self {
            seq: 0,
            gantt_id: 0,
            pc: 0,
            word: 0,
            disasm: String::new(),
            rd: None,
            rs1: None,
            rs2: None,
            class: InstrClass::Unknown,
            is_bubble: true,
            is_speculative: false,
            hazard: None,
            fu_cycles_left: 0,
            if_stall_cycles: 0,
            mem_stall_cycles: 0,
            instr: None,
            rs1_val: 0,
            rs2_val: 0,
            alu_result: 0,
            mem_addr: None,
            mem_result: None,
            branch_target: None,
            branch_taken: false,
            predicted_taken: false,
            predicted_target: None,
        }
    }

    pub fn from_word(pc: u32, word: u32) -> Self {
        let class = classify(word);
        let (rd, rs1, rs2) = operands(word);
        let disasm = crate::falcon::decoder::disasm(word);
        Self {
            seq: 0,
            gantt_id: 0,
            pc,
            word,
            disasm,
            rd,
            rs1,
            rs2,
            class,
            is_bubble: false,
            is_speculative: false,
            hazard: None,
            fu_cycles_left: 1,
            if_stall_cycles: 0,
            mem_stall_cycles: 0,
            instr: None,
            rs1_val: 0,
            rs2_val: 0,
            alu_result: 0,
            mem_addr: None,
            mem_result: None,
            branch_target: None,
            branch_taken: false,
            predicted_taken: false,
            predicted_target: None,
        }
    }
}

// ── Gantt diagram ─────────────────────────────────────────────────────────────

pub const MAX_GANTT_ROWS: usize = 256;
pub const MAX_GANTT_COLS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GanttCell {
    Empty,                 // instruction not in pipeline yet / already done
    InStage(Stage),        // instruction is in this stage
    InFu(FuKind),          // instruction is executing in a specific functional unit
    Speculative(Stage),    // instruction is in this stage but was fetched speculatively
    SpeculativeFu(FuKind), // instruction is executing speculatively in a FU
    Stall,                 // stalled in current stage
    Bubble,                // NOP bubble occupies this slot
    Flush,                 // instruction was flushed (branch misprediction)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GanttTrack {
    Stage(Stage),
    Fu(FuKind),
}

#[derive(Clone)]
pub struct GanttRow {
    pub gantt_id: u64,
    pub pc: u32,
    pub disasm: String,
    pub class: InstrClass,
    /// One cell per cycle, oldest first. Length ≤ MAX_GANTT_COLS.
    pub cells: VecDeque<GanttCell>,
    /// The cycle this row was first seen.
    pub first_cycle: u64,
    /// True if this row has reached WB (commit) or been flushed.
    pub done: bool,
    /// The last execution location emitted — used to detect stalls.
    pub last_stage: Option<GanttTrack>,
}

/// Map an instruction class to its EX-stage latency.
pub fn fu_latency_for_class(class: InstrClass, timing: &PipelineTiming) -> u8 {
    timing.latency(class)
}

/// Reversible execution state plus physical pipeline configuration and telemetry.
pub struct PipelineSimState {
    pub enabled: bool,
    pub bypass: PipelineBypassConfig,
    pub branch_resolve: BranchResolve,
    pub mode: PipelineMode,
    pub predict: BranchPredict,
    pub predictor: predictor::PredictorState,
    pub exec_regions: Vec<ExecRegion>,
    pub fetch_pc: u32,
    pub halted: bool,
    pub faulted: bool,
    pub stages: [Option<PipeSlot>; 5],
    pub fu_bank: FuBank,
    pub fu_capacity: [u8; FuKind::COUNT],
    pub fu_busy: [u8; 7],
    pub cycle_count: u64,
    pub instr_committed: u64,
    pub stall_count: u64,
    pub stall_by_type: [u64; HazardType::STALL_TYPE_COUNT],
    pub flush_count: u64,
    pub branches_executed: u64,
    pub class_counts: [u64; InstrClass::COUNT],
    pub gantt: VecDeque<GanttRow>,
    pub next_gantt_id: u64,
    pub next_seq: u64,
    pub sequential_mode: bool,
    pub hazard_msgs: Vec<(HazardType, String)>,
    pub hazard_traces: Vec<HazardTrace>,
    pub last_cycle_cache_only: bool,
    pub pending_fetch_trap: Option<(u32, u32, u32, u32)>,
}

pub struct PipelineExecSnapshot {
    fetch_pc: u32,
    halted: bool,
    faulted: bool,
    stages: [Option<PipeSlot>; 5],
    fu_bank: FuBank,
    fu_busy: [u8; 7],
    predictor: predictor::PredictorState,
    pending_fetch_trap: Option<(u32, u32, u32, u32)>,
    cycle_count: u64,
    instr_committed: u64,
    stall_count: u64,
    stall_by_type: [u64; HazardType::STALL_TYPE_COUNT],
    flush_count: u64,
    branches_executed: u64,
    class_counts: [u64; InstrClass::COUNT],
    last_cycle_cache_only: bool,
    hazard_msgs: Vec<(HazardType, String)>,
    hazard_traces: Vec<HazardTrace>,
    gantt: VecDeque<GanttRow>,
    next_gantt_id: u64,
    next_seq: u64,
}

impl crate::falcon::machine::JournaledPipeline for PipelineSimState {
    type Snapshot = PipelineExecSnapshot;

    fn exec_snapshot(&self) -> Self::Snapshot {
        PipelineExecSnapshot {
            fetch_pc: self.fetch_pc,
            halted: self.halted,
            faulted: self.faulted,
            stages: self.stages.clone(),
            fu_bank: self.fu_bank.clone(),
            fu_busy: self.fu_busy,
            predictor: self.predictor.clone(),
            pending_fetch_trap: self.pending_fetch_trap,
            cycle_count: self.cycle_count,
            instr_committed: self.instr_committed,
            stall_count: self.stall_count,
            stall_by_type: self.stall_by_type,
            flush_count: self.flush_count,
            branches_executed: self.branches_executed,
            class_counts: self.class_counts,
            last_cycle_cache_only: self.last_cycle_cache_only,
            hazard_msgs: self.hazard_msgs.clone(),
            hazard_traces: self.hazard_traces.clone(),
            gantt: self.gantt.clone(),
            next_gantt_id: self.next_gantt_id,
            next_seq: self.next_seq,
        }
    }

    fn restore_exec(&mut self, s: Self::Snapshot) {
        self.fetch_pc = s.fetch_pc;
        self.halted = s.halted;
        self.faulted = s.faulted;
        self.stages = s.stages;
        self.fu_bank = s.fu_bank;
        self.fu_busy = s.fu_busy;
        self.predictor = s.predictor;
        self.pending_fetch_trap = s.pending_fetch_trap;
        self.cycle_count = s.cycle_count;
        self.instr_committed = s.instr_committed;
        self.stall_count = s.stall_count;
        self.stall_by_type = s.stall_by_type;
        self.flush_count = s.flush_count;
        self.branches_executed = s.branches_executed;
        self.class_counts = s.class_counts;
        self.last_cycle_cache_only = s.last_cycle_cache_only;
        self.hazard_msgs = s.hazard_msgs;
        self.hazard_traces = s.hazard_traces;
        self.gantt = s.gantt;
        self.next_gantt_id = s.next_gantt_id;
        self.next_seq = s.next_seq;
    }

    fn inspect(&self) -> Option<&dyn crate::capability::PipelineInspect> {
        Some(self)
    }

    fn control(&mut self) -> Option<&mut dyn crate::capability::PipelineControl> {
        Some(self)
    }
}

/// The controls every pipeline offers, answered against RV32's own state.
///
/// Narrower than [`PipelineSimState`]'s inherent methods on purpose: this is
/// what a host may do to *any* pipeline, so it must not depend on RV32 having a
/// bypass matrix or a functional-unit bank. Those stay behind the concrete type.
impl crate::capability::PipelineControl for PipelineSimState {
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Start a fresh run from `address`: stages, units, counters and history
    /// all go, because none of it describes the new run.
    fn reset(&mut self, address: u64) {
        self.reset_stages(address as u32);
    }

    /// Follow a jump the host made: clear what was in flight and refill from
    /// `address`, keeping the statistics, which are still this run's.
    fn redirect(&mut self, address: u64) {
        self.redirect_pc(address as u32);
    }
}

impl PipelineSimState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            bypass: PipelineBypassConfig::default(),
            branch_resolve: BranchResolve::Ex,
            mode: PipelineMode::SingleCycle,
            predict: BranchPredict::NotTaken,
            predictor: predictor::PredictorState::default(),
            exec_regions: Vec::new(),
            fetch_pc: 0,
            halted: false,
            faulted: false,
            stages: Default::default(),
            fu_bank: std::array::from_fn(|_| Vec::new()),
            fu_capacity: [1; FuKind::COUNT],
            fu_busy: [0; 7],
            cycle_count: 0,
            instr_committed: 0,
            stall_count: 0,
            stall_by_type: [0; HazardType::STALL_TYPE_COUNT],
            flush_count: 0,
            branches_executed: 0,
            class_counts: [0; InstrClass::COUNT],
            gantt: VecDeque::new(),
            next_gantt_id: 1,
            next_seq: 1,
            sequential_mode: false,
            hazard_msgs: Vec::new(),
            hazard_traces: Vec::new(),
            last_cycle_cache_only: false,
            pending_fetch_trap: None,
        }
    }

    pub fn set_exec_regions(&mut self, regions: &[ExecRegion]) {
        self.exec_regions.clear();
        self.exec_regions.extend_from_slice(regions);
    }

    pub fn redirect_pc(&mut self, new_pc: u32) {
        self.stages = Default::default();
        self.fu_bank = std::array::from_fn(|_| Vec::new());
        self.fu_busy = [0; 7];
        self.fetch_pc = new_pc;
        self.halted = false;
        self.faulted = false;
        self.hazard_msgs.clear();
        self.hazard_traces.clear();
        self.last_cycle_cache_only = false;
        self.predictor.clear();
    }

    pub fn reset_stats(&mut self) {
        self.cycle_count = 0;
        self.instr_committed = 0;
        self.stall_count = 0;
        self.stall_by_type = [0; HazardType::STALL_TYPE_COUNT];
        self.branches_executed = 0;
        self.flush_count = 0;
        self.class_counts = [0; InstrClass::COUNT];
    }

    pub fn reset_stages(&mut self, base_pc: u32) {
        self.fetch_pc = base_pc;
        self.stages = Default::default();
        self.fu_bank = std::array::from_fn(|_| Vec::new());
        self.fu_busy = [0; 7];
        self.reset_stats();
        self.gantt.clear();
        self.next_gantt_id = 1;
        self.next_seq = 1;
        self.hazard_msgs.clear();
        self.hazard_traces.clear();
        self.last_cycle_cache_only = false;
        self.predictor.clear();
        self.halted = false;
        self.faulted = false;
    }

    pub fn cpi(&self) -> f64 {
        if self.instr_committed == 0 {
            0.0
        } else {
            self.cycle_count as f64 / self.instr_committed as f64
        }
    }

    pub fn set_predict(&mut self, predict: BranchPredict) {
        if self.predict != predict {
            self.predict = predict;
            self.predictor.clear();
        }
    }

    pub fn set_legacy_forwarding(&mut self, enabled: bool) {
        self.bypass.set_legacy_forwarding(enabled);
    }
}

impl Default for PipelineSimState {
    fn default() -> Self {
        Self::new()
    }
}
