//! Optional capabilities a backend can offer beyond running a program.
//!
//! [`Machine`](crate::Machine) is the floor: load, step, snapshot. It is
//! deliberately small, because every backend must implement all of it. But a
//! host that only has the floor can only draw a list of numbers — which is why
//! the TUI grew a second, RV32-only runtime for everything richer.
//!
//! A capability is how a backend says "I can also do this". Each one is an
//! independent trait exposed by the object that owns the state. Architecture
//! capabilities use accessors on `Machine` that default to `None`, so adding
//! one never breaks an existing backend. Cycle-level state such as
//! [`PipelineInspect`] is instead exposed by the Falcon runtime that owns and
//! journals it.
//!
//! ```text
//! Machine                    every backend
//!  ├── registers()           named register banks       → RegisterFile
//!  ├── memory()              inspectable guest memory   → MemoryInspect
//!  ├── code()                disassemble / assemble one → InstructionCodec
//!  └── caches()              inspectable cache levels   → CacheHierarchy
//!
//! Falcon runtime
//!  └── pipeline_inspect()    stages / hazards / history → PipelineInspect
//! ```
//!
//! ## For a host
//!
//! Ask, then use. The answer never changes for a given machine, so it is safe
//! to decide layout from it once:
//!
//! ```no_run
//! # fn demo(machine: &dyn raven_riscv_engine::Machine) {
//! if let Some(registers) = machine.registers() {
//!     for entry in registers.entries() {
//!         println!("{:<6} {:#x}", entry.display_name(), entry.value);
//!     }
//! }
//! # }
//! ```
//!
//! ## For a backend
//!
//! Implement the trait on your machine type and return `Some(self)` from the
//! accessor. Two lines, and every host that understands the capability lights
//! up for your ISA:
//!
//! ```ignore
//! impl RegisterFile for MyMachine { /* ... */ }
//!
//! impl Machine for MyMachine {
//!     fn registers(&self) -> Option<&dyn RegisterFile> { Some(self) }
//!     // ...
//! }
//! ```
//!
//! ## Rules a capability must follow
//!
//! - **Nothing ISA-shaped in the signature.** No 32-bit words, no fixed
//!   register count, no assumption that an instruction is four bytes. If a
//!   type in the signature would be wrong for a 16-bit machine, it is wrong.
//! - **Cheap enough to call while rendering.** A host may call these once per
//!   frame.
//! - **Read paths do not perturb the machine.** Showing memory must not warm a
//!   cache or move a statistic; that is what [`MemoryInspect::peek`] is for.

mod cache;
mod code;
mod memory;
mod pipeline;
mod registers;

pub use cache::{
    CacheHierarchy, CacheHistory, CacheInclusionPolicy, CacheLevelConfig, CacheLevelStats,
    CacheLevelView, CacheLineView, CacheReplacementPolicy, CacheRole, CacheSetView,
    CacheWriteAllocation, CacheWritePolicy,
};
pub use code::{InstructionCodec, InstructionField, InstructionInfo};
pub use memory::{MemoryInspect, MemoryRegion};
pub use pipeline::{
    PipelineHazardKind, PipelineInspect, PipelineInstructionClass, PipelineSlotView,
    PipelineStageView, PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow,
    PipelineTimelineState, PipelineTraceKind, PipelineTraceView, PipelineUnitView,
};
pub use registers::{RegisterBank, RegisterEntry, RegisterFile, RegisterId};
