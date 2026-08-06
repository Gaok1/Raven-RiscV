//! One instruction in flight, described in terms no ISA owns.

use crate::capability::{PipelineHazardKind, PipelineInstructionClass, PipelineSlotView};

/// An instruction as the pipeline sees it.
///
/// A backend fills in what it knows from its own decoder — what the instruction
/// is called, what it reads and writes, whether it can redirect control — and
/// carries whatever it needs for execution in `payload`. Everything after the
/// public fields is the engine's bookkeeping.
#[derive(Clone, Debug)]
pub struct PipelineOp<P> {
    pub address: u64,
    /// Encoded length in bytes. Ignored when the shape declares a fixed stride.
    pub length: u64,
    pub disassembly: String,
    pub class: PipelineInstructionClass,
    /// The architectural name of the result register, for a host to display.
    pub destination: Option<String>,
    /// Registers read, by architectural name. Hazard detection matches these
    /// against `writes` of older instructions, so names only have to be
    /// consistent within one backend.
    pub sources: Vec<String>,
    /// Every register written, including implicit ones such as a flags word.
    pub writes: Vec<String>,
    /// Whether this instruction can change the next fetch address.
    pub branch: bool,
    /// Where a taken branch goes, when the encoding says so.
    ///
    /// Supplying it is what lets the front end speculate: with no target the
    /// engine can only predict not-taken, which is what a datapath without a
    /// branch-target buffer does anyway.
    pub branch_target: Option<u64>,
    /// Set when fetching or decoding failed; the engine faults on retirement.
    pub fault: Option<String>,
    /// Whatever the backend needs to execute this instruction later.
    pub payload: P,
    /// True for an instruction that must not appear atomically interruptible in
    /// a host's timeline — an LR/SC pair, a lock-prefixed access.
    pub atomic: bool,

    pub(super) row: u64,
    /// Program order. Units may finish out of order; retirement may not.
    pub(super) seq: u64,
    pub(super) hazard: Option<PipelineHazardKind>,
    pub(super) speculative: bool,
    pub(super) predicted_taken: bool,
    pub(super) cycles_left: u8,
    /// The address the front end went on to fetch after this instruction.
    pub(super) predicted_next: u64,
}

impl<P> PipelineOp<P> {
    pub fn new(
        address: u64,
        disassembly: impl Into<String>,
        class: PipelineInstructionClass,
        payload: P,
    ) -> Self {
        Self {
            address,
            length: 0,
            disassembly: disassembly.into(),
            class,
            destination: None,
            sources: Vec::new(),
            writes: Vec::new(),
            branch: false,
            branch_target: None,
            fault: None,
            payload,
            atomic: false,
            row: 0,
            seq: 0,
            hazard: None,
            speculative: false,
            predicted_taken: false,
            cycles_left: 1,
            predicted_next: 0,
        }
    }

    /// An op standing in for a fetch that could not happen.
    ///
    /// It flows down the pipeline like any other instruction and faults the
    /// machine when it retires, so a bad address shows up where the program
    /// reached it rather than where the front end ran ahead to.
    pub fn faulted(address: u64, message: impl Into<String>, payload: P) -> Self {
        let mut op = Self::new(
            address,
            "fetch fault",
            PipelineInstructionClass::Unknown,
            payload,
        );
        op.fault = Some(message.into());
        op
    }

    /// Name the register this instruction writes, recording it as both the
    /// displayed destination and a hazard source.
    pub fn writing(mut self, register: impl Into<String>) -> Self {
        let name = register.into();
        self.destination = Some(name.clone());
        self.writes.push(name);
        self
    }

    /// Record a register this instruction writes without displaying it — a
    /// flags word, a stack pointer an addressing mode updates.
    pub fn also_writes(mut self, register: impl Into<String>) -> Self {
        self.writes.push(register.into());
        self
    }

    pub fn reading(mut self, registers: impl IntoIterator<Item = String>) -> Self {
        self.sources.extend(registers);
        self
    }

    pub fn branching(mut self) -> Self {
        self.branch = true;
        self
    }

    /// Mark this as a branch and say where it goes when taken.
    pub fn branching_to(mut self, target: Option<u64>) -> Self {
        self.branch = true;
        self.branch_target = target;
        self
    }

    /// Where the front end fetched after this instruction — the address a
    /// backend compares against the one execution actually reached.
    pub fn predicted_next(&self) -> u64 {
        self.predicted_next
    }

    /// Whether the front end guessed this branch taken.
    pub fn was_predicted_taken(&self) -> bool {
        self.predicted_taken
    }

    pub fn with_length(mut self, length: u64) -> Self {
        self.length = length;
        self
    }

    pub fn atomic(mut self) -> Self {
        self.atomic = true;
        self
    }

    /// The address after this instruction, given the shape's fixed stride.
    pub(super) fn sequential_address(&self, stride: Option<u64>) -> u64 {
        let length = stride.unwrap_or(self.length).max(1);
        self.address.wrapping_add(length)
    }

    pub(super) fn view(&self) -> PipelineSlotView<'_> {
        PipelineSlotView {
            address: self.address,
            disassembly: &self.disassembly,
            class: self.class,
            destination: self.destination.as_deref(),
            sources: [
                self.sources.first().map(String::as_str),
                self.sources.get(1).map(String::as_str),
            ],
            bubble: false,
            speculative: self.speculative,
            predicted_taken: self.predicted_taken,
            hazard: self.hazard,
            atomic: self.atomic,
            cycles_remaining: self.cycles_left,
        }
    }
}
