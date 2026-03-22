pub mod io;
pub mod allocator;
pub mod syscall;
pub mod random;

pub static mut ENABLED_DEBUG_MESSAGES : bool = false;

pub use syscall::{sys_exit, sys_exit_group, sys_getrandom, sys_pause_sim, RavenFD};
pub use random::{rand_u32, rand_u8, rand_i32, rand_range, rand_bool};

use crate::eprintln;

pub fn print_debug(mssg: &str) {
    unsafe {
        if(ENABLED_DEBUG_MESSAGES){
            eprintln!("[DEBUG]: {mssg}");
        }
    }
}