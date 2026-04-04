#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use crate::raven_api::atomic::{Arc, AtomicU32, Ordering};
use crate::raven_api::{HartTask, exit};

mod raven_api;

// How many times each worker hart will increment the shared counter.
const STEPS: u32 = 5;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Arc lets multiple harts share ownership of the same AtomicU32.
    let counter = Arc::new(AtomicU32::new(0));

    println!("Main hart: spawning two workers.");

    // Each hart gets its own Arc clone — the underlying counter is shared.
    let c1 = counter.clone();
    let c2 = counter.clone();

    let worker1 = HartTask::new(move || {
        for _ in 0..STEPS {
            c1.fetch_add(1, Ordering::Relaxed);
        }
        println!("Worker 1: done ({STEPS} increments).");
    })
    .start()
    .unwrap();

    let worker2 = HartTask::new(move || {
        for _ in 0..STEPS {
            c2.fetch_add(1, Ordering::Relaxed);
        }
        println!("Worker 2: done ({STEPS} increments).");
    })
    .start()
    .unwrap();

    // Block until both workers finish.
    worker1.join();
    worker2.join();

    // Acquire fence so we see every store the workers did.
    let total = counter.load(Ordering::Acquire);
    println!("Main hart: all workers joined. Counter = {total} (expected {}).", STEPS * 2);

    exit(0)
}
