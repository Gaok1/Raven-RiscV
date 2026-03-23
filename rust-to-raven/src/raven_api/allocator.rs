use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::NonNull;

use linked_list_allocator::Heap;

use crate::eprintln;
use crate::raven_api::syscall::{brk, exit};

/// Grow the heap in 64 KiB steps. This is just a performance knob — smaller
/// means more brk calls, larger means more wasted memory on small programs.
const HEAP_GROWTH: usize = 64 * 1024;

// SAFETY: Raven is single-threaded — no concurrent access is possible.
struct SyncHeap(UnsafeCell<Heap>);
unsafe impl Sync for SyncHeap {}

static HEAP: SyncHeap = SyncHeap(UnsafeCell::new(Heap::empty()));
static mut HEAP_READY: bool = false;

#[inline(always)]
fn heap() -> &'static mut Heap {
    // SAFETY: single-threaded.
    unsafe { &mut *HEAP.0.get() }
}

/// Initialise the heap with the first `HEAP_GROWTH` chunk from brk.
/// Called at most once.
fn ensure_init() {
    if unsafe { HEAP_READY } {
        return;
    }
    let start  = unsafe { brk(0) };
    let end    = start + HEAP_GROWTH;
    let actual = unsafe { brk(end) };
    if actual < end {
        eprintln!("allocator: initial brk failed");
        exit(1);
    }
    // SAFETY: [start, start+HEAP_GROWTH) is exclusively ours after brk.
    unsafe { heap().init(start as *mut u8, HEAP_GROWTH) };
    unsafe { HEAP_READY = true };
}

/// Ask Raven for more memory. Grows by at least `needed` bytes, rounded up
/// to `HEAP_GROWTH`. Returns false if Raven refuses — that is the real limit.
fn grow(needed: usize) -> bool {
    let by      = needed.next_multiple_of(HEAP_GROWTH);
    let top     = heap().top() as usize;
    let new_top = top + by;
    let actual = unsafe { brk(new_top) };
    if actual < new_top {
        return false; // Raven said no — honour the limit
    }
    // SAFETY: [top, top+by) is exclusively ours after brk returned new_top.
    unsafe { heap().extend(by) };
    true
}

// ── Global allocator ──────────────────────────────────────────────────────────

struct FreeListAlloc;

#[global_allocator]
static ALLOC: FreeListAlloc = FreeListAlloc;

unsafe impl GlobalAlloc for FreeListAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ensure_init();
        // Fast path: enough free space already.
        if let Ok(p) = heap().allocate_first_fit(layout) {
            return p.as_ptr();
        }
        // Slow path: ask Raven for more memory and retry.
        if !grow(layout.size() + layout.align()) {
            return core::ptr::null_mut(); // triggers alloc_error_handler
        }
        heap().allocate_first_fit(layout)
            .map(NonNull::as_ptr)
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: caller guarantees ptr came from a previous alloc call.
        unsafe { heap().deallocate(NonNull::new_unchecked(ptr), layout) }
    }
}

#[alloc_error_handler]
fn oom(layout: Layout) -> ! {
    eprintln!("OOM: size={} align={}", layout.size(), layout.align());
    exit(1)
}
