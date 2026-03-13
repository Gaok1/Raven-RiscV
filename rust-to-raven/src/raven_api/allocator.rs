use core::alloc::{GlobalAlloc, Layout};

use crate::{eprintln, raven_api::syscall::{sys_brk, sys_exit}};


/// Bump allocator backed by the Linux `brk` syscall.
///
/// Every allocation advances the program break forward; `dealloc` is a no-op.
/// Suitable for programs where total heap usage is bounded and predictable.
struct BumpAlloc;

#[global_allocator]
static ALLOC: BumpAlloc = BumpAlloc;

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Query current break, align up, then extend.
        let current = unsafe { sys_brk(0) };
        let aligned = current.wrapping_add(layout.align() - 1) & !(layout.align() - 1);
        let new_brk = aligned.wrapping_add(layout.size());
        let actual  = unsafe { sys_brk(new_brk) };
        if actual < new_brk { core::ptr::null_mut() } else { aligned as *mut u8 }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // bump allocator: memory is never freed
    }
}

#[alloc_error_handler]
fn oom(layout: Layout) -> ! {
    
    eprintln!("OOM: size={} align={}", layout.size(), layout.align());
    sys_exit(1)
}
