// src/falcon/asm/mod.rs
mod assembler;
mod errors;
mod program;
mod pseudo;
pub(crate) mod utils;

pub use assembler::assemble;
#[allow(unused_imports)]
pub use errors::AsmError;
#[allow(unused_imports)]
pub use program::Program;

#[cfg(test)]
#[path = "../../../tests/support/falcon_asm.rs"]
mod tests;
