#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use core::any::Any;
use core::hint::spin_loop;

use crate::raven_api::atomic::{Arc, AtomicBool, AtomicU32, Ordering};
use crate::raven_api::{HartTask, exit};
mod raven_api;

static WORKER_READY: AtomicBool = AtomicBool::new(false);
static WORKER_DONE: AtomicBool = AtomicBool::new(false);

struct SharedCounter {
    total: AtomicU32,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    
    let closure = || {
        println!("Olá tupacão!");
    };

    let task = HartTask::new(closure);

    task.start().unwrap().join();

    println!("Caboou");
    exit(0)

}
