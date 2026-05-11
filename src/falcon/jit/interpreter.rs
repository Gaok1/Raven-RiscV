use crate::falcon::errors::FalconError;
use crate::falcon::memory::Bus;

use super::backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
use super::profile::HotProfile;

/// Reference backend: thin wrapper around `falcon::exec::step`.
/// `exec.rs` is deliberately not edited — this preserves every byte of
/// interpreter semantics from before the refactor.
pub struct InterpreterBackend {
    profile: HotProfile,
}

impl InterpreterBackend {
    pub fn new() -> Self {
        Self {
            profile: HotProfile::new(),
        }
    }

    /// Direct accessor for the hot profile. Mirrors the trait method but
    /// avoids the `ExecutionBackend<B>` type-inference ambiguity when callers
    /// hold a concrete `InterpreterBackend`.
    pub fn profile(&self) -> &HotProfile {
        &self.profile
    }
}

impl Default for InterpreterBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bus> ExecutionBackend<B> for InterpreterBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::None
    }

    fn run_until_yield(&mut self, ctx: &mut ExecCtx<'_, B>) -> Result<ExecOutcome, FalconError> {
        let pc_before = ctx.cpu.pc;
        let alive = crate::falcon::exec::step(ctx.cpu, ctx.mem, ctx.console)?;

        if alive && ctx.cpu.pc != pc_before.wrapping_add(4) {
            self.profile.record_target(ctx.cpu.pc);
        }

        if !alive {
            return Ok(if ctx.console.reading {
                ExecOutcome::AwaitingInput
            } else {
                ExecOutcome::Halted
            });
        }
        Ok(ExecOutcome::Stepped { instructions: 1 })
    }

    fn hot_profile(&self) -> Option<&HotProfile> {
        Some(&self.profile)
    }
}
