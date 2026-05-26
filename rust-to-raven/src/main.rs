#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use crate::raven_api::{Coroutine, exit};

mod raven_api;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Coroutine demo: a generator that yields 1..=5.
    //
    // The closure runs on its own stack. `y.suspend(i)` hands `i` back to the
    // resumer and pauses; the next `resume` continues right after it, with the
    // stack intact. It's a pure user-space context switch — no ecall, no extra
    // hart.
    let mut counter = Coroutine::new(4096, |y| {
        for i in 1..=5usize {
            y.suspend(i);
        }
    });

    while let Some(v) = counter.resume(0) {
        println!("coroutine yielded {v}");
    }
    println!("coroutine done");

    exit(0)
}
