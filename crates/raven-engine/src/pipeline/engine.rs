//! Which engine a declared mode gets, and what a user changing it costs.
//!
//! The three models in this package are three different machines, not three
//! settings of one: an in-order datapath shifts stages, a scoreboard tracks
//! which unit owes which register, a reorder buffer renames. They cannot be one
//! type without one of them carrying fields the others never read. What they
//! *can* share is the protocol — take what `start_cycle` hands back, execute
//! it, call `advance` — and that is what makes this wrapper possible: a backend
//! declares a [`PipelineShape`] and drives whichever engine the shape's
//! [`PipelineMode`] names, without knowing which one it got.
//!
//! # Changing the model while the machine runs
//!
//! Nothing in flight has changed the machine. A backend executes an instruction
//! in one piece when the engine hands it back, so an instruction still in a
//! stage, a unit, a station or the buffer has done nothing yet — the whole
//! datapath is timing state. A model change can therefore throw it away and
//! have the new engine refetch from the oldest instruction that had not
//! executed, and the program continues exactly where it was.
//!
//! What does not come across is the history. Cycles, stalls and the Gantt rows
//! describe the model that produced them; carrying them into another model
//! would leave a CPI that describes neither machine. So swapping engines starts
//! the counters over, while *reconfiguring* one — a bypass turned off, a second
//! divider, the two in-order modes — keeps them, because that is still the same
//! machine being adjusted.

use crate::capability::{
    PipelineControl, PipelineDynamicInspect, PipelineEdge, PipelineInspect, PipelineSettingView,
    PipelineStageView, PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTraceView, PipelineTuning, PipelineUnitView,
};

use super::config::{PipelineMode, PipelineShape};
use super::ooo::{ScoreboardPipeline, TomasuloPipeline};
use super::op::PipelineOp;
use super::scalar::ScalarPipeline;
use super::tuning::{MODE_ROW, step};

/// Run the same call against whichever engine is here.
macro_rules! dispatch {
    ($engine:expr, |$inner:ident| $call:expr) => {
        match $engine {
            PipelineEngine::InOrder($inner) => $call,
            PipelineEngine::Scoreboard($inner) => $call,
            PipelineEngine::Tomasulo($inner) => $call,
        }
    };
}

/// The engine a declared [`PipelineMode`] selects, driven like any one of them.
///
/// A backend holds one of these instead of a particular engine and gets all
/// four models: the two in-order ones from [`ScalarPipeline`], and the two
/// dynamically scheduled ones from [`ooo`](super::ooo). Every method a backend
/// calls has the same name and signature it has on each engine, so switching a
/// backend over is a change of type and nothing else.
#[derive(Clone)]
pub enum PipelineEngine<P> {
    InOrder(ScalarPipeline<P>),
    Scoreboard(ScoreboardPipeline<P>),
    Tomasulo(TomasuloPipeline<P>),
}

impl<P> PipelineEngine<P> {
    /// Build the engine the shape asks for.
    pub fn new(shape: PipelineShape, entry: u64) -> Self {
        match shape.mode {
            PipelineMode::Scoreboard => Self::Scoreboard(ScoreboardPipeline::new(shape, entry)),
            PipelineMode::TomasuloRob => Self::Tomasulo(TomasuloPipeline::new(shape, entry)),
            PipelineMode::SingleCycle | PipelineMode::FunctionalUnits => {
                Self::InOrder(ScalarPipeline::new(shape, entry))
            }
        }
    }

    pub fn shape(&self) -> &PipelineShape {
        dispatch!(self, |inner| inner.shape())
    }

    /// The model actually running, which is the shape's — each engine stamps
    /// its own mode on the shape it was given.
    pub fn mode(&self) -> PipelineMode {
        self.shape().mode
    }

    pub fn enabled(&self) -> bool {
        dispatch!(self, |inner| inner.enabled())
    }

    pub fn fetch_pc(&self) -> u64 {
        dispatch!(self, |inner| inner.fetch_pc())
    }

    /// The oldest instruction that has not executed — where another engine
    /// would pick this program up.
    pub fn resume_pc(&self) -> u64 {
        dispatch!(self, |inner| inner.resume_pc())
    }

    /// The structures a dynamically scheduled model runs on, when the model
    /// running has any.
    ///
    /// `None` for the in-order engine, and that is the honest answer rather
    /// than an empty set of tables: an in-order datapath's state is its stages,
    /// which [`PipelineInspect`] already describes.
    pub fn dynamic(&self) -> Option<&dyn PipelineDynamicInspect> {
        match self {
            Self::InOrder(_) => None,
            Self::Scoreboard(inner) => Some(inner as &dyn PipelineDynamicInspect),
            Self::Tomasulo(inner) => Some(inner as &dyn PipelineDynamicInspect),
        }
    }

    /// How many instructions a declared unit may hold. Every model has the
    /// question; only the name for the answer differs.
    pub fn set_capacity(&mut self, unit: usize, instances: usize) -> bool {
        dispatch!(self, |inner| inner.set_capacity(unit, instances))
    }

    /// Replace the latency table, for a backend whose timing a user edits
    /// somewhere other than this package's own settings screen.
    pub fn set_timing(&mut self, timing: super::config::PipelineTiming) {
        dispatch!(self, |inner| inner.shape_mut().timing = timing);
    }

    /// Change the execution model, swapping the engine when the new model is
    /// not this one's.
    ///
    /// The in-flight work is refetched rather than migrated and the counters
    /// start over; see the module documentation for why both are the only
    /// honest answers. What does carry across is the machine a user configured:
    /// the shape, and how many of each unit there are.
    pub fn set_mode(&mut self, mode: PipelineMode) -> bool {
        if self.mode() == mode {
            return false;
        }
        // Reconfiguring one engine keeps its numbers, the way turning a bypass
        // off does; only replacing it cannot.
        if let Self::InOrder(pipeline) = self
            && !mode.is_out_of_order()
        {
            return pipeline.set_mode(mode);
        }

        let mut shape = self.shape().clone();
        shape.mode = mode;
        let capacities: Vec<usize> = (0..self.unit_count())
            .filter_map(|unit| self.unit(unit))
            .map(|unit| unit.capacity)
            .collect();
        let status = self.status();
        let message = self.status_message().map(str::to_string);
        let entry = self.resume_pc();

        *self = Self::new(shape, entry);
        self.set_enabled(status.enabled);
        for (unit, capacity) in capacities.into_iter().enumerate() {
            self.set_capacity(unit, capacity);
        }
        // A machine that had stopped stays stopped: a new engine is not a
        // reason for a halted program to become runnable again.
        if status.faulted {
            self.fault(message.unwrap_or_else(|| "faulted".into()));
        } else if status.halted {
            self.halt();
        }
        true
    }

    // ── The cycle ────────────────────────────────────────────────────────────

    pub fn start_cycle(&mut self) -> Option<PipelineOp<P>> {
        dispatch!(self, |inner| inner.start_cycle())
    }

    pub fn retry(&mut self, op: PipelineOp<P>, reason: impl Into<String>) {
        dispatch!(self, |inner| inner.retry(op, reason));
    }

    pub fn retire(&mut self, op: &PipelineOp<P>) {
        dispatch!(self, |inner| inner.retire(op));
    }

    pub fn resolve(&mut self, op: &PipelineOp<P>, actual_next: u64) -> Option<u64> {
        dispatch!(self, |inner| inner.resolve(op, actual_next))
    }

    pub fn halt(&mut self) {
        dispatch!(self, |inner| inner.halt());
    }

    pub fn fault(&mut self, message: impl Into<String>) {
        dispatch!(self, |inner| inner.fault(message));
    }

    pub fn advance(&mut self, redirect: Option<u64>) -> Option<u64> {
        dispatch!(self, |inner| inner.advance(redirect))
    }

    pub fn fetched(&mut self, op: PipelineOp<P>) {
        dispatch!(self, |inner| inner.fetched(op));
    }
}

// ── What a host sees ─────────────────────────────────────────────────────────

impl<P> PipelineControl for PipelineEngine<P> {
    fn set_enabled(&mut self, enabled: bool) {
        dispatch!(self, |inner| inner.set_enabled(enabled));
    }

    fn reset(&mut self, address: u64) {
        dispatch!(self, |inner| inner.reset(address));
    }

    fn redirect(&mut self, address: u64) {
        dispatch!(self, |inner| inner.redirect(address));
    }
}

impl<P> PipelineInspect for PipelineEngine<P> {
    fn status(&self) -> PipelineStatus {
        dispatch!(self, |inner| inner.status())
    }

    fn stats(&self) -> PipelineStats {
        dispatch!(self, |inner| inner.stats())
    }

    fn stage_count(&self) -> usize {
        dispatch!(self, |inner| inner.stage_count())
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        dispatch!(self, |inner| inner.stage(index))
    }

    /// Forwarded rather than defaulted: the phases a dynamically scheduled
    /// model reports are its own, and so is the graph between them.
    fn edges(&self) -> Vec<PipelineEdge> {
        dispatch!(self, |inner| inner.edges())
    }

    fn unit_count(&self) -> usize {
        dispatch!(self, |inner| inner.unit_count())
    }

    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>> {
        dispatch!(self, |inner| inner.unit(index))
    }

    fn trace_count(&self) -> usize {
        dispatch!(self, |inner| inner.trace_count())
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        dispatch!(self, |inner| inner.trace(index))
    }

    fn status_message(&self) -> Option<&str> {
        dispatch!(self, |inner| inner.status_message())
    }

    fn timeline_len(&self) -> usize {
        dispatch!(self, |inner| inner.timeline_len())
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        dispatch!(self, |inner| inner.timeline_row(index))
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        dispatch!(self, |inner| inner.timeline_cell(row, cell))
    }
}

impl<P> PipelineTuning for PipelineEngine<P> {
    fn setting_count(&self) -> usize {
        dispatch!(self, |inner| inner.setting_count())
    }

    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>> {
        dispatch!(self, |inner| inner.setting(index))
    }

    /// Every row but the first belongs to the engine holding it. The first one
    /// names the model, and changing that is the one adjustment no engine can
    /// make to itself — which is the whole reason this type exists.
    fn adjust(&mut self, index: usize, forward: bool) -> bool {
        if index == MODE_ROW {
            return self.set_mode(step(self.mode(), &PipelineMode::all(), forward));
        }
        dispatch!(self, |inner| inner.adjust(index, forward))
    }

    fn set_number(&mut self, index: usize, value: u64) -> bool {
        dispatch!(self, |inner| inner.set_number(index, value))
    }
}
