// falcon/memory.rs
use crate::falcon::errors::FalconError;

pub trait Bus {
    fn load8(&self, addr: u32) -> Result<u8, FalconError>;
    fn load16(&self, addr: u32) -> Result<u16, FalconError>;
    fn load32(&self, addr: u32) -> Result<u32, FalconError>;
    fn store8(&mut self, addr: u32, val: u8) -> Result<(), FalconError>;
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError>;
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError>;

    /// Instruction fetch — override to route through I-cache.
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        self.load32(addr)
    }

    /// D-cache tracked data reads — override to route through D-cache.
    fn dcache_read8(&mut self, addr: u32) -> Result<u8, FalconError> {
        self.load8(addr)
    }
    fn dcache_read16(&mut self, addr: u32) -> Result<u16, FalconError> {
        self.load16(addr)
    }
    fn dcache_read32(&mut self, addr: u32) -> Result<u32, FalconError> {
        self.load32(addr)
    }
}

pub struct Ram { data: Vec<u8> }

impl Ram {
    pub fn new(size: usize) -> Self { Self { data: vec![0; size] } }
}

impl Bus for Ram {
    fn load8(&self, a: u32) -> Result<u8, FalconError> {
        self.data.get(a as usize).copied().ok_or(FalconError::Bus("address out of bounds"))
    }
    fn load16(&self, a: u32) -> Result<u16, FalconError> {
        Ok(u16::from_le_bytes([self.load8(a)?, self.load8(a + 1)?]))
    }
    fn load32(&self, a: u32) -> Result<u32, FalconError> {
        Ok(u32::from_le_bytes([
            self.load8(a)?,
            self.load8(a + 1)?,
            self.load8(a + 2)?,
            self.load8(a + 3)?,
        ]))
    }
    fn store8(&mut self, a: u32, v: u8) -> Result<(), FalconError> {
        if let Some(slot) = self.data.get_mut(a as usize) {
            *slot = v;
            Ok(())
        } else {
            Err(FalconError::Bus("address out of bounds"))
        }
    }
    fn store16(&mut self, a: u32, v: u16) -> Result<(), FalconError> {
        let b = v.to_le_bytes();
        self.store8(a, b[0])?;
        self.store8(a + 1, b[1])
    }
    fn store32(&mut self, a: u32, v: u32) -> Result<(), FalconError> {
        let b = v.to_le_bytes();
        for i in 0..4 {
            self.store8(a + i as u32, b[i])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_load_word() {
        let mut ram = Ram::new(64);
        ram.store32(0x10, 0xDEADBEEF).unwrap();
        assert_eq!(ram.load32(0x10).unwrap(), 0xDEADBEEF);
    }
}
