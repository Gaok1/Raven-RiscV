//! Inspecting an execution pipeline without depending on one ISA's simulator.
//!
//! The pipeline runtime may update every cycle, so hosts borrow one item at a
//! time instead of cloning stage slots or the full timing history for a frame.

/// Broad instruction categories used for timing and presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineInstructionClass {
    Alu,
    Multiply,
    Divide,
    Load,
    Store,
    Branch,
    Jump,
    System,
    FloatingPoint,
    Unknown,
}

/// Hazards common to pipeline models, independent of register width or ISA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineHazardKind {
    ReadAfterWrite,
    LoadUse,
    BranchFlush,
    FunctionalUnitBusy,
    MemoryLatency,
    WriteAfterWrite,
    WriteAfterRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineTraceKind {
    Hazard(PipelineHazardKind),
    Forward,
}

/// Current lifecycle and mode flags for a pipeline runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineStatus {
    pub enabled: bool,
    pub sequential: bool,
    pub halted: bool,
    pub faulted: bool,
}

/// Counters used by timing summaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineStats {
    pub cycles: u64,
    pub committed: u64,
    pub stalls: u64,
    pub raw_stalls: u64,
    pub load_use_stalls: u64,
    pub branch_stalls: u64,
    pub functional_unit_stalls: u64,
    pub memory_stalls: u64,
    pub flushes: u64,
    pub branches: u64,
}

impl PipelineStats {
    pub fn cpi(&self) -> f64 {
        if self.committed == 0 {
            0.0
        } else {
            self.cycles as f64 / self.committed as f64
        }
    }
}

/// One instruction or bubble occupying a stage or functional unit.
#[derive(Clone, Copy, Debug)]
pub struct PipelineSlotView<'a> {
    pub address: u64,
    pub disassembly: &'a str,
    pub class: PipelineInstructionClass,
    pub destination: Option<&'a str>,
    pub sources: [Option<&'a str>; 2],
    pub bubble: bool,
    pub speculative: bool,
    pub predicted_taken: bool,
    pub hazard: Option<PipelineHazardKind>,
    pub atomic: bool,
    pub cycles_remaining: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineStageView<'a> {
    pub name: &'a str,
    pub slot: Option<PipelineSlotView<'a>>,
}

/// Activity summarized per functional-unit kind. `first` is enough for the
/// compact host strip; `active` still reports parallel occupancy.
#[derive(Clone, Copy, Debug)]
pub struct PipelineUnitView<'a> {
    pub name: &'a str,
    pub capacity: usize,
    pub active: usize,
    pub first: Option<PipelineSlotView<'a>>,
    pub latency_class: PipelineInstructionClass,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineTraceView<'a> {
    pub kind: PipelineTraceKind,
    pub from_stage: usize,
    pub to_stage: usize,
    pub detail: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineTimelineState {
    Empty,
    Active,
    Speculative,
    Stalled,
    Bubble,
    Flushed,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineTimelineCell<'a> {
    pub label: &'a str,
    pub state: PipelineTimelineState,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineTimelineRow<'a> {
    pub disassembly: &'a str,
    pub class: PipelineInstructionClass,
    pub first_cycle: u64,
    pub cells: usize,
    pub atomic: bool,
}

/// Read-only pipeline data for stage, hazard, utilization, and history panes.
pub trait PipelineInspect {
    fn status(&self) -> PipelineStatus;
    fn stats(&self) -> PipelineStats;

    fn stage_count(&self) -> usize;
    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>>;

    fn unit_count(&self) -> usize;
    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>>;

    fn trace_count(&self) -> usize;
    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>>;
    fn status_message(&self) -> Option<&str>;

    fn timeline_len(&self) -> usize;
    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>>;
    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>>;
}
