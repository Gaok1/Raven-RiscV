// Definição dos deslocamentos para cada campo (posição dos bits) na instrução R-type
pub const OPCODE_POS: u32 = 0;   // Bits [6:0]
pub const RD_POS: u32     = 7;   // Bits [11:7]
pub const FUNCT3_POS: u32 = 12;  // Bits [14:12]
pub const RS1_POS: u32    = 15;  // Bits [19:15]
pub const RS2_POS: u32    = 20;  // Bits [24:20]
pub const FUNCT7_POS: u32 = 25;  // Bits [31:25]

// ======================================================================
// OPCODE CONSTANTS (já deslocados para a posição correta)
// ======================================================================
pub const OPCODE_R: u32 = 0x33 << OPCODE_POS;  // 0x33 para instruções R-type (ADD, SUB, etc.)

// ======================================================================
// FUNCT3 CONSTANTS (3 bits) – já deslocados para bits [14:12]
// ======================================================================
pub const FUNCT3_ADD_SUB: u32 = 0x0 << FUNCT3_POS;  // Usado por ADD e SUB
pub const FUNCT3_SLL: u32     = 0x1 << FUNCT3_POS;  // Shift Left Logical
pub const FUNCT3_SLT: u32     = 0x2 << FUNCT3_POS;  // Set Less Than
pub const FUNCT3_SLTU: u32    = 0x3 << FUNCT3_POS;  // Set Less Than Unsigned
pub const FUNCT3_XOR: u32     = 0x4 << FUNCT3_POS;  // XOR
pub const FUNCT3_SRL_SRA: u32 = 0x5 << FUNCT3_POS;  // Shift Right Logical or Arithmetic
pub const FUNCT3_OR: u32      = 0x6 << FUNCT3_POS;  // OR
pub const FUNCT3_AND: u32     = 0x7 << FUNCT3_POS;  // AND

// ======================================================================
// FUNCT7 CONSTANTS (7 bits) – já deslocados para bits [31:25]
// ======================================================================
pub const FUNCT7_ADD: u32 = 0x00 << FUNCT7_POS;  // Para ADD, SLL, SRL
pub const FUNCT7_SUB: u32 = 0x20 << FUNCT7_POS;  // Para SUB, SRA
pub const FUNCT7_MUL: u32 = 0x01 << FUNCT7_POS;  // Para MUL (se utilizar extensão RV32M)


