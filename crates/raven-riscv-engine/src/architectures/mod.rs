//! Built-in architecture backends.
//!
//! Each ISA owns one physical module directory. Its adapter, assembler,
//! machine, codec, and ISA-specific tests stay behind that module boundary.

pub mod riscv32;
pub mod sap;
pub mod toy16;
