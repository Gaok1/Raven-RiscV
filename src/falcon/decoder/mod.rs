mod rtype;
mod itype;
mod stype;
mod btype;
mod jtype;

use crate::falcon::instruction::Instruction;
use crate::falcon::arch::*;

#[inline] fn bits(v:u32, hi:u8, lo:u8)->u32 { (v >> lo) & ((1u32 << (hi-lo+1)) - 1) }
#[inline] fn sext(v:u32, bits_n: u8) -> i32 {
    let shift = 32 - bits_n as u32;
    ((v << shift) as i32) >> shift
}

pub fn decode(word: u32) -> Result<Instruction, &'static str> {
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
        _ => Err("opcode desconhecido"),
    }
}


pub fn disasm(word: u32) -> String {
    match decode(word) {
        Ok(i) => match i {
            Instruction::Addi{rd,rs1,imm} => format!("addi x{rd}, x{rs1}, {}", imm),
            Instruction::Add {rd,rs1,rs2}  => format!("add  x{rd}, x{rs1}, x{rs2}"),
            Instruction::Beq {rs1,rs2,imm} => format!("beq  x{rs1}, x{rs2}, {}", imm),
            Instruction::Jal {rd,imm}      => format!("jal  x{rd}, {}", imm),
            Instruction::Jalr{rd,rs1,imm}  => format!("jalr x{rd}, x{rs1}, {}", imm),
            Instruction::Lw  {rd,rs1,imm}  => format!("lw   x{rd}, {}(x{rs1})", imm),
            Instruction::Sw  {rs2,rs1,imm} => format!("sw   x{rs2}, {}(x{rs1})", imm),
            Instruction::Ecall             => "ecall".into(),
            other => format!("{other:?}"), // fallback para o resto
        },
        Err(e) => format!(".word 0x{word:08x} ; {e}"),
    }
}


// expõe helpers pros submódulos

