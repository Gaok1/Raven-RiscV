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

/// What a run of bits is *for*.
///
/// The backend names the role; the host picks the colour. That split is what
/// lets one field-map renderer serve every ISA without a palette per backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BitRole {
    Opcode,
    /// The register written.
    Destination,
    /// A register read.
    Source,
    Immediate,
    /// Opcode-refining bits — RISC-V's `funct3`/`funct7`, and equivalents.
    Function,
    #[default]
    Other,
}

/// One contiguous run of bits in an instruction encoding.
///
/// A host draws these left to right to build a field map, so they are listed
/// most-significant first and their widths sum to the instruction's width. An
/// immediate scattered across the word — RISC-V's B and J formats — appears as
/// several segments that share [`BitRole::Immediate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionBitField {
    /// What to print over the bits: `"funct7"`, `"imm[10:5]"`, `"opcode"`.
    pub label: String,
    pub width: u8,
    pub role: BitRole,
}

impl InstructionBitField {
    pub fn new(label: impl Into<String>, width: u8, role: BitRole) -> Self {
        Self {
            label: label.into(),
            width,
            role,
        }
    }
}

/// Backend-neutral data for an instruction inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionInfo {
    pub mnemonic: String,
    pub class: &'static str,
    pub encoding: u64,
    pub encoding_bits: u8,
    pub fields: Vec<InstructionField>,
    /// The encoding's bit layout, most-significant segment first. Empty when
    /// the backend does not describe one; a host then draws the fields without
    /// a bit map rather than guessing where they sit.
    pub layout: Vec<InstructionBitField>,
}

impl InstructionInfo {
    /// True when [`Self::layout`] accounts for exactly the instruction's bits,
    /// which is what a field-map renderer requires before drawing.
    pub fn layout_is_complete(&self) -> bool {
        !self.layout.is_empty()
            && self
                .layout
                .iter()
                .map(|segment| u32::from(segment.width))
                .sum::<u32>()
                == u32::from(self.encoding_bits)
    }
}

/// Reading and writing a single instruction.
pub trait InstructionCodec {
    /// The most bytes one instruction of this ISA can occupy.
    ///
    /// A host reads this many bytes before calling anything below, because
    /// "long enough to decide" is a fact about the ISA and the host has no way
    /// to know it. Hosts used to assume eight, which silently truncated
    /// x86-64's ten-byte `movabs` and walked its listings one byte at a time.
    fn max_instruction_bytes(&self) -> usize {
        8
    }

    /// How many bytes the instruction starting at `address` occupies.
    ///
    /// A host walks a listing by adding this, so a variable-width ISA stays
    /// correct without the host knowing it is variable-width. `bytes` is the
    /// memory at `address`, at least [`Self::max_instruction_bytes`] long
    /// where memory allows.
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
