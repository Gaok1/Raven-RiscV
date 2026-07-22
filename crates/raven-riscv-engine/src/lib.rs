pub mod falcon;

/// Minimal host-side console and screen device used by the Falcon engine.
pub mod host {
    pub mod console;
    pub mod screen;

    pub use console::Console;
}

// Internal compatibility shim for Falcon modules that still refer to `crate::ui`.
mod ui {
    pub use crate::host::{Console, console, screen};
}

pub use falcon::{Falcon, RunResult};
