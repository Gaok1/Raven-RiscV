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