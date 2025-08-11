// src/falcon/encoder/mod.rs
use crate::falcon::instruction::Instruction;
use crate::falcon::arch::*;

#[inline] fn r(f7:u32, rs2:u32, rs1:u32, f3:u32, rd:u32, opc:u32) -> u32 {
    (f7<<25) | (rs2<<20) | (rs1<<15) | (f3<<12) | (rd<<7) | opc
}
#[inline] fn i(imm12:i32, rs1:u32, f3:u32, rd:u32, opc:u32) -> u32 {
    let imm = (imm12 as i32) & 0xFFF;
    ((imm as u32)<<20) | (rs1<<15) | (f3<<12) | (rd<<7) | opc
}
#[inline] fn s(imm12:i32, rs2:u32, rs1:u32, f3:u32, opc:u32) -> u32 {
    let imm = (imm12 as i32) & 0xFFF;
    let imm_lo = (imm & 0x1F) as u32;
    let imm_hi = ((imm >> 5) & 0x7F) as u32;
    (imm_hi<<25) | (rs2<<20) | (rs1<<15) | (f3<<12) | (imm_lo<<7) | opc
}
#[inline] fn b(imm_bytes:i32, rs2:u32, rs1:u32, f3:u32, opc:u32) -> u32 {
    // imm é deslocamento em BYTES relativo ao PC (múltiplo de 2)
    assert!(imm_bytes % 2 == 0, "B-imm deve ser múltiplo de 2");
    let imm = imm_bytes as u32;
    let b12  = ((imm >> 12) & 1) << 31;
    let b10_5= ((imm >> 5)  & 0x3F) << 25;
    let b4_1 = ((imm >> 1)  & 0xF)  << 8;
    let b11  = ((imm >> 11) & 1)    << 7;
    b12 | b10_5 | (rs2<<20) | (rs1<<15) | (f3<<12) | b4_1 | b11 | opc
}
#[inline] fn u(imm20:i32, rd:u32, opc:u32) -> u32 {
    // pega os 20 bits altos (alinhado a 12)
    ( (imm20 as u32) & 0xFFFFF000 ) | (rd<<7) | opc
}
#[inline] fn j(imm_bytes:i32, rd:u32, opc:u32) -> u32 {
    // J-imm em bytes, múltiplo de 2
    assert!(imm_bytes % 2 == 0, "J-imm deve ser múltiplo de 2");
    let imm = imm_bytes as u32;
    let b20   = ((imm >> 20) & 1) << 31;
    let b10_1 = ((imm >> 1)  & 0x3FF) << 21;
    let b11   = ((imm >> 11) & 1) << 20;
    let b19_12= ((imm >> 12) & 0xFF) << 12;
    b20 | b10_1 | b11 | b19_12 | (rd<<7) | opc
}

pub fn encode(inst: Instruction) -> Result<u32, &'static str> {
    use Instruction::*;
    Ok(match inst {
        // R-type
        Add{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_RTYPE as u32),
        Sub{rd,rs1,rs2} => r(0x20, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_RTYPE as u32),
        And{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x7, rd as u32, OPC_RTYPE as u32),
        Or {rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x6, rd as u32, OPC_RTYPE as u32),
        Xor{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x4, rd as u32, OPC_RTYPE as u32),
        Sll{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x1, rd as u32, OPC_RTYPE as u32),
        Srl{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x5, rd as u32, OPC_RTYPE as u32),
        Sra{rd,rs1,rs2} => r(0x20, rs2 as u32, rs1 as u32, 0x5, rd as u32, OPC_RTYPE as u32),

        // I-type (OP-IMM)
        Addi{rd,rs1,imm} => i(imm, rs1 as u32, 0x0, rd as u32, OPC_OPIMM as u32),
        Andi{rd,rs1,imm} => i(imm, rs1 as u32, 0x7, rd as u32, OPC_OPIMM as u32),
        Ori {rd,rs1,imm} => i(imm, rs1 as u32, 0x6, rd as u32, OPC_OPIMM as u32),
        Xori{rd,rs1,imm} => i(imm, rs1 as u32, 0x4, rd as u32, OPC_OPIMM as u32),
        Slli{rd,rs1,shamt} => r(0x00, (shamt & 0x1F) as u32, rs1 as u32, 0x1, rd as u32, OPC_OPIMM as u32),
        Srli{rd,rs1,shamt} => r(0x00, (shamt & 0x1F) as u32, rs1 as u32, 0x5, rd as u32, OPC_OPIMM as u32),
        Srai{rd,rs1,shamt} => r(0x20, (shamt & 0x1F) as u32, rs1 as u32, 0x5, rd as u32, OPC_OPIMM as u32),

        // Loads
        Lb {rd,rs1,imm} => i(imm, rs1 as u32, 0x0, rd as u32, OPC_LOAD as u32),
        Lh {rd,rs1,imm} => i(imm, rs1 as u32, 0x1, rd as u32, OPC_LOAD as u32),
        Lw {rd,rs1,imm} => i(imm, rs1 as u32, 0x2, rd as u32, OPC_LOAD as u32),
        Lbu{rd,rs1,imm} => i(imm, rs1 as u32, 0x4, rd as u32, OPC_LOAD as u32),
        Lhu{rd,rs1,imm} => i(imm, rs1 as u32, 0x5, rd as u32, OPC_LOAD as u32),

        // Stores
        Sb{rs2,rs1,imm} => s(imm, rs2 as u32, rs1 as u32, 0x0, OPC_STORE as u32),
        Sh{rs2,rs1,imm} => s(imm, rs2 as u32, rs1 as u32, 0x1, OPC_STORE as u32),
        Sw{rs2,rs1,imm} => s(imm, rs2 as u32, rs1 as u32, 0x2, OPC_STORE as u32),

        // Branches (imm em BYTES, relativo a PC)
        Beq{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x0, OPC_BRANCH as u32),
        Bne{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x1, OPC_BRANCH as u32),
        Blt{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x4, OPC_BRANCH as u32),
        Bge{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x5, OPC_BRANCH as u32),
        Bltu{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x6, OPC_BRANCH as u32),
        Bgeu{rs1,rs2,imm} => b(imm, rs2 as u32, rs1 as u32, 0x7, OPC_BRANCH as u32),

        // U/J
        Lui{rd,imm}   => u(imm, rd as u32, OPC_LUI as u32),
        Auipc{rd,imm} => u(imm, rd as u32, OPC_AUIPC as u32),
        Jal{rd,imm}   => j(imm, rd as u32, OPC_JAL as u32),

        Jalr{rd,rs1,imm} => i(imm, rs1 as u32, 0x0, rd as u32, OPC_JALR as u32),

        Ecall => 0x00000073,  // SYSTEM/ECALL
        Ebreak => 0x00100073, // SYSTEM/EBREAK
    })
}
