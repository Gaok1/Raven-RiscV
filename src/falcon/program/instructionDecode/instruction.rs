use crate::falcon::registers::RegisterAddress;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionOpCode {
    RType,
    IType,
    SType,
    BType,
    UType,
    JType,
}
