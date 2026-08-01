//! Architecture selection shared by the CLI and the TUI.
//!
//! Everything that resolves an `--arch` value, an `architecture=` config key, or
//! a FALC image's backend name goes through here, so the set of valid IDs is
//! defined once — by the engine's registry — instead of being restated at every
//! call site.

use raven_riscv_engine::{Architecture, ArchitectureRegistry};
use std::sync::Arc;

/// The backend used when the user does not ask for one.
pub const DEFAULT_ID: &str = raven_riscv_engine::architectures::riscv32::ID;

/// The registry of built-in backends.
pub fn registry() -> &'static ArchitectureRegistry {
    ArchitectureRegistry::builtin()
}

/// Resolve a backend by ID, naming the valid alternatives when it is unknown.
pub fn lookup(id: &str) -> Result<Arc<dyn Architecture>, String> {
    registry().get(id).ok_or_else(|| {
        format!(
            "unknown architecture '{id}' (use {})",
            registry().ids().join(" or ")
        )
    })
}
