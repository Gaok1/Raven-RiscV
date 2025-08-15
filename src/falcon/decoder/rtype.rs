use crate::falcon::instruction::Instruction;
use super::{bits};

pub(super) fn decode(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let funct7 = bits(word, 31, 25) as u8;

    Ok(match (funct7, funct3) {
        (0x00, 0x0) => Instruction::Add{rd,rs1,rs2},
        (0x20, 0x0) => Instruction::Sub{rd,rs1,rs2},
        (0x00, 0x7) => Instruction::And{rd,rs1,rs2},
        (0x00, 0x6) => Instruction::Or {rd,rs1,rs2},
        (0x00, 0x4) => Instruction::Xor{rd,rs1,rs2},
        (0x00, 0x1) => Instruction::Sll{rd,rs1,rs2},
        (0x00, 0x5) => Instruction::Srl{rd,rs1,rs2},
        (0x20, 0x5) => Instruction::Sra{rd,rs1,rs2},
        (0x00, 0x2) => Instruction::Slt{rd,rs1,rs2},
        (0x00, 0x3) => Instruction::Sltu{rd,rs1,rs2},
        (0x01, 0x0) => Instruction::Mul{rd,rs1,rs2},
        (0x01, 0x1) => Instruction::Mulh{rd,rs1,rs2},
        (0x01, 0x2) => Instruction::Mulhsu{rd,rs1,rs2},
        (0x01, 0x3) => Instruction::Mulhu{rd,rs1,rs2},
        (0x01, 0x4) => Instruction::Div{rd,rs1,rs2},
        (0x01, 0x5) => Instruction::Divu{rd,rs1,rs2},
        (0x01, 0x6) => Instruction::Rem{rd,rs1,rs2},
        (0x01, 0x7) => Instruction::Remu{rd,rs1,rs2},
        _ => return Err("R-type inválido"),
    })
}
