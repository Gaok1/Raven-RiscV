mod atype;
mod btype;
mod fptype;
mod itype;
mod jtype;
mod rtype;
mod stype;

use crate::falcon::arch::*;
use crate::falcon::{errors::FalconError, instruction::Instruction};

#[inline]
fn bits(v: u32, hi: u8, lo: u8) -> u32 {
    (v >> lo) & ((1u32 << (hi - lo + 1)) - 1)
}
#[inline]
fn sext(v: u32, bits_n: u8) -> i32 {
    let shift = 32 - bits_n as u32;
    ((v << shift) as i32) >> shift
}

pub fn decode(word: u32) -> Result<Instruction, FalconError> {
    let opcode = bits(word, 6, 0) as u8;
    match opcode {
        OPC_RTYPE => rtype::decode(word),
        OPC_OPIMM => itype::decode_opimm(word),
        OPC_LOAD => itype::decode_loads(word),
        OPC_STORE => stype::decode(word),
        OPC_BRANCH => btype::decode(word),
        OPC_JAL => jtype::decode_jal(word),
        OPC_JALR => itype::decode_jalr(word),
        OPC_LUI => itype::decode_lui(word),
        OPC_AUIPC => itype::decode_auipc(word),
        OPC_SYSTEM => itype::decode_system(word),
        OPC_AMO => atype::decode(word),
        0x0F => Ok(Instruction::Fence), // MISC-MEM: fence/fence.i → nop
        // RV32F
        OPC_FLW => fptype::decode_flw(word),
        OPC_FSW => fptype::decode_fsw(word),
        OPC_FP => fptype::decode_fp(word),
        OPC_FMADD => fptype::decode_r4(word, OPC_FMADD),
        OPC_FMSUB => fptype::decode_r4(word, OPC_FMSUB),
        OPC_FNMSUB => fptype::decode_r4(word, OPC_FNMSUB),
        OPC_FNMADD => fptype::decode_r4(word, OPC_FNMADD),
        _ => Err(FalconError::Decode("unknown opcode")),
    }
}

#[allow(dead_code)]
pub fn disasm(word: u32) -> String {
    match decode(word) {
        Ok(i) => match i {
            Instruction::Addi { rd, rs1, imm } => format!("addi x{rd}, x{rs1}, {}", imm),
            Instruction::Add { rd, rs1, rs2 } => format!("add  x{rd}, x{rs1}, x{rs2}"),
            Instruction::Beq { rs1, rs2, imm } => format!("beq  x{rs1}, x{rs2}, {}", imm),
            Instruction::Jal { rd, imm } => format!("jal  x{rd}, {}", imm),
            Instruction::Jalr { rd, rs1, imm } => format!("jalr x{rd}, x{rs1}, {}", imm),
            Instruction::Lw { rd, rs1, imm } => format!("lw   x{rd}, {}(x{rs1})", imm),
            Instruction::Sw { rs2, rs1, imm } => format!("sw   x{rs2}, {}(x{rs1})", imm),
            Instruction::Ecall => "ecall".into(),
            Instruction::Ebreak => "ebreak".into(),
            Instruction::Halt => "halt".into(),
            other => format!("{other:?}"), // fallback para o resto
        },
        Err(e) => format!(".word 0x{word:08x} ; {e}"),
    }
}

// expose helpers to submodules
