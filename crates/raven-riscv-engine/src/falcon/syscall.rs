//! Syscall dispatch (`ecall`, `a7` = syscall number).
//!
//! Split into one module per ABI so a new calling convention (a different OS
//! or a bare-metal SBI-style ABI) can be added later without touching the
//! existing ones — each implements [`abi::SyscallAbi`] and gets added to the
//! `abis!` list below.
//!
//! Contract with callers ([`crate::falcon::exec`]): a *valid, recognized*
//! Linux syscall number never halts execution just because Raven doesn't
//! model it. Anything not claimed by any ABI module falls through to the
//! ENOSYS default at the bottom of [`handle_syscall_with_cycle_override`],
//! which reports the error in `a0` and keeps running — the same thing a real
//! Linux kernel does for a syscall number it doesn't implement. Only a
//! handful of deliberate termination points (`exit`, `exit_group`, fatal
//! `kill`/`tgkill`, `hart_exit`) ever return "stop".

mod abi;
mod errno;
mod falcon_abi;
mod gfx_abi;
mod helpers;
mod linux_abi;

use crate::{
    falcon::{errors::FalconError, memory::Bus, registers::Cpu},
    ui::{Console, console::ConsoleColor},
};
use abi::{SyscallAbi, SyscallCtx};
use errno::LINUX_ENOSYS;

pub use falcon_abi::{FALCON_HART_EXIT, FALCON_HART_START, FALCON_MAP_EXEC};
pub use gfx_abi::{
    GFX_FILL_RECT, GFX_POLL_KEY, GFX_PRESENT, GFX_SCREEN_CLEAR, GFX_SCREEN_INIT, GFX_SET_PIXEL,
    GFX_SLEEP_MS, GFX_TIME_MS,
};

/// ABIs tried in order for every syscall. First one that recognizes the code
/// wins; unrecognized codes fall through to the ENOSYS default. Built fresh
/// (unit structs, zero cost) wherever needed rather than returned, since a
/// `dyn SyscallAbi<B>` trait object can't outlive the borrow of `B` itself.
macro_rules! abi_handlers {
    () => {
        [
            &linux_abi::LinuxAbi as &dyn SyscallAbi<B>,
            &falcon_abi::FalconAbi,
            &gfx_abi::GfxAbi,
        ]
    };
}

fn syscall_name<B: Bus>(code: u32) -> &'static str {
    abi_handlers!()
        .iter()
        .find_map(|a| a.name(code))
        .unwrap_or("unknown")
}

/// Syscalls too frequent/noisy to trace even with `trace_syscalls` on
/// (per-character I/O and per-pixel/per-frame graphics calls would flood the
/// console). `screen_init` (rare, interesting) is deliberately not excluded.
fn should_trace_syscall(code: u32) -> bool {
    !matches!(
        code,
        63 | 64
            | 66
            | 65
            | 1000
            | 1001
            | 1002
            | 1003
            | 1004
            | 1005
            | 1006
            | 1008
            | 1010
            | 1011
            | 1012
            | 1013
            | 1014
            | 1015
            | GFX_SCREEN_CLEAR
            | GFX_SET_PIXEL
            | GFX_FILL_RECT
            | GFX_PRESENT
            | GFX_POLL_KEY
            | GFX_TIME_MS
            | GFX_SLEEP_MS
    )
}

fn trace_syscall<B: Bus>(console: &mut Console, cpu: &Cpu, code: u32) {
    if !console.trace_syscalls || !should_trace_syscall(code) {
        return;
    }
    console.push_colored(
        format!(
            "[H{}] syscall {} ({})",
            cpu.hart_id,
            code,
            syscall_name::<B>(code)
        ),
        ConsoleColor::Warning,
    );
}

/// Handles syscalls invoked via `ecall`.
///
/// ABI (Linux-style): `a7` = syscall number, `a0..a5` = args, `a0` = return
/// value (negative values mean `-errno`, represented as `u32`).
pub fn handle_syscall<B: Bus>(
    code: u32,
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
) -> Result<bool, FalconError> {
    handle_syscall_with_cycle_override(code, cpu, mem, console, None)
}

pub fn handle_syscall_with_cycle_override<B: Bus>(
    code: u32,
    cpu: &mut Cpu,
    mem: &mut B,
    console: &mut Console,
    cycle_override: Option<u64>,
) -> Result<bool, FalconError> {
    trace_syscall::<B>(console, cpu, code);

    let mut ctx = SyscallCtx {
        cpu,
        mem,
        console,
        cycle_override,
    };
    for handler in abi_handlers!() {
        if let Some(result) = handler.handle(code, &mut ctx) {
            return result;
        }
    }

    // No ABI module claims this code. A real kernel would still hand the
    // caller -ENOSYS rather than crash, so do the same instead of stopping
    // the run — this is the one rule that must hold for every valid Linux
    // syscall number we haven't (yet) modeled.
    ctx.console.push_colored(
        format!("syscall {code} not implemented, returning -ENOSYS"),
        ConsoleColor::Warning,
    );
    ctx.cpu.write(10, LINUX_ENOSYS);
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/support/falcon_syscall.rs"]
mod tests;
