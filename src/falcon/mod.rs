pub mod arch;
pub mod errors;
pub mod registers;
pub mod memory;
pub mod instruction;
pub mod exec;

pub mod decoder;

// 🆕
pub mod encoder;
pub mod asm;

pub mod program;

pub use registers::Cpu;
pub use memory::{Bus, Ram};
pub use instruction::Instruction;
