// falcon/mmu/trace.rs — what the last translations did, step by step.
//
// `Mmu::translate` answers with a physical address and a stall; a viewer that
// wants to *show* a translation needs the rest — which TLB sets were probed,
// every PTE the walk read, the entry installed and the one it pushed out, and
// why a fault faulted. Each translation leaves one fixed-size record here. The
// ring is part of the `Mmu`, so the step journal's snapshot of the MMU rewinds
// it along with the TLB.

use super::tlb::TlbEntry;
use super::{AccessType, LEVELS, PrivMode};
use std::collections::VecDeque;

/// Records kept, oldest dropped first.
pub const TRACE_CAPACITY: usize = 64;

/// Why a translation faulted, in terms a reader can act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultReason {
    /// A PTE on the path has V=0: nothing is mapped there.
    NotValid,
    /// A PTE has W=1 with R=0, an encoding the spec reserves.
    Reserved,
    /// A megapage leaf whose frame is not 4 MiB-aligned.
    Misaligned,
    /// The last level held a pointer instead of a leaf.
    NoLeaf,
    /// The leaf lacks the R, W or X bit this access needs.
    NotPermitted,
    /// U-mode touched a page without the U bit.
    SupervisorPage,
    /// S-mode touched a page with the U bit.
    UserPage,
    /// A page table lies outside RAM.
    OutsideMemory,
}

impl FaultReason {
    pub fn describe(self, access: AccessType) -> &'static str {
        match self {
            FaultReason::NotValid => "V=0: nothing is mapped here",
            FaultReason::Reserved => "W=1 with R=0 is a reserved encoding",
            FaultReason::Misaligned => "megapage frame is not 4 MiB-aligned",
            FaultReason::NoLeaf => "the last level points to another table",
            FaultReason::NotPermitted => match access {
                AccessType::Fetch => "page is not executable (X=0)",
                AccessType::Load => "page is not readable (R=0)",
                AccessType::Store => "page is not writable (W=0)",
            },
            FaultReason::SupervisorPage => "U-mode cannot touch a page without U",
            FaultReason::UserPage => "S-mode cannot touch a page with U",
            FaultReason::OutsideMemory => "page table lies outside RAM",
        }
    }
}

/// One PTE the walk read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkStep {
    /// Sv32 level: 1 is the root (indexed by VPN[1]), 0 the leaf table.
    pub level: u8,
    /// The entry index within its table.
    pub index: u16,
    pub pte_addr: u32,
    pub pte: u32,
    /// Where the read was served: `Some(n)` for cache level n (1 is L1),
    /// `None` for memory, or when no cache was consulted.
    pub served_by: Option<u8>,
    /// What the read cost through the cache hierarchy.
    pub cycles: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceOutcome {
    Hit,
    Walked,
    Fault(FaultReason),
}

/// One translation, complete enough to replay on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranslationRecord {
    /// Counts translations since the trace was cleared, from 1.
    pub seq: u64,
    pub hart: u32,
    /// The instruction that asked: its own address for a fetch.
    pub pc: u32,
    pub access: AccessType,
    pub priv_mode: PrivMode,
    pub vaddr: u32,
    pub asid: u16,
    /// False when the TLB is switched off: nothing probed, nothing installed.
    pub tlb_used: bool,
    /// The sets probed: the 4 KiB set, then the 4 MiB one.
    pub probed_sets: [u16; 2],
    pub hit_slot: Option<u16>,
    /// A store hit a clean entry, so the walk ran again only to set D.
    pub dirty_rewalk: bool,
    pub steps: [WalkStep; LEVELS],
    pub step_count: u8,
    pub outcome: TraceOutcome,
    pub paddr: Option<u32>,
    pub megapage: bool,
    /// Everything the translation cost: the hit latency, or the walker's own
    /// cycles plus each PTE read through the caches.
    pub cycles: u32,
    pub installed_slot: Option<u16>,
    pub evicted: Option<TlbEntry>,
    pub set_accessed: bool,
    pub set_dirty: bool,
}

impl TranslationRecord {
    pub fn steps(&self) -> &[WalkStep] {
        &self.steps[..self.step_count as usize]
    }

    pub fn fault(&self) -> Option<FaultReason> {
        match self.outcome {
            TraceOutcome::Fault(reason) => Some(reason),
            _ => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct TranslationTrace {
    records: VecDeque<TranslationRecord>,
    seq: u64,
    hart: u32,
    pc: u32,
}

impl TranslationTrace {
    /// Oldest first.
    pub fn records(&self) -> impl DoubleEndedIterator<Item = &TranslationRecord> + '_ {
        self.records.iter()
    }

    pub fn last(&self) -> Option<&TranslationRecord> {
        self.records.back()
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut TranslationRecord> {
        self.records.back_mut()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.seq = 0;
    }

    /// Name the instruction whose accesses come next.
    pub fn note_instruction(&mut self, hart: u32, pc: u32) {
        self.hart = hart;
        self.pc = pc;
    }

    /// A fresh record for an access starting now, numbered and attributed.
    pub(super) fn start(
        &mut self,
        access: AccessType,
        priv_mode: PrivMode,
        vaddr: u32,
        asid: u16,
    ) -> TranslationRecord {
        self.seq += 1;
        TranslationRecord {
            seq: self.seq,
            hart: self.hart,
            pc: if matches!(access, AccessType::Fetch) {
                vaddr
            } else {
                self.pc
            },
            access,
            priv_mode,
            vaddr,
            asid,
            tlb_used: false,
            probed_sets: [0; 2],
            hit_slot: None,
            dirty_rewalk: false,
            steps: [WalkStep::default(); LEVELS],
            step_count: 0,
            outcome: TraceOutcome::Walked,
            paddr: None,
            megapage: false,
            cycles: 0,
            installed_slot: None,
            evicted: None,
            set_accessed: false,
            set_dirty: false,
        }
    }

    pub(super) fn push(&mut self, record: TranslationRecord) {
        if self.records.len() == TRACE_CAPACITY {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }
}
