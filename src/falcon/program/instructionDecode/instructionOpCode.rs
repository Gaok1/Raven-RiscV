/// Primeiros 8 bits de Op Code


// =====================================
// 🧮 Arithmetic Instructions (Integers)
// =====================================
pub const ADD:  u8 = 0x01;
pub const SUB:  u8 = 0x02;
pub const MUL:  u8 = 0x03;
pub const DIV:  u8 = 0x04;
pub const MOV:  u8 = 0x05;

// =====================================
// 🔢 Floating-Point Arithmetic (Float)
// =====================================
pub const ADDF: u8 = 0x06;
pub const SUBF: u8 = 0x07;
pub const MULF: u8 = 0x08;
pub const DIVF: u8 = 0x09;

// =====================================
// 🔢 Floating-Point Arithmetic (Double)
// =====================================
pub const ADDFD: u8 = 0x0A;
pub const SUBFD: u8 = 0x0B;
pub const MULFD: u8 = 0x0C;
pub const DIVFD: u8 = 0x0D;

// =====================================
// 🔁 Control Flow Instructions
// =====================================
pub const JMP:   u8 = 0x0E;
pub const JNZ:   u8 = 0x0F;
pub const JZ:    u8 = 0x10;
pub const JGT:   u8 = 0x11;
pub const JLT:   u8 = 0x12;
pub const JGE:   u8 = 0x13;
pub const JLE:   u8 = 0x14;
pub const BEGIN: u8 = 0x15;
pub const END:   u8 = 0x16;
pub const HALT:  u8 = 0x17;

// =====================================
// 💾 Memory Access - Load Instructions
// =====================================
pub const LB: u8 = 0x18;
pub const LH: u8 = 0x19;
pub const LW: u8 = 0x1A;
pub const LD: u8 = 0x1B;
pub const LA: u8 = 0x1C;

// =====================================
// 💾 Memory Access - Store Instructions
// =====================================
pub const SB: u8 = 0x1D;
pub const SH: u8 = 0x1E;
pub const SW: u8 = 0x1F;
pub const SD: u8 = 0x20;

// =====================================
// 📌 Pointer Arithmetic Instructions
// =====================================
pub const PTADD: u8 = 0x21;
pub const PTSUB: u8 = 0x22;
pub const PTMUL: u8 = 0x23;
pub const PTDIV: u8 = 0x24;

// =====================================
// 💾 Float Load/Store Instructions
// =====================================
pub const FL:  u8 = 0x25;
pub const FS:  u8 = 0x26;
pub const FDL: u8 = 0x27;
pub const FDS: u8 = 0x28;

// =====================================
// 🔄 Integer/Float Conversion Instructions
// =====================================
pub const ITOF: u8 = 0x29;
pub const FTOI: u8 = 0x2A;

// =====================================
// 📥 Stack Instructions (PUSH)
// =====================================
pub const PUSH_B: u8 = 0x2B;
pub const PUSH_H: u8 = 0x2C;
pub const PUSH_W: u8 = 0x2D;
pub const PUSH_D: u8 = 0x2E;

// =====================================
// 📤 Stack Instructions (POP)
// =====================================
pub const POP_B: u8 = 0x2F;
pub const POP_H: u8 = 0x30;
pub const POP_W: u8 = 0x31;
pub const POP_D: u8 = 0x32;

// =====================================
// 🔎 Stack Instructions (PEEK)
// =====================================
pub const PEEK_B: u8 = 0x33;
pub const PEEK_H: u8 = 0x34;
pub const PEEK_W: u8 = 0x35;
pub const PEEK_D: u8 = 0x36;
