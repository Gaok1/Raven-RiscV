//! The declared shape, offered to a host as a settings screen.
//!
//! Every knob here is one a shape already has, so a backend that declares a
//! datapath gets an experiment bench with it: turn a bypass off and watch the
//! stalls appear, move the branch predictor and watch the flushes move, add a
//! divider and watch two divides overlap. That used to exist for exactly one
//! architecture.
//!
//! All three engines answer the same screen, and the first row is the same
//! question for each of them — which model is scheduling the instructions. What
//! follows it differs, because the models differ: a scoreboard has no bypass
//! network to toggle and no predictor to choose, and a reorder buffer has a
//! depth that nothing else has. A row a model cannot honour is a row it does
//! not offer.

use crate::capability::{
    PipelineInspect, PipelineSettingValue, PipelineSettingView, PipelineTuning,
};

use super::config::{BranchPredict, BranchResolve, PipelineMode, PipelineTiming};
use super::ooo::{ScoreboardPipeline, TomasuloPipeline};
use super::scalar::ScalarPipeline;

const CONTROL: &str = "CONTROL";
const FORWARDING: &str = "FORWARDING";
const UNITS: &str = "FUNCTIONAL UNITS";
const STATIONS: &str = "RESERVATION STATIONS";
const BUFFER: &str = "REORDER BUFFER";
const LATENCY: &str = "CYCLES PER INSTRUCTION";

/// Where the model row sits. The same index in every engine, so a host's cursor
/// is still on the model when the model change swaps the engine underneath it.
pub(super) const MODE_ROW: usize = 0;

/// Rows before the units in the in-order engine: the model, where branches
/// resolve, how they are guessed, and the four bypasses.
const SCALAR_FIXED: usize = 7;
/// A scoreboard offers the model and nothing else: it has no bypasses, and a
/// predictor would hide the cost the model exists to show.
const SCOREBOARD_FIXED: usize = 1;
/// The model, the predictor it does use, and how deep the buffer is.
const TOMASULO_FIXED: usize = 3;

const MODE_SUMMARY: &str = "Which model schedules instructions once they leave \
                            decode. The first two keep program order from end \
                            to end; the scoreboard and Tomasulo run work the \
                            moment its operands are ready. Those two are \
                            engines of their own, so switching to or from them \
                            refetches from the oldest instruction still in \
                            flight and starts the counters over.";

const RESOLVE_SUMMARY: &str = "Where a branch's real direction becomes known. \
                               The later it resolves, the more speculative work \
                               a wrong guess throws away.";

const PREDICT_SUMMARY: &str = "How the front end guesses a branch before it \
                               resolves. A wrong guess costs a flush; the \
                               two-bit predictor learns from the branches it \
                               has already seen.";

const CAPACITY_SUMMARY: &str = "How many instructions of this class can be in \
                                flight at once. Beyond that a later one waits \
                                until the unit frees up.";

const STATION_SUMMARY: &str = "How many instructions of this class can be \
                               waiting or running at once. A station and the \
                               hardware it feeds are one resource here, so this \
                               is both how many may queue and how many may run.";

const BUFFER_SUMMARY: &str = "How many instructions may be in flight at once. \
                              Shrinking it until issue starts backing up is how \
                              you find out what the buffer was buying.";

const BYPASS_SUMMARY: [&str; 4] = [
    "Forwards a result the moment execute computes it, straight back into the \
     next instruction's operands. Without it a dependent instruction waits for \
     the producer to write back.",
    "Forwards a value that has come out of the memory stage. This is the path a \
     load's result takes, and it is why a load-use pair costs one cycle instead \
     of three.",
    "Lets an instruction being decoded read a register the writeback stage is \
     committing in the same cycle, instead of seeing the old value.",
    "Forwards store data directly into a load of the same address, so the load \
     does not have to wait for the store to reach memory.",
];

// ── Rows every model shares ──────────────────────────────────────────────────

/// The model row, which every engine reports and only
/// [`PipelineEngine`](super::PipelineEngine) can change.
pub(super) fn mode_setting(mode: PipelineMode) -> PipelineSettingView<'static> {
    PipelineSettingView {
        group: CONTROL,
        name: "Execution",
        value: PipelineSettingValue::Choice(mode.label()),
        summary: MODE_SUMMARY,
    }
}

/// Unit rows come after an engine's own controls, then the latency table. Every
/// model has both, so the arithmetic is written once.
fn unit_row(index: usize, fixed: usize, units: usize) -> Option<usize> {
    index.checked_sub(fixed).filter(|unit| *unit < units)
}

fn latency_row(index: usize, fixed: usize, units: usize) -> Option<usize> {
    index
        .checked_sub(fixed + units)
        .filter(|field| *field < PipelineTiming::field_names().len())
}

fn row_count(fixed: usize, units: usize) -> usize {
    fixed + units + PipelineTiming::field_names().len()
}

fn capacity_setting<'a>(
    name: &'a str,
    capacity: usize,
    group: &'static str,
    summary: &'static str,
) -> PipelineSettingView<'a> {
    PipelineSettingView {
        group,
        name,
        value: PipelineSettingValue::Number(capacity.max(1) as u64),
        summary,
    }
}

fn latency_setting(timing: &PipelineTiming, field: usize) -> PipelineSettingView<'static> {
    PipelineSettingView {
        group: LATENCY,
        name: PipelineTiming::field_names()[field],
        value: PipelineSettingValue::Number(timing.get(field)),
        summary: PipelineTiming::descriptions()[field],
    }
}

/// Nudge a `Number` row by one, through whatever `set_number` the engine has.
/// Rows of any other kind are their engine's own business.
fn step_number(tuning: &mut impl PipelineTuning, index: usize, forward: bool) -> bool {
    let current = match tuning.setting(index) {
        Some(PipelineSettingView {
            value: PipelineSettingValue::Number(value),
            ..
        }) => value,
        _ => return false,
    };
    let next = if forward {
        current.saturating_add(1)
    } else {
        current.saturating_sub(1)
    };
    tuning.set_number(index, next)
}

/// The next (or previous) entry in a fixed cycle of policies.
pub(super) fn step<T: Copy + PartialEq>(current: T, all: &[T], forward: bool) -> T {
    let at = all.iter().position(|value| *value == current).unwrap_or(0);
    let next = if forward {
        (at + 1) % all.len()
    } else {
        (at + all.len() - 1) % all.len()
    };
    all[next]
}

// ── In order ─────────────────────────────────────────────────────────────────

impl<P> ScalarPipeline<P> {
    fn bypass_flag(&mut self, index: usize) -> &mut bool {
        let bypass = &mut self.shape.bypass;
        match index - 3 {
            0 => &mut bypass.ex_to_ex,
            1 => &mut bypass.mem_to_ex,
            2 => &mut bypass.wb_to_id,
            _ => &mut bypass.store_to_load,
        }
    }
}

impl<P> PipelineTuning for ScalarPipeline<P> {
    fn setting_count(&self) -> usize {
        row_count(SCALAR_FIXED, self.shape().units.len())
    }

    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>> {
        let shape = self.shape();
        if index < 3 {
            return Some(match index {
                MODE_ROW => mode_setting(shape.mode),
                1 => PipelineSettingView {
                    group: CONTROL,
                    name: "Branch resolve",
                    value: PipelineSettingValue::Choice(shape.resolve.label()),
                    summary: RESOLVE_SUMMARY,
                },
                _ => PipelineSettingView {
                    group: CONTROL,
                    name: "Branch predict",
                    value: PipelineSettingValue::Choice(shape.predict.label()),
                    summary: PREDICT_SUMMARY,
                },
            });
        }
        if index < SCALAR_FIXED {
            let bypass = shape.bypass;
            let path = index - 3;
            let on = [
                bypass.ex_to_ex,
                bypass.mem_to_ex,
                bypass.wb_to_id,
                bypass.store_to_load,
            ][path];
            return Some(PipelineSettingView {
                group: FORWARDING,
                name: ["EX→EX", "MEM→EX", "WB→ID", "Store→Load"][path],
                value: PipelineSettingValue::Toggle(on),
                summary: BYPASS_SUMMARY[path],
            });
        }
        let units = shape.units.len();
        if let Some(unit) = unit_row(index, SCALAR_FIXED, units) {
            let view = self.unit(unit)?;
            return Some(capacity_setting(
                view.name,
                view.capacity,
                UNITS,
                CAPACITY_SUMMARY,
            ));
        }
        let field = latency_row(index, SCALAR_FIXED, units)?;
        Some(latency_setting(&shape.timing, field))
    }

    fn adjust(&mut self, index: usize, forward: bool) -> bool {
        match index {
            // Only the in-order pair: this is the in-order engine, and the
            // dynamically scheduled models are engines of their own.
            MODE_ROW => self.set_mode(step(
                self.shape().mode,
                &[PipelineMode::SingleCycle, PipelineMode::FunctionalUnits],
                forward,
            )),
            1 => {
                let resolve = step(
                    self.shape().resolve,
                    &[BranchResolve::Id, BranchResolve::Ex, BranchResolve::Mem],
                    forward,
                );
                self.shape_mut().resolve = resolve;
                true
            }
            2 => {
                let predict = step(
                    self.shape().predict,
                    &[
                        BranchPredict::NotTaken,
                        BranchPredict::Taken,
                        BranchPredict::Btfnt,
                        BranchPredict::TwoBit,
                    ],
                    forward,
                );
                self.set_predict(predict)
            }
            _ if index < SCALAR_FIXED => {
                let flag = self.bypass_flag(index);
                *flag = !*flag;
                true
            }
            _ => step_number(self, index, forward),
        }
    }

    fn set_number(&mut self, index: usize, value: u64) -> bool {
        let units = self.shape().units.len();
        if let Some(unit) = unit_row(index, SCALAR_FIXED, units) {
            // Capacity lives in the shape's static declaration, so a change has
            // to go somewhere the shape owns.
            return self.set_capacity(unit, value.max(1) as usize);
        }
        let Some(field) = latency_row(index, SCALAR_FIXED, units) else {
            return false;
        };
        self.shape_mut().timing.set(field, value);
        true
    }
}

// ── Scoreboard ───────────────────────────────────────────────────────────────

impl<P> PipelineTuning for ScoreboardPipeline<P> {
    fn setting_count(&self) -> usize {
        row_count(SCOREBOARD_FIXED, self.unit_count())
    }

    /// The model, then one row per unit, then the latency table.
    ///
    /// No forwarding section and no predictor, and both absences are the model
    /// rather than an omission: a scoreboard has no bypass network, and it does
    /// not speculate, so a row for either would be a control that does nothing.
    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>> {
        if index == MODE_ROW {
            return Some(mode_setting(self.shape().mode));
        }
        let units = self.unit_count();
        if let Some(unit) = unit_row(index, SCOREBOARD_FIXED, units) {
            let view = self.unit(unit)?;
            return Some(capacity_setting(
                view.name,
                view.capacity,
                UNITS,
                CAPACITY_SUMMARY,
            ));
        }
        let field = latency_row(index, SCOREBOARD_FIXED, units)?;
        Some(latency_setting(&self.shape().timing, field))
    }

    fn adjust(&mut self, index: usize, forward: bool) -> bool {
        // Becoming another model means becoming another engine, which is
        // something only whoever owns this one can do.
        if index == MODE_ROW {
            return false;
        }
        step_number(self, index, forward)
    }

    fn set_number(&mut self, index: usize, value: u64) -> bool {
        let units = self.unit_count();
        if let Some(unit) = unit_row(index, SCOREBOARD_FIXED, units) {
            return self.set_capacity(unit, value.max(1) as usize);
        }
        let Some(field) = latency_row(index, SCOREBOARD_FIXED, units) else {
            return false;
        };
        self.shape_mut().timing.set(field, value);
        true
    }
}

// ── Tomasulo ─────────────────────────────────────────────────────────────────

impl<P> PipelineTuning for TomasuloPipeline<P> {
    fn setting_count(&self) -> usize {
        row_count(TOMASULO_FIXED, self.unit_count())
    }

    /// The model, the predictor, the buffer depth, the stations, the latencies.
    ///
    /// No forwarding section here either, for the opposite reason: the common
    /// data bus is not optional wiring, it is what the model *is*. Nor a branch
    /// resolve row — a mispredict is discovered at commit and nowhere else.
    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>> {
        match index {
            MODE_ROW => return Some(mode_setting(self.shape().mode)),
            1 => {
                return Some(PipelineSettingView {
                    group: CONTROL,
                    name: "Branch predict",
                    value: PipelineSettingValue::Choice(self.shape().predict.label()),
                    summary: PREDICT_SUMMARY,
                });
            }
            2 => {
                return Some(PipelineSettingView {
                    group: BUFFER,
                    name: "Entries",
                    value: PipelineSettingValue::Number(self.buffer_size() as u64),
                    summary: BUFFER_SUMMARY,
                });
            }
            _ => {}
        }
        let units = self.unit_count();
        if let Some(unit) = unit_row(index, TOMASULO_FIXED, units) {
            let view = self.unit(unit)?;
            return Some(capacity_setting(
                view.name,
                view.capacity,
                STATIONS,
                STATION_SUMMARY,
            ));
        }
        let field = latency_row(index, TOMASULO_FIXED, units)?;
        Some(latency_setting(&self.shape().timing, field))
    }

    fn adjust(&mut self, index: usize, forward: bool) -> bool {
        match index {
            MODE_ROW => false,
            1 => {
                let predict = step(
                    self.shape().predict,
                    &[
                        BranchPredict::NotTaken,
                        BranchPredict::Taken,
                        BranchPredict::Btfnt,
                        BranchPredict::TwoBit,
                    ],
                    forward,
                );
                self.set_predict(predict)
            }
            _ => step_number(self, index, forward),
        }
    }

    fn set_number(&mut self, index: usize, value: u64) -> bool {
        if index == 2 {
            self.set_buffer_size(value.max(1) as usize);
            return true;
        }
        let units = self.unit_count();
        if let Some(unit) = unit_row(index, TOMASULO_FIXED, units) {
            return self.set_capacity(unit, value.max(1) as usize);
        }
        let Some(field) = latency_row(index, TOMASULO_FIXED, units) else {
            return false;
        };
        self.shape_mut().timing.set(field, value);
        true
    }
}
