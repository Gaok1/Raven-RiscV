//! Typed edit targets and the errors a manual edit can produce.
//!
//! These newtypes make illegal edits unrepresentable: a [`RegId`] is always a
//! valid `0..=31` index, a [`MemWidth`] always carries its byte count and
//! range, and [`EditError`] is the single failure currency every editing path
//! speaks. Parsing user text into a value lives in [`super::parse`].

use std::fmt;

/// An integer-register index, guaranteed to be in `0..=31`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegId(u8);

impl RegId {
    /// Build a `RegId`, returning `None` for indices outside `0..=31`.
    pub fn new(index: u8) -> Option<Self> {
        (index < 32).then_some(Self(index))
    }

    /// The raw `0..=31` index.
    pub fn index(self) -> u8 {
        self.0
    }

    /// `x0` is hard-wired to zero and may never be written.
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// A float-register index, guaranteed to be in `0..=31`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FRegId(u8);

impl FRegId {
    pub fn new(index: u8) -> Option<Self> {
        (index < 32).then_some(Self(index))
    }

    pub fn index(self) -> u8 {
        self.0
    }
}

/// Where a register write lands: a general-purpose register or the PC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegTarget {
    X(RegId),
    Pc,
}

/// The byte width of a memory cell, mirroring the Run tab's `mem_view_bytes`
/// setting. Carries the helpers parsing needs to bound a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemWidth {
    B1,
    B2,
    B4,
}

impl MemWidth {
    /// Map the `mem_view_bytes` setting (1 / 2 / 4) to a width; anything else
    /// falls back to a full word.
    pub fn from_view_bytes(bytes: u32) -> Self {
        match bytes {
            1 => MemWidth::B1,
            2 => MemWidth::B2,
            _ => MemWidth::B4,
        }
    }

    /// Number of bytes this width occupies (1, 2 or 4).
    pub fn bytes(self) -> u32 {
        match self {
            MemWidth::B1 => 1,
            MemWidth::B2 => 2,
            MemWidth::B4 => 4,
        }
    }

    /// Number of value bits (8, 16 or 32).
    pub fn bits(self) -> u32 {
        self.bytes() * 8
    }

    /// Largest unsigned value that fits (e.g. `0xFF` for `B1`).
    pub fn max_unsigned(self) -> u64 {
        (1u64 << self.bits()) - 1
    }

    /// Inclusive signed range that fits (e.g. `(-128, 127)` for `B1`).
    pub fn signed_range(self) -> (i64, i64) {
        let half = 1i64 << (self.bits() - 1);
        (-half, half - 1)
    }
}

/// Why a manual edit was rejected. Every variant renders to a one-line status
/// message via [`EditError::message`]; the editor stays open on rejection so
/// the user can fix the input rather than losing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// The text could not be parsed in the active format.
    ParseFailed { input: String },
    /// The value parsed but does not fit the target's width.
    OutOfRange { width: MemWidth, signed: bool },
    /// `x0` is hard-wired to zero and cannot be written.
    X0Immutable,
}

impl EditError {
    /// A short, human-readable explanation for the status line.
    pub fn message(&self) -> String {
        match self {
            EditError::ParseFailed { input } => {
                format!("cannot parse \"{input}\"")
            }
            EditError::OutOfRange { width, signed } => {
                let bytes = width.bytes();
                if *signed {
                    let (lo, hi) = width.signed_range();
                    format!("out of range for {bytes}-byte signed cell ({lo}..={hi})")
                } else {
                    format!(
                        "out of range for {bytes}-byte cell (max 0x{:X})",
                        width.max_unsigned()
                    )
                }
            }
            EditError::X0Immutable => "x0 is hard-wired to zero".to_string(),
        }
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}
