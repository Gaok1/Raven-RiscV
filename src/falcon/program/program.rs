use crate::falcon::registers::Register;

#[derive(Debug, Default, Clone)]
struct MemorySegmentsPointer {
    pub data_section_start_pointer: u64,
    pub data_section_end_pointer: u64,
    pub text_section_start_pointer: u64,
    pub text_section_end_pointer: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Program {
    //struct to manage programs context
    segmentation: MemorySegmentsPointer,
    pub registers: Vec<Register>,
    pub opcodes: Vec<u64>,
}
