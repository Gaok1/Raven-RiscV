pub mod sim;

use crate::falcon::instruction::Instruction;
use crate::ui::app::CpiConfig;
use crate::ui::view::disasm::disasm_word;
use ratatui::style::Color;
use std::cell::Cell;
use std::collections::VecDeque;
use std::time::{Duration, Instant}; // used by PipelineSpeed / PipelineSimState

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

    pub fn color(self) -> Color {
        match self {
            Self::Alu => Color::Cyan,
            Self::Mul => Color::Magenta,
            Self::Div => Color::Red,
            Self::Load => Color::Green,
            Self::Store => Color::Yellow,
            Self::Branch => Color::LightYellow,
            Self::Jump => Color::LightMagenta,
            Self::System => Color::Gray,
            Self::Fp => Color::LightCyan,
            Self::Unknown => Color::DarkGray,
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
            Ok(Ecall | Ebreak | Halt) => Self::System,
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
                ScW { rd, rs1, rs2 }
                | AmoswapW { rd, rs1, rs2 }
                | AmoaddW { rd, rs1, rs2 }
                | AmoxorW { rd, rs1, rs2 }
                | AmoandW { rd, rs1, rs2 }
                | AmoorW { rd, rs1, rs2 }
                | AmomaxW { rd, rs1, rs2 }
                | AmominW { rd, rs1, rs2 }
                | AmomaxuW { rd, rs1, rs2 }
                | AmominuW { rd, rs1, rs2 },
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
            Self::SingleCycle => "Single-cycle",
            Self::FunctionalUnits => "Functional Units",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchPredict {
    NotTaken,
    Taken,
}
impl BranchPredict {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotTaken => "Not-taken",
            Self::Taken => "Taken",
        }
    }
}

// ── FU latency (derived from global CpiConfig) ──────────────────────────────

/// Map an instruction class to its EX-stage latency using the global CPI config.
/// Values are additive: effective latency = 1 + cpi.field (minimum 1 cycle).
/// Branch/Jump cost in pipeline comes from flush penalty, not FU latency.
pub fn fu_latency_for_class(class: InstrClass, cpi: &CpiConfig) -> u8 {
    let extra: u64 = match class {
        InstrClass::Alu => cpi.alu,
        InstrClass::Mul => cpi.mul,
        InstrClass::Div => cpi.div,
        InstrClass::Fp => cpi.fp,
        InstrClass::Load => cpi.load,
        InstrClass::Store => cpi.store,
        InstrClass::System => cpi.system,
        // Branch and Jump: flush penalty already captures their pipeline cost
        InstrClass::Branch | InstrClass::Jump | InstrClass::Unknown => 0,
    };
    ((1u64 + extra) as u8).max(1)
}

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
    pub fn color(self) -> Color {
        match self {
            Self::Raw | Self::LoadUse => Color::Rgb(225, 180, 80),
            Self::BranchFlush => Color::Rgb(210, 72, 68),
            Self::FuBusy => Color::Rgb(195, 105, 250),
            Self::MemLatency => Color::Rgb(110, 175, 220),
            Self::Waw => Color::Rgb(115, 178, 235),
            Self::War => Color::Rgb(88, 200, 148),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceKind {
    Hazard(HazardType),
    Forward,
}

impl TraceKind {
    pub fn color(self) -> Color {
        match self {
            Self::Hazard(h) => h.color(),
            Self::Forward => Color::Rgb(110, 175, 220),
        }
    }

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

// ── Pipeline slot ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PipeSlot {
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
        let disasm = disasm_word(word);
        Self {
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

pub const MAX_GANTT_ROWS: usize = 12;
pub const MAX_GANTT_COLS: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GanttCell {
    Empty,          // instruction not in pipeline yet / already done
    InStage(Stage), // instruction is in this stage
    Stall,          // stalled in current stage
    Bubble,         // NOP bubble occupies this slot
    Flush,          // instruction was flushed
}

#[derive(Clone)]
pub struct GanttRow {
    pub pc: u32,
    pub disasm: String,
    pub class: InstrClass,
    /// One cell per cycle, oldest first. Length ≤ MAX_GANTT_COLS.
    pub cells: VecDeque<GanttCell>,
    /// The cycle this row was first seen.
    pub first_cycle: u64,
    /// True if this row has reached WB (commit) or been flushed.
    pub done: bool,
    /// The last stage emitted — used to detect stalls (same stage two cycles in a row).
    pub last_stage: Option<Stage>,
}

// ── Subtabs ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PipelineSubtab {
    Main,
    Config,
}

// ── Speed control ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipelineSpeed {
    Slow,
    Normal,
    Fast,
    Instant,
}

impl PipelineSpeed {
    pub fn interval(self) -> Duration {
        match self {
            Self::Slow => Duration::from_millis(600),
            Self::Normal => Duration::from_millis(300),
            Self::Fast => Duration::from_millis(80),
            Self::Instant => Duration::ZERO,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Slow => "Slow",
            Self::Normal => "Normal",
            Self::Fast => "Fast",
            Self::Instant => "Instant",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Slow => Self::Normal,
            Self::Normal => Self::Fast,
            Self::Fast => Self::Instant,
            Self::Instant => Self::Slow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineConfig {
    pub enabled: bool,
    pub forwarding: bool,
    pub branch_resolve: BranchResolve,
    pub mode: PipelineMode,
    pub predict: BranchPredict,
    pub speed: PipelineSpeed,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            forwarding: true,
            branch_resolve: BranchResolve::Ex,
            mode: PipelineMode::SingleCycle,
            predict: BranchPredict::NotTaken,
            speed: PipelineSpeed::Normal,
        }
    }
}

impl PipelineConfig {
    pub fn from_state(state: &PipelineSimState) -> Self {
        Self {
            enabled: state.enabled,
            forwarding: state.forwarding,
            branch_resolve: state.branch_resolve,
            mode: state.mode,
            predict: state.predict,
            speed: state.speed,
        }
    }

    pub fn apply_to_state(self, state: &mut PipelineSimState) {
        state.enabled = self.enabled;
        state.forwarding = self.forwarding;
        state.branch_resolve = self.branch_resolve;
        state.mode = self.mode;
        state.predict = self.predict;
        state.speed = self.speed;
    }
}

pub fn serialize_pipeline_config(cfg: &PipelineConfig) -> String {
    let mut s = String::from("# Raven Pipeline Config v1\n");
    s.push_str(&format!("enabled={}\n", cfg.enabled));
    s.push_str(&format!("forwarding={}\n", cfg.forwarding));
    s.push_str(&format!("mode={:?}\n", cfg.mode));
    s.push_str(&format!("branch_resolve={:?}\n", cfg.branch_resolve));
    s.push_str(&format!("predict={:?}\n", cfg.predict));
    s.push_str(&format!("speed={:?}\n", cfg.speed));
    s
}

pub fn parse_pipeline_config(text: &str) -> Result<PipelineConfig, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
        }
    }

    let get_bool = |key: &str, default: bool| -> bool {
        map.get(key)
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
            .unwrap_or(default)
    };

    let mode = match map.get("mode").map(String::as_str).unwrap_or("singlecycle") {
        "singlecycle" | "single-cycle" => PipelineMode::SingleCycle,
        "functionalunits" | "functional-units" | "functional_units" => {
            PipelineMode::FunctionalUnits
        }
        other => return Err(format!("Unknown pipeline mode: {other}")),
    };

    let branch_resolve = match map
        .get("branch_resolve")
        .map(String::as_str)
        .unwrap_or("ex")
    {
        "id" => BranchResolve::Id,
        "ex" => BranchResolve::Ex,
        "mem" => BranchResolve::Mem,
        other => return Err(format!("Unknown branch_resolve: {other}")),
    };

    let predict = match map.get("predict").map(String::as_str).unwrap_or("nottaken") {
        "nottaken" | "not-taken" | "not_taken" => BranchPredict::NotTaken,
        "taken" => BranchPredict::Taken,
        other => return Err(format!("Unknown predict mode: {other}")),
    };

    let speed = match map.get("speed").map(String::as_str).unwrap_or("normal") {
        "slow" => PipelineSpeed::Slow,
        "normal" => PipelineSpeed::Normal,
        "fast" => PipelineSpeed::Fast,
        "instant" => PipelineSpeed::Instant,
        other => return Err(format!("Unknown pipeline speed: {other}")),
    };

    Ok(PipelineConfig {
        enabled: get_bool("enabled", true),
        forwarding: get_bool("forwarding", true),
        branch_resolve,
        mode,
        predict,
        speed,
    })
}

// ── Main pipeline simulator state ────────────────────────────────────────────

pub struct PipelineSimState {
    // ── Config ──
    pub enabled: bool,
    pub forwarding: bool,
    pub branch_resolve: BranchResolve,
    pub mode: PipelineMode,
    pub predict: BranchPredict,

    // ── Pipeline own state (shares cpu/mem with RunState) ──
    pub fetch_pc: u32,
    pub halted: bool,
    pub faulted: bool,

    // ── Stages [IF=0, ID=1, EX=2, MEM=3, WB=4] ──
    pub stages: [Option<PipeSlot>; 5],

    // ── Stats ──
    pub cycle_count: u64,
    pub instr_committed: u64,
    pub stall_count: u64,
    pub flush_count: u64,
    pub class_counts: [u64; InstrClass::COUNT],

    // ── Gantt history ──
    pub gantt: VecDeque<GanttRow>,

    // ── Speed control ──
    pub speed: PipelineSpeed,
    pub last_tick: Instant,

    // ── UI state ──
    pub subtab: PipelineSubtab,
    pub config_cursor: usize,
    pub gantt_scroll: usize,

    // ── Active hazard message (set each tick) ──
    pub hazard_msgs: Vec<(HazardType, String)>,
    pub hazard_traces: Vec<HazardTrace>,

    // ── Hover state para botões da UI ──
    pub hover_subtab_main: bool,
    pub hover_subtab_config: bool,
    pub hover_core: bool,
    pub hover_reset: bool,
    pub hover_speed: bool,
    pub hover_state: bool,

    // ── Config subtab mouse ──
    pub hover_config_row: Option<usize>,
    /// (y, x_start, x_end) for each config row — set by render, read by mouse
    pub config_row_rects: Cell<[(u16, u16, u16); 4]>,

    // ── Geometrias dos botões (y, x_start, x_end) para mouse hit-test ──
    pub btn_subtab_main_rect: Cell<(u16, u16, u16)>,
    pub btn_subtab_config_rect: Cell<(u16, u16, u16)>,
    pub btn_core_rect: Cell<(u16, u16, u16)>,
    pub btn_reset_rect: Cell<(u16, u16, u16)>,
    pub btn_speed_rect: Cell<(u16, u16, u16)>,
    pub btn_state_rect: Cell<(u16, u16, u16)>,
}

impl PipelineSimState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            forwarding: true,
            branch_resolve: BranchResolve::Ex,
            mode: PipelineMode::SingleCycle,
            predict: BranchPredict::NotTaken,
            fetch_pc: 0,
            halted: false,
            faulted: false,
            stages: Default::default(),
            cycle_count: 0,
            instr_committed: 0,
            stall_count: 0,
            flush_count: 0,
            class_counts: [0; InstrClass::COUNT],
            gantt: VecDeque::new(),
            speed: PipelineSpeed::Normal,
            last_tick: Instant::now(),
            subtab: PipelineSubtab::Main,
            config_cursor: 0,
            gantt_scroll: 0,
            hazard_msgs: Vec::new(),
            hazard_traces: Vec::new(),
            hover_config_row: None,
            config_row_rects: Cell::new([(0, 0, 0); 4]),
            hover_subtab_main: false,
            hover_subtab_config: false,
            hover_core: false,
            hover_reset: false,
            hover_speed: false,
            hover_state: false,
            btn_subtab_main_rect: Cell::new((0, 0, 0)),
            btn_subtab_config_rect: Cell::new((0, 0, 0)),
            btn_core_rect: Cell::new((0, 0, 0)),
            btn_reset_rect: Cell::new((0, 0, 0)),
            btn_speed_rect: Cell::new((0, 0, 0)),
            btn_state_rect: Cell::new((0, 0, 0)),
        }
    }

    /// Reset pipeline stages and stats (shares cpu/mem with RunState).
    /// Flush in-flight stages and redirect fetch to `new_pc` without clearing stats.
    /// Used when the user manually moves the PC (e.g. clicking an instruction).
    pub fn redirect_pc(&mut self, new_pc: u32) {
        self.stages = Default::default();
        self.fetch_pc = new_pc;
        self.halted = false;
        self.faulted = false;
        self.hazard_msgs.clear();
        self.hazard_traces.clear();
    }

    pub fn reset_stages(&mut self, base_pc: u32) {
        self.fetch_pc = base_pc;
        self.stages = Default::default();
        self.cycle_count = 0;
        self.instr_committed = 0;
        self.stall_count = 0;
        self.flush_count = 0;
        self.class_counts = [0; InstrClass::COUNT];
        self.gantt.clear();
        self.gantt_scroll = 0;
        self.hazard_msgs.clear();
        self.hazard_traces.clear();
        self.halted = false;
        self.faulted = false;
        self.last_tick = Instant::now();
    }

    /// CPI = cycles / instructions (safe, returns 0.0 if no instrs)
    pub fn cpi(&self) -> f64 {
        if self.instr_committed == 0 {
            0.0
        } else {
            self.cycle_count as f64 / self.instr_committed as f64
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/support/ui_pipeline_mod.rs"]
mod tests;
