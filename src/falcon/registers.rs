// falcon/registers.rs
#[derive(Default, Clone)]
pub struct Cpu {
    pub x: [u32; 32],   // x0..x31
    pub pc: u32,
    /// buffer emulado de entrada (STDIN)
    pub stdin: Vec<u8>,
    /// buffer emulado de saída (STDOUT)
    pub stdout: Vec<u8>,
    /// Exit status when program terminates via Linux `exit`/`exit_group`.
    pub exit_code: Option<u32>,
}

impl Cpu {
    #[inline] pub fn read(&self, r: u8) -> u32 { if r == 0 { 0 } else { self.x[r as usize] } }
    #[inline] pub fn write(&mut self, r: u8, v: u32) { if r != 0 { self.x[r as usize] = v; } }
}


