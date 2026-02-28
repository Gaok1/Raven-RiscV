use crate::falcon::{instruction::Instruction, errors::FalconError};
use super::{bits, sext};

pub(super) fn decode(word:u32)->Result<Instruction,FalconError>{
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let imm = {
        let hi = bits(word,31,25);
        let lo = bits(word,11,7);
        sext((hi<<5)|lo, 12)
    };

    Ok(match funct3 {
        0x0 => Instruction::Sb{rs2,rs1,imm},
        0x1 => Instruction::Sh{rs2,rs1,imm},
        0x2 => Instruction::Sw{rs2,rs1,imm},
        _ => return Err(FalconError::Decode("Invalid store")),
    })
}
