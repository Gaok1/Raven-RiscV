

type RegisterAddress = u64;

struct Instruction {
    opcode: u64,
    operands: Vec<RegisterAddress>,
}

impl Instruction {
    fn new(opcode: u64, operands: Vec<RegisterAddress>) -> Self {
        Instruction { opcode, operands }
    }
    fn get_opcode(&self) -> u64 {
        self.opcode
    }
    fn get_operands(&self) -> &Vec<RegisterAddress> {
        &self.operands
    }
}