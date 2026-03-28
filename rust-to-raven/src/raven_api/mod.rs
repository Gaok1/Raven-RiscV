pub mod io;
pub mod allocator;
pub mod syscall;
pub mod random;
pub mod hart;

pub static mut ENABLED_DEBUG_MESSAGES : bool = false;

pub use syscall::{exit, exit_group, getrandom, hart_start, hart_exit, pause_sim, RavenFD};
pub use hart::{spawn_hart, spawn_hart_fn};
pub use random::{rand_u32, rand_u8, rand_i32, rand_range, rand_bool};

use crate::eprintln;

pub fn print_debug(mssg: &str) {
    unsafe {
        if(ENABLED_DEBUG_MESSAGES){
            eprintln!("[DEBUG]: {mssg}");
        }
    }
}