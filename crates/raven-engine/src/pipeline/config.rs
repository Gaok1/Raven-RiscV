//! What an architecture *declares* about its pipeline.
//!
//! Everything here is vocabulary a pipelined machine has regardless of its ISA:
//! where a branch resolves, which bypasses exist, how long a class of work takes
//! in a functional unit. A backend fills in a [`PipelineShape`] and the engine in
//! [`super::scalar`] supplies the rest.

use crate::capability::{PipelineInstructionClass, PipelineStageRole};

/// One stage of a declared datapath.
///
/// The name is what a host prints; the role is what the stage *is*, which is how
/// the engine finds the stage that decodes, the one that executes, the one that
/// touches memory — without any of them having to be called IF/ID/EX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageSpec {
    pub name: &'static str,
    pub role: PipelineStageRole,
}

impl StageSpec {
    pub const fn new(name: &'static str, role: PipelineStageRole) -> Self {
        Self { name, role }
    }
}

/// The classic five-stage datapath, for a backend that wants it verbatim.
pub static RISC_FIVE_STAGE: [StageSpec; 5] = [
    StageSpec::new("IF", PipelineStageRole::Fetch),
    StageSpec::new("ID", PipelineStageRole::Decode),
    StageSpec::new("EX", PipelineStageRole::Execute),
    StageSpec::new("MEM", PipelineStageRole::Memory),
    StageSpec::new("WB", PipelineStageRole::Writeback),
];

/// Where a branch's real direction becomes known.
///
/// The later it resolves, the more speculative work a misprediction throws away,
/// which is the whole reason a host lets a user move it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchResolve {
    /// Resolved at the end of decode — one bubble.
    Id,
    /// Resolved at the end of execute — two bubbles.
    Ex,
    /// Resolved at the end of the memory stage — three bubbles.
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

    /// The stage role that resolves the branch, so the engine can locate it in a
    /// datapath that does not use the classic names.
    pub fn role(self) -> PipelineStageRole {
        match self {
            Self::Id => PipelineStageRole::Decode,
            Self::Ex => PipelineStageRole::Execute,
            Self::Mem => PipelineStageRole::Memory,
        }
    }
}

/// How instructions are scheduled once they leave decode.
///
/// The first two keep program order all the way through. The last two do not:
/// they are the classic dynamic-scheduling designs, where an instruction runs
/// as soon as its operands are ready rather than as soon as its turn comes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipelineMode {
    /// One instruction in execute at a time.
    SingleCycle,
    /// Execute dispatches into the unit bank; issue and retirement stay in
    /// program order.
    FunctionalUnits,
    /// CDC 6600 scoreboard: dynamic scheduling without renaming, so a WAW
    /// stalls at issue and a WAR is guarded at write-result.
    Scoreboard,
    /// Tomasulo with a reorder buffer: renaming makes the name hazards
    /// disappear, execution runs out of order, commit stays in order and
    /// exceptions stay precise.
    TomasuloRob,
}

impl PipelineMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SingleCycle => "Serialized",
            Self::FunctionalUnits => "Parallel UFs",
            Self::Scoreboard => "Scoreboard",
            Self::TomasuloRob => "Tomasulo + ROB",
        }
    }

    /// Whether this model may execute instructions out of program order.
    pub fn is_out_of_order(self) -> bool {
        matches!(self, Self::Scoreboard | Self::TomasuloRob)
    }

    /// Every mode, in the order a settings screen cycles through them.
    pub fn all() -> [Self; 4] {
        [
            Self::SingleCycle,
            Self::FunctionalUnits,
            Self::Scoreboard,
            Self::TomasuloRob,
        ]
    }
}

/// How the front end guesses a branch before it resolves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchPredict {
    NotTaken,
    Taken,
    /// Backwards taken, forwards not-taken — the loop heuristic.
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

/// Which bypass paths the datapath actually has wired.
///
/// With none of them a dependent instruction waits for its producer to write
/// back; with all of them only a load-use pair still stalls. That difference is
/// the single most visible thing about a pipeline, so it is configuration rather
/// than something baked into an engine.
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

    /// True when any operand bypass exists at all — the question a generic
    /// engine asks before deciding whether a RAW dependency has to stall.
    pub const fn forwards_operands(self) -> bool {
        self.ex_to_ex || self.mem_to_ex || self.wb_to_id
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

/// Extra execution cycles per instruction class.
///
/// Values are additive over the one cycle every instruction already spends in
/// execute, so an all-zero table is a perfectly ordinary single-cycle datapath.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Every class costs exactly one cycle — the timing a teaching datapath
    /// wants, where the point is the structure and not the latencies.
    pub const fn single_cycle() -> Self {
        Self {
            alu: 0,
            mul: 0,
            div: 0,
            load: 0,
            store: 0,
            branch_taken: 0,
            branch_not_taken: 0,
            jump: 0,
            system: 0,
            fp: 0,
            stage_overhead: 0,
        }
    }

    pub fn field_names() -> &'static [&'static str] {
        &[
            "ALU",
            "MUL",
            "DIV",
            "Load+",
            "Store+",
            "Branch-T",
            "Branch-NT",
            "Jump",
            "System",
            "FP",
            "Stages",
        ]
    }

    pub fn get(&self, idx: usize) -> u64 {
        match idx {
            0 => self.alu,
            1 => self.mul,
            2 => self.div,
            3 => self.load,
            4 => self.store,
            5 => self.branch_taken,
            6 => self.branch_not_taken,
            7 => self.jump,
            8 => self.system,
            9 => self.fp,
            10 => self.stage_overhead,
            _ => 0,
        }
    }

    pub fn set(&mut self, idx: usize, val: u64) {
        match idx {
            0 => self.alu = val,
            1 => self.mul = val,
            2 => self.div = val,
            3 => self.load = val,
            4 => self.store = val,
            5 => self.branch_taken = val,
            6 => self.branch_not_taken = val,
            7 => self.jump = val,
            8 => self.system = val,
            9 => self.fp = val,
            10 => self.stage_overhead = val,
            _ => {}
        }
    }

    pub fn descriptions() -> &'static [&'static str] {
        &[
            "add/sub/logic/shift/lui/auipc/imm",
            "mul/mulh/mulhsu/mulhu",
            "div/divu/rem/remu",
            "load (extra over cache miss)",
            "store (extra over cache)",
            "branch when taken (pipeline flush)",
            "branch when not taken",
            "jal / jalr",
            "ecall / ebreak / halt",
            "RV32F float instructions",
            "stage overhead added when pipeline is off (IF+ID+WB)",
        ]
    }

    /// Extra cycles this class spends in execute, beyond the first.
    pub fn extra_cycles(&self, class: PipelineInstructionClass) -> u64 {
        match class {
            PipelineInstructionClass::Alu => self.alu,
            PipelineInstructionClass::Multiply => self.mul,
            PipelineInstructionClass::Divide => self.div,
            PipelineInstructionClass::FloatingPoint => self.fp,
            PipelineInstructionClass::Load => self.load,
            PipelineInstructionClass::Store => self.store,
            PipelineInstructionClass::System => self.system,
            PipelineInstructionClass::Branch
            | PipelineInstructionClass::Jump
            | PipelineInstructionClass::Unknown => 0,
        }
    }

    /// Total cycles a class occupies its functional unit, never below one.
    pub fn latency(&self, class: PipelineInstructionClass) -> u8 {
        u8::try_from(1 + self.extra_cycles(class))
            .unwrap_or(u8::MAX)
            .max(1)
    }
}

/// A functional unit the execute stage dispatches to.
///
/// Declaring these is what turns the utilization strip on: the engine reports a
/// unit as active when the work in execute belongs to its class, so a backend
/// says which units exist rather than writing an occupancy query per unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSpec {
    pub name: &'static str,
    /// The classes this unit handles. An empty list matches every class, which
    /// is what a datapath with one undifferentiated ALU wants.
    pub classes: &'static [PipelineInstructionClass],
    /// The class a host should use for this unit's latency colour when the unit
    /// is idle and there is no work to read it from.
    pub latency_class: PipelineInstructionClass,
    /// The stage this unit sits in. A load/store unit lives in the memory
    /// stage, not with the ALUs, and reports occupancy from there.
    pub stage: PipelineStageRole,
    pub capacity: usize,
}

impl UnitSpec {
    pub const fn new(
        name: &'static str,
        classes: &'static [PipelineInstructionClass],
        latency_class: PipelineInstructionClass,
    ) -> Self {
        Self {
            name,
            classes,
            latency_class,
            stage: PipelineStageRole::Execute,
            capacity: 1,
        }
    }

    pub const fn in_stage(mut self, stage: PipelineStageRole) -> Self {
        self.stage = stage;
        self
    }

    pub const fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn handles(&self, class: PipelineInstructionClass) -> bool {
        self.classes.is_empty() || self.classes.contains(&class)
    }
}

/// The whole of what an architecture declares about its pipeline.
///
/// A backend hands one of these to [`ScalarPipeline::new`](super::ScalarPipeline::new)
/// and is then done with pipeline mechanics: stage shifting, hazard detection,
/// forwarding, branch prediction, flush accounting and the timing history all
/// come from the shape.
#[derive(Clone, Debug)]
pub struct PipelineShape {
    pub stages: &'static [StageSpec],
    /// Fixed instruction stride in bytes, or `None` for a variable-length ISA
    /// where each op reports its own length.
    pub instruction_bytes: Option<u64>,
    pub bypass: PipelineBypassConfig,
    pub predict: BranchPredict,
    pub resolve: BranchResolve,
    pub timing: PipelineTiming,
    /// Whether execute is one shared stage or dispatches into the unit bank.
    pub mode: PipelineMode,
    /// Functional units the utilization strip shows, and — in
    /// [`PipelineMode::FunctionalUnits`] — the ones execute dispatches into.
    /// Empty means the datapath has nothing worth breaking out beyond stages.
    pub units: &'static [UnitSpec],
    /// Whether instructions fetched behind an unresolved branch are marked
    /// speculative in the stage and timeline views.
    pub speculative_fetch: bool,
    /// Timeline rows kept before the oldest is dropped.
    pub history: usize,
}

impl PipelineShape {
    /// A fully forwarded in-order datapath with single-cycle execution — the
    /// shape a teaching backend wants before it starts changing anything.
    pub fn scalar(stages: &'static [StageSpec], instruction_bytes: Option<u64>) -> Self {
        Self {
            stages,
            instruction_bytes,
            bypass: PipelineBypassConfig::legacy_enabled(),
            predict: BranchPredict::NotTaken,
            resolve: BranchResolve::Ex,
            timing: PipelineTiming::single_cycle(),
            mode: PipelineMode::SingleCycle,
            units: &[],
            speculative_fetch: true,
            history: 256,
        }
    }

    pub fn with_units(mut self, units: &'static [UnitSpec]) -> Self {
        self.units = units;
        self
    }

    /// Dispatch out of execute into the declared units, so work of different
    /// classes overlaps instead of queueing behind one shared stage.
    pub fn with_parallel_units(mut self, units: &'static [UnitSpec]) -> Self {
        self.units = units;
        self.mode = PipelineMode::FunctionalUnits;
        self
    }

    /// Whether execute dispatches into the unit bank.
    pub fn dispatches_to_units(&self) -> bool {
        self.mode == PipelineMode::FunctionalUnits && !self.units.is_empty()
    }

    /// The unit that handles `class`, if the datapath declares one.
    pub fn unit_for(&self, class: PipelineInstructionClass) -> Option<usize> {
        self.units.iter().position(|unit| unit.handles(class))
    }

    pub fn with_bypass(mut self, bypass: PipelineBypassConfig) -> Self {
        self.bypass = bypass;
        self
    }

    pub fn with_predict(mut self, predict: BranchPredict) -> Self {
        self.predict = predict;
        self
    }

    pub fn with_resolve(mut self, resolve: BranchResolve) -> Self {
        self.resolve = resolve;
        self
    }

    pub fn with_timing(mut self, timing: PipelineTiming) -> Self {
        self.timing = timing;
        self
    }

    pub fn with_history(mut self, history: usize) -> Self {
        self.history = history;
        self
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// The first stage playing `role`, if the datapath has one.
    pub fn role_stage(&self, role: PipelineStageRole) -> Option<usize> {
        self.stages.iter().position(|spec| spec.role == role)
    }

    /// The stage that reads operands — decode, or the second stage in a
    /// datapath that names its stages something else.
    pub fn decode_stage(&self) -> usize {
        self.role_stage(PipelineStageRole::Decode)
            .unwrap_or(1)
            .min(self.stages.len().saturating_sub(1))
    }

    /// The stage that computes — execute, or the last stage when a datapath is
    /// short enough not to have a separate one.
    pub fn execute_stage(&self) -> usize {
        self.role_stage(PipelineStageRole::Execute)
            .unwrap_or(self.stages.len().saturating_sub(1))
    }

    /// The stage where `class`'s result first exists.
    ///
    /// An ALU result appears at the end of execute; a load's only after memory.
    /// A datapath with no memory stage — SAP's three-stage one — resolves a load
    /// in execute, so one rule covers both shapes.
    pub fn result_stage(&self, class: PipelineInstructionClass) -> usize {
        if class == PipelineInstructionClass::Load {
            self.role_stage(PipelineStageRole::Memory)
                .unwrap_or_else(|| self.execute_stage())
        } else {
            self.execute_stage()
        }
    }

    /// The stage a resolved branch redirects from.
    pub fn resolve_stage(&self) -> usize {
        self.role_stage(self.resolve.role())
            .unwrap_or_else(|| self.execute_stage())
    }

    /// How far to shift a PC before indexing a prediction table, so entries do
    /// not collide on an ISA whose instructions are wider than a byte.
    pub fn index_shift(&self) -> u32 {
        self.instruction_bytes
            .filter(|bytes| *bytes > 0)
            .map_or(0, |bytes| bytes.trailing_zeros())
    }
}
