// Arithmetic Instructions
pub const ADD:   u8 = 0x01;
pub const SUB:   u8 = 0x02;
pub const MUL:   u8 = 0x03;
pub const DIV:   u8 = 0x04;
pub const MOV:   u8 = 0x05;

// Floating Point Arithmetic
pub const ADDF:  u8 = 0x10;
pub const SUBF:  u8 = 0x11;
pub const MULF:  u8 = 0x12;
pub const DIVF:  u8 = 0x13;

// Double Precision Floating Point
pub const ADDFD: u8 = 0x14;
pub const SUBFD: u8 = 0x15;
pub const MULFD: u8 = 0x16;
pub const DIVFD: u8 = 0x17;

// Control Flow
pub const JMP:   u8 = 0x20;
pub const JNZ:   u8 = 0x21;
pub const JZ:    u8 = 0x22;
pub const JGT:   u8 = 0x23;
pub const JLT:   u8 = 0x24;
pub const JGE:   u8 = 0x25;
pub const JLE:   u8 = 0x26;
pub const BEGIN: u8 = 0x27;
pub const END:   u8 = 0x28;
pub const HALT:  u8 = 0x29;

// Memory Access - LOAD
pub const LB:    u8 = 0x30;
pub const LH:    u8 = 0x31;
pub const LW:    u8 = 0x32;
pub const LD:    u8 = 0x33;
pub const LA:    u8 = 0x34;

// Memory Access - STORE
pub const SB:    u8 = 0x40;
pub const SH:    u8 = 0x41;
pub const SW:    u8 = 0x42;
pub const SD:    u8 = 0x43;

// Pointer Arithmetic
pub const PTADD: u8 = 0x50;
pub const PTSUB: u8 = 0x51;
pub const PTMUL: u8 = 0x52;
pub const PTDIV: u8 = 0x53;

// Float Load/Store
pub const FL:    u8 = 0x60;
pub const FS:    u8 = 0x61;
pub const FDL:   u8 = 0x62;
pub const FDS:   u8 = 0x63;

// Conversion Instructions
pub const ITOF:  u8 = 0x70;
pub const FTOI:  u8 = 0x71;

// Stack Instructions
pub const PUSH_B: u8 = 0x80;
pub const PUSH_H: u8 = 0x81;
pub const PUSH_W: u8 = 0x82;
pub const PUSH_D: u8 = 0x83;

pub const POP_B:  u8 = 0x90;
pub const POP_H:  u8 = 0x91;
pub const POP_W:  u8 = 0x92;
pub const POP_D:  u8 = 0x93;

pub const PEEK_B: u8 = 0xA0;
pub const PEEK_H: u8 = 0xA1;
pub const PEEK_W: u8 = 0xA2;
pub const PEEK_D: u8 = 0xA3;
