use std::collections::HashMap;

// Structure returned with code and data
pub struct Program {
    /// Assembled code (instructions) in little-endian format.
    pub text: Vec<u32>,
    /// Raw data bytes, also in little-endian format.
    pub data: Vec<u8>,
    /// Base address for data region.
    pub data_base: u32,
    /// Total size in bytes of the BSS segment (not stored in `data`).
    pub bss_size: u32,
    /// Visible comments (`#! text`) attached to instructions, keyed by instruction address.
    pub comments: HashMap<u32, String>,
    /// All label names at each instruction address (may be multiple labels on same addr).
    pub labels: HashMap<u32, Vec<String>>,
    /// Maps 0-based source line → first instruction address emitted from that line.
    pub line_addrs: HashMap<usize, u32>,
    /// Maps label name → 0-based source line where it is defined.
    pub label_to_line: HashMap<String, usize>,
    /// Block comments (`##! text`) shown above an instruction, keyed by instruction address.
    pub block_comments: HashMap<u32, String>,
}
