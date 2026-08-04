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

/// What a stage is *for*.
///
/// A host colours, orders and explains stages from this instead of matching
/// their names. Matching names meant a five-stage RISC pipeline rendered and
/// anything else fell through to a default — the role says what a stage does
/// even when an ISA calls it something else entirely.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PipelineStageRole {
    Fetch,
    Decode,
    /// Dispatch to a functional unit — where an out-of-order design queues.
    Issue,
    Execute,
    Memory,
    Writeback,
    /// Retire in program order, for designs that separate it from writeback.
    Commit,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineStageView<'a> {
    pub name: &'a str,
    pub slot: Option<PipelineSlotView<'a>>,
    /// What this stage does, independent of what it is called.
    pub role: PipelineStageRole,
}

/// How instructions move between two stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineEdgeKind {
    /// Normal flow: what leaves `from` enters `to` next cycle.
    Sequential,
    /// A bypass that skips the normal path — operand forwarding.
    Forward,
    /// Flow back to an earlier stage: a branch redirect or a replay.
    Feedback,
}

/// A directed edge in the pipeline graph.
///
/// Stage indices refer to [`PipelineInspect::stage`]. Together with the stages
/// these describe the whole datapath, which is what a host draws — rather than
/// assuming the stages sit in one row in index order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineEdge {
    pub from: usize,
    pub to: usize,
    pub kind: PipelineEdgeKind,
}

impl PipelineEdge {
    pub fn sequential(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            kind: PipelineEdgeKind::Sequential,
        }
    }
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
    /// The stage this cell is in, so a host can colour the timeline the same
    /// way it colours the stage boxes without parsing the label.
    pub role: PipelineStageRole,
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

    /// The datapath as a graph: how instructions move between stages.
    ///
    /// The default is the straight chain `0 → 1 → … → n-1`, which is what an
    /// in-order scalar pipeline is. Override it to describe anything else — a
    /// design that issues to parallel units, one that commits out of line, one
    /// with an explicit replay path. A host lays the stages out from this, so a
    /// backend never has to know how they will be drawn.
    fn edges(&self) -> Vec<PipelineEdge> {
        (1..self.stage_count())
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect()
    }

    /// Stages that begin the datapath — those nothing sequential feeds into.
    ///
    /// A host walks the graph from here, so it lays out a diamond or a fan-out
    /// as readily as a straight line.
    fn entry_stages(&self) -> Vec<usize> {
        let edges = self.edges();
        (0..self.stage_count())
            .filter(|stage| {
                !edges
                    .iter()
                    .any(|edge| edge.to == *stage && edge.kind == PipelineEdgeKind::Sequential)
            })
            .collect()
    }

    /// Stage indices in layout order: breadth-first from the entry stages along
    /// sequential edges, so a host draws the datapath in flow order without
    /// assuming it matches index order.
    ///
    /// Every stage appears exactly once; any not reachable from an entry is
    /// appended, so nothing is silently dropped from the diagram.
    fn stage_order(&self) -> Vec<usize> {
        let edges = self.edges();
        let mut order = Vec::with_capacity(self.stage_count());
        let mut queue: std::collections::VecDeque<usize> = self.entry_stages().into();
        while let Some(stage) = queue.pop_front() {
            if order.contains(&stage) {
                continue;
            }
            order.push(stage);
            for edge in &edges {
                if edge.from == stage && edge.kind == PipelineEdgeKind::Sequential {
                    queue.push_back(edge.to);
                }
            }
        }
        let unreachable: Vec<usize> = (0..self.stage_count())
            .filter(|stage| !order.contains(stage))
            .collect();
        order.extend(unreachable);
        order
    }

    fn unit_count(&self) -> usize;
    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>>;

    fn trace_count(&self) -> usize;
    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>>;
    fn status_message(&self) -> Option<&str>;

    fn timeline_len(&self) -> usize;
    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>>;
    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>>;
}

/// Common mutable controls for any pipeline model.
pub trait PipelineControl {
    fn set_enabled(&mut self, enabled: bool);
    fn reset(&mut self, address: u64);
    fn redirect(&mut self, address: u64);
}
