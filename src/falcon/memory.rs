const MEMORY_SIZE: usize = 4096;

#[derive(Clone)]
pub struct Memory {
    mem: Vec<u8>,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            mem: vec![0; MEMORY_SIZE],
        }
    }

    // Leitura e escrita de 8 bits (Byte)
    pub fn read_byte(&self, addr: usize) -> u8 {
        self.mem.get(addr).copied().unwrap_or(0)
    }

    pub fn write_byte(&mut self, addr: usize, value: u8) {
        if let Some(byte) = self.mem.get_mut(addr) {
            *byte = value;
        }
    }

    // Leitura e escrita de 16 bits (Halfword - RISC-V padrão)
    pub fn read_halfword(&self, addr: usize) -> u16 {
        if addr + 2 <= self.mem.len() {
            u16::from_le_bytes(self.mem[addr..addr + 2].try_into().unwrap())
        } else {
            0
        }
    }

    pub fn write_halfword(&mut self, addr: usize, value: u16) {
        if addr + 2 <= self.mem.len() {
            self.mem[addr..addr + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    // Leitura e escrita de 32 bits (Word - RISC-V padrão)
    pub fn read_word(&self, addr: usize) -> u32 {
        if addr + 4 <= self.mem.len() {
            u32::from_le_bytes(self.mem[addr..addr + 4].try_into().unwrap())
        } else {
            0
        }
    }

    pub fn write_word(&mut self, addr: usize, value: u32) {
        if addr + 4 <= self.mem.len() {
            self.mem[addr..addr + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    // Leitura e escrita de 32 bits ponto flutuante (f32)
    pub fn read_float(&self, addr: usize) -> f32 {
        if addr + 4 <= self.mem.len() {
            f32::from_le_bytes(self.mem[addr..addr + 4].try_into().unwrap())
        } else {
            0.0
        }
    }

    pub fn write_float(&mut self, addr: usize, value: f32) {
        if addr + 4 <= self.mem.len() {
            self.mem[addr..addr + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
}
