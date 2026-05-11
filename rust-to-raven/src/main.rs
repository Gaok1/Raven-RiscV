#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use crate::raven_api::atomic::{Arc, AtomicU32, Ordering};
use crate::raven_api::{HartTask, exit};

mod raven_api;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let instructions: [u32; 2] = [
        0x00b50533, // add a0, a0, a1
        0x00008067, // ret
    ];

    let sum: fn(i32, i32) -> i32 = unsafe { //inseguro
        raven_api::syscall::map_exec(
            instructions.as_ptr() as usize,
            instructions.len() * core::mem::size_of::<u32>(),
        );
        core::mem::transmute(instructions.as_ptr())
    };

    println!("Hello, world! 2 + 3 = {}", sum(2, 3));

    exit(0)
}
