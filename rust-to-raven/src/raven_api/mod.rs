pub mod io;
pub mod allocator;
pub mod syscall;
pub mod graphics;
pub mod random;
pub mod hardware_thread;
pub mod atomic;
pub mod coroutine;

pub static mut ENABLED_DEBUG_MESSAGES : bool = false;

pub use syscall::{exit, exit_group, getrandom, map_exec, pause_sim, RavenFD};
pub use graphics::{
    rgb, screen_clear, screen_clear_color, screen_fill_rect, screen_fill_rect_color, screen_init,
    screen_poll_key, screen_present, screen_set_pixel, screen_set_pixel_color, screen_sleep_ms,
    screen_time_ms, Color, KEY_BACKSPACE, KEY_DOWN, KEY_ENTER, KEY_ESC, KEY_LEFT, KEY_RIGHT,
    KEY_UP,
};
pub use hardware_thread::hart::{HartHandle, HartTask, alloc_hart_stack, spawn_hart, spawn_hart_fn};
pub use coroutine::{Coroutine, Yielder};
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
