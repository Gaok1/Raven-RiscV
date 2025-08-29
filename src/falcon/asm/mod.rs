// src/falcon/asm/mod.rs
mod assembler;
mod errors;
mod program;
mod pseudo;
mod utils;

pub use assembler::assemble;
pub use errors::AsmError;
pub use program::Program;

#[cfg(test)]
mod tests;

