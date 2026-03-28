// falcon/decoder/fptype.rs — RV32F decoder
use super::{bits, sext};
use crate::falcon::{errors::FalconError, instruction::Instruction};

/// Decode OPC_FP (0x53) — all FP arithmetic, comparison, conversion, and move instructions
pub(super) fn decode_fp(word: u32) -> Result<Instruction, FalconError> {
    let rd = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let funct7 = bits(word, 31, 25) as u8;

    Ok(match (funct7, funct3, rs2) {
        (0x00, _, _) => Instruction::FaddS { rd, rs1, rs2 },
        (0x04, _, _) => Instruction::FsubS { rd, rs1, rs2 },
        (0x08, _, _) => Instruction::FmulS { rd, rs1, rs2 },
        (0x0C, _, _) => Instruction::FdivS { rd, rs1, rs2 },
        (0x2C, _, 0) => Instruction::FsqrtS { rd, rs1 },
        (0x10, 0, _) => Instruction::FsgnjS { rd, rs1, rs2 },
        (0x10, 1, _) => Instruction::FsgnjnS { rd, rs1, rs2 },
        (0x10, 2, _) => Instruction::FsgnjxS { rd, rs1, rs2 },
        (0x14, 0, _) => Instruction::FminS { rd, rs1, rs2 },
        (0x14, 1, _) => Instruction::FmaxS { rd, rs1, rs2 },
        (0x50, 2, _) => Instruction::FeqS { rd, rs1, rs2 },
        (0x50, 1, _) => Instruction::FltS { rd, rs1, rs2 },
        (0x50, 0, _) => Instruction::FleS { rd, rs1, rs2 },
        (0x60, rm, 0) => Instruction::FcvtWS { rd, rs1, rm },
        (0x60, rm, 1) => Instruction::FcvtWuS { rd, rs1, rm },
        (0x68, _, 0) => Instruction::FcvtSW { rd, rs1 },
        (0x68, _, 1) => Instruction::FcvtSWu { rd, rs1 },
        (0x70, 0, 0) => Instruction::FmvXW { rd, rs1 },
        (0x70, 1, 0) => Instruction::FclassS { rd, rs1 },
        (0x78, 0, 0) => Instruction::FmvWX { rd, rs1 },
        _ => return Err(FalconError::Decode("unknown OP-FP encoding")),
    })
}

/// Decode R4-type (FMADD/FMSUB/FNMSUB/FNMADD)
pub(super) fn decode_r4(word: u32, opc: u8) -> Result<Instruction, FalconError> {
    let rd = bits(word, 11, 7) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let rs3 = bits(word, 31, 27) as u8;
    // fmt = bits[26:25]; we only support fmt=0b00 (single), but decode regardless
    Ok(match opc {
        0x43 => Instruction::FmaddS { rd, rs1, rs2, rs3 },
        0x47 => Instruction::FmsubS { rd, rs1, rs2, rs3 },
        0x4B => Instruction::FnmsubS { rd, rs1, rs2, rs3 },
        0x4F => Instruction::FnmaddS { rd, rs1, rs2, rs3 },
        _ => return Err(FalconError::Decode("unknown R4-type opcode")),
    })
}

/// Decode OPC_FLW (0x07) — I-type float load
pub(super) fn decode_flw(word: u32) -> Result<Instruction, FalconError> {
    let rd = bits(word, 11, 7) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let imm = sext(bits(word, 31, 20), 12);
    // funct3=0x2 = flw; other values reserved
    let funct3 = bits(word, 14, 12);
    if funct3 != 0x2 {
        return Err(FalconError::Decode("unknown LOAD-FP funct3"));
    }
    Ok(Instruction::Flw { rd, rs1, imm })
}

/// Decode OPC_FSW (0x27) — S-type float store
pub(super) fn decode_fsw(word: u32) -> Result<Instruction, FalconError> {
    let rs1 = bits(word, 19, 15) as u8;
    let rs2 = bits(word, 24, 20) as u8;
    let funct3 = bits(word, 14, 12);
    if funct3 != 0x2 {
        return Err(FalconError::Decode("unknown STORE-FP funct3"));
    }
    let imm_lo = bits(word, 11, 7);
    let imm_hi = bits(word, 31, 25);
    let imm = sext((imm_hi << 5) | imm_lo, 12);
    Ok(Instruction::Fsw { rs2, rs1, imm })
}
