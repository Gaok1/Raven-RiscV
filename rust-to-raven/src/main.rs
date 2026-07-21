#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use crate::raven_api::{Color, Coroutine, KEY_DOWN, KEY_UP, exit};
mod raven_api;

use raven_api::graphics;

struct Player {
    x:u32,
    y:u32,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    graphics::screen_init(200, 200);
    graphics::screen_clear(Color::rgb(1, 1, 42));
    let mut p = Player { x: 100, y: 100 };

    loop{
        match graphics::screen_poll_key() {
            KEY_UP => {
                if p.y > 0 {
                    p.y -= 1;
                }
            },
            KEY_DOWN => {
                if p.y < 200 {
                    p.y += 1;
                }
            },

            _ => {}
        }

        
    }

    exit(0);
}