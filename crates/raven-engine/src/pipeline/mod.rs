//! The pipeline package an architecture gets for declaring its datapath.
//!
//! Pipelining is not an ISA feature. Stage occupancy, operand forwarding,
//! hazard detection, branch prediction, flush accounting and the Gantt history
//! are the same mechanism whether the instructions are RV32, x86-64 or an eight
//! instruction teaching machine. What differs is only what an instruction *is*:
//! how wide it is, what it reads and writes, what class of work it does.
//!
//! So that is the whole of what a backend supplies:
//!
//! - a [`PipelineShape`] — the stages, their roles, which bypasses exist, where
//!   branches resolve, how long each class of work takes;
//! - a [`PipelineOp`] per instruction, from its own decoder;
//! - execution, when an op retires.
//!
//! [`ScalarPipeline`] supplies the rest, including the
//! [`PipelineInspect`](crate::capability::PipelineInspect) and
//! [`PipelineControl`](crate::capability::PipelineControl) implementations, so a
//! backend that declares a shape is immediately as inspectable as any other.
//!
//! ```ignore
//! static STAGES: [StageSpec; 5] = pipeline::RISC_FIVE_STAGE;
//!
//! let shape = PipelineShape::scalar(&STAGES, Some(4))
//!     .with_predict(BranchPredict::TwoBit)
//!     .with_timing(PipelineTiming::default());
//! let pipeline: ScalarPipeline<MyDecoded> = ScalarPipeline::new(shape, entry);
//! ```
//!
//! Everything past the default is one call on the shape rather than a second
//! engine: [`with_bypass`](PipelineShape::with_bypass) changes which operands
//! forward and therefore which dependencies stall,
//! [`with_timing`](PipelineShape::with_timing) makes classes of work take
//! different numbers of cycles, and
//! [`with_parallel_units`](PipelineShape::with_parallel_units) turns execute
//! into a bank of functional units that overlap — a divide and an add in flight
//! together, still retiring in order.
//!
//! Leaving program order behind does take a different engine, because the
//! question changes from "has the instruction ahead finished?" to "has anyone
//! produced what I read?". [`ooo`] holds those: a [`ScoreboardPipeline`] issues
//! in order and writes out of it. They take the same [`PipelineShape`] and the
//! same [`PipelineOp`]s, and a backend drives them with the same cycle, so a
//! declared datapath can be run under any of the models without the backend
//! knowing which one it is.

mod config;
mod inspect;
mod op;
mod predictor;
mod scalar;
pub mod ooo;
mod timeline;
mod tuning;

#[cfg(test)]
mod tests;

pub use config::{
    BranchPredict, BranchResolve, PipelineBypassConfig, PipelineMode, PipelineShape,
    PipelineTiming, RISC_FIVE_STAGE, StageSpec, UnitSpec,
};
pub use ooo::ScoreboardPipeline;
pub use op::PipelineOp;
pub use predictor::TwoBitPredictor;
pub use scalar::ScalarPipeline;
