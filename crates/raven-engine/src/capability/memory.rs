//! Inspecting guest memory.
//!
//! Separate from [`Machine::read_memory`](crate::Machine::read_memory) on
//! purpose: that one is the *program's* view and may fault, warm a cache, or
//! move a statistic. This one is the *observer's* view, safe to call while
//! drawing a frame.

use crate::MachineError;

/// Somewhere in memory worth offering as a jump target in an address pane.
///
/// Backends name whatever they actually have. A machine with no notion of a
/// heap simply does not report one, and hosts show one button fewer.
///
/// `address` must be readable — a host jumps straight to it, so a region that
/// points past the end of memory shows an empty pane. For a stack that grows
/// downward this means the *last byte the stack occupies*, not the stack
/// pointer: those differ by one when the stack is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    pub name: &'static str,
    pub address: u64,
}

impl MemoryRegion {
    pub fn new(name: &'static str, address: u64) -> Self {
        Self { name, address }
    }
}

/// Reading and writing guest memory for display.
pub trait MemoryInspect {
    /// Total addressable bytes in this machine.
    fn size(&self) -> u64;

    /// Read up to `bytes` for display, starting at `address`.
    ///
    /// Never fails and never disturbs the machine: a read past the end of
    /// memory returns what it could, so a host can scroll to the last address
    /// without special-casing the edge.
    fn peek(&self, address: u64, bytes: usize) -> Vec<u8>;

    /// Write bytes on a user's behalf — an edited memory cell.
    fn poke(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError>;

    /// Named addresses a host can offer as shortcuts.
    fn regions(&self) -> Vec<MemoryRegion> {
        Vec::new()
    }

    /// Read exactly `N` bytes as a little-endian unsigned integer, zero-padding
    /// past the end of memory. Convenience for hex dumps and word views.
    fn peek_word(&self, address: u64, bytes: usize) -> u64 {
        self.peek(address, bytes.min(8))
            .iter()
            .enumerate()
            .fold(0u64, |word, (i, byte)| word | (u64::from(*byte) << (i * 8)))
    }
}
