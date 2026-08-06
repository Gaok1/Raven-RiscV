//! Branch direction prediction, over plain addresses.
//!
//! Nothing here knows what a branch instruction looks like — a backend decides
//! that and asks for a direction. That keeps the one piece with interesting
//! state, the two-bit table, shared by every architecture.

use super::config::BranchPredict;

const TWO_BIT_TABLE_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TwoBitEntry {
    valid: bool,
    tag: u64,
    counter: u8,
}

impl Default for TwoBitEntry {
    fn default() -> Self {
        Self {
            valid: false,
            tag: 0,
            counter: 1,
        }
    }
}

/// A direct-mapped, tagged two-bit saturating counter table.
///
/// Tagged rather than plain-indexed so a teaching host can point at one entry
/// and say which branch owns it; an untagged table would silently blend two
/// branches that alias and make the counter's behaviour impossible to explain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwoBitPredictor {
    entries: [TwoBitEntry; TWO_BIT_TABLE_SIZE],
    /// Bits dropped from an address before indexing, so an ISA with four-byte
    /// instructions does not leave three quarters of the table unused.
    shift: u32,
}

impl Default for TwoBitPredictor {
    fn default() -> Self {
        Self::new(2)
    }
}

impl TwoBitPredictor {
    pub fn new(shift: u32) -> Self {
        Self {
            entries: [TwoBitEntry::default(); TWO_BIT_TABLE_SIZE],
            shift,
        }
    }

    pub fn clear(&mut self) {
        self.entries = [TwoBitEntry::default(); TWO_BIT_TABLE_SIZE];
    }

    fn index(&self, address: u64) -> usize {
        ((address >> self.shift) as usize) & (TWO_BIT_TABLE_SIZE - 1)
    }

    /// Whether the counter for `address` is in a taken state. An address the
    /// table has never seen predicts not-taken, which is what a cold front end
    /// does.
    pub fn predict(&self, address: u64) -> bool {
        let entry = self.entries[self.index(address)];
        entry.valid && entry.tag == address && entry.counter >= 2
    }

    pub fn update(&mut self, address: u64, taken: bool) {
        let index = self.index(address);
        let entry = &mut self.entries[index];
        if !entry.valid || entry.tag != address {
            *entry = TwoBitEntry {
                valid: true,
                tag: address,
                counter: 1,
            };
        }
        entry.counter = if taken {
            entry.counter.saturating_add(1).min(3)
        } else {
            entry.counter.saturating_sub(1)
        };
    }

    /// The direction this policy guesses for a branch at `address` heading to
    /// `target`.
    pub fn direction(&self, policy: BranchPredict, address: u64, target: u64) -> bool {
        match policy {
            BranchPredict::NotTaken => false,
            BranchPredict::Taken => true,
            // A backwards branch is almost always a loop closing.
            BranchPredict::Btfnt => target < address,
            BranchPredict::TwoBit => self.predict(address),
        }
    }
}
