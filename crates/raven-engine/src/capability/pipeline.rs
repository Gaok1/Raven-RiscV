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

impl PipelineInstructionClass {
    pub const COUNT: usize = 10;

    pub const ALL: [Self; Self::COUNT] = [
        Self::Alu,
        Self::Multiply,
        Self::Divide,
        Self::Load,
        Self::Store,
        Self::Branch,
        Self::Jump,
        Self::System,
        Self::FloatingPoint,
        Self::Unknown,
    ];

    /// Short name for a column header or a legend.
    pub fn label(self) -> &'static str {
        match self {
            Self::Alu => "ALU",
            Self::Multiply => "MUL",
            Self::Divide => "DIV",
            Self::Load => "Load",
            Self::Store => "Store",
            Self::Branch => "Branch",
            Self::Jump => "Jump",
            Self::System => "System",
            Self::FloatingPoint => "FP",
            Self::Unknown => "?",
        }
    }

    /// Index into a per-class counter array.
    pub fn as_usize(self) -> usize {
        self as usize
    }
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

impl PipelineHazardKind {
    /// How many hazard kinds actually cost cycles. The name hazards are
    /// informational in an in-order design, so they have no counter.
    pub const STALL_TYPE_COUNT: usize = 5;

    pub fn label(self) -> &'static str {
        match self {
            Self::ReadAfterWrite => "RAW",
            Self::LoadUse => "load-use",
            Self::BranchFlush => "branch flush",
            Self::FunctionalUnitBusy => "FU busy",
            Self::MemoryLatency => "cache stall",
            Self::WriteAfterWrite => "WAW",
            Self::WriteAfterRead => "WAR",
        }
    }

    /// Index into a `[u64; STALL_TYPE_COUNT]` array, or `None` for the name
    /// hazards, which are reported but never stall an in-order pipeline.
    pub fn as_stall_index(self) -> Option<usize> {
        match self {
            Self::ReadAfterWrite => Some(0),
            Self::LoadUse => Some(1),
            Self::BranchFlush => Some(2),
            Self::FunctionalUnitBusy => Some(3),
            Self::MemoryLatency => Some(4),
            Self::WriteAfterWrite | Self::WriteAfterRead => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineTraceKind {
    Hazard(PipelineHazardKind),
    Forward,
}

impl PipelineTraceKind {
    /// A fixed-width tag for a dense trace column.
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Hazard(PipelineHazardKind::ReadAfterWrite) => "RAW",
            Self::Hazard(PipelineHazardKind::LoadUse) => "LOAD",
            Self::Hazard(PipelineHazardKind::BranchFlush) => "CTRL",
            Self::Hazard(PipelineHazardKind::FunctionalUnitBusy) => "FU",
            Self::Hazard(PipelineHazardKind::MemoryLatency) => "MEM",
            Self::Hazard(PipelineHazardKind::WriteAfterWrite) => "WAW",
            Self::Hazard(PipelineHazardKind::WriteAfterRead) => "WAR",
            Self::Forward => "FWD",
        }
    }
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
    /// Cycles lost to a second writer of a register that already has one in
    /// flight. Always zero in an in-order design, and in a renaming one: it is
    /// the price a scoreboard pays for having no spare names.
    pub waw_stalls: u64,
    /// Cycles a finished result waited so it would not overwrite a register an
    /// older instruction had yet to read.
    pub war_stalls: u64,
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
    /// Cycles an instruction of `latency_class` occupies this unit — what
    /// `first`'s `cycles_remaining` counts down from.
    ///
    /// Without it a host can only show the countdown, not how far through the
    /// operation it is, and guessing the total from some other backend's timing
    /// fills the bar against the wrong denominator. `None` says this model does
    /// not own its timing: RV32 is tuned from a table the host holds, so there
    /// the host supplies the total. Declare it wherever the model knows it.
    pub latency: Option<u8>,
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
    /// Which instruction this row is. A host that lights a row up from
    /// somewhere else on screen needs to know which one it is looking at, and
    /// the disassembly cannot say — a loop body repeats it every trip.
    pub address: u64,
    pub disassembly: &'a str,
    pub class: PipelineInstructionClass,
    pub first_cycle: u64,
    pub cells: usize,
    pub atomic: bool,
}

// ── Dynamically scheduled internals ──────────────────────────────────────────
//
// A model that leaves program order has no stages worth drawing: the state that
// matters moved into the structures that replaced them. These describe those
// structures in terms no model owns — a place to wait, a place to keep order, a
// register that is owed a value — so one screen serves a scoreboard and a
// reorder buffer without knowing which it is looking at.

/// How far an in-flight instruction has got, where "got" is not a stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineEntryPhase {
    /// The slot exists and holds nothing. Reported rather than skipped: a bank
    /// of free stations beside a stalling front end is what a structural
    /// hazard looks like, and an empty list would hide it.
    Free,
    /// Issued, still missing an operand.
    Waiting,
    /// Every operand in hand; not started.
    Armed,
    Executing,
    /// Done. The result exists but is not architectural yet.
    Finished,
    /// At the head, allowed to change the machine.
    Committing,
}

impl PipelineEntryPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Waiting => "wait",
            Self::Armed => "armed",
            Self::Executing => "exec",
            Self::Finished => "ready",
            Self::Committing => "commit",
        }
    }

    pub fn occupied(self) -> bool {
        self != Self::Free
    }
}

/// One source operand, and whether it is a value or a promise.
///
/// This is renaming made visible: an operand either has what it needs, or it
/// names the entry that owes it. Nothing else about the model has to be
/// explained for that distinction to be readable.
#[derive(Clone, Copy, Debug)]
pub struct PipelineOperandView<'a> {
    pub register: &'a str,
    /// The entry that will produce it, or `None` when the value is available.
    pub producer: Option<u64>,
}

/// One place an instruction waits for its operands and its hardware.
///
/// A reservation station in a renaming model; a functional unit's scoreboard
/// row in one without. Both answer the same question — what is sitting here,
/// and what is it still missing.
#[derive(Clone, Copy, Debug)]
pub struct PipelineStationView<'a> {
    /// What to call this slot, such as `"DIV0"`.
    pub name: &'a str,
    /// The declared unit it belongs to, indexing [`PipelineInspect::unit`].
    pub unit: usize,
    /// Identity for cross-referencing the other tables: the buffer entry this
    /// station produces for, or the station's own tag in a model with no
    /// buffer. `None` only when the slot is free.
    pub tag: Option<u64>,
    pub slot: Option<PipelineSlotView<'a>>,
    pub phase: PipelineEntryPhase,
    /// Sources, in the order the instruction names them. Two, like
    /// [`PipelineSlotView::sources`] — the third operand of an x86 `div` is
    /// implicit and not what these tables are for.
    pub operands: [Option<PipelineOperandView<'a>>; 2],
}

/// One entry of the structure that keeps program order.
#[derive(Clone, Copy, Debug)]
pub struct PipelineBufferView<'a> {
    pub tag: u64,
    pub slot: PipelineSlotView<'a>,
    pub phase: PipelineEntryPhase,
    /// Issued while a branch ahead of it was still a guess.
    pub speculative: bool,
}

/// One register whose next value is owed by something in flight.
#[derive(Clone, Copy, Debug)]
pub struct PipelineRenameView<'a> {
    pub register: &'a str,
    /// The entry that owes it — a buffer tag, or a unit in a model without one.
    pub producer: u64,
}

/// The structures a dynamically scheduled model runs on.
///
/// Offered only by a model that has them: a host asks for this and draws the
/// workbench when it gets one, or draws stages when it does not. Everything is
/// borrowed one item at a time, like [`PipelineInspect`], because these tables
/// change every cycle and a host redraws them every frame.
pub trait PipelineDynamicInspect {
    /// Places to wait, free ones included, in a stable order.
    fn station_count(&self) -> usize;
    fn station(&self, index: usize) -> Option<PipelineStationView<'_>>;

    /// Program order, oldest first. Empty in a model that keeps no buffer —
    /// which is itself the thing worth seeing about a scoreboard.
    fn buffer_count(&self) -> usize {
        0
    }

    /// How many entries the buffer holds when full.
    fn buffer_capacity(&self) -> usize {
        0
    }

    fn buffer_entry(&self, _index: usize) -> Option<PipelineBufferView<'_>> {
        None
    }

    /// Registers with a producer in flight — the alias table, or the
    /// scoreboard's register-result table. Same question, two names.
    fn rename_count(&self) -> usize;
    fn rename(&self, index: usize) -> Option<PipelineRenameView<'_>>;

    /// Fetched and not yet issued, in program order.
    fn queue_count(&self) -> usize;
    fn queued(&self, index: usize) -> Option<PipelineSlotView<'_>>;
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

/// What one adjustable property of a datapath currently reads as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineSettingValue<'a> {
    /// A wire that is either there or not — a bypass path.
    Toggle(bool),
    /// One of a fixed set of policies, named.
    Choice(&'a str),
    /// A count a user types: how many units, how many cycles.
    Number(u64),
}

/// One row of a pipeline settings screen.
///
/// The host draws rows and sends back adjustments; it never knows that a row
/// means "MEM→EX bypass" rather than "how many dividers". That keeps the
/// settings screen the same screen for every backend, which is the point: a
/// datapath a user cannot change is a datapath they cannot experiment with.
#[derive(Clone, Copy, Debug)]
pub struct PipelineSettingView<'a> {
    /// Heading this row sits under, such as `"FORWARDING"`. Rows sharing a
    /// group arrive consecutively.
    pub group: &'a str,
    pub name: &'a str,
    pub value: PipelineSettingValue<'a>,
    /// One or two sentences on what changing it does, for the explanation pane.
    pub summary: &'a str,
}

/// A pipeline whose datapath a user can change while the machine is loaded.
///
/// Every setting is addressed by index, and the set is fixed for a given
/// backend, so a host can lay the screen out once.
pub trait PipelineTuning {
    fn setting_count(&self) -> usize;
    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>>;

    /// Step a setting to its next value, or its previous one. Returns whether
    /// anything changed, so a host can decide not to redraw.
    fn adjust(&mut self, index: usize, forward: bool) -> bool;

    /// Set a [`PipelineSettingValue::Number`] row directly, for a host that
    /// lets the value be typed. Rows of any other kind refuse.
    fn set_number(&mut self, index: usize, value: u64) -> bool;
}
