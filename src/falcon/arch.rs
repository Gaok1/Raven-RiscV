use super::{memory::Memory, program::program::Program, registers::Register};
const REGISTERS_LEN: usize = 31;


#[derive(Clone)]
pub struct FalconArch {
    pub registers: Vec<Register>,
    pub memory: Memory,
    pub programs: Vec<Program>,
}

impl FalconArch {
    fn new() -> Self {
        FalconArch {
            registers: vec![Register::new(); REGISTERS_LEN],
            memory: Memory::new(),
            programs: Vec::new(),
        }
    }
}
