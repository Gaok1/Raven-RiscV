use super::bits;
use crate::falcon::{errors::FalconError, instruction::Instruction};

pub(super) fn decode(word: u32) -> Result<Instruction, FalconError> {
    let rd = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let funct5 = bits(word, 31, 27) as u8;

    if funct3 != 0x2 {
        return Err(FalconError::Decode("AMO: only .W (funct3=2) supported"));
    }

    Ok(match funct5 {
        0x02 => Instruction::LrW { rd, rs1 },
        0x03 => Instruction::ScW { rd, rs1, rs2 },
        0x01 => Instruction::AmoswapW { rd, rs1, rs2 },
        0x00 => Instruction::AmoaddW { rd, rs1, rs2 },
        0x04 => Instruction::AmoxorW { rd, rs1, rs2 },
        0x0C => Instruction::AmoandW { rd, rs1, rs2 },
        0x08 => Instruction::AmoorW { rd, rs1, rs2 },
        0x14 => Instruction::AmomaxW { rd, rs1, rs2 },
        0x10 => Instruction::AmominW { rd, rs1, rs2 },
        0x1C => Instruction::AmomaxuW { rd, rs1, rs2 },
        0x18 => Instruction::AmominuW { rd, rs1, rs2 },
        _ => return Err(FalconError::Decode("AMO: unknown funct5")),
    })
}
