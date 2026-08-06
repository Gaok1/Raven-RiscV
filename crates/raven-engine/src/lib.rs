//! Trait-driven Raven simulation engine.
//!
//! Architectures are discovered through [`ArchitectureRegistry`]. The built-in
//! registry contains the production RISC-V 32 backend and a deliberately small
//! Toy16 backend used to prove that the public contracts are ISA-neutral.

mod architecture;
pub mod architectures;
mod cache_model;
pub mod capability;
mod falc;
pub mod pipeline;

/// Compatibility path for the production RV32 backend.
///
/// The implementation is owned by [`architectures::riscv32`]; this reexport
/// keeps the established `raven_engine::falcon` API stable.
pub use architectures::riscv32::falcon;

pub use architecture::{
    Architecture, ArchitectureCapabilities, ArchitectureDescriptor, ArchitectureRegistry,
    Assembler, CycleResult, Diagnostic, Endianness, InstructionDoc, Machine, MachineError,
    MachineSnapshot, MachineState, ProgramImage, ProgramSegment, RegisterValue, SourceMap,
    StepOutcome, ZeroFill,
};

/// The batteries-included RV32 runner, kept at the crate root because it is the
/// crate's documented entry point for graders and tests.
pub use falcon::engine::{Falcon, RunResult};

/// Minimal host-side console and screen device used by the Falcon engine.
#[allow(clippy::collapsible_if)]
pub mod host {
    pub mod console;
    pub mod fs_sim;
    pub mod screen;

    pub use console::Console;
    pub use fs_sim::FileSim;
}

// Internal compatibility shim for Falcon modules that still refer to `crate::ui`.
mod ui {
    pub use crate::host::{Console, console, screen};
}

/// Compatibility-free high-level runner. Selects an architecture explicitly.
pub struct Engine {
    architecture: std::sync::Arc<dyn Architecture>,
}

impl Engine {
    pub fn new(architecture: std::sync::Arc<dyn Architecture>) -> Self {
        Self { architecture }
    }

    pub fn from_registry(registry: &ArchitectureRegistry, id: &str) -> Result<Self, MachineError> {
        registry
            .get(id)
            .map(Self::new)
            .ok_or_else(|| MachineError::new(format!("unknown architecture '{id}'")))
    }

    pub fn architecture(&self) -> &dyn Architecture {
        self.architecture.as_ref()
    }

    pub fn assemble(&self, source: &str, base_address: u64) -> Result<ProgramImage, Diagnostic> {
        self.architecture.assembler().assemble(source, base_address)
    }

    pub fn create_machine(&self, memory_size: usize) -> Result<Box<dyn Machine>, MachineError> {
        self.architecture.create_machine(memory_size)
    }
}
