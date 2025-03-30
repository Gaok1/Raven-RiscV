use super::registers::Register;

// Registradores de Controle
const ZERO: u8 = 0; // x0: sempre zero
const RA: u8 = 1; // x1: return address
const SP: u8 = 2; // x2: stack pointer
const GP: u8 = 3; // x3: global pointer
const TP: u8 = 4; // x4: thread pointer

// Registradores Temporários
const T0: u8 = 5; // x5
const T1: u8 = 6; // x6
const T2: u8 = 7; // x7

// Registradores Salvos (preservados)
const S0: u8 = 8; // x8: frame pointer (às vezes chamado FP)
const FP: u8 = 8; // alias
const S1: u8 = 9; // x9

// Registradores de Argumentos
const A0: u8 = 10; // x10
const A1: u8 = 11; // x11
const A2: u8 = 12; // x12
const A3: u8 = 13; // x13
const A4: u8 = 14; // x14
const A5: u8 = 15; // x15
const A6: u8 = 16; // x16
const A7: u8 = 17; // x17

// Registradores Salvos adicionais
const S2: u8 = 18; // x18
const S3: u8 = 19; // x19
const S4: u8 = 20; // x20
const S5: u8 = 21; // x21
const S6: u8 = 22; // x22
const S7: u8 = 23; // x23
const S8: u8 = 24; // x24
const S9: u8 = 25; // x25
const S10: u8 = 26; // x26
const S11: u8 = 27; // x27

// Registradores Temporários adicionais
const T3: u8 = 28; // x28
const T4: u8 = 29; // x29
const T5: u8 = 30; // x30
const T6: u8 = 31; // x31
