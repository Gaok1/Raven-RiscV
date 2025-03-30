// ==========================
// ⚙️ OPCODES (7 bits)
// ==========================

// R-type (Arithmetic/Logic Register-Register operations)
pub const OP: u8 = 0x33;       // add, sub, mul, and, or, xor, etc.

// I-type (Arithmetic Immediate, Logical Immediate)
pub const OP_IMM: u8 = 0x13;   // addi, andi, ori, xori, etc.

// I-type (Loads)
pub const LOAD: u8 = 0x03;     // lb, lh, lw, lbu, lhu

// I-type (Jump and Link Register)
pub const JALR: u8 = 0x67;     // jalr

// S-type (Stores)
pub const STORE: u8 = 0x23;    // sb, sh, sw

// B-type (Branches)
pub const BRANCH: u8 = 0x63;   // beq, bne, blt, bge, etc.

// U-type
pub const LUI: u8 = 0x37;      // lui (Load Upper Immediate)
pub const AUIPC: u8 = 0x17;    // auipc (Add Upper Immediate to PC)

// J-type (Jump and Link)
pub const JAL: u8 = 0x6F;      // jal (Jump and Link)

// System instructions
pub const SYSTEM: u8 = 0x73;   // ecall, ebreak, csr instructions


// ==========================
// 📐 FUNCT3 (3 bits)
// ==========================

// Arithmetic instructions (R-type)
pub const ADD_SUB: u8 = 0x0;   // ADD, SUB
pub const SLL: u8     = 0x1;   // Shift Left Logical
pub const SLT: u8     = 0x2;   // Set Less Than
pub const SLTU: u8    = 0x3;   // Set Less Than Unsigned
pub const XOR: u8     = 0x4;   // XOR Logical
pub const SRL_SRA: u8 = 0x5;   // Shift Right Logical/Arithmetic
pub const OR: u8      = 0x6;   // OR Logical
pub const AND: u8     = 0x7;   // AND Logical

// Immediate arithmetic instructions (I-type OP_IMM)
pub const ADDI: u8  = 0x0;     // ADD Immediate
pub const SLLI: u8  = 0x1;     // Shift Left Logical Immediate
pub const SLTI: u8  = 0x2;     // Set Less Than Immediate
pub const SLTIU: u8 = 0x3;     // Set Less Than Immediate Unsigned
pub const XORI: u8  = 0x4;     // XOR Immediate
pub const SRLI_SRAI: u8 = 0x5; // Shift Right Logical/Arithmetic Immediate
pub const ORI: u8   = 0x6;     // OR Immediate
pub const ANDI: u8  = 0x7;     // AND Immediate

// Loads (I-type LOAD)
pub const LB:  u8 = 0x0;       // Load Byte
pub const LH:  u8 = 0x1;       // Load Halfword
pub const LW:  u8 = 0x2;       // Load Word
pub const LBU: u8 = 0x4;       // Load Byte Unsigned
pub const LHU: u8 = 0x5;       // Load Halfword Unsigned

// Stores (S-type STORE)
pub const SB: u8 = 0x0;        // Store Byte
pub const SH: u8 = 0x1;        // Store Halfword
pub const SW: u8 = 0x2;        // Store Word

// Branches (B-type BRANCH)
pub const BEQ:  u8 = 0x0;      // Branch if Equal
pub const BNE:  u8 = 0x1;      // Branch if Not Equal
pub const BLT:  u8 = 0x4;      // Branch if Less Than
pub const BGE:  u8 = 0x5;      // Branch if Greater or Equal
pub const BLTU: u8 = 0x6;      // Branch if Less Than Unsigned
pub const BGEU: u8 = 0x7;      // Branch if Greater or Equal Unsigned

// JALR (I-type)
pub const JALR_FUNCT3: u8 = 0x0;  // funct3 always 0 for JALR

// SYSTEM instructions (ecall/ebreak)
pub const PRIV: u8 = 0x0; // ECALL and EBREAK (funct3 always 0)


// ==========================
// 📏 FUNCT7 (7 bits)
// ==========================

// Arithmetic instructions differentiating ADD/SUB and shifts
pub const FUNCT7_ADD: u8 = 0x00;  // ADD, SLL, SRL
pub const FUNCT7_SUB: u8 = 0x20;  // SUB, SRA
pub const FUNCT7_MUL: u8 = 0x01;  // MUL, MULH, MULHSU, MULHU (RV32M extension)

// Shifts Immediate (I-type)
pub const FUNCT7_SLLI: u8 = 0x00; // Shift Left Logical Immediate
pub const FUNCT7_SRLI: u8 = 0x00; // Shift Right Logical Immediate
pub const FUNCT7_SRAI: u8 = 0x20; // Shift Right Arithmetic Immediate
