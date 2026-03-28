// raven_api/hart.rs — high-level hart-spawning API
//
// Stack allocation:
//
//   alloc_hart_stack(size)  — allocates a 16-byte-aligned stack on the heap
//                             and returns a &'static mut [u8] ready for
//                             spawn_hart / spawn_hart_fn.
//
// Hart spawning (two flavours):
//
//   spawn_hart_fn(entry, stack, arg)   — function pointer, no extra allocation
//   spawn_hart(closure, stack)         — FnOnce closure, boxes the task
//                                        (cannot be #[no_mangle] — generic)

extern crate alloc;

use alloc::boxed::Box;
use core::alloc::Layout;
use core::hint::spin_loop;
use portable_atomic::{AtomicBool, Ordering};

use super::syscall::hart_start;

const DEFAULT_HART_STACK_SIZE: usize = 8192;

// ── Stack allocator ──────────────────────────────────────────────────────────

/// Allocate a 16-byte-aligned stack of `size` bytes for a hart.
///
/// Returns a `&'static mut [u8]` that can be passed directly to
/// [`spawn_hart`] or [`spawn_hart_fn`]. The memory is intentionally leaked —
/// hart stacks live for the duration of the program.
///
/// `size` is rounded up to the nearest multiple of 16 so that the stack-top
/// address (end of the slice) is always 16-byte aligned, as required by the
/// RISC-V ABI.
///
/// # Panics
/// Panics if `size` is zero or the allocator returns null (OOM).
///
/// # Example
/// ```no_run
/// let stack = alloc_hart_stack(8192);
/// spawn_hart_fn(worker, stack, /*arg=*/0);
/// ```
#[unsafe(no_mangle)]
pub fn alloc_hart_stack(size: usize) -> &'static mut [u8] {
    assert!(size > 0, "hart stack size must be > 0");
    let size = size.next_multiple_of(16);
    let layout = Layout::from_size_align(size, 16).expect("alloc_hart_stack: invalid layout");
    // SAFETY: layout is non-zero and well-aligned.
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "alloc_hart_stack: out of memory");
    // SAFETY: ptr is valid for `size` bytes, exclusively owned, and we
    // intentionally leak it ('static lifetime).
    unsafe { core::slice::from_raw_parts_mut(ptr, size) }
}

// ── Closure trampoline ───────────────────────────────────────────────────────

// Type-erased task builder. The closure is stored here until `start()` is called.
pub struct HartTask {
    f: Box<dyn FnOnce() + Send>,
    stack: &'static mut [u8],
    done: Box<AtomicBool>,
}

impl HartTask {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self::with_stack_size(f, DEFAULT_HART_STACK_SIZE)
    }

    pub fn with_stack_size<F>(f: F, stack_size: usize) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self::with_stack(f, alloc_hart_stack(stack_size))
    }

    pub fn with_stack<F>(f: F, stack: &'static mut [u8]) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            f: Box::new(f),
            stack,
            done: Box::new(AtomicBool::new(false)),
        }
    }

    pub fn start(self) -> HartHandle {
        let HartTask { f, stack, done } = self;
        let done_ptr = Box::into_raw(done);
        let payload = Box::new(HartTaskPayload { f, done: done_ptr });
        let ptr = Box::into_raw(payload) as u32;
        let sp = stack.as_ptr_range().end as u32;
        let code = unsafe { hart_start(hart_trampoline as *const () as u32, sp, ptr) };
        assert_eq!(code, 0, "failed to start hart: syscall returned {code}");

        HartHandle { done: done_ptr }
    }
}

struct HartTaskPayload {
    f: Box<dyn FnOnce() + Send>,
    done: *mut AtomicBool,
}

pub struct HartHandle {
    done: *mut AtomicBool,
}

impl HartHandle {
    pub fn is_finished(&self) -> bool {
        // SAFETY: `done` is allocated in `HartTask::start` and stays valid until
        // `join(self)` consumes the handle and frees it.
        unsafe { (*self.done).load(Ordering::Acquire) }
    }

    pub fn join(self) {
        while !self.is_finished() {
            spin_loop();
        }
        // SAFETY: `self` is consumed, so this is the only place that frees `done`.
        unsafe { drop(Box::from_raw(self.done)) };
    }
}

// The new hart's entry point.  Its a0 = ptr (set by the simulator).
// extern "C" ensures the first argument arrives in a0, matching Raven's ABI.
// Uses hart_exit() so only this hart terminates; the main hart keeps running.
#[unsafe(no_mangle)]
extern "C" fn hart_trampoline(ptr: u32) -> ! {
    let task = unsafe { Box::from_raw(ptr as *mut HartTaskPayload) };
    (task.f)();
    unsafe { (*task.done).store(true, Ordering::Release) };
    unsafe { super::syscall::hart_exit() }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Spawn a hart from a **function pointer** — no heap allocation.
///
/// `entry` is called with `arg` in `a0`.
/// Pass the full slice — the stack-top address is computed for you.
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
/// // or:
/// spawn_hart_fn(worker, alloc_hart_stack(4096), /*arg=*/1);
/// ```
#[unsafe(no_mangle)]
pub fn spawn_hart_fn(entry: fn(u32) -> !, stack: &'static mut [u8], arg: u32) -> i32 {
    let sp = stack.as_ptr_range().end as u32;
    unsafe { hart_start(entry as u32, sp, arg) }
}

/// Spawn a hart from a **closure** — boxes the closure on the heap.
///
/// The closure is owned by the new hart and called exactly once.
/// It must never return — call [`hart_exit`] or [`exit`] before it ends.
///
/// Note: cannot be `#[no_mangle]` because it is a generic function.
///
/// Returns 0 on success.
///
/// # Example
/// ```no_run
/// let value = 42u32;
/// spawn_hart(move || {
///     println!("hart got value = {value}");
///     hart_exit();
/// }, alloc_hart_stack(8192));
/// ```
pub fn spawn_hart<F>(f: F, stack: &'static mut [u8]) -> HartHandle
where
    F: FnOnce() + Send + 'static,
{
    HartTask::with_stack(f, stack).start()
}
