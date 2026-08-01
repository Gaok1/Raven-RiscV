//! RV32 pipeline execution core. Presentation and interaction live in the host TUI.

pub mod forwarding;
pub mod predictor;
pub mod sim;
mod inspect;

use crate::falcon::instruction::Instruction;
use crate::falcon::registers::ExecRegion;
use std::collections::VecDeque;

// ── Instruction class ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstrClass {
    Alu,
    Mul,
    Div,
    Load,
    Store,
    Branch,
    Jump,
    System,
    Fp,
    Unknown,
}

impl InstrClass {
    pub const COUNT: usize = 10;

    pub fn label(self) -> &'static str {
        match self {
            Self::Alu => "ALU",
            Self::Mul => "MUL",
            Self::Div => "DIV",
            Self::Load => "Load",
            Self::Store => "Store",
            Self::Branch => "Branch",
            Self::Jump => "Jump",
            Self::System => "System",
            Self::Fp => "FP",
            Self::Unknown => "?",
        }
    }

    pub fn as_usize(self) -> usize {
        self as usize
    }

    /// Classify an instruction word into an InstrClass.
    pub fn from_word(word: u32) -> Self {
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
            ) => Self::Alu,
            Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => Self::Mul,
            Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => Self::Div,
            Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. }) => Self::Load,
            Ok(LrW { .. }) => Self::Load,
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
            ) => Self::Store,
            Ok(Beq { .. } | Bne { .. } | Blt { .. } | Bge { .. } | Bltu { .. } | Bgeu { .. }) => {
                Self::Branch
            }
            Ok(Jal { .. } | Jalr { .. }) => Self::Jump,
            Ok(Ecall | Ebreak | Halt | Fence | FenceI) => Self::System,
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
            ) => Self::Fp,
            _ => Self::Unknown,
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
}

// ── Pipeline config enums ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchResolve {
    /// Branch resolved at end of ID → 1 bubble (pipeline stalls IF while branch in ID)
    Id,
    /// Branch resolved at end of EX → 2 bubbles
    Ex,
    /// Branch resolved at end of MEM → 3 bubbles
    Mem,
}

impl BranchResolve {
    pub fn label(self) -> &'static str {
        match self {
            Self::Id => "ID (1 stall)",
            Self::Ex => "EX (2 stalls)",
            Self::Mem => "MEM (3 stalls)",
        }
    }
    /// Number of pipeline stages after the branch that must be flushed.
    pub fn flush_depth(self) -> usize {
        match self {
            Self::Id => 1,
            Self::Ex => 2,
            Self::Mem => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipelineMode {
    SingleCycle,
    FunctionalUnits,
}

impl PipelineMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SingleCycle => "Serialized",
            Self::FunctionalUnits => "Parallel UFs",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchPredict {
    NotTaken,
    Taken,
    Btfnt,
    TwoBit,
}
impl BranchPredict {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotTaken => "Not-taken",
            Self::Taken => "Always-taken",
            Self::Btfnt => "BTFNT",
            Self::TwoBit => "2-bit Dynamic",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PipelineBypassConfig {
    pub ex_to_ex: bool,
    pub mem_to_ex: bool,
    pub wb_to_id: bool,
    pub store_to_load: bool,
}

impl PipelineBypassConfig {
    pub const CONFIG_ROWS: usize = 13;

    pub const fn new(ex_to_ex: bool, mem_to_ex: bool, wb_to_id: bool, store_to_load: bool) -> Self {
        Self {
            ex_to_ex,
            mem_to_ex,
            wb_to_id,
            store_to_load,
        }
    }

    pub const fn legacy_enabled() -> Self {
        Self::new(true, true, true, false)
    }

    pub const fn disabled() -> Self {
        Self::new(false, false, false, false)
    }

    pub fn set_legacy_forwarding(&mut self, enabled: bool) {
        *self = if enabled {
            Self::legacy_enabled()
        } else {
            Self::disabled()
        };
    }

    pub fn legacy_forwarding_enabled(self) -> bool {
        self.ex_to_ex && self.mem_to_ex && self.wb_to_id
    }

    pub fn summary(self) -> String {
        let mut enabled = Vec::new();
        if self.ex_to_ex {
            enabled.push("EX->EX");
        }
        if self.mem_to_ex {
            enabled.push("MEM->EX");
        }
        if self.wb_to_id {
            enabled.push("WB->ID");
        }
        if self.store_to_load {
            enabled.push("Store->Load");
        }
        if enabled.is_empty() {
            "none".to_string()
        } else {
            enabled.join(" | ")
        }
    }
}

impl Default for PipelineBypassConfig {
    fn default() -> Self {
        Self::legacy_enabled()
    }
}

// ── FU latency (derived from global CpiConfig) ──────────────────────────────

/// Map an instruction class to its EX-stage latency using the global CPI config.
/// Values are additive: effective latency = 1 + cpi.field (minimum 1 cycle).

// ── Hazard type ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HazardType {
    Raw,
    LoadUse,
    BranchFlush,
    FuBusy,
    MemLatency,
    Waw,
    War,
}

impl HazardType {
    /// Number of stall-causing hazard types (WAW/WAR are informational, not counted).
    pub const STALL_TYPE_COUNT: usize = 5;

    pub fn label(self) -> &'static str {
        match self {
            Self::Raw => "RAW",
            Self::LoadUse => "load-use",
            Self::BranchFlush => "branch flush",
            Self::FuBusy => "FU busy",
            Self::MemLatency => "cache stall",
            Self::Waw => "WAW",
            Self::War => "WAR",
        }
    }

    /// Index into the `stall_by_type` array.  Returns `None` for WAW/WAR which
    /// are informational only and do not cause pipeline stalls in an in-order pipeline.
    pub fn as_stall_index(self) -> Option<usize> {
        match self {
            Self::Raw => Some(0),
            Self::LoadUse => Some(1),
            Self::BranchFlush => Some(2),
            Self::FuBusy => Some(3),
            Self::MemLatency => Some(4),
            Self::Waw | Self::War => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceKind {
    Hazard(HazardType),
    Forward,
}

impl TraceKind {
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Hazard(HazardType::Raw) => "RAW",
            Self::Hazard(HazardType::LoadUse) => "LOAD",
            Self::Hazard(HazardType::BranchFlush) => "CTRL",
            Self::Hazard(HazardType::FuBusy) => "FU",
            Self::Hazard(HazardType::MemLatency) => "MEM",
            Self::Hazard(HazardType::Waw) => "WAW",
            Self::Hazard(HazardType::War) => "WAR",
            Self::Forward => "FWD",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HazardTrace {
    pub kind: TraceKind,
    pub from_stage: usize,
    pub to_stage: usize,
    pub detail: String,
}

// ── Stage names ───────────────────────────────────────────────────────────────

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
        match self {
            Self::IF => "IF",
            Self::ID => "ID",
            Self::EX => "EX",
            Self::MEM => "MEM",
            Self::WB => "WB",
        }
    }
    pub fn all() -> [Stage; 5] {
        [Stage::IF, Stage::ID, Stage::EX, Stage::MEM, Stage::WB]
    }
}

// ── Functional-unit names ─────────────────────────────────────────────────────

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
    pub const COUNT: usize = 6;

    pub fn label(self) -> &'static str {
        match self {
            Self::Alu => "ALU",
            Self::Mul => "MUL",
            Self::Div => "DIV",
            Self::Fpu => "FPU",
            Self::Lsu => "LSU",
            Self::Sys => "SYS",
        }
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

    pub fn from_class(class: InstrClass) -> Option<Self> {
        match class {
            InstrClass::Alu | InstrClass::Branch | InstrClass::Jump => Some(Self::Alu),
            InstrClass::Mul => Some(Self::Mul),
            InstrClass::Div => Some(Self::Div),
            InstrClass::Fp => Some(Self::Fpu),
            InstrClass::Load | InstrClass::Store => Some(Self::Lsu),
            InstrClass::System => Some(Self::Sys),
            InstrClass::Unknown => None,
        }
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
        let class = InstrClass::from_word(word);
        let (rd, rs1, rs2) = InstrClass::operands(word);
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


/// Extra execution cycles used by the pipeline and sequential RV32 runtimes.
#[derive(Clone, Debug)]
pub struct PipelineTiming {
    pub alu: u64,
    pub mul: u64,
    pub div: u64,
    pub load: u64,
    pub store: u64,
    pub branch_taken: u64,
    pub branch_not_taken: u64,
    pub jump: u64,
    pub system: u64,
    pub fp: u64,
    pub stage_overhead: u64,
}

impl Default for PipelineTiming {
    fn default() -> Self {
        Self {
            alu: 0,
            mul: 2,
            div: 19,
            load: 0,
            store: 0,
            branch_taken: 2,
            branch_not_taken: 0,
            jump: 1,
            system: 9,
            fp: 4,
            stage_overhead: 3,
        }
    }
}

impl PipelineTiming {
    pub fn field_names() -> &'static [&'static str] {
        &["ALU", "MUL", "DIV", "Load+", "Store+", "Branch-T", "Branch-NT", "Jump", "System", "FP", "Stages"]
    }

    pub fn get(&self, idx: usize) -> u64 {
        match idx {
            0 => self.alu, 1 => self.mul, 2 => self.div, 3 => self.load,
            4 => self.store, 5 => self.branch_taken, 6 => self.branch_not_taken,
            7 => self.jump, 8 => self.system, 9 => self.fp, 10 => self.stage_overhead,
            _ => 0,
        }
    }

    pub fn set(&mut self, idx: usize, val: u64) {
        match idx {
            0 => self.alu = val, 1 => self.mul = val, 2 => self.div = val,
            3 => self.load = val, 4 => self.store = val, 5 => self.branch_taken = val,
            6 => self.branch_not_taken = val, 7 => self.jump = val, 8 => self.system = val,
            9 => self.fp = val, 10 => self.stage_overhead = val, _ => {}
        }
    }

    pub fn descriptions() -> &'static [&'static str] {
        &[
            "add/sub/logic/shift/lui/auipc/imm", "mul/mulh/mulhsu/mulhu",
            "div/divu/rem/remu", "load (extra over cache miss)", "store (extra over cache)",
            "branch when taken (pipeline flush)", "branch when not taken", "jal / jalr",
            "ecall / ebreak / halt", "RV32F float instructions",
            "stage overhead added when pipeline is off (IF+ID+WB)",
        ]
    }
}

/// Map an instruction class to its EX-stage latency.
pub fn fu_latency_for_class(class: InstrClass, timing: &PipelineTiming) -> u8 {
    let extra = match class {
        InstrClass::Alu => timing.alu,
        InstrClass::Mul => timing.mul,
        InstrClass::Div => timing.div,
        InstrClass::Fp => timing.fp,
        InstrClass::Load => timing.load,
        InstrClass::Store => timing.store,
        InstrClass::System => timing.system,
        InstrClass::Branch | InstrClass::Jump | InstrClass::Unknown => 0,
    };
    ((1 + extra) as u8).max(1)
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
            fetch_pc: self.fetch_pc, halted: self.halted, faulted: self.faulted,
            stages: self.stages.clone(), fu_bank: self.fu_bank.clone(), fu_busy: self.fu_busy,
            predictor: self.predictor.clone(), pending_fetch_trap: self.pending_fetch_trap,
            cycle_count: self.cycle_count, instr_committed: self.instr_committed,
            stall_count: self.stall_count, stall_by_type: self.stall_by_type,
            flush_count: self.flush_count, branches_executed: self.branches_executed,
            class_counts: self.class_counts, last_cycle_cache_only: self.last_cycle_cache_only,
            hazard_msgs: self.hazard_msgs.clone(), hazard_traces: self.hazard_traces.clone(),
            gantt: self.gantt.clone(), next_gantt_id: self.next_gantt_id, next_seq: self.next_seq,
        }
    }

    fn restore_exec(&mut self, s: Self::Snapshot) {
        self.fetch_pc = s.fetch_pc; self.halted = s.halted; self.faulted = s.faulted;
        self.stages = s.stages; self.fu_bank = s.fu_bank; self.fu_busy = s.fu_busy;
        self.predictor = s.predictor; self.pending_fetch_trap = s.pending_fetch_trap;
        self.cycle_count = s.cycle_count; self.instr_committed = s.instr_committed;
        self.stall_count = s.stall_count; self.stall_by_type = s.stall_by_type;
        self.flush_count = s.flush_count; self.branches_executed = s.branches_executed;
        self.class_counts = s.class_counts; self.last_cycle_cache_only = s.last_cycle_cache_only;
        self.hazard_msgs = s.hazard_msgs; self.hazard_traces = s.hazard_traces;
        self.gantt = s.gantt; self.next_gantt_id = s.next_gantt_id; self.next_seq = s.next_seq;
    }

    fn inspect(&self) -> Option<&dyn crate::capability::PipelineInspect> { Some(self) }
}

impl PipelineSimState {
    pub fn new() -> Self {
        Self {
            enabled: true, bypass: PipelineBypassConfig::default(), branch_resolve: BranchResolve::Ex,
            mode: PipelineMode::SingleCycle, predict: BranchPredict::NotTaken,
            predictor: predictor::PredictorState::default(), exec_regions: Vec::new(), fetch_pc: 0,
            halted: false, faulted: false, stages: Default::default(),
            fu_bank: std::array::from_fn(|_| Vec::new()), fu_capacity: [1; FuKind::COUNT], fu_busy: [0; 7],
            cycle_count: 0, instr_committed: 0, stall_count: 0,
            stall_by_type: [0; HazardType::STALL_TYPE_COUNT], flush_count: 0,
            branches_executed: 0, class_counts: [0; InstrClass::COUNT], gantt: VecDeque::new(),
            next_gantt_id: 1, next_seq: 1, sequential_mode: false, hazard_msgs: Vec::new(),
            hazard_traces: Vec::new(), last_cycle_cache_only: false, pending_fetch_trap: None,
        }
    }

    pub fn set_exec_regions(&mut self, regions: &[ExecRegion]) {
        self.exec_regions.clear(); self.exec_regions.extend_from_slice(regions);
    }

    pub fn redirect_pc(&mut self, new_pc: u32) {
        self.stages = Default::default(); self.fu_bank = std::array::from_fn(|_| Vec::new());
        self.fu_busy = [0; 7]; self.fetch_pc = new_pc; self.halted = false; self.faulted = false;
        self.hazard_msgs.clear(); self.hazard_traces.clear(); self.last_cycle_cache_only = false;
        self.predictor.clear();
    }

    pub fn reset_stats(&mut self) {
        self.cycle_count = 0; self.instr_committed = 0; self.stall_count = 0;
        self.stall_by_type = [0; HazardType::STALL_TYPE_COUNT]; self.branches_executed = 0;
        self.flush_count = 0; self.class_counts = [0; InstrClass::COUNT];
    }

    pub fn reset_stages(&mut self, base_pc: u32) {
        self.fetch_pc = base_pc; self.stages = Default::default();
        self.fu_bank = std::array::from_fn(|_| Vec::new()); self.fu_busy = [0; 7];
        self.reset_stats(); self.gantt.clear(); self.next_gantt_id = 1; self.next_seq = 1;
        self.hazard_msgs.clear(); self.hazard_traces.clear(); self.last_cycle_cache_only = false;
        self.predictor.clear(); self.halted = false; self.faulted = false;
    }

    pub fn cpi(&self) -> f64 {
        if self.instr_committed == 0 { 0.0 } else { self.cycle_count as f64 / self.instr_committed as f64 }
    }

    pub fn set_predict(&mut self, predict: BranchPredict) {
        if self.predict != predict { self.predict = predict; self.predictor.clear(); }
    }

    pub fn set_legacy_forwarding(&mut self, enabled: bool) { self.bypass.set_legacy_forwarding(enabled); }
}

impl Default for PipelineSimState {
    fn default() -> Self { Self::new() }
}
