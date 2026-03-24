#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod raven_api;

use crate::raven_api::syscall::{exit, pause_sim};

// Guessing game — demonstrates:
//   read_int!()      parse a signed integer from stdin
//   rand_range!()    random u32 in [lo, hi)
//   println!()       formatted output to stdout
//   eprintln!()      formatted output to stderr (shown in red in Raven)
//   pause_sim()  freeze execution so you can inspect state in Raven

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("=== Guess the number! ===");
    println!("I picked a number between 1 and 100.\n");

    let secret = rand_range!(1u32, 101u32); // [1, 100]
    eprintln!("[debug] secret = {secret}");  // visible on stderr (red in console)

    let mut attempts = 0u32;

    loop {
        print!("Your guess: ");
        let guess = read_int!();
        attempts += 1;

        if !(1..=100).contains(&guess) {
            println!("  Out of range! Try between 1 and 100.");
            continue;
        }

        let guess = guess as u32;

        if guess < secret {
            println!("  Too low!");
        } else if guess > secret {
            println!("  Too high!");
        } else {
            println!("\nCorrect! You got it in {attempts} attempt(s).");
            println!("The number {secret} in binary: {secret:032b}");
            break;
        }
    }

    pause_sim(); // inspect registers and memory before exit
    exit(0);
}
