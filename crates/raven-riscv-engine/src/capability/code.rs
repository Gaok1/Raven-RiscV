//! Turning machine code into text and back.
//!
//! An instruction listing is the one pane no host can build generically: it has
//! to know where the next instruction starts and what to call it. Fixed-width
//! ISAs get both from the descriptor, but a variable-width one does not, so the
//! backend answers per address.

use crate::Diagnostic;

/// One ISA-defined field in a decoded instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionField {
    pub name: &'static str,
    pub value: String,
}

/// Backend-neutral data for an instruction inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionInfo {
    pub mnemonic: String,
    pub class: &'static str,
    pub encoding: u64,
    pub encoding_bits: u8,
    pub fields: Vec<InstructionField>,
}

/// Reading and writing a single instruction.
pub trait InstructionCodec {
    /// How many bytes the instruction starting at `address` occupies.
    ///
    /// A host walks a listing by adding this, so a variable-width ISA stays
    /// correct without the host knowing it is variable-width. `bytes` is the
    /// memory at `address`, long enough to decide.
    fn instruction_width(&self, address: u64, bytes: &[u8]) -> usize;

    /// Render the instruction encoded at `address` as assembly text.
    ///
    /// `None` when the bytes do not decode — a host shows them as raw data
    /// rather than inventing a mnemonic.
    fn disassemble(&self, address: u64, bytes: &[u8]) -> Option<String>;

    /// Encode one line of assembly for `address`, for an inline edit.
    fn assemble(&self, address: u64, text: &str) -> Result<Vec<u8>, Diagnostic>;

    /// Decode fields for an instruction-details pane.
    fn inspect(&self, _address: u64, _bytes: &[u8]) -> Option<InstructionInfo> {
        None
    }
}
