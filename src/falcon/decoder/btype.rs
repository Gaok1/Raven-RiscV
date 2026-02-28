use crate::falcon::{instruction::Instruction, errors::FalconError};
use super::{bits, sext};

pub(super) fn decode(word:u32)->Result<Instruction,FalconError>{
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    // B-imm: [12|10:5|4:1|11] << 1
    let imm_bits =
          (bits(word,31,31) << 12)
        | (bits(word,30,25) << 5)
        | (bits(word,11,8)  << 1)
        | (bits(word,7,7)   << 11);
    let imm = sext(imm_bits, 13);

    Ok(match funct3 {
        0x0 => Instruction::Beq{rs1,rs2,imm},
        0x1 => Instruction::Bne{rs1,rs2,imm},
        0x4 => Instruction::Blt{rs1,rs2,imm},
        0x5 => Instruction::Bge{rs1,rs2,imm},
        0x6 => Instruction::Bltu{rs1,rs2,imm},
        0x7 => Instruction::Bgeu{rs1,rs2,imm},
        _ => return Err(FalconError::Decode("Invalid branch")),
    })
}
