mod rtype;
mod itype;
mod stype;
mod btype;
mod jtype;

use crate::falcon::{instruction::Instruction, errors::FalconError};
use crate::falcon::arch::*;

#[inline] fn bits(v:u32, hi:u8, lo:u8)->u32 { (v >> lo) & ((1u32 << (hi-lo+1)) - 1) }
#[inline] fn sext(v:u32, bits_n: u8) -> i32 {
    let shift = 32 - bits_n as u32;
    ((v << shift) as i32) >> shift
}

pub fn decode(word: u32) -> Result<Instruction, FalconError> {
    let opcode = bits(word, 6, 0) as u8;
    match opcode {
        OPC_RTYPE  => rtype::decode(word),
        OPC_OPIMM  => itype::decode_opimm(word),
        OPC_LOAD   => itype::decode_loads(word),
        OPC_STORE  => stype::decode(word),
        OPC_BRANCH => btype::decode(word),
        OPC_JAL    => jtype::decode_jal(word),
        OPC_JALR   => itype::decode_jalr(word),
        OPC_LUI    => itype::decode_lui(word),
        OPC_AUIPC  => itype::decode_auipc(word),
        OPC_SYSTEM => itype::decode_system(word),
        _ => Err(FalconError::Decode("unknown opcode")),
    }
}
// expose helpers to submodules
