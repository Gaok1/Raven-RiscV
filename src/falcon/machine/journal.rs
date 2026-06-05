//! The step journal: a bounded stack of reversible state deltas.
//!
//! The clock is the simulator's timeline, so the journal is a stack indexed by
//! clock value. Each entry is a [`ChangeSet`] — the minimum needed to undo one
//! step or edit. [`Machine::stepback`](super::Machine::stepback) pops the top
//! and applies its [`Rewind`]; this module only stores the data, keeping the
//! reversal logic next to the state it touches.

use std::collections::VecDeque;

use crate::falcon::cache::CacheSnapshot;
use crate::falcon::registers::Cpu;

/// How to undo the memory side of a journaled change. The CPU is always
/// restored from [`ChangeSet::cpu_before`]; this says what to do about RAM and
/// the cache subsystem on top of that.
pub(super) enum Rewind {
    /// Nothing but registers/PC changed (a `write_reg` / `write_pc` /
    /// `write_freg` edit). Restoring the CPU clone is the whole undo.
    CpuOnly,
    /// A normal step or memory edit: restore the cache subsystem wholesale and
    /// replay the RAM pre-images in reverse to undo the byte writes.
    Delta {
        cache_before: CacheSnapshot,
        /// `(addr, old_byte)` pairs in write order; replay back-to-front.
        ram_log: Vec<(u32, u8)>,
    },
    /// A boundary checkpoint (entry to a GO/JIT burst that writes RAM directly,
    /// bypassing the per-byte log). The only safe undo is the full RAM image
    /// plus the cache subsystem.
    Full {
        cache_before: CacheSnapshot,
        ram_before: Vec<u8>,
    },
}

/// What a journaled change represents, surfaced by
/// [`Machine::stepback`](super::Machine::stepback) so the caller can refresh the
/// bookkeeping that matches it — a single instruction pops the exec-trace and
/// decrements its run count, whereas an interactive edit or a GO-burst
/// checkpoint touches neither.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepbackKind {
    /// One interpreted instruction (sequential single-step).
    Step,
    /// A manual register / float / memory edit.
    Edit,
    /// A GO/JIT burst boundary checkpoint.
    Checkpoint,
}

/// One reversible unit of history: the CPU as it was, the pipeline snapshot as
/// it was, plus how to undo the memory effects.
///
/// `S` is the pipeline's reversible snapshot type ([`super::JournaledPipeline::Snapshot`]).
/// For a machine without a pipeline it is `()`, so this collapses to the old
/// CPU+memory change-set at zero cost.
pub(super) struct ChangeSet<S> {
    /// Clock value at which this change was recorded (stack key, for display).
    pub clock: u64,
    /// What kind of change this was, for the caller's post-undo refresh.
    pub kind: StepbackKind,
    pub cpu_before: Cpu,
    /// The pipeline's reversible state as it was *before* this change. Captured
    /// by the same journaling methods that snapshot the CPU, so the pipeline can
    /// never drift out of the undo history — see [`super::Machine`].
    pub pipe_before: S,
    pub rewind: Rewind,
}

/// A bounded stack of [`ChangeSet`]s. Pushing past the capacity evicts the
/// oldest entry, so memory stays bounded while the most recent history (the
/// part a user can step back through) is always retained.
pub(super) struct StepJournal<S> {
    stack: VecDeque<ChangeSet<S>>,
    cap: usize,
}

impl<S> StepJournal<S> {
    /// Create a journal holding at most `cap` change-sets (`cap >= 1`).
    pub fn new(cap: usize) -> Self {
        Self {
            stack: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Push the newest change-set, evicting the oldest if at capacity.
    pub fn push(&mut self, change: ChangeSet<S>) {
        if self.stack.len() == self.cap {
            self.stack.pop_front();
        }
        self.stack.push_back(change);
    }

    /// Pop the newest change-set (the next one a step-back undoes).
    pub fn pop(&mut self) -> Option<ChangeSet<S>> {
        self.stack.pop_back()
    }

    /// Clock value at the top of the stack, if any.
    pub fn top_clock(&self) -> Option<u64> {
        self.stack.back().map(|c| c.clock)
    }

    /// Whether the newest entry is a full-RAM checkpoint. Such an entry holds
    /// the entire RAM image, so it stays correct even after untracked writes —
    /// the signal `mem_mut_unjournaled` uses to decide it can keep the history.
    pub fn top_is_full_checkpoint(&self) -> bool {
        matches!(self.stack.back().map(|c| &c.rewind), Some(Rewind::Full { .. }))
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }
}
