//! `Machine` — the single sanctioned owner of mutable simulator state.
//!
//! The Run tab needs to *undo* execution (step-back) and *edit* live state
//! (registers, floats, RAM, the PC) without ever leaving the step journal in a
//! lying state. The danger with the old `pub(crate) cpu` / `pub(crate) mem`
//! fields was silence: any new code could mutate them ad-hoc and simply forget
//! to record the change, turning step-back into a source of subtle bugs.
//!
//! `Machine` closes that hole at the type boundary. The `cpu` and `mem` fields
//! are **private**; reads go through the cheap `&`-borrowing accessors
//! ([`Machine::cpu`] / [`Machine::mem`]); and the *only* ways to mutate state
//! are the journaling methods below. The single silent path —
//! [`Machine::cpu_mut_unjournaled`] / [`Machine::mem_mut_unjournaled`] — names
//! itself loudly and clears the journal from the inside, so it cannot quietly
//! corrupt the undo history either. Forgetting to journal is no longer
//! expressible.
//!
//! ## How a change is captured
//!
//! Every runtime RAM write funnels through `Ram::store8`, so the journal
//! records byte-level pre-images there (see [`crate::falcon::memory::Ram`])
//! rather than wrapping the bus — this is correct even under a write-back cache,
//! where a store may touch only a cache line and an eviction writes RAM at an
//! unrelated address. The cache subsystem (everything but the large RAM) is
//! captured by a cheap clone ([`CacheSnapshot`]). Together they make one step
//! exactly reversible.
//!
//! ## Honest limits
//!
//! Console output already shown is not un-printed (the internal buffer is
//! restored, the terminal is not). Step-back is per the selected hart. GO/JIT
//! bursts and background harts write RAM directly and are only reversible to
//! their last [`Machine::checkpoint`]. These are documented on the methods.

#![allow(dead_code)] // Phase 1: standalone module; wired into RunState in Phase 2.

pub mod journal;
pub mod parse;
pub mod types;

use crate::falcon::cache::CacheController;
use crate::falcon::errors::FalconError;
use crate::falcon::jit::{ExecCtx, ExecOutcome, ExecutionBackend, InterpreterBackend};
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

use journal::{ChangeSet, Rewind, StepJournal};
use types::{EditError, FRegId, MemWidth, RegTarget};

/// Default journal depth: how many steps/edits a user can rewind.
const JOURNAL_CAPACITY: usize = 1024;

/// Owns the CPU, the memory hierarchy, and the step journal, and is the sole
/// gateway for mutating them. See the module docs for the design rationale.
pub struct Machine {
    cpu: Cpu,
    mem: CacheController,
    journal: StepJournal,
    /// Reused across steps so the interpreter's hot-branch profile persists.
    interp: InterpreterBackend,
    /// Logical timeline. Advances by one per recorded change; the value is the
    /// stack key of the most recent [`ChangeSet`].
    clock: u64,
}

impl Machine {
    pub fn new(cpu: Cpu, mem: CacheController) -> Self {
        Self {
            cpu,
            mem,
            journal: StepJournal::new(JOURNAL_CAPACITY),
            interp: InterpreterBackend::new(),
            clock: 0,
        }
    }

    // ── Reads (the ~260 existing sites borrow through these) ──

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn mem(&self) -> &CacheController {
        &self.mem
    }

    // ── Execution ──

    /// Execute one instruction through the interpreter, journaling the change
    /// so it can be stepped back. Returns the interpreter's [`ExecOutcome`].
    ///
    /// The change-set is recorded even when the step returns an error, so a
    /// partially-applied faulting instruction is still reversible.
    pub fn step_interpreted(
        &mut self,
        console: &mut Console,
    ) -> Result<ExecOutcome, FalconError> {
        let cpu_before = self.cpu.clone();
        let cache_before = self.mem.snapshot_state();
        self.mem.ram_mut().begin_recording();

        let outcome = {
            let mut ctx = ExecCtx::new(&mut self.cpu, &mut self.mem, console);
            self.interp.run_until_yield(&mut ctx)
        };

        let ram_log = self.mem.ram_mut().take_recording();
        self.record(cpu_before, Rewind::Delta { cache_before, ram_log });
        outcome
    }

    // ── Sanctioned edits (each journaled, each undoable by `stepback`) ──

    /// Write an integer register or the PC. Writing `x0` is rejected
    /// ([`EditError::X0Immutable`]); the caller's editor stays open.
    pub fn write_reg(&mut self, target: RegTarget, value: u32) -> Result<(), EditError> {
        if let RegTarget::X(reg) = target
            && reg.is_zero()
        {
            return Err(EditError::X0Immutable);
        }
        let cpu_before = self.cpu.clone();
        match target {
            RegTarget::X(reg) => self.cpu.write(reg.index(), value),
            RegTarget::Pc => self.cpu.pc = value,
        }
        self.record(cpu_before, Rewind::CpuOnly);
        Ok(())
    }

    /// Write a float register from raw IEEE-754 bits.
    pub fn write_freg(&mut self, freg: FRegId, bits: u32) {
        let cpu_before = self.cpu.clone();
        self.cpu.fwrite_bits(freg.index(), bits);
        self.record(cpu_before, Rewind::CpuOnly);
    }

    /// Write a `width`-byte cell at `addr` through the normal cache-aware store
    /// path, so subsequent loads see the edit. `value` is the little-endian
    /// payload (already range-checked by [`parse::parse_cell`]).
    pub fn write_mem(
        &mut self,
        addr: u32,
        width: MemWidth,
        value: u64,
    ) -> Result<(), FalconError> {
        let cpu_before = self.cpu.clone();
        let cache_before = self.mem.snapshot_state();
        self.mem.ram_mut().begin_recording();

        let result = match width {
            MemWidth::B1 => self.mem.store8(addr, value as u8),
            MemWidth::B2 => self.mem.store16(addr, value as u16),
            MemWidth::B4 => self.mem.store32(addr, value as u32),
        };

        let ram_log = self.mem.ram_mut().take_recording();
        self.record(cpu_before, Rewind::Delta { cache_before, ram_log });
        result
    }

    // ── Step-back ──

    /// True when there is at least one change to undo.
    pub fn can_stepback(&self) -> bool {
        !self.journal.is_empty()
    }

    /// Undo the most recent journaled change (one step, edit, or back to the
    /// last checkpoint). Returns `false` when the journal is empty.
    pub fn stepback(&mut self) -> bool {
        let Some(change) = self.journal.pop() else {
            return false;
        };
        self.cpu = change.cpu_before;
        match change.rewind {
            Rewind::CpuOnly => {}
            Rewind::Delta { cache_before, ram_log } => {
                self.mem.restore_state(cache_before);
                // Replay pre-images back-to-front so overlapping writes land on
                // their oldest value.
                let ram = self.mem.ram_mut();
                for &(addr, old) in ram_log.iter().rev() {
                    ram.poke8(addr, old);
                }
            }
            Rewind::Full { cache_before, ram_before } => {
                self.mem.restore_state(cache_before);
                self.mem.ram_mut().copy_from_slice(&ram_before, ram_before.len());
            }
        }
        self.clock = self.journal.top_clock().unwrap_or(0);
        true
    }

    /// Push a full-state checkpoint. Used at the boundary of a GO/JIT burst or
    /// pipeline run, whose writes bypass the per-byte log — step-back can only
    /// rewind to here, not into the burst.
    pub fn checkpoint(&mut self) {
        let cpu_before = self.cpu.clone();
        let cache_before = self.mem.snapshot_state();
        let ram_before = self.mem.ram().as_bytes().to_vec();
        self.record(cpu_before, Rewind::Full { cache_before, ram_before });
    }

    /// Drop all history (reset / program reload / mode switch).
    pub fn clear_journal(&mut self) {
        self.journal.clear();
        self.clock = 0;
    }

    /// Number of change-sets currently retained (≤ [`JOURNAL_CAPACITY`]).
    pub fn journal_depth(&self) -> usize {
        self.journal.len()
    }

    // ── Escape hatches: the name announces "this skips the journal" ──

    /// Mutable CPU access that does **not** journal. For reset / loader /
    /// multi-core sync only — all audited. Does not clear existing history, so
    /// use only where the timeline is being rebuilt anyway.
    pub fn cpu_mut_unjournaled(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// Mutable memory access that does **not** journal. For reset / loader /
    /// GO-JIT / pipeline / MMU-sync only.
    ///
    /// An unrecorded RAM write would invalidate the byte-level pre-images of
    /// every [`Rewind::Delta`] entry, so the journal must usually be dropped
    /// here. The one exception is a [`Rewind::Full`] checkpoint on top: it holds
    /// the whole RAM image and so stays a correct rewind target across the
    /// untracked writes that follow. That is exactly the GO/JIT pattern —
    /// [`Machine::checkpoint`] then `mem_mut_unjournaled` for the burst — and it
    /// keeps step-back-to-the-boundary working.
    pub fn mem_mut_unjournaled(&mut self) -> &mut CacheController {
        if !self.journal.top_is_full_checkpoint() {
            self.journal.clear();
            self.clock = 0;
        }
        &mut self.mem
    }

    // ── Internal ──

    /// Advance the clock and push one change-set.
    fn record(&mut self, cpu_before: Cpu, rewind: Rewind) {
        self.clock += 1;
        self.journal.push(ChangeSet {
            clock: self.clock,
            cpu_before,
            rewind,
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/support/falcon_machine.rs"]
mod tests;
