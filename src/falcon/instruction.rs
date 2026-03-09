// falcon/instruction.rs
#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    // R-type
    Add{ rd:u8, rs1:u8, rs2:u8 }, Sub{ rd:u8, rs1:u8, rs2:u8 },
    And{ rd:u8, rs1:u8, rs2:u8 }, Or{ rd:u8, rs1:u8, rs2:u8 },
    Xor{ rd:u8, rs1:u8, rs2:u8 }, Sll{ rd:u8, rs1:u8, rs2:u8 },
    Srl{ rd:u8, rs1:u8, rs2:u8 }, Sra{ rd:u8, rs1:u8, rs2:u8 },
    Slt{ rd:u8, rs1:u8, rs2:u8 }, Sltu{ rd:u8, rs1:u8, rs2:u8 },
    Mul{ rd:u8, rs1:u8, rs2:u8 }, Mulh{ rd:u8, rs1:u8, rs2:u8 },
    Mulhsu{ rd:u8, rs1:u8, rs2:u8 }, Mulhu{ rd:u8, rs1:u8, rs2:u8 },
    Div{ rd:u8, rs1:u8, rs2:u8 }, Divu{ rd:u8, rs1:u8, rs2:u8 },
    Rem{ rd:u8, rs1:u8, rs2:u8 }, Remu{ rd:u8, rs1:u8, rs2:u8 },

    // I-type
    Addi{ rd:u8, rs1:u8, imm:i32 }, Andi{ rd:u8, rs1:u8, imm:i32 },
    Ori{ rd:u8, rs1:u8, imm:i32 }, Xori{ rd:u8, rs1:u8, imm:i32 },
    Slti{ rd:u8, rs1:u8, imm:i32 }, Sltiu{ rd:u8, rs1:u8, imm:i32 },
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

    // System (MVP: ecall and ebreak; 'halt' is an assembler alias)
    Ecall, Ebreak, Halt,

    // Memory ordering (RV32I base — executed as nop in single-core simulator)
    Fence,

    // RV32A — atomic memory operations (single-core: no real contention)
    LrW   { rd:u8, rs1:u8 },
    ScW   { rd:u8, rs1:u8, rs2:u8 },
    AmoswapW { rd:u8, rs1:u8, rs2:u8 },
    AmoaddW  { rd:u8, rs1:u8, rs2:u8 },
    AmoxorW  { rd:u8, rs1:u8, rs2:u8 },
    AmoandW  { rd:u8, rs1:u8, rs2:u8 },
    AmoorW   { rd:u8, rs1:u8, rs2:u8 },
    AmomaxW  { rd:u8, rs1:u8, rs2:u8 },
    AmominW  { rd:u8, rs1:u8, rs2:u8 },
    AmomaxuW { rd:u8, rs1:u8, rs2:u8 },
    AmominuW { rd:u8, rs1:u8, rs2:u8 },

    // RV32F — floating-point extension
    // Load/Store (I/S-type with float rd/rs2)
    Flw  { rd:u8, rs1:u8, imm:i32 },
    Fsw  { rs2:u8, rs1:u8, imm:i32 },

    // Arithmetic (R-type, operate on f registers)
    FaddS  { rd:u8, rs1:u8, rs2:u8 },
    FsubS  { rd:u8, rs1:u8, rs2:u8 },
    FmulS  { rd:u8, rs1:u8, rs2:u8 },
    FdivS  { rd:u8, rs1:u8, rs2:u8 },
    FsqrtS { rd:u8, rs1:u8 },
    FminS  { rd:u8, rs1:u8, rs2:u8 },
    FmaxS  { rd:u8, rs1:u8, rs2:u8 },

    // Sign injection (used by fmv.s/fneg.s/fabs.s pseudos)
    FsgnjS  { rd:u8, rs1:u8, rs2:u8 },
    FsgnjnS { rd:u8, rs1:u8, rs2:u8 },
    FsgnjxS { rd:u8, rs1:u8, rs2:u8 },

    // Comparison (result in integer register)
    FeqS { rd:u8, rs1:u8, rs2:u8 },
    FltS { rd:u8, rs1:u8, rs2:u8 },
    FleS { rd:u8, rs1:u8, rs2:u8 },

    // Conversion
    FcvtWS  { rd:u8, rs1:u8, rm:u8 }, // f32 → i32 (signed);  rm = rounding mode (0=rne,1=rtz,2=rdn,3=rup,4=rmm,7=dyn)
    FcvtWuS { rd:u8, rs1:u8, rm:u8 }, // f32 → u32 (unsigned); rm = rounding mode
    FcvtSW  { rd:u8, rs1:u8 }, // i32 → f32
    FcvtSWu { rd:u8, rs1:u8 }, // u32 → f32

    // Move (bit-pattern, between int and float register files)
    FmvXW { rd:u8, rs1:u8 }, // float-bits → int reg
    FmvWX { rd:u8, rs1:u8 }, // int reg → float-bits

    // Classify
    FclassS { rd:u8, rs1:u8 },

    // Fused multiply-add (R4-type)
    FmaddS  { rd:u8, rs1:u8, rs2:u8, rs3:u8 }, //  rs1*rs2 + rs3
    FmsubS  { rd:u8, rs1:u8, rs2:u8, rs3:u8 }, //  rs1*rs2 - rs3
    FnmsubS { rd:u8, rs1:u8, rs2:u8, rs3:u8 }, // -rs1*rs2 + rs3
    FnmaddS { rd:u8, rs1:u8, rs2:u8, rs3:u8 }, // -rs1*rs2 - rs3
}
