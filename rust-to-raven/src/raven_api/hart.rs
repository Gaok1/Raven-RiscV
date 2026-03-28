// raven_api/hart.rs — high-level hart-spawning API
//
// Two flavours:
//
//   spawn_hart_fn(entry, stack, arg)   — function pointer, zero allocation
//   spawn_hart(closure, stack)         — FnOnce closure, heap-allocates the task
//
// Both compute the stack-top address automatically from the supplied slice.

extern crate alloc;

use alloc::boxed::Box;

use super::syscall::hart_start;

// ── Closure trampoline ───────────────────────────────────────────────────────

// Type-erased task wrapper. Lets us use a single non-generic trampoline.
// Using `FnOnce()` (not `-> !`) avoids the unstable never-type in trait objects.
struct HartTask {
    f: Box<dyn FnOnce() + Send>,
}

// The new hart's entry point.  Its a0 = ptr (set by the simulator).
// extern "C" ensures the first argument arrives in a0, matching Raven's ABI.
// Uses hart_exit() so only this hart terminates; the main hart keeps running.
extern "C" fn hart_trampoline(ptr: u32) -> ! {
    let task = unsafe { Box::from_raw(ptr as *mut HartTask) };
    (task.f)();
    super::syscall::hart_exit()
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Spawn a hart from a **function pointer** — no heap allocation.
///
/// `entry` is called with `arg` in `a0`.
/// `stack` must be a `'static` mutable byte slice; the hart uses it as its
/// call stack.  Pass the full slice — the stack-top address is computed for you.
///
/// Returns 0 on success.
///
/// # Example
/// ```no_run
/// static mut STACK: [u8; 4096] = [0; 4096];
///
/// fn worker(id: u32) -> ! {
///     println!("hart {id} running");
///     exit(0);
/// }
///
/// spawn_hart_fn(worker, unsafe { &mut STACK }, /*arg=*/1);
/// ```
pub fn spawn_hart_fn(entry: fn(u32) -> !, stack: &'static mut [u8], arg: u32) -> i32 {
    let sp = stack.as_ptr_range().end as u32;
    hart_start(entry as u32, sp, arg)
}

/// Spawn a hart from a **closure** — boxes the closure on the heap.
///
/// The closure is leaked and owned by the new hart; it is called exactly once
/// and must never return (call `exit(code)` before the closure ends).
/// `stack` must be a `'static` mutable byte slice large enough for the hart's
/// call frames (typically 4–16 KB).
///
/// Returns 0 on success.
///
/// # Example
/// ```no_run
/// static mut STACK: [u8; 4096] = [0; 4096];
///
/// let value = 42u32;
/// spawn_hart(move || {
///     println!("hart got value = {value}");
///     exit(0);
/// }, unsafe { &mut STACK });
/// ```
pub fn spawn_hart<F>(f: F, stack: &'static mut [u8]) -> i32
where
    F: FnOnce() + Send + 'static,
{
    let task = Box::new(HartTask { f: Box::new(f) });
    let ptr = Box::into_raw(task) as u32;
    let sp = stack.as_ptr_range().end as u32;
    hart_start(hart_trampoline as *const () as u32, sp, ptr)
}
