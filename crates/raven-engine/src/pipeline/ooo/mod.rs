//! Dynamic scheduling: the two designs where an instruction runs when its
//! operands are ready rather than when its turn comes.
//!
//! Both models were written against RV32 and lived in the host's UI layer,
//! which is why no other architecture had them. Nothing about either is
//! RISC-V's: a scoreboard tracks which unit owes which register, and Tomasulo
//! renames a register to the entry that will produce it. Both questions are
//! about *names and units*, and a [`PipelineShape`] declares the units while a
//! [`PipelineOp`] names the registers.
//!
//! [`PipelineShape`]: super::PipelineShape
//! [`PipelineOp`]: super::PipelineOp

pub mod registers;

pub use registers::{Operands, Producer, RegisterTable};
