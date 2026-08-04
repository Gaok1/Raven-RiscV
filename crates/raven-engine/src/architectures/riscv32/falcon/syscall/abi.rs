//! Pluggable syscall ABI trait.
//!
//! Each ABI module (Linux, Falcon teaching extensions, graphics, ...) owns a
//! disjoint slice of the `a7` code space. `handle` returns `None` when a code
//! isn't part of that ABI, letting the dispatcher in `syscall.rs` try the
//! next one; this is what lets a new ABI (e.g. a different OS convention) be
//! added later as one more module without touching the existing ones.

use crate::{
    falcon::{errors::FalconError, memory::Bus, registers::Cpu},
    ui::Console,
};

/// Borrowed simulator state handed to an ABI's `handle` for the duration of
/// one syscall dispatch.
pub struct SyscallCtx<'a, B: Bus> {
    pub cpu: &'a mut Cpu,
    pub mem: &'a mut B,
    pub console: &'a mut Console,
    pub cycle_override: Option<u64>,
}

pub trait SyscallAbi<B: Bus> {
    /// Human-readable name for tracing, if this ABI recognizes `code`.
    fn name(&self, code: u32) -> Option<&'static str>;

    /// Handles `code` if it belongs to this ABI.
    ///
    /// Returns `None` to let the dispatcher fall through to the next ABI (or
    /// to the safe ENOSYS default if no ABI claims it). `Some(Ok(true))`
    /// continues execution; `Some(Ok(false))` requests a stop (only for
    /// deliberate termination points such as `exit`/`exit_group`).
    fn handle(&self, code: u32, ctx: &mut SyscallCtx<B>) -> Option<Result<bool, FalconError>>;
}
