use crate::falcon::instruction::Instruction;
use super::{bits, sext};

pub(super) fn decode_jal(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    // J-imm: [20|10:1|11|19:12] << 1
    let imm_bits =
          (bits(word,31,31) << 20)
        | (bits(word,30,21) << 1)
        | (bits(word,20,20) << 11)
        | (bits(word,19,12) << 12);
    Ok(Instruction::Jal{ rd, imm: sext(imm_bits, 21) })
}
