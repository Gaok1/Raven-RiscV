// falcon/instruction.rs
#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    // R-type
    Add{ rd:u8, rs1:u8, rs2:u8 }, Sub{ rd:u8, rs1:u8, rs2:u8 },
    And{ rd:u8, rs1:u8, rs2:u8 }, Or{ rd:u8, rs1:u8, rs2:u8 },
    Xor{ rd:u8, rs1:u8, rs2:u8 }, Sll{ rd:u8, rs1:u8, rs2:u8 },
    Srl{ rd:u8, rs1:u8, rs2:u8 }, Sra{ rd:u8, rs1:u8, rs2:u8 },

    // I-type
    Addi{ rd:u8, rs1:u8, imm:i32 }, Andi{ rd:u8, rs1:u8, imm:i32 },
    Ori{ rd:u8, rs1:u8, imm:i32 }, Xori{ rd:u8, rs1:u8, imm:i32 },
    Slli{ rd:u8, rs1:u8, shamt:u8 }, Srli{ rd:u8, rs1:u8, shamt:u8 }, Srai{ rd:u8, rs1:u8, shamt:u8 },
    Lb{ rd:u8, rs1:u8, imm:i32 }, Lh{ rd:u8, rs1:u8, imm:i32 }, Lw{ rd:u8, rs1:u8, imm:i32 },
    Lbu{ rd:u8, rs1:u8, imm:i32 }, Lhu{ rd:u8, rs1:u8, imm:i32 },
    Jalr{ rd:u8, rs1:u8, imm:i32 },

    // S-type
    Sb{ rs2:u8, rs1:u8, imm:i32 }, Sh{ rs2:u8, rs1:u8, imm:i32 }, Sw{ rs2:u8, rs1:u8, imm:i32 },

    // B-type
    Beq{ rs1:u8, rs2:u8, imm:i32 }, Bne{ rs1:u8, rs2:u8, imm:i32 },
    Blt{ rs1:u8, rs2:u8, imm:i32 }, Bge{ rs1:u8, rs2:u8, imm:i32 },
    Bltu{ rs1:u8, rs2:u8, imm:i32 }, Bgeu{ rs1:u8, rs2:u8, imm:i32 },

    // U/J
    Lui{ rd:u8, imm:i32 }, Auipc{ rd:u8, imm:i32 },
    Jal{ rd:u8, imm:i32 },

    // System (MVP: só ecall/ebreak como “halt”)
    Ecall, Ebreak,
}
