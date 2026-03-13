// falcon/registers.rs
#[derive(Default, Clone)]
pub struct Cpu {
    pub x: [u32; 32],   // x0..x31 (integer registers)
    pub f: [u32; 32],   // f0..f31 (float registers, stored as IEEE 754 bits)
    pub fcsr: u32,      // float control/status register (fflags only; FRM=RNE)
    pub pc: u32,
    /// buffer emulado de entrada (STDIN)
    pub stdin: Vec<u8>,
    /// buffer emulado de saída (STDOUT)
    pub stdout: Vec<u8>,
    /// Exit status when program terminates via Linux `exit`/`exit_group`.
    pub exit_code: Option<u32>,
    /// LR/SC reservation address (None = no active reservation).
    pub lr_reservation: Option<u32>,
    /// Set when execution paused at an `ebreak` instruction (not a fault).
    pub ebreak_hit: bool,
    /// Current program break (heap end). Set by the loader; advanced by SYS_BRK.
    pub heap_break: u32,
}

impl Cpu {
    #[inline] pub fn read(&self, r: u8) -> u32 { if r == 0 { 0 } else { self.x[r as usize] } }
    #[inline] pub fn write(&mut self, r: u8, v: u32) { if r != 0 { self.x[r as usize] = v; } }

    // Float register helpers (all registers are writable, unlike x0)
    #[inline] pub fn fread(&self, r: u8) -> f32 { f32::from_bits(self.f[r as usize]) }
    #[inline] pub fn fwrite(&mut self, r: u8, v: f32) { self.f[r as usize] = v.to_bits(); }
    #[inline] pub fn fread_bits(&self, r: u8) -> u32 { self.f[r as usize] }
    #[inline] pub fn fwrite_bits(&mut self, r: u8, v: u32) { self.f[r as usize] = v; }
}
