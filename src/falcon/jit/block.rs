//! Basic-block descriptor stub for the future JIT.
//!
//! Phase A: types only, no detection logic. Phase B will populate
//! `BasicBlock` instances by scanning instruction words until the first
//! control-flow terminator.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockTerminator {
    Branch,
    Jal,
    Jalr,
    Ecall,
    Ebreak,
    Halt,
    Fence,
    FallThrough,
}

#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub start_pc: u32,
    pub end_pc: u32,
    pub words: Vec<u32>,
    pub terminator: BlockTerminator,
}
