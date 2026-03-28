pub mod io;
pub mod allocator;
pub mod syscall;
pub mod random;
pub mod hart;

pub static mut ENABLED_DEBUG_MESSAGES : bool = false;

pub use syscall::{exit, exit_group, getrandom, pause_sim, RavenFD};
pub use hart::{HartHandle, HartTask, alloc_hart_stack, spawn_hart, spawn_hart_fn};
pub use random::{rand_u32, rand_u8, rand_i32, rand_range, rand_bool};

use crate::eprintln;

#[unsafe(no_mangle)]
pub fn print_debug(mssg: &str) {
    unsafe {
        if(ENABLED_DEBUG_MESSAGES){
            eprintln!("[DEBUG]: {mssg}");
        }
    }
}
