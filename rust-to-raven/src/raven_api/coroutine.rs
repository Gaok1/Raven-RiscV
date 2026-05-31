//! Stackful cooperative coroutines for Raven, generic over the value type.
//!
//! A [`Coroutine<T>`] runs a closure on its own stack. The closure receives a
//! [`Yielder<T>`]; calling [`Yielder::suspend`] hands a `T` back to whoever
//! called [`Coroutine::resume`], keeping the coroutine's stack and registers
//! alive so the next `resume` continues exactly where it left off.
//!
//! Coroutines are cooperative: exactly one runs per resume/suspend chain and
//! control only moves on an explicit resume/suspend. This is *not* the parallel
//! [`hart`](crate::raven_api::hardware_thread::hart) machinery — a coroutine
//! switch is a pure user-space register/stack swap, no `ecall` involved.
//! Different harts may still drive independent coroutines concurrently.
//!
//! `resume(send)` and `suspend(value)` exchange a value of any type `T` in each
//! direction. The in-flight value is carried as a `Box<T>` — a fixed-size
//! pointer to whatever `T` happens to be — so the machinery stays uniform while
//! the public API still deals in plain `T` values. That makes the coroutine a
//! generator over arbitrary types:
//!
//! ```no_run
//! // generic over the value type — here u64, which would not fit the old usize
//! let mut fib = Coroutine::new(4096, |y| {
//!     let (mut a, mut b): (u64, u64) = (0, 1);
//!     for _ in 0..10 {
//!         y.suspend(a);
//!         let next = a + b;
//!         a = b;
//!         b = next;
//!     }
//! });
//! while let Some(v) = fib.resume(0) {
//!     println!("fib = {v}");
//! }
//! ```
//!
//! The `Coroutine` owns its stack and frees it on drop.
//!
//! Note: the value passed to the *first* `resume` is discarded — the closure
//! only observes sent values as the return of `suspend`, and the first
//! `suspend` returns the value from the *second* `resume`. This matches the
//! usual generator protocol.

extern crate alloc;

use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::boxed::Box;
use core::alloc::Layout;

// ── Context switch ────────────────────────────────────────────────────────────

/// Saved callee-saved register block. `#[repr(C)]` so the field offsets match
/// the hand-written switch in the `global_asm!` below: ra=0, sp=4,
/// s0..s11 = 8..52, and (with the F extension) fs0..fs11 = 56..100.
#[repr(C)]
struct Ctx {
    ra: u32,
    sp: u32,
    s: [u32; 12],
    #[cfg(target_feature = "f")]
    fs: [u32; 12],
}

impl Ctx {
    const fn zeroed() -> Self {
        Ctx {
            ra: 0,
            sp: 0,
            s: [0; 12],
            #[cfg(target_feature = "f")]
            fs: [0; 12],
        }
    }
}

unsafe extern "C" {
    /// Save the current callee-saved state into `*from`, restore it from `*to`,
    /// then return into the restored `ra`/`sp`. Defined in the `global_asm!`.
    fn _raven_coro_switch(from: *mut Ctx, to: *mut Ctx);
}

// Only callee-saved registers are swapped; the compiler spills caller-saved
// registers around the call. The fs0..fs11 block is assembled only when the F
// extension is present (the crate builds with target-feature=+f). Exactly one
// of the two definitions below is compiled, so the symbol is defined once.

#[cfg(target_feature = "f")]
core::arch::global_asm!(
    ".globl _raven_coro_switch",
    "_raven_coro_switch:",
    "sw   ra,    0(a0)", "sw   sp,    4(a0)",
    "sw   s0,    8(a0)", "sw   s1,   12(a0)", "sw   s2,   16(a0)", "sw   s3,   20(a0)",
    "sw   s4,   24(a0)", "sw   s5,   28(a0)", "sw   s6,   32(a0)", "sw   s7,   36(a0)",
    "sw   s8,   40(a0)", "sw   s9,   44(a0)", "sw   s10,  48(a0)", "sw   s11,  52(a0)",
    "fsw  fs0,  56(a0)", "fsw  fs1,  60(a0)", "fsw  fs2,  64(a0)", "fsw  fs3,  68(a0)",
    "fsw  fs4,  72(a0)", "fsw  fs5,  76(a0)", "fsw  fs6,  80(a0)", "fsw  fs7,  84(a0)",
    "fsw  fs8,  88(a0)", "fsw  fs9,  92(a0)", "fsw  fs10, 96(a0)", "fsw  fs11,100(a0)",
    "lw   ra,    0(a1)", "lw   sp,    4(a1)",
    "lw   s0,    8(a1)", "lw   s1,   12(a1)", "lw   s2,   16(a1)", "lw   s3,   20(a1)",
    "lw   s4,   24(a1)", "lw   s5,   28(a1)", "lw   s6,   32(a1)", "lw   s7,   36(a1)",
    "lw   s8,   40(a1)", "lw   s9,   44(a1)", "lw   s10,  48(a1)", "lw   s11,  52(a1)",
    "flw  fs0,  56(a1)", "flw  fs1,  60(a1)", "flw  fs2,  64(a1)", "flw  fs3,  68(a1)",
    "flw  fs4,  72(a1)", "flw  fs5,  76(a1)", "flw  fs6,  80(a1)", "flw  fs7,  84(a1)",
    "flw  fs8,  88(a1)", "flw  fs9,  92(a1)", "flw  fs10, 96(a1)", "flw  fs11,100(a1)",
    "ret",
);

#[cfg(not(target_feature = "f"))]
core::arch::global_asm!(
    ".globl _raven_coro_switch",
    "_raven_coro_switch:",
    "sw   ra,    0(a0)", "sw   sp,    4(a0)",
    "sw   s0,    8(a0)", "sw   s1,   12(a0)", "sw   s2,   16(a0)", "sw   s3,   20(a0)",
    "sw   s4,   24(a0)", "sw   s5,   28(a0)", "sw   s6,   32(a0)", "sw   s7,   36(a0)",
    "sw   s8,   40(a0)", "sw   s9,   44(a0)", "sw   s10,  48(a0)", "sw   s11,  52(a0)",
    "lw   ra,    0(a1)", "lw   sp,    4(a1)",
    "lw   s0,    8(a1)", "lw   s1,   12(a1)", "lw   s2,   16(a1)", "lw   s3,   20(a1)",
    "lw   s4,   24(a1)", "lw   s5,   28(a1)", "lw   s6,   32(a1)", "lw   s7,   36(a1)",
    "lw   s8,   40(a1)", "lw   s9,   44(a1)", "lw   s10,  48(a1)", "lw   s11,  52(a1)",
    "ret",
);

// ── Coroutine state ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ready,
    Suspended,
    Running,
    Done,
}

// Heap-pinned coroutine state. Lives behind a `Box` so its address is stable
// for the duration of the coroutine (the switch and initial trampoline hold raw
// pointers into it). `transfer` is the single slot the in-flight value occupies
// between resume and suspend; it is a `Box<T>` so the slot is a fixed-size
// pointer regardless of how big `T` is.
struct CoroInner<T> {
    ctx: Ctx,
    caller: Ctx,
    state: State,
    transfer: Option<Box<T>>,
    entry: Option<Box<dyn FnMut(&mut Yielder<T>)>>,
    stack_ptr: *mut u8,
    stack_layout: Layout,
}

/// Entered via `ret` from the first switch into a fresh coroutine: `sp` is the
/// coroutine's stack top and the body has never run. `resume` primes `s11`
/// with the heap-pinned `CoroInner<T>` pointer before the first switch; the
/// trampoline reads it back from that callee-saved register so no global
/// single-hart slot is needed.
extern "C" fn coro_trampoline<T>() {
    let inner: *mut CoroInner<T>;
    // SAFETY: `Coroutine::new` stores the `CoroInner<T>` pointer in `s11` for
    // the first entry, and `_raven_coro_switch` restores that saved register
    // block before returning here.
    unsafe { core::arch::asm!("mv {}, s11", out(reg) inner) };
    let mut entry = unsafe { (*inner).entry.take() }.expect("coroutine entry missing");

    let mut y = Yielder { inner };
    entry(&mut y);

    // Body returned: mark done and drop any pending value. The coroutine is
    // Done, so `resume` will never switch back into here.
    unsafe {
        (*inner).state = State::Done;
        (*inner).transfer = None;
        _raven_coro_switch(&mut (*inner).ctx, &mut (*inner).caller);
    }
    loop {} // unreachable
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Handed to the coroutine body; the only way to suspend.
pub struct Yielder<T> {
    inner: *mut CoroInner<T>,
}

impl<T> Yielder<T> {
    /// Suspend the running coroutine, handing `value` back to the resumer (it
    /// becomes the return of [`Coroutine::resume`]). Returns the value passed to
    /// the next `resume`.
    pub fn suspend(&mut self, value: T) -> T {
        // SAFETY: `inner` points at the live, heap-pinned CoroInner currently
        // running on this stack. No `&mut` to it is held across the switch, so
        // the raw accesses do not alias.
        unsafe {
            (*self.inner).transfer = Some(Box::new(value));
            (*self.inner).state = State::Suspended;
            _raven_coro_switch(&mut (*self.inner).ctx, &mut (*self.inner).caller);
            *(*self.inner)
                .transfer
                .take()
                .expect("coroutine resumed without a value")
        }
    }
}

/// A stackful coroutine, generic over the value type `T` exchanged with the
/// body. Owns its stack (freed on drop).
pub struct Coroutine<T> {
    inner: Box<CoroInner<T>>,
}

impl<T: 'static> Coroutine<T> {
    /// Create a coroutine that runs `f` on a fresh `stack_size`-byte stack
    /// (rounded up to a multiple of 16). The coroutine does not start until the
    /// first [`resume`](Coroutine::resume).
    ///
    /// # Panics
    /// Panics if the stack allocation fails (OOM).
    pub fn new<F>(stack_size: usize, f: F) -> Self
    where
        F: FnMut(&mut Yielder<T>) + 'static,
    {
        let size = stack_size.next_multiple_of(16);
        let layout = Layout::from_size_align(size, 16).expect("Coroutine::new: invalid layout");
        // SAFETY: size is non-zero and the layout is valid.
        let stack_ptr = unsafe { alloc_zeroed(layout) };
        assert!(!stack_ptr.is_null(), "Coroutine::new: out of memory");

        let mut inner = Box::new(CoroInner {
            ctx: Ctx::zeroed(),
            caller: Ctx::zeroed(),
            state: State::Ready,
            transfer: None,
            entry: Some(Box::new(f) as Box<dyn FnMut(&mut Yielder<T>)>),
            stack_ptr,
            stack_layout: layout,
        });

        // Prime the context so the first switch `ret`s into the trampoline with
        // sp at the 16-byte-aligned stack top.
        let top = (stack_ptr as usize + size) & !0xf;
        inner.ctx.sp = top as u32;
        inner.ctx.ra = coro_trampoline::<T> as *const () as u32;
        inner.ctx.s[11] = (&mut *inner as *mut CoroInner<T>) as u32;

        Coroutine { inner }
    }

    /// Resume the coroutine, passing `send` in (it becomes the return of the
    /// `suspend` that paused it). Returns `Some(value)` for the value yielded
    /// back, or `None` once the body has finished. Resuming a finished
    /// coroutine returns `None`.
    ///
    /// The `send` of the very first `resume` is discarded (see the module note).
    pub fn resume(&mut self, send: T) -> Option<T> {
        if self.inner.state == State::Done {
            return None;
        }
        self.inner.transfer = Some(Box::new(send));
        self.inner.state = State::Running;

        // SAFETY: both contexts live inside the boxed CoroInner; the switch saves
        // our context into `caller` and enters the coroutine.
        unsafe { _raven_coro_switch(&mut self.inner.caller, &mut self.inner.ctx) };

        // `transfer` now holds the yielded value, or None if the body finished.
        self.inner.transfer.take().map(|boxed| *boxed)
    }

    /// `true` once the coroutine body has returned.
    pub fn done(&self) -> bool {
        self.inner.state == State::Done
    }
}

impl<T> Drop for Coroutine<T> {
    fn drop(&mut self) {
        // SAFETY: allocated in `new` with this exact layout and never freed
        // elsewhere. The Box<CoroInner<T>> frees the rest of the state.
        unsafe { dealloc(self.inner.stack_ptr, self.inner.stack_layout) };
    }
}
