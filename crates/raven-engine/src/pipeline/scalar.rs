//! The in-order pipeline every architecture gets from its [`PipelineShape`].
//!
//! A backend supplies instructions as [`PipelineOp`]s and executes them when
//! they retire. Everything between — stage occupancy, operand forwarding,
//! hazard detection and classification, branch prediction and flush accounting,
//! multi-cycle functional units, and the timing history — happens here, once,
//! for every ISA.
//!
//! The cycle a backend drives looks like this:
//!
//! ```ignore
//! let retiring = pipeline.start_cycle();
//! let mut redirect = None;
//! if let Some(op) = retiring {
//!     let outcome = execute(&op)?;          // the backend's own semantics
//!     pipeline.retire(&op);
//!     redirect = pipeline.resolve(&op, next_pc);
//! }
//! if let Some(address) = pipeline.advance(redirect) {
//!     pipeline.fetched(decode_at(address));  // the backend's own decoder
//! }
//! ```

use crate::capability::{
    PipelineHazardKind, PipelineInstructionClass, PipelineStageRole, PipelineStats,
    PipelineTimelineState, PipelineTraceKind,
};

use super::config::{BranchPredict, PipelineMode, PipelineShape};
use super::op::PipelineOp;
use super::predictor::TwoBitPredictor;
use super::timeline::{Cell, Timeline, Trace};

/// `Clone` because a host may journal its machine for step-back, and the
/// datapath is part of what a cycle changed. Cloning one is cloning what is in
/// flight — bounded by the stage count and the history cap, never by how long
/// the program has run.
#[derive(Clone)]
pub struct ScalarPipeline<P> {
    pub(super) shape: PipelineShape,
    pub(super) stages: Vec<Option<PipelineOp<P>>>,
    pub(super) enabled: bool,
    pub(super) halted: bool,
    pub(super) faulted: bool,
    pub(super) fetch_pc: u64,
    pub(super) stats: PipelineStats,
    pub(super) traces: Vec<Trace>,
    pub(super) timeline: Timeline,
    pub(super) predictor: TwoBitPredictor,
    pub(super) status: Option<String>,
    /// Work dispatched out of execute, one bank per declared unit. Empty unless
    /// the shape asked for [`PipelineMode::FunctionalUnits`].
    pub(super) units: Vec<Vec<PipelineOp<P>>>,
    /// How many instructions each unit may hold. Starts at what the shape
    /// declares and can be raised at runtime, which is the experiment a
    /// settings screen exists for.
    pub(super) unit_capacity: Vec<usize>,
    next_row: u64,
    next_seq: u64,
}

impl<P> ScalarPipeline<P> {
    pub fn new(shape: PipelineShape, entry: u64) -> Self {
        let stages = shape.stage_count();
        let history = shape.history;
        let shift = shape.index_shift();
        let unit_capacity: Vec<usize> = shape
            .units
            .iter()
            .map(|unit| unit.capacity.max(1))
            .collect();
        let units = shape.units.len();
        Self {
            shape,
            stages: (0..stages).map(|_| None).collect(),
            enabled: false,
            halted: false,
            faulted: false,
            fetch_pc: entry,
            stats: PipelineStats::default(),
            traces: Vec::new(),
            timeline: Timeline::new(history),
            predictor: TwoBitPredictor::new(shift),
            status: None,
            units: (0..units).map(|_| Vec::new()).collect(),
            unit_capacity,
            next_row: 0,
            next_seq: 0,
        }
    }

    pub fn shape(&self) -> &PipelineShape {
        &self.shape
    }

    /// Change the declared shape while the machine runs — the knobs a host puts
    /// in front of a user. Stage *count* must not change; everything else may.
    pub fn shape_mut(&mut self) -> &mut PipelineShape {
        &mut self.shape
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn fetch_pc(&self) -> u64 {
        self.fetch_pc
    }

    /// Where a fresh engine would start fetching to carry this program on: the
    /// oldest instruction that has not executed yet.
    ///
    /// Nothing in the datapath has changed the machine — a backend executes an
    /// instruction in one piece when it retires — so everything in flight is
    /// timing state, and refetching from here loses no work. That is what makes
    /// swapping the execution model mid-run a thing that can be done at all.
    pub fn resume_pc(&self) -> u64 {
        self.stages
            .iter()
            .flatten()
            .chain(self.units.iter().flatten())
            .min_by_key(|op| op.seq)
            .map_or(self.fetch_pc, |op| op.address)
    }

    /// How many instructions a declared unit may hold at once.
    pub fn set_capacity(&mut self, unit: usize, instances: usize) -> bool {
        let Some(slot) = self.unit_capacity.get_mut(unit) else {
            return false;
        };
        *slot = instances.max(1);
        true
    }

    /// Change how the front end guesses, forgetting what the old policy learnt.
    pub fn set_predict(&mut self, predict: BranchPredict) -> bool {
        if self.shape.predict == predict {
            return false;
        }
        self.shape.predict = predict;
        self.predictor.clear();
        true
    }

    /// Change how execute schedules work.
    ///
    /// Only the in-order pair is reachable from here: this engine keeps program
    /// order by construction, and offering a user a scoreboard it cannot run
    /// would be a lie. [`PipelineEngine`](super::PipelineEngine) is what swaps
    /// between engines.
    pub fn set_mode(&mut self, mode: PipelineMode) -> bool {
        if mode.is_out_of_order() || self.shape.mode == mode {
            return false;
        }
        self.shape.mode = mode;
        // Work already dispatched belongs to the model that was running;
        // leaving it there would let it retire under rules it was never issued
        // under.
        self.drain_units();
        true
    }

    /// Send everything sitting in a unit back into the datapath, in order.
    fn drain_units(&mut self) {
        let mut waiting: Vec<PipelineOp<P>> =
            self.units.iter_mut().flat_map(std::mem::take).collect();
        waiting.sort_by_key(|op| op.seq);
        let landing = self.shape.execute_stage();
        for op in waiting {
            if self.stages[landing].is_none() {
                self.stages[landing] = Some(op);
            }
            // Anything that will not fit was speculative work the model change
            // invalidated; dropping it is the same as a flush.
        }
    }

    /// The instruction occupying `stage`, for a backend that has to look inside
    /// the datapath — reporting a fault against the instruction that caused it,
    /// or letting a debugger stop part-way down.
    pub fn stage_op(&self, stage: usize) -> Option<&PipelineOp<P>> {
        self.stages.get(stage)?.as_ref()
    }

    // ── The cycle ────────────────────────────────────────────────────────────

    /// Open a cycle and take whatever reached the end of the datapath.
    pub fn start_cycle(&mut self) -> Option<PipelineOp<P>> {
        self.stats.cycles = self.stats.cycles.saturating_add(1);
        self.traces.clear();
        self.stages.last_mut().and_then(Option::take)
    }

    /// Put a retiring instruction back because the machine could not finish it
    /// — waiting on input, most often. The cycle is spent; the work is not.
    pub fn retry(&mut self, op: PipelineOp<P>, reason: impl Into<String>) {
        if let Some(last) = self.stages.last_mut() {
            *last = Some(op);
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
    /// Returns the address to redirect to when the front end guessed wrong —
    /// pass it straight to [`advance`](Self::advance). A backend calls this for
    /// every branch, taken or not, so the predictor learns from both.
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
        self.stages.fill_with(|| None);
        self.status = Some("halted".into());
    }

    pub fn fault(&mut self, message: impl Into<String>) {
        self.faulted = true;
        self.stages.fill_with(|| None);
        self.status = Some(message.into());
    }

    /// Shift the datapath one cycle and return the address to fetch next, if
    /// the front end has room for one.
    pub fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        if self.halted || self.faulted {
            return None;
        }
        self.status = None;

        if let Some(target) = redirect {
            self.flush(target);
        }
        for op in self.stages.iter_mut().flatten() {
            op.hazard = None;
        }

        let decode = self.shape.decode_stage();
        let execute = self.shape.execute_stage();
        let parallel = self.shape.dispatches_to_units();
        let holding = if parallel {
            self.tick_unit_bank();
            self.dispatch_to_unit(execute, decode)
        } else {
            self.tick_shared_execute(execute, decode)
        };

        // Drain the back of the datapath first so a slot opens for the
        // instruction behind it in the same cycle. Work that cannot move on
        // holds its stage, and nothing may move into or past it.
        for destination in (decode + 2..self.stages.len()).rev() {
            if self.stages[destination].is_none() && holding != Some(destination - 1) {
                self.stages[destination] = self.stages[destination - 1].take();
            }
        }
        if parallel {
            self.promote_ready_unit(execute);
        }

        self.report_name_hazards(decode);
        match self.resolve_operands(decode) {
            Operands::Stalled { producer, source } => self.stall(decode, producer, source),
            Operands::Ready(forwards) => {
                for (producer, source) in forwards {
                    self.traces.push(Trace {
                        kind: PipelineTraceKind::Forward,
                        from: producer,
                        to: execute,
                        detail: format!("{source} from {}", self.shape.stages[producer].name),
                    });
                }
                if self.stages[decode + 1].is_none() {
                    self.stages[decode + 1] = self.stages[decode].take();
                }
                if self.stages[decode].is_none() {
                    self.stages[decode] = self.stages[0].take();
                }
            }
        }

        let empty = self.stages.iter().all(Option::is_none)
            && self.units.iter().all(Vec::is_empty);
        let may_fetch = self.stages[0].is_none() && (self.enabled || empty);
        self.record_timeline();
        may_fetch.then_some(self.fetch_pc)
    }

    /// Accept a decoded instruction into the first stage.
    ///
    /// This is where the front end speculates: a branch whose target the op
    /// carries redirects the fetch pointer here, long before execution knows
    /// whether it was right.
    pub fn fetched(&mut self, mut op: PipelineOp<P>) {
        op.row = self.next_row;
        self.next_row = self.next_row.saturating_add(1);
        op.seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        op.cycles_left = self.shape.timing.latency(op.class);
        op.speculative = self.shape.speculative_fetch
            && self.stages.iter().flatten().any(|older| older.branch);

        let sequential = op.sequential_address(self.shape.instruction_bytes);
        op.predicted_next = match (op.branch, op.branch_target) {
            (true, Some(target)) => {
                op.predicted_taken =
                    self.predictor
                        .direction(self.shape.predict, op.address, target);
                if op.predicted_taken { target } else { sequential }
            }
            _ => sequential,
        };
        self.fetch_pc = op.predicted_next;

        self.timeline.open(
            op.row,
            op.address,
            op.disassembly.clone(),
            op.class,
            op.atomic,
            self.stats.cycles,
            Cell {
                label: self.shape.stages[0].name,
                state: if op.speculative {
                    PipelineTimelineState::Speculative
                } else {
                    PipelineTimelineState::Active
                },
                role: self.shape.stages[0].role,
            },
        );
        self.stages[0] = Some(op);
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    pub(super) fn reset_to(&mut self, address: u64, keep_history: bool) {
        self.stages.fill_with(|| None);
        for bank in &mut self.units {
            bank.clear();
        }
        self.halted = false;
        self.faulted = false;
        self.fetch_pc = address;
        self.status = None;
        self.traces.clear();
        if !keep_history {
            self.stats = PipelineStats::default();
            self.timeline.clear();
            self.next_row = 0;
            self.next_seq = 0;
            self.predictor.clear();
        }
    }

    // ── Internals ────────────────────────────────────────────────────────────

    fn flush(&mut self, target: u64) {
        let flushed: Vec<u64> = self
            .stages
            .iter()
            .flatten()
            .chain(self.units.iter().flatten())
            .map(|op| op.row)
            .collect();
        for row in &flushed {
            self.timeline.push(
                *row,
                Cell {
                    label: "FL",
                    state: PipelineTimelineState::Flushed,
                    role: PipelineStageRole::Other,
                },
            );
        }
        self.stages.fill_with(|| None);
        for bank in &mut self.units {
            bank.clear();
        }
        self.fetch_pc = target;
        // What was actually thrown away, not what the declared resolve stage
        // predicts: a flush that catches a half-full front end costs less.
        let depth = flushed.len() as u64;
        self.stats.flushes = self.stats.flushes.saturating_add(1);
        self.stats.stalls = self.stats.stalls.saturating_add(depth);
        self.stats.branch_stalls = self.stats.branch_stalls.saturating_add(depth);
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(PipelineHazardKind::BranchFlush),
            from: self.stages.len().saturating_sub(1),
            to: 0,
            detail: format!("redirect to 0x{target:X}"),
        });
    }

    /// Spend one cycle of a multi-cycle instruction that occupies the execute
    /// stage itself. Returns the stage it still holds.
    ///
    /// This is the serialized model: long-running work blocks the datapath
    /// behind it, because there is nowhere else for it to be.
    fn tick_shared_execute(&mut self, execute: usize, decode: usize) -> Option<usize> {
        let busy = self.stages[execute]
            .as_ref()
            .is_some_and(|op| op.cycles_left > 1);
        if !busy {
            return None;
        }
        let detail = if let Some(op) = self.stages[execute].as_mut() {
            op.cycles_left -= 1;
            op.hazard = Some(PipelineHazardKind::FunctionalUnitBusy);
            format!("{} needs {} more cycles", op.disassembly, op.cycles_left)
        } else {
            String::new()
        };
        self.unit_busy_stall(execute, decode, detail);
        Some(execute)
    }

    /// Spend one cycle in every busy unit.
    fn tick_unit_bank(&mut self) {
        for op in self.units.iter_mut().flatten() {
            op.cycles_left = op.cycles_left.saturating_sub(1);
        }
    }

    /// Hand the instruction in execute to a unit that can take it.
    ///
    /// Once it is in a unit the stage is free, which is the whole point: a
    /// divide and an add can be in flight at once. Returns the stage held when
    /// every unit for that class is full.
    ///
    /// Dispatch itself costs nothing. The cycle the instruction spent in
    /// execute counts against its latency, so single-cycle work leaves in the
    /// same cycle it would have in a serialized datapath — turning the unit
    /// bank on can only overlap work, never slow it down.
    fn dispatch_to_unit(&mut self, execute: usize, decode: usize) -> Option<usize> {
        let Some(op) = self.stages[execute].as_ref() else {
            return None;
        };
        // A class no unit claims stays in the stage and flows on as usual.
        let Some(unit) = self.shape.unit_for(op.class) else {
            return None;
        };
        if self.units[unit].len() >= self.unit_capacity[unit] {
            let detail = format!("{} is full", self.shape.units[unit].name);
            if let Some(op) = self.stages[execute].as_mut() {
                op.hazard = Some(PipelineHazardKind::FunctionalUnitBusy);
            }
            self.unit_busy_stall(execute, decode, detail);
            return Some(execute);
        }
        let mut op = self.stages[execute].take().expect("checked just above");
        op.cycles_left = op.cycles_left.saturating_sub(1);
        self.units[unit].push(op);
        None
    }

    /// Move the oldest finished unit result back into the datapath.
    ///
    /// Only the oldest, and only when it is done: units finish out of order,
    /// but an in-order machine may not let a younger result overtake an older
    /// one. A long divide therefore holds up the adds that finished behind it,
    /// which is exactly the effect worth showing.
    fn promote_ready_unit(&mut self, execute: usize) {
        let landing = (execute + 1).min(self.stages.len().saturating_sub(1));
        if self.stages[landing].is_some() {
            return;
        }
        let oldest = self
            .units
            .iter()
            .enumerate()
            .filter_map(|(unit, bank)| {
                let (slot, op) = bank
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, op)| op.seq)?;
                Some((op.seq, unit, slot, op.cycles_left))
            })
            .min_by_key(|(seq, ..)| *seq);
        let Some((_, unit, slot, cycles_left)) = oldest else {
            return;
        };
        if cycles_left > 0 {
            return;
        }
        let mut op = self.units[unit].remove(slot);
        op.hazard = None;
        self.stages[landing] = Some(op);
    }

    fn unit_busy_stall(&mut self, execute: usize, decode: usize, detail: String) {
        self.stats.stalls = self.stats.stalls.saturating_add(1);
        self.stats.functional_unit_stalls = self.stats.functional_unit_stalls.saturating_add(1);
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(PipelineHazardKind::FunctionalUnitBusy),
            from: execute,
            to: decode,
            detail,
        });
    }

    fn stall(&mut self, decode: usize, producer: usize, source: String) {
        let kind = self.stages[producer]
            .as_ref()
            .filter(|op| op.class == PipelineInstructionClass::Load)
            .filter(|_| self.shape.bypass.forwards_operands())
            .map_or(PipelineHazardKind::ReadAfterWrite, |_| {
                PipelineHazardKind::LoadUse
            });
        self.stats.stalls = self.stats.stalls.saturating_add(1);
        match kind {
            PipelineHazardKind::LoadUse => {
                self.stats.load_use_stalls = self.stats.load_use_stalls.saturating_add(1);
            }
            _ => self.stats.raw_stalls = self.stats.raw_stalls.saturating_add(1),
        }
        if let Some(decoded) = self.stages[decode].as_mut() {
            decoded.hazard = Some(kind);
        }
        let detail = match kind {
            PipelineHazardKind::LoadUse => format!("{source} is still being loaded"),
            _ => format!("waiting for {source}"),
        };
        self.traces.push(Trace {
            kind: PipelineTraceKind::Hazard(kind),
            from: producer,
            to: decode,
            detail,
        });
    }

    /// Which of the decoded instruction's operands a bypass covers, and the one
    /// no bypass can.
    ///
    /// By the time this runs the older instructions have already shifted, so a
    /// producer sitting at stage `p` has *finished* every stage before it. An
    /// operand is therefore forwardable when the producer is already past the
    /// stage that computes its result — and when the datapath has no bypasses
    /// at all, nothing is: the consumer waits for the producer to leave.
    fn resolve_operands(&self, decode: usize) -> Operands {
        let Some(decoded) = self.stages[decode].as_ref() else {
            return Operands::Ready(Vec::new());
        };
        let forwarding = self.shape.bypass.forwards_operands();
        let mut forwards = Vec::new();
        for source in &decoded.sources {
            // Stages closer to decode hold younger instructions, so the first
            // writer found is the one this operand must see.
            let producer = (decode + 1..self.stages.len()).find(|&stage| {
                self.stages[stage]
                    .as_ref()
                    .is_some_and(|older| older.writes.iter().any(|written| written == source))
            });
            let Some(producer) = producer else { continue };
            let class = self.stages[producer]
                .as_ref()
                .map_or(PipelineInstructionClass::Unknown, |older| older.class);
            if forwarding && producer > self.shape.result_stage(class) {
                forwards.push((producer, source.clone()));
            } else {
                return Operands::Stalled {
                    producer,
                    source: source.clone(),
                };
            }
        }
        Operands::Ready(forwards)
    }

    /// Note where two in-flight instructions write the same register.
    ///
    /// An in-order datapath retires them in order, so this never stalls — but
    /// it is the hazard an out-of-order design has to rename away, and seeing it
    /// named here is the point.
    fn report_name_hazards(&mut self, decode: usize) {
        let Some(decoded) = self.stages[decode].as_ref() else {
            return;
        };
        let mut found = Vec::new();
        for stage in decode + 1..self.stages.len() {
            let Some(older) = self.stages[stage].as_ref() else {
                continue;
            };
            for register in &decoded.writes {
                if older.writes.contains(register) {
                    found.push((PipelineHazardKind::WriteAfterWrite, stage, register.clone()));
                } else if older.sources.contains(register) {
                    found.push((PipelineHazardKind::WriteAfterRead, stage, register.clone()));
                }
            }
        }
        for (kind, producer, register) in found {
            self.traces.push(Trace {
                kind: PipelineTraceKind::Hazard(kind),
                from: producer,
                to: decode,
                detail: format!("{register} in flight"),
            });
        }
    }

    fn record_timeline(&mut self) {
        let state = |op: &PipelineOp<P>| {
            if op.hazard.is_some() {
                PipelineTimelineState::Stalled
            } else if op.speculative {
                PipelineTimelineState::Speculative
            } else {
                PipelineTimelineState::Active
            }
        };
        let stages = self.stages.iter().enumerate().filter_map(|(index, op)| {
            let op = op.as_ref()?;
            let spec = self.shape.stages[index];
            Some((
                op.row,
                Cell {
                    label: spec.name,
                    state: state(op),
                    role: spec.role,
                },
            ))
        });
        // Work in a unit is executing, so it reads as an execute cell named
        // for the unit rather than for the stage it came out of.
        let units = self.units.iter().enumerate().flat_map(|(unit, bank)| {
            let name = self.shape.units[unit].name;
            bank.iter().map(move |op| {
                (
                    op.row,
                    Cell {
                        label: name,
                        state: state(op),
                        role: PipelineStageRole::Execute,
                    },
                )
            })
        });
        let cells: Vec<(u64, Cell)> = stages.chain(units).collect();
        for (row, cell) in cells {
            self.timeline.push(row, cell);
        }
    }
}

enum Operands {
    Ready(Vec<(usize, String)>),
    Stalled { producer: usize, source: String },
}
