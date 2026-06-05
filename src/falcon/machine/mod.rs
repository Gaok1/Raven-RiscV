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
pub use journal::StepbackKind;
use types::{EditError, FRegId, MemWidth, RegTarget};

/// A pipeline whose per-cycle microarchitectural state the [`Machine`] owns and
/// journals, so step-back reverts the pipeline together with the CPU and memory.
///
/// The pipeline simulator is a UI-layer type ([`crate::ui::pipeline::PipelineSimState`]),
/// so `falcon` cannot name it directly. This trait is the seam: `Machine` is
/// generic over the pipeline, snapshots its reversible state into every
/// change-set, and restores it on undo — without `falcon` depending on `ui`.
///
/// `Snapshot` holds *only* the reversible execution state (stage latches,
/// functional-unit occupancy, fetch PC, hazard counters, …) — never the UI's
/// view/config state. Because the snapshot is taken by the same journaling
/// methods that snapshot the CPU, the pipeline can never silently drift out of
/// the undo history: that is the whole point of folding it into `Machine`.
pub trait JournaledPipeline {
    /// The reversible slice of the pipeline state. `Clone` is not required —
    /// each snapshot is produced fresh by [`Self::exec_snapshot`] and moved into
    /// the journal, then moved back out on undo.
    type Snapshot;
    /// Capture the reversible execution state as of *now* (before a tick).
    fn exec_snapshot(&self) -> Self::Snapshot;
    /// Restore the reversible execution state from a snapshot (on step-back).
    fn restore_exec(&mut self, snapshot: Self::Snapshot);
}

/// The "no pipeline" instantiation: a [`Machine`] that only ever single-steps
/// the interpreter or takes edits. Its snapshot is `()`, so the journal carries
/// zero extra bytes and the pipeline restore is a no-op.
#[derive(Default)]
pub struct NoPipeline;

impl JournaledPipeline for NoPipeline {
    type Snapshot = ();
    fn exec_snapshot(&self) {}
    fn restore_exec(&mut self, _: ()) {}
}

/// Default journal depth: how many steps/edits a user can rewind.
const JOURNAL_CAPACITY: usize = 1024;

/// Owns the CPU, the memory hierarchy, the pipeline, and the step journal, and
/// is the sole gateway for mutating them. See the module docs for the rationale.
///
/// `P` is the pipeline ([`JournaledPipeline`]); it defaults to [`NoPipeline`] so
/// the interpreter-only and test paths pay nothing for it.
pub struct Machine<P: JournaledPipeline = NoPipeline> {
    cpu: Cpu,
    mem: CacheController,
    /// The pipeline simulator. Private like `cpu`/`mem`: execution may only
    /// advance it through [`Machine::step_pipeline`], which journals the cycle.
    /// UI/config mutation goes through [`Machine::pipeline_mut`] (unjournaled).
    pipeline: P,
    journal: StepJournal<P::Snapshot>,
    /// Reused across steps so the interpreter's hot-branch profile persists.
    interp: InterpreterBackend,
    /// Logical timeline. Advances by one per recorded change; the value is the
    /// stack key of the most recent [`ChangeSet`].
    clock: u64,
}

impl<P: JournaledPipeline> Machine<P> {
    pub fn new(cpu: Cpu, mem: CacheController, pipeline: P) -> Self {
        Self {
            cpu,
            mem,
            pipeline,
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

    /// Shared read access to the pipeline (rendering, hit-testing, stats).
    pub fn pipeline(&self) -> &P {
        &self.pipeline
    }

    /// Mutable pipeline access for **UI/config** changes only — hover flags,
    /// scroll, subtab, gantt cosmetics, forwarding/branch config, resets. This
    /// does **not** journal and deliberately does **not** clear the journal:
    /// these mutations happen between steps and must not erase undo history.
    ///
    /// It must never be used to *advance execution* (tick the stages): that is
    /// [`Machine::step_pipeline`]'s job, which captures the cycle. Resetting the
    /// pipeline stages for a fresh run should be paired with
    /// [`Machine::clear_journal`], since the old history no longer applies.
    pub fn pipeline_mut(&mut self) -> &mut P {
        &mut self.pipeline
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
        self.record(cpu_before, StepbackKind::Step, Rewind::Delta { cache_before, ram_log });
        outcome
    }

    /// Point the shared MMU at the CPU's current `satp` / privilege before a
    /// sequential step — a background hart may have left it elsewhere.
    ///
    /// Unlike [`Machine::mem_mut_unjournaled`] this keeps the journal: it writes
    /// only MMU metadata (no RAM), the values are a pure function of the CPU
    /// `satp`/priv that a step-back restores, and the immediately-following
    /// [`Machine::step_interpreted`] snapshots the synced MMU into its
    /// change-set. So step-back stays exact without the sync erasing history.
    pub fn sync_mmu(&mut self) {
        let mmu = self.mem.mmu_mut();
        mmu.satp = crate::falcon::mmu::Satp::new(self.cpu.satp);
        mmu.priv_mode = self.cpu.priv_mode;
    }

    /// Apply one instruction's worth of cycle/stats accounting after a
    /// journaled [`Machine::step_interpreted`].
    ///
    /// This mutates the cache subsystem but deliberately does **not** touch the
    /// journal: the change-set the step just pushed snapshotted the cache
    /// *before* the instruction, so a [`Machine::stepback`] reverts this
    /// accounting along with the instruction. Keep it paired with
    /// `step_interpreted` (the GO/JIT path accounts cycles through
    /// [`Machine::mem_mut_unjournaled`] instead).
    pub fn account_step_cycles(&mut self, cpi_cycles: u64) {
        self.mem.add_instruction_cycles(cpi_cycles);
        self.mem.snapshot_stats();
    }

    /// Advance the pipeline by **one clock cycle**, journaling the whole cycle
    /// so step-back reverts it exactly. This is the *only* way execution can
    /// touch the pipeline stages, so a tick can never escape the undo history.
    ///
    /// The closure receives the machine's owned pipeline, CPU, and memory and
    /// runs the cycle (typically `pipeline_tick`); its return value is passed
    /// back to the caller. Before the tick this snapshots the CPU, the
    /// pipeline's reversible state, and the cache subsystem, and records the
    /// per-byte RAM pre-images the cycle writes — exactly like a single
    /// interpreter step, plus the pipeline latches.
    ///
    /// The MMU is re-synced to the CPU's `satp`/privilege first (a background
    /// hart may have left it elsewhere), through the journal-preserving
    /// [`Machine::sync_mmu`] rather than an unjournaled hatch — that earlier
    /// erased the history on every cycle and was why pipeline step-back never
    /// worked.
    pub fn step_pipeline<R>(
        &mut self,
        tick: impl FnOnce(&mut P, &mut Cpu, &mut CacheController) -> R,
    ) -> R {
        self.sync_mmu();
        let cpu_before = self.cpu.clone();
        let pipe_before = self.pipeline.exec_snapshot();
        let cache_before = self.mem.snapshot_state();
        self.mem.ram_mut().begin_recording();

        let result = tick(&mut self.pipeline, &mut self.cpu, &mut self.mem);

        let ram_log = self.mem.ram_mut().take_recording();
        self.record_with_pipe(
            cpu_before,
            pipe_before,
            StepbackKind::Step,
            Rewind::Delta { cache_before, ram_log },
        );
        result
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
        self.record(cpu_before, StepbackKind::Edit, Rewind::CpuOnly);
        Ok(())
    }

    /// Write a float register from raw IEEE-754 bits.
    pub fn write_freg(&mut self, freg: FRegId, bits: u32) {
        let cpu_before = self.cpu.clone();
        self.cpu.fwrite_bits(freg.index(), bits);
        self.record(cpu_before, StepbackKind::Edit, Rewind::CpuOnly);
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
        self.record(cpu_before, StepbackKind::Edit, Rewind::Delta { cache_before, ram_log });
        result
    }

    // ── Step-back ──

    /// True when there is at least one change to undo.
    pub fn can_stepback(&self) -> bool {
        !self.journal.is_empty()
    }

    /// Undo the most recent journaled change (one step, edit, or back to the
    /// last checkpoint). Returns the [`StepbackKind`] of what was undone, or
    /// `None` when the journal is empty.
    pub fn stepback(&mut self) -> Option<StepbackKind> {
        let change = self.journal.pop()?;
        let kind = change.kind;
        self.cpu = change.cpu_before;
        self.pipeline.restore_exec(change.pipe_before);
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
        Some(kind)
    }

    /// Push a full-state checkpoint. Used at the boundary of a GO/JIT burst or
    /// pipeline run, whose writes bypass the per-byte log — step-back can only
    /// rewind to here, not into the burst.
    pub fn checkpoint(&mut self) {
        let cpu_before = self.cpu.clone();
        let cache_before = self.mem.snapshot_state();
        let ram_before = self.mem.ram().as_bytes().to_vec();
        self.record(cpu_before, StepbackKind::Checkpoint, Rewind::Full { cache_before, ram_before });
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

    /// Mutable CPU **and** memory at once, without journaling. Callers that need
    /// both disjoint borrows simultaneously — the execution `ExecCtx`, the
    /// pipeline tick, and the per-step MMU sync — cannot stack two separate
    /// `&mut self` accessors, so this hands out the pair in one borrow. Same
    /// journal semantics as [`Machine::mem_mut_unjournaled`].
    pub fn cpu_mem_mut_unjournaled(&mut self) -> (&mut Cpu, &mut CacheController) {
        if !self.journal.top_is_full_checkpoint() {
            self.journal.clear();
            self.clock = 0;
        }
        (&mut self.cpu, &mut self.mem)
    }

    // ── Internal ──

    /// Advance the clock and push one change-set, snapshotting the pipeline
    /// *now*. Used by every path that does not itself advance the pipeline
    /// (single-step interpreter, edits, checkpoint) — there the current pipeline
    /// state *is* the before-state.
    fn record(&mut self, cpu_before: Cpu, kind: StepbackKind, rewind: Rewind) {
        let pipe_before = self.pipeline.exec_snapshot();
        self.record_with_pipe(cpu_before, pipe_before, kind, rewind);
    }

    /// Advance the clock and push one change-set with an explicitly-captured
    /// pipeline snapshot. [`Machine::step_pipeline`] uses this because the tick
    /// mutates the pipeline, so the before-state must be captured up front.
    fn record_with_pipe(
        &mut self,
        cpu_before: Cpu,
        pipe_before: P::Snapshot,
        kind: StepbackKind,
        rewind: Rewind,
    ) {
        self.clock += 1;
        self.journal.push(ChangeSet {
            clock: self.clock,
            kind,
            cpu_before,
            pipe_before,
            rewind,
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/support/falcon_machine.rs"]
mod tests;
