pub type RegisterAddress = u8;

#[derive(Debug, Clone, Copy, Default)]
pub struct Register {
    pub value: u32,
    pub address: RegisterAddress,
}

impl Register {
    pub fn new() -> Self {
        Self { value: 0, address: 0 }
    }

    pub fn valued(value: u32, address: RegisterAddress) -> Self {
        Self { value, address }
    }

    // Leitura
    pub fn read_byte(&self) -> u8 {
        self.value as u8
    }

    pub fn read_half(&self) -> u16 {
        self.value as u16
    }

    pub fn read_word(&self) -> u32 {
        self.value
    }

    pub fn read_float(&self) -> f32 {
        f32::from_bits(self.value)
    }

    // Escrita
    pub fn write_byte(&mut self, byte: u8) {
        self.value = (self.value & !0xFF) | (byte as u32);
    }

    pub fn write_half(&mut self, half: u16) {
        self.value = (self.value & !0xFFFF) | (half as u32);
    }

    pub fn write_word(&mut self, word: u32) {
        self.value = word;
    }

    pub fn write_float(&mut self, f: f32) {
        self.value = f.to_bits();
    }

    // Criação do conjunto de registradores RISC-V
    pub fn risc_v_set() -> [Register; 32] {
        let mut regs = [Register::default(); 32];
        for i in 0..32 {
            regs[i].address = i as u8;
        }
        regs
    }
}
