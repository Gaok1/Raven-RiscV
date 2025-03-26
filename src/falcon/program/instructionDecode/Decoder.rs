// pegar o opcode da instrução 
// decodificiar e retornar struct Instruction

// Primeiros 6 bits de Op Code (55 instruções)
// 1 bit para determinar se é imediato ou não
// 5 bits para endereçar registradores
// 5 bits para endereçar registradores
//  se for imediato, 4 bytes finais são o valor possível de imediato

use super::instruction::{self, Instruction};
const OP_CODE_MASK: u64 = 0xFC00000000000000;

pub fn decode_instruction(codified_instruction : u64)-> Instruction{


    todo!()
}


fn is_imeadiate(opcode: u64) -> bool {
    todo!()
}