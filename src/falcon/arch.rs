use super::{memory::Memory, program::program::Program, registers::Register};
const REGISTERS_LEN: usize = 32;


#[derive(Clone)]
pub struct FalconArch {
    pub registers: [Register; REGISTERS_LEN],
    pub memory: Memory,
    pub programs: Vec<Program>,
}

impl FalconArch {
    fn new() -> Self {
        FalconArch {
            registers: Register::risc_v_set(),
            memory: Memory::new(),
            programs: Vec::new(),
        }
    }
}
