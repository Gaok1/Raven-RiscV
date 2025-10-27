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
}
