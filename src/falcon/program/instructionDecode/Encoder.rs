use super::{instructionOpCode::*, CodifiedInstruction};




 
pub fn encode() -> CodifiedInstruction {
    0
}



// ======================================================================
// Função para codificar uma instrução R-type (32 bits)
// ======================================================================
// Formato R-type:
// | funct7 (7 bits) | rs2 (5 bits) | rs1 (5 bits) | funct3 (3 bits) | rd (5 bits) | opcode (7 bits) |
fn encode_r_type(rd: u32, rs1: u32, rs2: u32, funct3: u32, funct7: u32) -> CodifiedInstruction {
    (funct7)                   |  // já deslocado para bits [31:25]
    (rs2 << RS2_POS)           |  // desloca rs2 para bits [24:20]
    (rs1 << RS1_POS)           |  // desloca rs1 para bits [19:15]
    (funct3)                   |  // já deslocado para bits [14:12]
    (rd << RD_POS)             |  // desloca rd para bits [11:7]
    (OPCODE_R)                  // opcode ocupa bits [6:0] (já deslocado)
}
