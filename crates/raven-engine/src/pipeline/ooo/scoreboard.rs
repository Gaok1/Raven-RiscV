//! CDC 6600 scoreboard: dynamic scheduling without renaming.
//!
//! An instruction issues in program order into a functional unit, waits there
//! until every register it reads has been produced, executes, and writes its
//! result. Units finish at different times, so results reach the register file
//! out of program order — which is the point, and also what turns the two name
//! hazards from notes into costs:
//!
//! - **WAW** stalls at issue. With no spare names, a register can have only one
//!   in-flight writer, so a second one waits for the first.
//! - **WAR** is guarded at write-result. A result may not overwrite a register
//!   an older instruction has not read yet.
//!
//! Renaming makes both disappear, which is why they are worth watching here.
//!
//! # What this model does not do
//!
//! *No speculation.* Issuing a branch blocks issue until it writes its result;
//! the front end keeps fetching down the sequential path and is squashed when
//! the branch went elsewhere. The shape's [`BranchPredict`] has no effect — a
//! scoreboard machine had no predictor, and giving it one would hide the cost
//! this model exists to show. Since nothing speculative ever issues, a
//! misprediction throws away only the fetch queue.
//!
//! *Operands are read at write-result.* A backend executes an instruction in
//! one piece when the engine hands it back, so an instruction reads its sources
//! at the moment it writes its result rather than at Read Operands. The WAR
//! guard is therefore one phase more conservative than the CDC's — it waits for
//! the older reader to *write* rather than to *read* — in exchange for an
//! engine that needs nothing from a backend but instructions and their
//! semantics. Every stall it reports is a real one; a few last longer than the
//! hardware's would.
//!
//! *Exceptions are imprecise.* A younger independent instruction may already
//! have written its result when an older one faults. That is not a shortcut: it
//! is what a machine without a reorder buffer does, and it is the reason the
//! next model has one.
//!
//! [`BranchPredict`]: super::super::BranchPredict

use std::collections::VecDeque;

use crate::capability::{
    PipelineControl, PipelineEdge, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineStageRole, PipelineStageView, PipelineStats, PipelineStatus,
    PipelineTimelineCell, PipelineTimelineRow, PipelineTimelineState, PipelineTraceKind,
    PipelineTraceView, PipelineUnitView,
};

use super::super::config::{PipelineMode, PipelineShape, StageSpec, UnitSpec};
use super::super::op::PipelineOp;
use super::super::timeline::{Cell, Timeline, Trace};
use super::registers::{Producer, RegisterTable};

/// The phases a scoreboard schedules through.
///
/// These are the model's own, not the declared datapath's. A scoreboard
/// replaces everything after fetch, so reporting a shape's IF/ID/EX/MEM/WB here
/// would name stages that are no longer the ones doing the work.
pub static SCOREBOARD_PHASES: [StageSpec; 5] = [
    StageSpec::new("IF", PipelineStageRole::Fetch),
    StageSpec::new("IS", PipelineStageRole::Issue),
    StageSpec::new("RO", PipelineStageRole::Decode),
    StageSpec::new("EX", PipelineStageRole::Execute),
    StageSpec::new("WR", PipelineStageRole::Writeback),
];

const FETCH: usize = 0;
const ISSUE: usize = 1;
const READ: usize = 2;
const EXECUTE: usize = 3;
const WRITE: usize = 4;

/// The unit a scoreboard falls back on when a shape declares none: one
/// undifferentiated pipe, which is what a machine with a single ALU is.
static GENERIC_UNIT: [UnitSpec; 1] = [UnitSpec::new("EX", &[], PipelineInstructionClass::Alu)];

/// Where an instruction is within the four phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Issued into a unit, waiting for the registers it reads to be produced.
    Wait,
    /// Every source is available; the unit starts next cycle.
    Read,
    /// Running, with `cycles_left` on the op to go.
    Execute,
    /// Finished, waiting for the write-result guard to clear.
    Write,
}

/// One functional-unit instance and the scoreboard row that tracks it.
struct Unit<P> {
    /// Which declared [`UnitSpec`] this is an instance of.
    spec: usize,
    /// Kept beside the op so the bookkeeping survives the moment the op is out
    /// with the backend being executed.
    class: PipelineInstructionClass,
    /// Whether this instruction can change the next fetch address, which is the
    /// question issue is really asking — not what class it belongs to.
    redirects: bool,
    op: Option<PipelineOp<P>>,
    phase: Phase,
    /// Registers read and written, in the table's index space.
    sources: Vec<usize>,
    destinations: Vec<usize>,
    /// Sources still owed a value, and the unit that owes each one.
    pending: Vec<(usize, Producer)>,
}

impl<P> Unit<P> {
    fn idle(spec: usize) -> Self {
        Self {
            spec,
            class: PipelineInstructionClass::Unknown,
            redirects: false,
            op: None,
            phase: Phase::Wait,
            sources: Vec::new(),
            destinations: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn busy(&self) -> bool {
        self.op.is_some()
    }

    fn seq(&self) -> u64 {
        self.op.as_ref().map_or(u64::MAX, |op| op.seq)
    }

    fn clear(&mut self) {
        *self = Self::idle(self.spec);
    }
}

/// A dynamically scheduled pipeline that issues in order and writes out of it.
///
/// Driven exactly like [`ScalarPipeline`](super::super::ScalarPipeline) — the
/// backend takes what `start_cycle` hands back, executes it, and calls
/// `advance` — so an architecture switches models without changing its loop.
pub struct ScoreboardPipeline<P> {
    shape: PipelineShape,
    /// The units the shape declares, or the generic one when it declares none.
    specs: &'static [UnitSpec],
    /// One entry per declared unit: how many instances of it exist.
    capacity: Vec<usize>,
    units: Vec<Unit<P>>,
    /// Fetched and not yet issued, in program order.
    queue: VecDeque<PipelineOp<P>>,
    registers: RegisterTable,
    enabled: bool,
    halted: bool,
    faulted: bool,
    fetch_pc: u64,
    stats: PipelineStats,
    traces: Vec<Trace>,
    timeline: Timeline,
    status: Option<String>,
    /// The unit whose result `start_cycle` handed out and `advance` will free.
    /// Held rather than cleared so [`retry`](Self::retry) can put it back.
    writing: Option<usize>,
    /// An unresolved branch is in flight; nothing may issue behind it.
    branch_pending: bool,
    /// A system instruction is in flight; it must see architectural state.
    system_pending: bool,
    next_row: u64,
    next_seq: u64,
}

impl<P> ScoreboardPipeline<P> {
    pub fn new(mut shape: PipelineShape, entry: u64) -> Self {
        // Whatever the shape was declared with, this engine is the scoreboard,
        // and a host reads the mode off the shape to say what is running.
        shape.mode = PipelineMode::Scoreboard;
        let history = shape.history;
        let specs = if shape.units.is_empty() {
            &GENERIC_UNIT[..]
        } else {
            shape.units
        };
        let capacity: Vec<usize> = specs.iter().map(|spec| spec.capacity.max(1)).collect();
        let mut pipeline = Self {
            shape,
            specs,
            capacity,
            units: Vec::new(),
            queue: VecDeque::new(),
            registers: RegisterTable::new(),
            enabled: false,
            halted: false,
            faulted: false,
            fetch_pc: entry,
            stats: PipelineStats::default(),
            traces: Vec::new(),
            timeline: Timeline::new(history),
            status: None,
            writing: None,
            branch_pending: false,
            system_pending: false,
            next_row: 0,
            next_seq: 0,
        };
        pipeline.build_units();
        pipeline
    }

    pub fn shape(&self) -> &PipelineShape {
        &self.shape
    }

    pub fn shape_mut(&mut self) -> &mut PipelineShape {
        &mut self.shape
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn fetch_pc(&self) -> u64 {
        self.fetch_pc
    }

    /// How many instances of a declared unit exist. Raising it is the
    /// experiment the model is for: a second divider is a second thing that can
    /// be in flight, and the stalls that vanish say what the first one cost.
    ///
    /// Takes effect once the machine is idle, because an instruction already
    /// issued is holding a unit by index.
    pub fn set_capacity(&mut self, unit: usize, instances: usize) -> bool {
        let Some(slot) = self.capacity.get_mut(unit) else {
            return false;
        };
        *slot = instances.max(1);
        if self.units.iter().all(|unit| !unit.busy()) {
            self.build_units();
        }
        true
    }

    // ── The cycle ────────────────────────────────────────────────────────────

    /// Open a cycle and take the result that reaches the register file.
    ///
    /// One per cycle: there is a single result bus, so the oldest unit that
    /// passes the write-result guards wins it and the rest wait. Whoever the
    /// guards turned away is charged a stall here, which is what makes a WAR
    /// hazard something a user can see costing time.
    pub fn start_cycle(&mut self) -> Option<PipelineOp<P>> {
        self.stats.cycles = self.stats.cycles.saturating_add(1);
        self.traces.clear();
        for op in self.units.iter_mut().filter_map(|unit| unit.op.as_mut()) {
            op.hazard = None;
        }
        for op in &mut self.queue {
            op.hazard = None;
        }

        let index = self.writable_unit()?;
        self.writing = Some(index);
        self.units[index].op.take()
    }

    /// Put a result back because the machine could not finish it — waiting on
    /// input, most often. The cycle is spent; the unit keeps its row.
    pub fn retry(&mut self, op: PipelineOp<P>, reason: impl Into<String>) {
        if let Some(index) = self.writing.take() {
            self.units[index].op = Some(op);
        }
        self.status = Some(reason.into());
    }

    pub fn retire(&mut self, op: &PipelineOp<P>) {
        self.stats.committed = self.stats.committed.saturating_add(1);
        if op.branch {
            self.stats.branches = self.stats.branches.saturating_add(1);
        }
    }

    /// Report where a retired branch actually went.
    ///
    /// Returns the address to redirect to when the front end ran down the wrong
    /// path. There is no predictor to teach: the front end always assumed the
    /// sequential path, so this is a plain comparison against it.
    pub fn resolve(&mut self, op: &PipelineOp<P>, actual_next: u64) -> Option<u64> {
        (op.branch && actual_next != op.predicted_next).then_some(actual_next)
    }

    pub fn halt(&mut self) {
        self.halted = true;
        self.abandon();
        self.status = Some("halted".into());
    }

    pub fn fault(&mut self, message: impl Into<String>) {
        self.faulted = true;
        self.abandon();
        self.status = Some(message.into());
    }

    /// Run the rest of the cycle and return the address to fetch next, if the
    /// front end has room for one.
    ///
    /// The phases run backwards — write, execute, read, issue — so that a value
    /// written this cycle is visible to the instruction waiting on it in the
    /// same cycle, rather than a cycle later.
    pub fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        self.complete_write();
        if self.halted || self.faulted {
            return None;
        }
        self.status = None;

        if let Some(target) = redirect {
            self.flush(target);
        }
        self.tick_execute();
        self.start_ready();
        self.read_operands();
        self.issue();

        self.record_timeline();
        self.may_fetch().then_some(self.fetch_pc)
    }

    /// Accept a decoded instruction into the fetch queue.
    ///
    /// Nothing speculates here, so the front end simply walks the sequential
    /// path and lives with being squashed when a branch resolves elsewhere.
    pub fn fetched(&mut self, mut op: PipelineOp<P>) {
        op.row = self.next_row;
        self.next_row = self.next_row.saturating_add(1);
        op.seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        op.cycles_left = self.shape.timing.latency(op.class);
        op.predicted_next = op.sequential_address(self.shape.instruction_bytes);
        self.fetch_pc = op.predicted_next;

        self.timeline.open(
            op.row,
            op.disassembly.clone(),
            op.class,
            op.atomic,
            self.stats.cycles,
            Cell {
                label: SCOREBOARD_PHASES[FETCH].name,
                state: PipelineTimelineState::Active,
                role: PipelineStageRole::Fetch,
            },
        );
        self.queue.push_back(op);
    }

    // ── Phases ───────────────────────────────────────────────────────────────

    /// The oldest unit whose result may reach the register file this cycle.
    fn writable_unit(&mut self) -> Option<usize> {
        let mut ready: Vec<usize> = (0..self.units.len())
            .filter(|index| self.units[*index].busy() && self.units[*index].phase == Phase::Write)
            .collect();
        ready.sort_by_key(|index| self.units[*index].seq());

        for index in ready {
            let Some((kind, detail)) = self.write_blocked(index) else {
                return Some(index);
            };
            if let Some(op) = self.units[index].op.as_mut() {
                op.hazard = Some(kind);
            }
            self.stall(kind, READ, WRITE, detail);
        }
        None
    }

    /// Why a finished unit may not write yet, if it may not.
    fn write_blocked(&self, index: usize) -> Option<(PipelineHazardKind, String)> {
        let unit = &self.units[index];
        let mine = mnemonic(unit.op.as_ref()?);

        // WAR. Any other instruction that reads one of these registers and is
        // not waiting for it is necessarily older — a younger reader would have
        // this very unit recorded as the producer it waits on.
        for register in &unit.destinations {
            let reader = self.units.iter().enumerate().find(|(other, held)| {
                *other != index
                    && held.busy()
                    && held.sources.contains(register)
                    && !held.pending.iter().any(|(waiting, _)| waiting == register)
            });
            if let Some((_, held)) = reader {
                return Some((
                    PipelineHazardKind::WriteAfterRead,
                    format!(
                        "{mine} may not overwrite {} until {} has read it",
                        self.registers.name(*register),
                        held.op.as_ref().map_or("?", mnemonic),
                    ),
                ));
            }
        }

        // Memory keeps program order against itself: a store must not land
        // before a load that came first, whatever their units did.
        if is_memory(unit.class) {
            let older = self.units.iter().enumerate().find(|(other, held)| {
                *other != index && held.busy() && is_memory(held.class) && held.seq() < unit.seq()
            });
            if let Some((_, held)) = older {
                return Some((
                    PipelineHazardKind::FunctionalUnitBusy,
                    format!(
                        "{mine} waits for {} — memory keeps program order",
                        held.op.as_ref().map_or("?", mnemonic),
                    ),
                ));
            }
        }

        // A system instruction is the machine's own business: it runs when
        // nothing else is in flight, so it observes fully architectural state.
        if unit.class == PipelineInstructionClass::System
            && self
                .units
                .iter()
                .enumerate()
                .any(|(other, held)| other != index && held.busy())
        {
            return Some((
                PipelineHazardKind::FunctionalUnitBusy,
                format!("{mine} waits for the machine to drain"),
            ));
        }

        None
    }

    /// Free the unit whose result the backend has now executed, and hand the
    /// register it produced to whoever was waiting on it.
    fn complete_write(&mut self) {
        let Some(index) = self.writing.take() else {
            return;
        };
        let unit = &mut self.units[index];
        let class = unit.class;
        let redirects = unit.redirects;
        let destinations = std::mem::take(&mut unit.destinations);
        unit.clear();

        for register in destinations {
            self.registers.release(register, index);
        }
        for waiting in &mut self.units {
            waiting.pending.retain(|(_, producer)| *producer != index);
        }
        if redirects {
            self.branch_pending = false;
        }
        if class == PipelineInstructionClass::System {
            self.system_pending = false;
        }
    }

    /// Spend a cycle in every running unit. `cycles_left` counts the cycles
    /// still owed *after* the current one, so a unit leaves at the end of the
    /// cycle in which it runs out rather than one later.
    fn tick_execute(&mut self) {
        for unit in &mut self.units {
            if unit.phase != Phase::Execute {
                continue;
            }
            let Some(op) = unit.op.as_mut() else { continue };
            if op.cycles_left == 0 {
                unit.phase = Phase::Write;
            } else {
                op.cycles_left -= 1;
            }
        }
    }

    /// Hand every unit that read its operands last cycle to its own hardware.
    ///
    /// The cycle it starts in is one of the cycles it runs for, so single-cycle
    /// work is finished at the end of it.
    fn start_ready(&mut self) {
        for unit in &mut self.units {
            if unit.phase != Phase::Read {
                continue;
            }
            let Some(op) = unit.op.as_mut() else { continue };
            op.cycles_left = op.cycles_left.saturating_sub(1);
            unit.phase = Phase::Execute;
        }
    }

    /// Read Operands: a unit whose sources are all produced takes them from the
    /// register file. There is no bypass to take them from anywhere else — that
    /// is what the write-result ordering is for.
    fn read_operands(&mut self) {
        let mut waiting = Vec::new();
        for (index, unit) in self.units.iter_mut().enumerate() {
            if unit.phase != Phase::Wait || !unit.busy() {
                continue;
            }
            if unit.pending.is_empty() {
                unit.phase = Phase::Read;
                continue;
            }
            let (register, producer) = unit.pending[0];
            if let Some(op) = unit.op.as_mut() {
                op.hazard = Some(PipelineHazardKind::ReadAfterWrite);
            }
            waiting.push((index, register, producer));
        }
        for (index, register, producer) in waiting {
            let detail = format!(
                "{} waits for {} from {}",
                self.units[index].op.as_ref().map_or("?", mnemonic),
                self.registers.name(register),
                self.units
                    .get(producer)
                    .and_then(|unit| unit.op.as_ref())
                    .map_or("an older instruction", mnemonic),
            );
            self.stall(PipelineHazardKind::ReadAfterWrite, EXECUTE, ISSUE, detail);
        }
    }

    /// Issue: one instruction per cycle, in program order, into a free unit
    /// that handles its class. Everything that can stop it here is a hazard
    /// renaming or speculation would have removed.
    fn issue(&mut self) {
        let Some(front) = self.queue.front() else {
            return;
        };
        if self.branch_pending {
            let detail = format!(
                "{} cannot issue behind an unresolved branch",
                mnemonic(front)
            );
            self.block_issue(PipelineHazardKind::BranchFlush, detail);
            return;
        }
        if self.system_pending {
            let detail = format!(
                "{} waits — a system instruction serializes",
                mnemonic(front)
            );
            self.block_issue(PipelineHazardKind::FunctionalUnitBusy, detail);
            return;
        }

        let op = self.queue.pop_front().expect("checked just above");
        let operands = self.registers.resolve(&op);

        // WAW. Nothing renames here, so a register has room for one in-flight
        // writer and the second one waits for the first to write.
        if let Some(taken) = operands
            .destinations
            .iter()
            .find(|register| self.registers.producer(**register).is_some())
        {
            let detail = format!(
                "{} must wait — an older instruction still writes {}",
                mnemonic(&op),
                self.registers.name(*taken),
            );
            self.queue.push_front(op);
            self.block_issue(PipelineHazardKind::WriteAfterWrite, detail);
            return;
        }

        let Some(index) = self.free_unit(op.class) else {
            let detail = format!("all {} units are busy", self.spec_for(op.class).name);
            self.queue.push_front(op);
            self.block_issue(PipelineHazardKind::FunctionalUnitBusy, detail);
            return;
        };

        let pending: Vec<(usize, Producer)> = operands
            .sources
            .iter()
            .filter_map(|register| {
                self.registers
                    .producer(*register)
                    .map(|producer| (*register, producer))
            })
            .collect();
        for register in &operands.destinations {
            self.registers.claim(*register, index);
        }
        if op.branch {
            self.branch_pending = true;
        }
        if op.class == PipelineInstructionClass::System {
            self.system_pending = true;
        }

        let unit = &mut self.units[index];
        unit.class = op.class;
        unit.redirects = op.branch;
        unit.phase = Phase::Wait;
        unit.sources = operands.sources;
        unit.destinations = operands.destinations;
        unit.pending = pending;
        unit.op = Some(op);
    }

    // ── Internals ────────────────────────────────────────────────────────────

    fn build_units(&mut self) {
        self.units = self
            .capacity
            .iter()
            .enumerate()
            .flat_map(|(spec, instances)| (0..*instances).map(move |_| Unit::idle(spec)))
            .collect();
    }

    /// The declared unit that handles a class. A class no unit claims goes to
    /// the first one, so a shape that forgot to declare a unit still runs.
    fn spec_index(&self, class: PipelineInstructionClass) -> usize {
        self.specs
            .iter()
            .position(|spec| spec.handles(class))
            .unwrap_or(0)
    }

    fn spec_for(&self, class: PipelineInstructionClass) -> &'static UnitSpec {
        &self.specs[self.spec_index(class)]
    }

    fn free_unit(&self, class: PipelineInstructionClass) -> Option<usize> {
        let spec = self.spec_index(class);
        self.units
            .iter()
            .position(|unit| unit.spec == spec && !unit.busy())
    }

    fn block_issue(&mut self, kind: PipelineHazardKind, detail: String) {
        if let Some(front) = self.queue.front_mut() {
            front.hazard = Some(kind);
        }
        self.stall(kind, ISSUE, FETCH, detail);
    }

    fn stall(&mut self, kind: PipelineHazardKind, from: usize, to: usize, detail: String) {
        self.stats.stalls = self.stats.stalls.saturating_add(1);
        let counter = match kind {
            PipelineHazardKind::WriteAfterWrite => &mut self.stats.waw_stalls,
            PipelineHazardKind::WriteAfterRead => &mut self.stats.war_stalls,
            PipelineHazardKind::FunctionalUnitBusy => &mut self.stats.functional_unit_stalls,
            PipelineHazardKind::BranchFlush => &mut self.stats.branch_stalls,
            PipelineHazardKind::LoadUse => &mut self.stats.load_use_stalls,
            PipelineHazardKind::MemoryLatency => &mut self.stats.memory_stalls,
            PipelineHazardKind::ReadAfterWrite => &mut self.stats.raw_stalls,
        };
        *counter = counter.saturating_add(1);
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(kind),
            from,
            to,
            detail,
        });
    }

    /// Squash the front end and fetch from `target`.
    ///
    /// Only the queue: issue was blocked from the moment the branch went in, so
    /// nothing behind it ever claimed a register or a unit. That is the whole
    /// bargain this model makes — no speculation, and therefore no speculative
    /// state to unwind.
    fn flush(&mut self, target: u64) {
        let discarded: Vec<u64> = self.queue.drain(..).map(|op| op.row).collect();
        for row in &discarded {
            self.timeline.push(
                *row,
                Cell {
                    label: "FL",
                    state: PipelineTimelineState::Flushed,
                    role: PipelineStageRole::Other,
                },
            );
        }
        self.fetch_pc = target;
        let depth = discarded.len() as u64;
        self.stats.flushes = self.stats.flushes.saturating_add(1);
        self.stats.stalls = self.stats.stalls.saturating_add(depth);
        self.stats.branch_stalls = self.stats.branch_stalls.saturating_add(depth);
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush),
            from: WRITE,
            to: FETCH,
            detail: format!("redirect to 0x{target:X}"),
        });
    }

    /// Drop everything in flight, on a halt or a fault.
    fn abandon(&mut self) {
        self.queue.clear();
        for unit in &mut self.units {
            unit.clear();
        }
        self.registers.clear_claims();
        self.writing = None;
        self.branch_pending = false;
        self.system_pending = false;
    }

    fn idle(&self) -> bool {
        self.queue.is_empty() && self.units.iter().all(|unit| !unit.busy())
    }

    /// How deep the front end runs ahead of issue: as many instructions as the
    /// declared datapath has stages before execute, so a shape that describes a
    /// short front end gets a short one here too.
    fn queue_depth(&self) -> usize {
        self.shape.execute_stage().max(1)
    }

    fn may_fetch(&self) -> bool {
        self.queue.len() < self.queue_depth() && (self.enabled || self.idle())
    }

    fn oldest(&self, phase: Phase) -> Option<&Unit<P>> {
        self.units
            .iter()
            .filter(|unit| unit.busy() && unit.phase == phase)
            .min_by_key(|unit| unit.seq())
    }

    pub(super) fn reset_to(&mut self, address: u64, keep_history: bool) {
        self.abandon();
        self.registers = RegisterTable::new();
        self.halted = false;
        self.faulted = false;
        self.fetch_pc = address;
        self.status = None;
        self.traces.clear();
        self.build_units();
        if !keep_history {
            self.stats = PipelineStats::default();
            self.timeline.clear();
            self.next_row = 0;
            self.next_seq = 0;
        }
    }

    fn record_timeline(&mut self) {
        let state = |op: &PipelineOp<P>| {
            if op.hazard.is_some() {
                PipelineTimelineState::Stalled
            } else {
                PipelineTimelineState::Active
            }
        };
        let queued = self.queue.iter().map(|op| {
            (
                op.row,
                Cell {
                    label: SCOREBOARD_PHASES[FETCH].name,
                    state: state(op),
                    role: PipelineStageRole::Fetch,
                },
            )
        });
        // Work being executed reads as the unit doing it rather than as a
        // phase: which unit an instruction landed in is the thing a scoreboard
        // view is about.
        let scheduled = self.units.iter().filter_map(|unit| {
            let op = unit.op.as_ref()?;
            let phase = match unit.phase {
                Phase::Wait => ISSUE,
                Phase::Read => READ,
                Phase::Execute => EXECUTE,
                Phase::Write => WRITE,
            };
            let label = if unit.phase == Phase::Execute {
                self.specs[unit.spec].name
            } else {
                SCOREBOARD_PHASES[phase].name
            };
            Some((
                op.row,
                Cell {
                    label,
                    state: state(op),
                    role: SCOREBOARD_PHASES[phase].role,
                },
            ))
        });
        let cells: Vec<(u64, Cell)> = queued.chain(scheduled).collect();
        for (row, cell) in cells {
            self.timeline.push(row, cell);
        }
    }
}

fn is_memory(class: PipelineInstructionClass) -> bool {
    matches!(
        class,
        PipelineInstructionClass::Load | PipelineInstructionClass::Store
    )
}

/// The leading word of a disassembly, for a one-line hazard explanation.
fn mnemonic<P>(op: &PipelineOp<P>) -> &str {
    op.disassembly.split_whitespace().next().unwrap_or("?")
}

// ── What a host sees ─────────────────────────────────────────────────────────

impl<P> PipelineControl for ScoreboardPipeline<P> {
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self, address: u64) {
        self.reset_to(address, false);
    }

    fn redirect(&mut self, address: u64) {
        self.reset_to(address, true);
    }
}

impl<P> PipelineInspect for ScoreboardPipeline<P> {
    fn status(&self) -> PipelineStatus {
        PipelineStatus {
            enabled: self.enabled,
            sequential: !self.enabled,
            halted: self.halted,
            faulted: self.faulted,
        }
    }

    fn stats(&self) -> PipelineStats {
        self.stats
    }

    fn stage_count(&self) -> usize {
        SCOREBOARD_PHASES.len()
    }

    /// Each phase reports its oldest occupant. That is all a single slot can
    /// say about a model where several instructions are in the same phase at
    /// once — [`unit`](Self::unit) is where the rest of them are.
    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let spec = SCOREBOARD_PHASES.get(index)?;
        let op = match index {
            FETCH => self.queue.front(),
            ISSUE => self.oldest(Phase::Wait).and_then(|unit| unit.op.as_ref()),
            READ => self.oldest(Phase::Read).and_then(|unit| unit.op.as_ref()),
            EXECUTE => self
                .oldest(Phase::Execute)
                .and_then(|unit| unit.op.as_ref()),
            _ => self.oldest(Phase::Write).and_then(|unit| unit.op.as_ref()),
        };
        Some(PipelineStageView {
            name: spec.name,
            role: spec.role,
            slot: op.map(PipelineOp::view),
        })
    }

    /// The four phases in a line, and the redirect a resolved branch takes back
    /// to the front end.
    ///
    /// No forwarding edges, and their absence is the model: a scoreboard has no
    /// bypass network, which is why an instruction has to wait for its producer
    /// to reach the register file rather than catching the value in flight.
    fn edges(&self) -> Vec<PipelineEdge> {
        let mut edges: Vec<PipelineEdge> = (1..SCOREBOARD_PHASES.len())
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        edges.push(PipelineEdge {
            from: WRITE,
            to: FETCH,
            kind: PipelineEdgeKind::Feedback,
        });
        edges
    }

    fn unit_count(&self) -> usize {
        self.specs.len()
    }

    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>> {
        let spec = self.specs.get(index)?;
        let mine = || {
            self.units
                .iter()
                .filter(|unit| unit.spec == index && unit.busy())
        };
        let working = mine().min_by_key(|unit| unit.seq());
        Some(PipelineUnitView {
            name: spec.name,
            capacity: self.capacity.get(index).copied().unwrap_or(1),
            active: mine().count(),
            first: working
                .and_then(|unit| unit.op.as_ref())
                .map(PipelineOp::view),
            latency_class: working.map_or(spec.latency_class, |unit| unit.class),
        })
    }

    fn trace_count(&self) -> usize {
        self.traces.len()
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        let trace = self.traces.get(index)?;
        Some(PipelineTraceView {
            kind: trace.kind,
            from_stage: trace.from,
            to_stage: trace.to,
            detail: &trace.detail,
        })
    }

    fn status_message(&self) -> Option<&str> {
        self.status.as_deref()
    }

    fn timeline_len(&self) -> usize {
        self.timeline.len()
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        self.timeline.row(index)
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        self.timeline.cell(row, cell)
    }
}
