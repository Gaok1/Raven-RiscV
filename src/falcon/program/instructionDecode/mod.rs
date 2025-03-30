pub mod instruction;
#[allow(non_snake_case)]
pub mod instructionOpCode;
#[allow(non_snake_case)]
pub mod instructionFunction;

#[allow(non_snake_case)]
pub mod Decoder;

pub mod Encoder;


pub type CodifiedInstruction = u32;