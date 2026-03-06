// src/falcon/arch.rs

// Constantes de opcode (RV32I)
pub const OPC_RTYPE: u8  = 0x33;
pub const OPC_OPIMM: u8  = 0x13;
pub const OPC_LOAD: u8   = 0x03;
pub const OPC_STORE: u8  = 0x23;
pub const OPC_BRANCH: u8 = 0x63;
pub const OPC_LUI: u8    = 0x37;
pub const OPC_AUIPC: u8  = 0x17;
pub const OPC_JAL: u8    = 0x6F;
pub const OPC_JALR: u8   = 0x67;
pub const OPC_SYSTEM: u8 = 0x73;

// RV32F opcodes
pub const OPC_FLW:    u8 = 0x07; // LOAD-FP  (I-type)
pub const OPC_FSW:    u8 = 0x27; // STORE-FP (S-type)
pub const OPC_FMADD:  u8 = 0x43; // R4-type
pub const OPC_FMSUB:  u8 = 0x47; // R4-type
pub const OPC_FNMSUB: u8 = 0x4B; // R4-type
pub const OPC_FNMADD: u8 = 0x4F; // R4-type
pub const OPC_FP:     u8 = 0x53; // OP-FP (aritmética, comparação, conversão, move)