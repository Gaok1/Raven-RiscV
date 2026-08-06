//! Tomasulo with a reorder buffer: renaming, out-of-order execution, in-order
//! commit, precise exceptions.
//!
//! An instruction issues into a reservation station and a reorder-buffer entry.
//! The station holds it until every operand it waits on has been broadcast; the
//! buffer holds it from issue until its turn to change the machine comes round.
//! Those two jobs are what the design separates, and everything else follows:
//!
//! - A register does not name a value here, it names *whoever will produce one*.
//!   A second writer takes the name over — so the **WAW** stall a
//!   [scoreboard](super::ScoreboardPipeline) pays at issue does not exist.
//! - A result goes to the buffer entry that will commit it, never to a register
//!   an older instruction might still read — so the **WAR** guard does not
//!   exist either. Both counters stay at zero here, and that is the measurement
//!   worth making.
//! - Work finishes in whatever order the units manage, and commits in the order
//!   it was written. A fault therefore hits a machine that no younger
//!   instruction has touched.
//!
//! # Everything architectural happens at the head
//!
//! A backend executes an instruction in one piece when the engine hands it
//! back, and the engine only ever hands back the oldest entry. That single rule
//! is what makes this model's promises hold rather than merely be claimed:
//!
//! *Values are right.* Operands are read at commit, in program order, so the
//! register file an instruction reads is exactly the one program order says it
//! should. The renaming above it is what decides *when* work may run; it never
//! has to also carry the values, the way the hardware's common data bus does.
//!
//! *Exceptions are precise*, and stores reach memory in program order, for the
//! same reason and at no extra cost.
//!
//! *A misprediction is found at commit*, so recovery squashes the whole machine
//! rather than only the entries younger than the branch. That is the price: a
//! wrong guess costs the full speculative window. The rest of the package
//! resolves control the same way — a backend reports where a branch went when
//! it executes it — so this is the model paying a consistent cost, not a
//! shortcut taken here.

use std::collections::VecDeque;

use crate::capability::{
    PipelineControl, PipelineEdge, PipelineEdgeKind, PipelineHazardKind, PipelineInspect,
    PipelineInstructionClass, PipelineStageRole, PipelineStageView, PipelineStats, PipelineStatus,
    PipelineTimelineCell, PipelineTimelineRow, PipelineTimelineState, PipelineTraceKind,
    PipelineTraceView, PipelineUnitView,
};

use super::super::config::{PipelineMode, PipelineShape, StageSpec, UnitSpec};
use super::super::op::PipelineOp;
use super::super::predictor::TwoBitPredictor;
use super::super::timeline::{Cell, Timeline, Trace};
use super::registers::RegisterTable;

/// The phases an instruction passes through.
///
/// Writeback and commit are separate stages here, which is the whole shape of
/// the design: a result exists long before it is allowed to count.
pub static TOMASULO_PHASES: [StageSpec; 5] = [
    StageSpec::new("IF", PipelineStageRole::Fetch),
    StageSpec::new("IS", PipelineStageRole::Issue),
    StageSpec::new("EX", PipelineStageRole::Execute),
    StageSpec::new("WB", PipelineStageRole::Writeback),
    StageSpec::new("CM", PipelineStageRole::Commit),
];

const FETCH: usize = 0;
const ISSUE: usize = 1;
const EXECUTE: usize = 2;
const WRITEBACK: usize = 3;
const COMMIT: usize = 4;

/// Buffer entries when a shape says nothing about it.
///
/// Deep enough that the reservation stations are what a user is experimenting
/// with, shallow enough that filling it up is something they can actually make
/// happen.
const DEFAULT_BUFFER: usize = 16;

/// The unit a shape that declares none falls back on.
static GENERIC_UNIT: [UnitSpec; 1] = [UnitSpec::new("EX", &[], PipelineInstructionClass::Alu)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// In a station, waiting for operands or for the unit to take it.
    Issued,
    /// Running.
    Executing,
    /// Finished, holding its station until it gets the bus.
    Done,
    /// Result broadcast. Nothing is left to do but wait for its turn.
    Ready,
}

/// One reorder-buffer entry: an instruction and its place in program order.
struct Entry<P> {
    /// A tag, never reused, so a station can name a producer that has since
    /// left without any chance of naming a different instruction instead.
    id: usize,
    /// `None` only while the backend has the instruction out being committed.
    op: Option<PipelineOp<P>>,
    phase: Phase,
    destinations: Vec<usize>,
    class: PipelineInstructionClass,
    /// Issued while a branch ahead of it was still unresolved.
    speculative: bool,
}

/// One reservation station: a place at a unit, and what it is still waiting on.
///
/// It exists from issue until the result is broadcast — not until commit. That
/// is the point of having both: the buffer keeps program order long after the
/// hardware that did the work has moved on.
struct Station {
    /// Which declared unit this station belongs to.
    unit: usize,
    /// The buffer entry it will produce for.
    entry: usize,
    /// Registers still owed a value, and the tag that will carry each.
    pending: Vec<(usize, usize)>,
    started: bool,
}

/// A renaming pipeline: out of order in the middle, in order at both ends.
///
/// Driven exactly like [`ScalarPipeline`](super::super::ScalarPipeline) and
/// [`ScoreboardPipeline`](super::ScoreboardPipeline), so an architecture
/// changes model without changing its loop.
pub struct TomasuloPipeline<P> {
    shape: PipelineShape,
    specs: &'static [UnitSpec],
    /// Stations per declared unit. A station and the hardware it feeds are one
    /// resource here: splitting them would need a second number on the shape
    /// that no other model has, and what is worth seeing — a full unit backing
    /// issue up — happens either way.
    capacity: Vec<usize>,
    stations: Vec<Station>,
    /// Program order, front = oldest.
    buffer: VecDeque<Entry<P>>,
    buffer_size: usize,
    /// The register alias table: which tag owes each register its next value.
    registers: RegisterTable,
    queue: VecDeque<PipelineOp<P>>,
    predictor: TwoBitPredictor,
    enabled: bool,
    halted: bool,
    faulted: bool,
    fetch_pc: u64,
    stats: PipelineStats,
    traces: Vec<Trace>,
    timeline: Timeline,
    status: Option<String>,
    /// Set while the backend has the head out being committed.
    committing: bool,
    /// A system instruction is in the buffer; nothing issues behind it.
    system_pending: bool,
    next_id: usize,
    next_row: u64,
    next_seq: u64,
}

impl<P> TomasuloPipeline<P> {
    pub fn new(mut shape: PipelineShape, entry: u64) -> Self {
        shape.mode = PipelineMode::TomasuloRob;
        let history = shape.history;
        let shift = shape.index_shift();
        let specs = if shape.units.is_empty() {
            &GENERIC_UNIT[..]
        } else {
            shape.units
        };
        let capacity: Vec<usize> = specs.iter().map(|spec| spec.capacity.max(1)).collect();
        Self {
            shape,
            specs,
            capacity,
            stations: Vec::new(),
            buffer: VecDeque::new(),
            buffer_size: DEFAULT_BUFFER,
            registers: RegisterTable::new(),
            queue: VecDeque::new(),
            predictor: TwoBitPredictor::new(shift),
            enabled: false,
            halted: false,
            faulted: false,
            fetch_pc: entry,
            stats: PipelineStats::default(),
            traces: Vec::new(),
            timeline: Timeline::new(history),
            status: None,
            committing: false,
            system_pending: false,
            next_id: 0,
            next_row: 0,
            next_seq: 0,
        }
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

    /// How many instructions may be in flight at once. Shrinking it until issue
    /// starts backing up is how a user finds out what the buffer was buying.
    pub fn set_buffer_size(&mut self, entries: usize) {
        self.buffer_size = entries.max(1);
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// How many stations a declared unit has.
    pub fn set_capacity(&mut self, unit: usize, stations: usize) -> bool {
        let Some(slot) = self.capacity.get_mut(unit) else {
            return false;
        };
        *slot = stations.max(1);
        true
    }

    // ── The cycle ────────────────────────────────────────────────────────────

    /// Open a cycle and take the oldest instruction, if it has finished.
    ///
    /// Only ever the oldest, and only when everything about it is settled. A
    /// younger instruction whose result has been sitting ready for ten cycles
    /// waits, and that wait is the entire guarantee the buffer exists to make.
    pub fn start_cycle(&mut self) -> Option<PipelineOp<P>> {
        self.stats.cycles = self.stats.cycles.saturating_add(1);
        self.traces.clear();
        for op in self.buffer.iter_mut().filter_map(|entry| entry.op.as_mut()) {
            op.hazard = None;
        }
        for op in &mut self.queue {
            op.hazard = None;
        }

        let head = self.buffer.front_mut()?;
        if head.phase != Phase::Ready {
            return None;
        }
        self.committing = true;
        head.op.take()
    }

    /// Put the head back because the machine could not finish it.
    pub fn retry(&mut self, op: PipelineOp<P>, reason: impl Into<String>) {
        if self.committing {
            self.committing = false;
            if let Some(head) = self.buffer.front_mut() {
                head.op = Some(op);
            }
        }
        self.status = Some(reason.into());
    }

    pub fn retire(&mut self, op: &PipelineOp<P>) {
        self.stats.committed = self.stats.committed.saturating_add(1);
        if op.branch {
            self.stats.branches = self.stats.branches.saturating_add(1);
        }
    }

    /// Report where a committed branch actually went, teaching the predictor
    /// either way and returning the address to recover to when it guessed
    /// wrong.
    pub fn resolve(&mut self, op: &PipelineOp<P>, actual_next: u64) -> Option<u64> {
        if !op.branch {
            return None;
        }
        let taken = actual_next != op.sequential_address(self.shape.instruction_bytes);
        self.predictor.update(op.address, taken);
        (actual_next != op.predicted_next).then_some(actual_next)
    }

    pub fn halt(&mut self) {
        self.halted = true;
        self.abandon();
        self.status = Some("halted".into());
    }

    /// Raise a fault against the instruction the backend was committing.
    ///
    /// Nothing younger has changed the machine — it was all still in the
    /// buffer — so the state a handler sees is the state program order says it
    /// should. That is what a reorder buffer is for, and it costs nothing here
    /// beyond throwing the buffer away.
    pub fn fault(&mut self, message: impl Into<String>) {
        self.faulted = true;
        self.abandon();
        self.status = Some(message.into());
    }

    /// Run the rest of the cycle and return the address to fetch next.
    ///
    /// Commit, then broadcast, then execute, then issue: backwards, so a value
    /// broadcast this cycle frees a station in the same cycle rather than the
    /// next one.
    pub fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        self.complete_commit();
        if self.halted || self.faulted {
            return None;
        }
        self.status = None;

        if let Some(target) = redirect {
            self.squash(target);
        }
        self.broadcast();
        self.execute();
        self.issue();

        self.record_timeline();
        self.may_fetch().then_some(self.fetch_pc)
    }

    /// Accept a decoded instruction into the fetch queue, guessing where a
    /// branch will go so the queue behind it is worth having.
    pub fn fetched(&mut self, mut op: PipelineOp<P>) {
        op.row = self.next_row;
        self.next_row = self.next_row.saturating_add(1);
        op.seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        op.cycles_left = self.shape.timing.latency(op.class);
        op.speculative = self.unresolved_branch();

        let sequential = op.sequential_address(self.shape.instruction_bytes);
        op.predicted_next = match (op.branch, op.branch_target) {
            (true, Some(target)) => {
                op.predicted_taken =
                    self.predictor
                        .direction(self.shape.predict, op.address, target);
                if op.predicted_taken {
                    target
                } else {
                    sequential
                }
            }
            _ => sequential,
        };
        self.fetch_pc = op.predicted_next;

        self.timeline.open(
            op.row,
            op.disassembly.clone(),
            op.class,
            op.atomic,
            self.stats.cycles,
            Cell {
                label: TOMASULO_PHASES[FETCH].name,
                state: if op.speculative {
                    PipelineTimelineState::Speculative
                } else {
                    PipelineTimelineState::Active
                },
                role: PipelineStageRole::Fetch,
            },
        );
        self.queue.push_back(op);
    }

    // ── Phases ───────────────────────────────────────────────────────────────

    /// Retire the head the backend has just executed and give up the names it
    /// was holding.
    fn complete_commit(&mut self) {
        if !self.committing {
            return;
        }
        self.committing = false;
        let Some(entry) = self.buffer.pop_front() else {
            return;
        };
        for register in entry.destinations {
            // Only if nothing younger has taken the name over in the meantime.
            // A rename is precisely the case where this must not fire.
            self.registers.release(register, entry.id);
        }
        if entry.class == PipelineInstructionClass::System {
            self.system_pending = false;
        }
    }

    /// The common data bus: one finished result per cycle, oldest first,
    /// reaching every station waiting on it.
    ///
    /// This is the wire renaming is built on. A waiting instruction hears the
    /// tag it was told to listen for and stops waiting — it never has to go
    /// back to the register file, and no name it holds can be spoiled by
    /// anyone else in the meantime.
    fn broadcast(&mut self) {
        let finished: Vec<usize> = self
            .buffer
            .iter()
            .filter(|entry| entry.phase == Phase::Done)
            .map(|entry| entry.id)
            .collect();
        let Some(index) = finished.first().and_then(|id| {
            self.stations
                .iter()
                .position(|station| station.entry == *id)
        }) else {
            return;
        };
        let station = self.stations.remove(index);
        let mut listeners = 0;
        for waiting in &mut self.stations {
            let before = waiting.pending.len();
            waiting.pending.retain(|(_, tag)| *tag != station.entry);
            listeners += before - waiting.pending.len();
        }
        let Some(entry) = self.entry_mut(station.entry) else {
            return;
        };
        entry.phase = Phase::Ready;
        if listeners > 0 {
            let detail = format!(
                "{} broadcasts to {listeners} waiting operand(s)",
                entry.op.as_ref().map_or("?", mnemonic),
            );
            self.traces.push(Trace {
                kind: PipelineTraceKind::Forward,
                from: WRITEBACK,
                to: ISSUE,
                detail,
            });
        }
    }

    /// Tick what is running, then start whatever is now able to.
    ///
    /// `cycles_left` counts the cycles still owed *after* the current one, so
    /// the cycle an instruction starts in is one of the cycles it runs for and
    /// single-cycle work is finished at the end of it.
    fn execute(&mut self) {
        for entry in &mut self.buffer {
            if entry.phase != Phase::Executing {
                continue;
            }
            let Some(op) = entry.op.as_mut() else {
                continue;
            };
            if op.cycles_left == 0 {
                entry.phase = Phase::Done;
            } else {
                op.cycles_left -= 1;
            }
        }

        let ready: Vec<usize> = self
            .stations
            .iter()
            .filter(|station| !station.started && station.pending.is_empty())
            .map(|station| station.entry)
            .collect();
        let waiting: Vec<(usize, usize, usize)> = self
            .stations
            .iter()
            .filter(|station| !station.started)
            .filter_map(|station| {
                let (register, tag) = *station.pending.first()?;
                Some((station.entry, register, tag))
            })
            .collect();

        for id in ready {
            if let Some(station) = self.stations.iter_mut().find(|station| station.entry == id) {
                station.started = true;
            }
            if let Some(entry) = self.entry_mut(id) {
                entry.phase = Phase::Executing;
                if let Some(op) = entry.op.as_mut() {
                    op.cycles_left = op.cycles_left.saturating_sub(1);
                }
            }
        }
        for (id, register, tag) in waiting {
            if let Some(entry) = self.entry_mut(id)
                && let Some(op) = entry.op.as_mut()
            {
                op.hazard = Some(PipelineHazardKind::ReadAfterWrite);
            }
            let detail = format!(
                "{} waits for {} from {}",
                self.entry(id)
                    .and_then(|e| e.op.as_ref())
                    .map_or("?", mnemonic),
                self.registers.name(register),
                self.entry(tag)
                    .and_then(|e| e.op.as_ref())
                    .map_or("an older instruction", mnemonic),
            );
            self.stall(PipelineHazardKind::ReadAfterWrite, EXECUTE, ISSUE, detail);
        }
    }

    /// Issue: one instruction per cycle, in program order, renaming as it goes.
    ///
    /// Nothing here can stall on a name. What can stall is running out of
    /// somewhere to put the instruction, which is a different kind of limit and
    /// the one a user should be adjusting.
    fn issue(&mut self) {
        let Some(front) = self.queue.front() else {
            return;
        };
        if self.system_pending {
            let detail = format!(
                "{} waits — a system instruction commits alone",
                mnemonic(front)
            );
            self.block_issue(detail);
            return;
        }
        if self.buffer.len() >= self.buffer_size {
            let detail = format!("{} waits — the buffer is full", mnemonic(front));
            self.block_issue(detail);
            return;
        }
        let unit = self.unit_for(front.class);
        let system = front.class == PipelineInstructionClass::System;
        if !system && self.occupancy(unit) >= self.capacity[unit] {
            let detail = format!("no free {} station", self.specs[unit].name);
            self.block_issue(detail);
            return;
        }

        let op = self.queue.pop_front().expect("checked just above");
        let operands = self.registers.resolve(&op);
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        if system {
            // A system instruction has no operands the engine can reason about
            // and no result to broadcast: it does its whole job at the head,
            // with the machine drained ahead of it.
            self.system_pending = true;
        } else {
            // An operand waits only for a producer that has not broadcast yet.
            // One that has is a value in a buffer entry, which the head will
            // read when its turn comes.
            let pending: Vec<(usize, usize)> = operands
                .sources
                .iter()
                .filter_map(|register| {
                    let tag = self.registers.producer(*register)?;
                    let live = self.entry(tag)?.phase != Phase::Ready;
                    live.then_some((*register, tag))
                })
                .collect();
            self.stations.push(Station {
                unit,
                entry: id,
                pending,
                started: false,
            });
        }

        // The rename. Whoever held the name loses it without being told and
        // without anyone waiting — which is the WAW hazard not happening.
        for register in &operands.destinations {
            self.registers.claim(*register, id);
        }

        let speculative = op.speculative || self.unresolved_branch();
        self.buffer.push_back(Entry {
            id,
            class: op.class,
            destinations: operands.destinations,
            phase: if system { Phase::Ready } else { Phase::Issued },
            speculative,
            op: Some(op),
        });
    }

    // ── Internals ────────────────────────────────────────────────────────────

    fn entry(&self, id: usize) -> Option<&Entry<P>> {
        self.buffer.iter().find(|entry| entry.id == id)
    }

    fn entry_mut(&mut self, id: usize) -> Option<&mut Entry<P>> {
        self.buffer.iter_mut().find(|entry| entry.id == id)
    }

    /// A branch is in flight whose direction nobody knows yet, so everything
    /// issued behind it is a guess.
    fn unresolved_branch(&self) -> bool {
        self.buffer
            .iter()
            .any(|entry| entry.op.as_ref().is_some_and(|op| op.branch))
    }

    fn unit_for(&self, class: PipelineInstructionClass) -> usize {
        self.specs
            .iter()
            .position(|spec| spec.handles(class))
            .unwrap_or(0)
    }

    fn occupancy(&self, unit: usize) -> usize {
        self.stations
            .iter()
            .filter(|station| station.unit == unit)
            .count()
    }

    fn block_issue(&mut self, detail: String) {
        if let Some(front) = self.queue.front_mut() {
            front.hazard = Some(PipelineHazardKind::FunctionalUnitBusy);
        }
        self.stall(PipelineHazardKind::FunctionalUnitBusy, ISSUE, FETCH, detail);
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

    /// Throw the speculative machine away and fetch from `target`.
    ///
    /// Everything, not just the entries younger than the branch: the branch
    /// only became a known-wrong guess by committing, and committing is the
    /// last thing that happens to it. The alias table can therefore be emptied
    /// rather than rebuilt from survivors, because there are none.
    fn squash(&mut self, target: u64) {
        let discarded: Vec<u64> = self
            .queue
            .drain(..)
            .map(|op| op.row)
            .chain(
                self.buffer
                    .drain(..)
                    .filter_map(|entry| entry.op)
                    .map(|op| op.row),
            )
            .collect();
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
        self.stations.clear();
        self.registers.clear_claims();
        self.system_pending = false;
        self.fetch_pc = target;

        let depth = discarded.len() as u64;
        self.stats.flushes = self.stats.flushes.saturating_add(1);
        self.stats.stalls = self.stats.stalls.saturating_add(depth);
        self.stats.branch_stalls = self.stats.branch_stalls.saturating_add(depth);
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush),
            from: COMMIT,
            to: FETCH,
            detail: format!("mispredict — {depth} squashed, fetch to 0x{target:X}"),
        });
    }

    fn abandon(&mut self) {
        self.queue.clear();
        self.buffer.clear();
        self.stations.clear();
        self.registers.clear_claims();
        self.committing = false;
        self.system_pending = false;
    }

    fn idle(&self) -> bool {
        self.queue.is_empty() && self.buffer.is_empty()
    }

    /// How far the front end runs ahead of issue: as many instructions as the
    /// declared datapath has stages before execute.
    fn queue_depth(&self) -> usize {
        self.shape.execute_stage().max(1)
    }

    fn may_fetch(&self) -> bool {
        self.queue.len() < self.queue_depth() && (self.enabled || self.idle())
    }

    fn oldest(&self, phase: Phase) -> Option<&PipelineOp<P>> {
        self.buffer
            .iter()
            .find(|entry| entry.phase == phase)
            .and_then(|entry| entry.op.as_ref())
    }

    pub(super) fn reset_to(&mut self, address: u64, keep_history: bool) {
        self.abandon();
        self.registers = RegisterTable::new();
        self.halted = false;
        self.faulted = false;
        self.fetch_pc = address;
        self.status = None;
        self.traces.clear();
        if !keep_history {
            self.stats = PipelineStats::default();
            self.timeline.clear();
            self.next_id = 0;
            self.next_row = 0;
            self.next_seq = 0;
            self.predictor.clear();
        }
    }

    fn record_timeline(&mut self) {
        let cell = |label: &'static str, role, stalled: bool, speculative: bool| Cell {
            label,
            state: if stalled {
                PipelineTimelineState::Stalled
            } else if speculative {
                PipelineTimelineState::Speculative
            } else {
                PipelineTimelineState::Active
            },
            role,
        };
        let queued = self.queue.iter().map(|op| {
            (
                op.row,
                cell(
                    TOMASULO_PHASES[FETCH].name,
                    PipelineStageRole::Fetch,
                    op.hazard.is_some(),
                    op.speculative,
                ),
            )
        });
        let head = self.buffer.front().map(|entry| entry.id);
        let scheduled = self.buffer.iter().filter_map(|entry| {
            let op = entry.op.as_ref()?;
            let (label, role, stalled) = match entry.phase {
                Phase::Issued => (
                    TOMASULO_PHASES[ISSUE].name,
                    PipelineStageRole::Issue,
                    op.hazard.is_some(),
                ),
                Phase::Executing => (
                    self.specs[self.unit_for(entry.class)].name,
                    PipelineStageRole::Execute,
                    false,
                ),
                Phase::Done => (
                    TOMASULO_PHASES[WRITEBACK].name,
                    PipelineStageRole::Writeback,
                    false,
                ),
                Phase::Ready if head == Some(entry.id) => (
                    TOMASULO_PHASES[COMMIT].name,
                    PipelineStageRole::Commit,
                    false,
                ),
                // A result that has been broadcast and is still in the buffer
                // is waiting for its turn, and showing that wait is most of
                // the point of having a buffer at all.
                Phase::Ready => (
                    TOMASULO_PHASES[WRITEBACK].name,
                    PipelineStageRole::Writeback,
                    true,
                ),
            };
            Some((op.row, cell(label, role, stalled, entry.speculative)))
        });
        let cells: Vec<(u64, Cell)> = queued.chain(scheduled).collect();
        for (row, cell) in cells {
            self.timeline.push(row, cell);
        }
    }
}

/// The leading word of a disassembly, for a one-line hazard explanation.
fn mnemonic<P>(op: &PipelineOp<P>) -> &str {
    op.disassembly.split_whitespace().next().unwrap_or("?")
}

// ── What a host sees ─────────────────────────────────────────────────────────

impl<P> PipelineControl for TomasuloPipeline<P> {
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

impl<P> PipelineInspect for TomasuloPipeline<P> {
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
        TOMASULO_PHASES.len()
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let spec = TOMASULO_PHASES.get(index)?;
        let op = match index {
            FETCH => self.queue.front(),
            ISSUE => self.oldest(Phase::Issued),
            EXECUTE => self.oldest(Phase::Executing),
            WRITEBACK => self.oldest(Phase::Done),
            // The head, and only when it is finished: an entry still executing
            // is not at commit, it is in the middle of the machine.
            _ => self
                .buffer
                .front()
                .filter(|entry| entry.phase == Phase::Ready)
                .and_then(|entry| entry.op.as_ref()),
        };
        Some(PipelineStageView {
            name: spec.name,
            role: spec.role,
            slot: op.map(PipelineOp::view),
        })
    }

    /// The five phases in a line, the recovery path from commit back to fetch,
    /// and the common data bus.
    ///
    /// That last edge is the design: a result reaching a waiting instruction
    /// directly, without passing through a register anyone else could spoil. A
    /// scoreboard draws no such wire, which is why it has to stall for names.
    fn edges(&self) -> Vec<PipelineEdge> {
        let mut edges: Vec<PipelineEdge> = (1..TOMASULO_PHASES.len())
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        edges.push(PipelineEdge {
            from: WRITEBACK,
            to: ISSUE,
            kind: PipelineEdgeKind::Forward,
        });
        edges.push(PipelineEdge {
            from: COMMIT,
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
        let working = self
            .stations
            .iter()
            .filter(|station| station.unit == index)
            .min_by_key(|station| station.entry)
            .and_then(|station| self.entry(station.entry));
        Some(PipelineUnitView {
            name: spec.name,
            capacity: self.capacity.get(index).copied().unwrap_or(1),
            active: self.occupancy(index),
            first: working
                .and_then(|entry| entry.op.as_ref())
                .map(PipelineOp::view),
            latency_class: working.map_or(spec.latency_class, |entry| entry.class),
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
