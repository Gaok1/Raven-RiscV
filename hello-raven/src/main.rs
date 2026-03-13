#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod allocator;
mod io;
mod syscall;

use alloc::vec::Vec;
use alloc::string::String;
use syscall::sys_exit;

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main()
}

// ── Examples ──────────────────────────────────────────────────────────────────

fn factorial(n: u32) -> u32 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}

fn fib(n: u32) -> u32 {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

fn main() -> ! {
    // Compute results into Vec (heap allocation via brk)
    let facts: Vec<(u32, u32)> = (0u32..=12).map(|i| (i, factorial(i))).collect();
    let fibs:  Vec<(u32, u32)> = (0u32..=20).map(|i| (i, fib(i))).collect();

    // Build titles with String to exercise the allocator
    let header_fact = String::from("Factorial");
    let header_fib  = String::from("Fibonacci");

    println!("┌─────────────────────────────┐");
    println!("│{:^29}│", header_fact);
    println!("├──────┬──────────────────────┤");
    for (n, v) in &facts {
        println!("│ {:>4} │ {:>20} │", n, v);
    }
    println!("└──────┴──────────────────────┘");

    println!();

    println!("┌─────────────────────────────┐");
    println!("│{:^29}│", header_fib);
    println!("├──────┬──────────────────────┤");
    for (n, v) in &fibs {
        println!("│ {:>4} │ {:>20} │", n, v);
    }
    println!("└──────┴──────────────────────┘");

    sys_exit(0)
}

// ── Panic handler ─────────────────────────────────────────────────────────────

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    eprintln!("panic: {}", info);
    sys_exit(101)
}
