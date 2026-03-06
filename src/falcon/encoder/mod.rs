// src/falcon/encoder/mod.rs
use crate::falcon::instruction::Instruction;
use crate::falcon::arch::*;


#[inline] fn r(f7:u32, rs2:u32, rs1:u32, f3:u32, rd:u32, opc:u32) -> u32 {
    (f7<<25) | (rs2<<20) | (rs1<<15) | (f3<<12) | (rd<<7) | opc
}

#[inline] fn i(imm12:i32, rs1:u32, f3:u32, rd:u32, opc:u32) -> u32 {
    let imm = (imm12 as i32) & 0xFFF; // limit to 12 bits
    ((imm as u32)<<20) | (rs1<<15) | (f3<<12) | (rd<<7) | opc
}

#[inline] fn s(imm12:i32, rs2:u32, rs1:u32, f3:u32, opc:u32) -> u32 {
    let imm = (imm12 as i32) & 0xFFF;
    let imm_lo = (imm & 0x1F) as u32;
    let imm_hi = ((imm >> 5) & 0x7F) as u32;
    (imm_hi<<25) | (rs2<<20) | (rs1<<15) | (f3<<12) | (imm_lo<<7) | opc
}
#[inline] fn b(imm_bytes:i32, rs2:u32, rs1:u32, f3:u32, opc:u32) -> u32 {
    // imm is a BYTES offset relative to the PC (multiple of 2)
    assert!(imm_bytes % 2 == 0, "B-imm must be a multiple of 2");
    let imm = imm_bytes as u32;
    let b12  = ((imm >> 12) & 1) << 31;
    let b10_5= ((imm >> 5)  & 0x3F) << 25;
    let b4_1 = ((imm >> 1)  & 0xF)  << 8;
    let b11  = ((imm >> 11) & 1)    << 7;
    b12 | b10_5 | (rs2<<20) | (rs1<<15) | (f3<<12) | b4_1 | b11 | opc
}
#[inline] fn u(imm20:i32, rd:u32, opc:u32) -> u32 {
    // take the upper 20 bits (aligned to 12)
    ( (imm20 as u32) & 0xFFFFF000 ) | (rd<<7) | opc
}
// R4-type: fmadd/fmsub/fnmsub/fnmadd (fmt=0b00 = single, rm=0 = RNE)
#[inline] fn r4(rs3:u32, rs2:u32, rs1:u32, rd:u32, opc:u32) -> u32 {
    (rs3<<27) | (rs2<<20) | (rs1<<15) | (rd<<7) | opc
    // fmt bits [26:25]=0b00 (single precision), rm bits [14:12]=0b000 (RNE) — both zero
}
#[inline] fn j(imm_bytes:i32, rd:u32, opc:u32) -> u32 {
    // J-imm in bytes, multiple of 2
    assert!(imm_bytes % 2 == 0, "J-imm must be a multiple of 2");
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
        Slt{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x2, rd as u32, OPC_RTYPE as u32),
        Sltu{rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x3, rd as u32, OPC_RTYPE as u32),
        Mul{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_RTYPE as u32),
        Mulh{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x1, rd as u32, OPC_RTYPE as u32),
        Mulhsu{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x2, rd as u32, OPC_RTYPE as u32),
        Mulhu{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x3, rd as u32, OPC_RTYPE as u32),
        Div{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x4, rd as u32, OPC_RTYPE as u32),
        Divu{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x5, rd as u32, OPC_RTYPE as u32),
        Rem{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x6, rd as u32, OPC_RTYPE as u32),
        Remu{rd,rs1,rs2} => r(0x01, rs2 as u32, rs1 as u32, 0x7, rd as u32, OPC_RTYPE as u32),

        // I-type (OP-IMM)
        Addi{rd,rs1,imm} => i(imm, rs1 as u32, 0x0, rd as u32, OPC_OPIMM as u32),
        Andi{rd,rs1,imm} => i(imm, rs1 as u32, 0x7, rd as u32, OPC_OPIMM as u32),
        Ori {rd,rs1,imm} => i(imm, rs1 as u32, 0x6, rd as u32, OPC_OPIMM as u32),
        Xori{rd,rs1,imm} => i(imm, rs1 as u32, 0x4, rd as u32, OPC_OPIMM as u32),
        Slti{rd,rs1,imm} => i(imm, rs1 as u32, 0x2, rd as u32, OPC_OPIMM as u32),
        Sltiu{rd,rs1,imm} => i(imm, rs1 as u32, 0x3, rd as u32, OPC_OPIMM as u32),
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

        // Branches (imm in BYTES, relative to PC)
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

        Ecall => 0x0000_0073,          // SYSTEM/ECALL
        Ebreak | Halt => 0x0010_0073,  // SYSTEM/EBREAK (alias: HALT)
        Fence => 0x0000_100F,          // MISC-MEM/FENCE (iorw, iorw)

        // RV32F — LOAD-FP / STORE-FP
        Flw{rd,rs1,imm}  => i(imm, rs1 as u32, 0x2, rd as u32, OPC_FLW as u32),
        Fsw{rs2,rs1,imm} => s(imm, rs2 as u32, rs1 as u32, 0x2, OPC_FSW as u32),

        // RV32F — OP-FP (funct7 encodes operation, funct3=rm=0 for RNE)
        FaddS {rd,rs1,rs2} => r(0x00, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FsubS {rd,rs1,rs2} => r(0x04, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FmulS {rd,rs1,rs2} => r(0x08, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FdivS {rd,rs1,rs2} => r(0x0C, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FsqrtS{rd,rs1}     => r(0x2C, 0,          rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FminS {rd,rs1,rs2} => r(0x14, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FmaxS {rd,rs1,rs2} => r(0x14, rs2 as u32, rs1 as u32, 0x1, rd as u32, OPC_FP as u32),

        FsgnjS {rd,rs1,rs2} => r(0x10, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FsgnjnS{rd,rs1,rs2} => r(0x10, rs2 as u32, rs1 as u32, 0x1, rd as u32, OPC_FP as u32),
        FsgnjxS{rd,rs1,rs2} => r(0x10, rs2 as u32, rs1 as u32, 0x2, rd as u32, OPC_FP as u32),

        FeqS{rd,rs1,rs2} => r(0x50, rs2 as u32, rs1 as u32, 0x2, rd as u32, OPC_FP as u32),
        FltS{rd,rs1,rs2} => r(0x50, rs2 as u32, rs1 as u32, 0x1, rd as u32, OPC_FP as u32),
        FleS{rd,rs1,rs2} => r(0x50, rs2 as u32, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),

        FcvtWS {rd,rs1} => r(0x60, 0, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FcvtWuS{rd,rs1} => r(0x60, 1, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FcvtSW {rd,rs1} => r(0x68, 0, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FcvtSWu{rd,rs1} => r(0x68, 1, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),

        FmvXW  {rd,rs1} => r(0x70, 0, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FmvWX  {rd,rs1} => r(0x78, 0, rs1 as u32, 0x0, rd as u32, OPC_FP as u32),
        FclassS{rd,rs1} => r(0x70, 0, rs1 as u32, 0x1, rd as u32, OPC_FP as u32),

        // RV32F — R4-type (fused multiply-add)
        FmaddS {rd,rs1,rs2,rs3} => r4(rs3 as u32, rs2 as u32, rs1 as u32, rd as u32, OPC_FMADD  as u32),
        FmsubS {rd,rs1,rs2,rs3} => r4(rs3 as u32, rs2 as u32, rs1 as u32, rd as u32, OPC_FMSUB  as u32),
        FnmsubS{rd,rs1,rs2,rs3} => r4(rs3 as u32, rs2 as u32, rs1 as u32, rd as u32, OPC_FNMSUB as u32),
        FnmaddS{rd,rs1,rs2,rs3} => r4(rs3 as u32, rs2 as u32, rs1 as u32, rd as u32, OPC_FNMADD as u32),
    })
}
