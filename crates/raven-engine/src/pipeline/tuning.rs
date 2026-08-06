//! The declared shape, offered to a host as a settings screen.
//!
//! Every knob here is one a shape already has, so a backend that declares a
//! datapath gets an experiment bench with it: turn a bypass off and watch the
//! stalls appear, move the branch predictor and watch the flushes move, add a
//! divider and watch two divides overlap. That used to exist for exactly one
//! architecture.

use crate::capability::{PipelineSettingValue, PipelineSettingView, PipelineTuning};

use super::config::{BranchPredict, BranchResolve, PipelineMode};
use super::op::PipelineOp;
use super::scalar::ScalarPipeline;

const FORWARDING: &str = "FORWARDING";
const CONTROL: &str = "CONTROL";
const UNITS: &str = "FUNCTIONAL UNITS";
const LATENCY: &str = "CYCLES PER INSTRUCTION";

/// The rows that are the same for every shape. Unit capacities and latencies
/// follow, and there are as many of those as the shape declares.
const FIXED: usize = 7;

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

const CONTROL_SUMMARY: [&str; 3] = [
    "Serialized keeps one instruction in execute at a time. Parallel units \
     dispatch each class to its own unit, so a long divide and a short add are \
     in flight together — still retiring in program order.",
    "Where a branch's real direction becomes known. The later it resolves, the \
     more speculative work a wrong guess throws away.",
    "How the front end guesses a branch before it resolves. A wrong guess costs \
     a flush; the two-bit predictor learns from the branches it has already \
     seen.",
];

impl<P> ScalarPipeline<P> {
    fn bypass_flag(&mut self, index: usize) -> &mut bool {
        let bypass = &mut self.shape.bypass;
        match index {
            0 => &mut bypass.ex_to_ex,
            1 => &mut bypass.mem_to_ex,
            2 => &mut bypass.wb_to_id,
            _ => &mut bypass.store_to_load,
        }
    }

    /// Unit rows come after the fixed ones, then the latency table.
    fn unit_row(&self, index: usize) -> Option<usize> {
        index
            .checked_sub(FIXED)
            .filter(|unit| *unit < self.shape.units.len())
    }

    fn latency_row(&self, index: usize) -> Option<usize> {
        index
            .checked_sub(FIXED + self.shape.units.len())
            .filter(|field| *field < super::PipelineTiming::field_names().len())
    }
}

impl<P> PipelineTuning for ScalarPipeline<P> {
    fn setting_count(&self) -> usize {
        FIXED + self.shape.units.len() + super::PipelineTiming::field_names().len()
    }

    fn setting(&self, index: usize) -> Option<PipelineSettingView<'_>> {
        if index < 4 {
            let bypass = self.shape.bypass;
            let on = [
                bypass.ex_to_ex,
                bypass.mem_to_ex,
                bypass.wb_to_id,
                bypass.store_to_load,
            ][index];
            return Some(PipelineSettingView {
                group: FORWARDING,
                name: ["EX→EX", "MEM→EX", "WB→ID", "Store→Load"][index],
                value: PipelineSettingValue::Toggle(on),
                summary: BYPASS_SUMMARY[index],
            });
        }
        if index < FIXED {
            let control = index - 4;
            let value = match control {
                0 => self.shape.mode.label(),
                1 => self.shape.resolve.label(),
                _ => self.shape.predict.label(),
            };
            return Some(PipelineSettingView {
                group: CONTROL,
                name: ["Execution", "Branch resolve", "Branch predict"][control],
                value: PipelineSettingValue::Choice(value),
                summary: CONTROL_SUMMARY[control],
            });
        }
        if let Some(unit) = self.unit_row(index) {
            let spec = &self.shape.units[unit];
            return Some(PipelineSettingView {
                group: UNITS,
                name: spec.name,
                value: PipelineSettingValue::Number(spec.capacity.max(1) as u64),
                summary: "How many instructions of this class can be in flight \
                          at once. Beyond that a later one waits in execute \
                          until the unit frees up.",
            });
        }
        let field = self.latency_row(index)?;
        Some(PipelineSettingView {
            group: LATENCY,
            name: super::PipelineTiming::field_names()[field],
            value: PipelineSettingValue::Number(self.shape.timing.get(field)),
            summary: super::PipelineTiming::descriptions()[field],
        })
    }

    fn adjust(&mut self, index: usize, forward: bool) -> bool {
        if index < 4 {
            let flag = self.bypass_flag(index);
            *flag = !*flag;
            return true;
        }
        if index < FIXED {
            match index - 4 {
                0 => {
                    // Only the in-order modes: this is the in-order engine, and
                    // offering a user a scoreboard it cannot run would be a lie.
                    // The dynamic-scheduling models are their own engine.
                    self.shape.mode = step(
                        self.shape.mode,
                        &[PipelineMode::SingleCycle, PipelineMode::FunctionalUnits],
                        forward,
                    );
                    // Work already dispatched belongs to the model that was
                    // running; leaving it there would let it retire under rules
                    // it was never issued under.
                    self.drain_units();
                }
                1 => {
                    self.shape.resolve = step(
                        self.shape.resolve,
                        &[BranchResolve::Id, BranchResolve::Ex, BranchResolve::Mem],
                        forward,
                    );
                }
                _ => {
                    self.shape.predict = step(
                        self.shape.predict,
                        &[
                            BranchPredict::NotTaken,
                            BranchPredict::Taken,
                            BranchPredict::Btfnt,
                            BranchPredict::TwoBit,
                        ],
                        forward,
                    );
                    self.predictor.clear();
                }
            }
            return true;
        }
        let current = match self.setting(index) {
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
        self.set_number(index, next)
    }

    fn set_number(&mut self, index: usize, value: u64) -> bool {
        if let Some(unit) = self.unit_row(index) {
            // Capacity lives in the shape's static declaration, so a change has
            // to go somewhere the shape owns.
            self.unit_capacity[unit] = value.max(1) as usize;
            return true;
        }
        let Some(field) = self.latency_row(index) else {
            return false;
        };
        self.shape.timing.set(field, value);
        true
    }
}

/// The next (or previous) entry in a fixed cycle of policies.
fn step<T: Copy + PartialEq>(current: T, all: &[T], forward: bool) -> T {
    let at = all.iter().position(|value| *value == current).unwrap_or(0);
    let next = if forward {
        (at + 1) % all.len()
    } else {
        (at + all.len() - 1) % all.len()
    };
    all[next]
}

impl<P> ScalarPipeline<P> {
    /// Send everything sitting in a unit back into the datapath, in order.
    ///
    /// Used when the execution model changes underneath in-flight work.
    fn drain_units(&mut self) {
        let mut waiting: Vec<PipelineOp<P>> = self
            .units
            .iter_mut()
            .flat_map(std::mem::take)
            .collect();
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
}
