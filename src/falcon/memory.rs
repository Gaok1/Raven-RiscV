// falcon/memory.rs
pub trait Bus {
    fn load8(&self, addr: u32) -> u8;
    fn load16(&self, addr: u32) -> u16;
    fn load32(&self, addr: u32) -> u32;
    fn store8(&mut self, addr: u32, val: u8);
    fn store16(&mut self, addr: u32, val: u16);
    fn store32(&mut self, addr: u32, val: u32);
}

pub struct Ram { data: Vec<u8> }

impl Ram {
    pub fn new(size: usize) -> Self { Self { data: vec![0; size] } }
}

impl Bus for Ram {
    fn load8(&self, a: u32) -> u8 { self.data[a as usize] }
    fn load16(&self, a: u32) -> u16 { u16::from_le_bytes([self.load8(a), self.load8(a+1)]) }
    fn load32(&self, a: u32) -> u32 { u32::from_le_bytes([
        self.load8(a), self.load8(a+1), self.load8(a+2), self.load8(a+3)
    ])}
    fn store8(&mut self, a: u32, v: u8) { self.data[a as usize] = v; }
    fn store16(&mut self, a: u32, v: u16) { let b=v.to_le_bytes(); self.store8(a,b[0]); self.store8(a+1,b[1]); }
    fn store32(&mut self, a: u32, v: u32) {
        let b=v.to_le_bytes(); for i in 0..4 { self.store8(a+i as u32, b[i]); }
    }
}
