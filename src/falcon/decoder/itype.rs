use crate::falcon::instruction::Instruction;
use super::{bits, sext};

pub(super) fn decode_opimm(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let imm = sext(bits(word,31,20), 12);

    Ok(match funct3 {
        0x0 => Instruction::Addi{ rd, rs1, imm },
        0x7 => Instruction::Andi{ rd, rs1, imm },
        0x6 => Instruction::Ori { rd, rs1, imm },
        0x4 => Instruction::Xori{ rd, rs1, imm },
        0x2 => Instruction::Slti{ rd, rs1, imm },
        0x3 => Instruction::Sltiu{ rd, rs1, imm },
        0x1 => {
            let shamt = bits(word,24,20) as u8;
            Instruction::Slli { rd, rs1, shamt }
        }
        0x5 => {
            let shamt = bits(word,24,20) as u8;
            if bits(word,31,25)==0 { Instruction::Srli { rd, rs1, shamt } }
            else                    { Instruction::Srai { rd, rs1, shamt } }
        }
        _ => return Err("I-type OP-IMM inválido"),
    })
}

pub(super) fn decode_loads(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    let imm = sext(bits(word,31,20), 12);

    Ok(match funct3 {
        0x0 => Instruction::Lb { rd, rs1, imm },
        0x1 => Instruction::Lh { rd, rs1, imm },
        0x2 => Instruction::Lw { rd, rs1, imm },
        0x4 => Instruction::Lbu{ rd, rs1, imm },
        0x5 => Instruction::Lhu{ rd, rs1, imm },
        _ => return Err("Load inválido"),
    })
}

pub(super) fn decode_jalr(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let funct3 = bits(word, 14, 12) as u8;
    let rs1 = bits(word, 19, 15) as u8;
    if funct3 != 0 { return Err("JALR com funct3 != 0"); }
    let imm = sext(bits(word,31,20), 12);
    Ok(Instruction::Jalr{ rd, rs1, imm })
}

pub(super) fn decode_lui(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let imm = (word & 0xFFFFF000) as i32;
    Ok(Instruction::Lui{ rd, imm })
}

pub(super) fn decode_auipc(word:u32)->Result<Instruction,&'static str>{
    let rd  = bits(word, 11, 7) as u8;
    let imm = (word & 0xFFFFF000) as i32;
    Ok(Instruction::Auipc{ rd, imm })
}

pub(super) fn decode_system(_word:u32)->Result<Instruction,&'static str>{
    // MVP: trata ECALL/EBREAK como halt
    Ok(Instruction::Ecall)
}
