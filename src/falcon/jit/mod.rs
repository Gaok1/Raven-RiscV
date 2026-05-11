//! JIT compiler scaffolding.
//!
//! Phase A: only modularization — the `InterpreterBackend` wraps the existing
//! `falcon::exec::step` so every execution call site goes through the
//! `ExecutionBackend` trait. Phase B will add a real codegen backend behind
//! the `jit` cargo feature.
//!
//! Pipeline mode (`ui::pipeline::sim::pipeline_tick`) is intentionally **not**
//! routed through this trait: pipeline mode is its own staged dispatcher, used
//! for metric-fidelity simulation, not a candidate for codegen.

pub mod backend;
pub mod block;
pub mod cache;
pub mod factory;
pub mod interpreter;
pub mod profile;

#[cfg(feature = "jit")]
pub mod codegen;

pub use backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
pub use factory::make_backend;
pub use interpreter::InterpreterBackend;
pub use profile::HotProfile;

#[cfg(test)]
#[path = "../../../tests/support/falcon_jit.rs"]
mod tests;
