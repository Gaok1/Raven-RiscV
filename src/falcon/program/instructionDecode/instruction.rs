use crate::falcon::registers::RegisterAddress;





pub struct Instruction {
    opcode: u8,
    operands: Vec<RegisterAddress>,
}

impl Instruction {
    fn new(opcode: u8, operands: Vec<RegisterAddress>) -> Self {
        Instruction { opcode, operands }
    }
    fn get_opcode(&self) -> u8 {
        self.opcode
    }
    fn get_operands(&self) -> &Vec<RegisterAddress> {
        &self.operands
    }
}