use crate::falcon::registers::Register;

#[derive(Debug, Default, Clone)]
pub struct MemoryMap {
    pub data_section_start: u64,
    pub data_section_end: u64,
    pub text_section_start: u64,
    pub text_section_end: u64,
    pub end_of_memory: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Program {
    pub memory: MemoryMap,
    pub registers: [Register; 32], // Fixado para RISC-V
    pub opcodes: Vec<u32>,         // Opcodes de 32 bits
}
