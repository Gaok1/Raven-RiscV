//! Falcon graphics extensions (`a7` in `2000..`) — see `crate::ui::screen`.

use super::abi::{SyscallAbi, SyscallCtx};
use super::errno::LINUX_EINVAL;
use crate::falcon::{errors::FalconError, memory::Bus, registers::Cpu};
use crate::ui::{Console, console::ConsoleColor};

pub const GFX_SCREEN_INIT: u32 = 2000;
pub const GFX_SCREEN_CLEAR: u32 = 2001;
pub const GFX_SET_PIXEL: u32 = 2002;
pub const GFX_FILL_RECT: u32 = 2003;
pub const GFX_PRESENT: u32 = 2004;
pub const GFX_POLL_KEY: u32 = 2005;
pub const GFX_TIME_MS: u32 = 2006;
pub const GFX_SLEEP_MS: u32 = 2007;

pub struct GfxAbi;

impl<B: Bus> SyscallAbi<B> for GfxAbi {
    fn name(&self, code: u32) -> Option<&'static str> {
        Some(match code {
            GFX_SCREEN_INIT => "screen_init",
            GFX_SCREEN_CLEAR => "screen_clear",
            GFX_SET_PIXEL => "screen_set_pixel",
            GFX_FILL_RECT => "screen_fill_rect",
            GFX_PRESENT => "screen_present",
            GFX_POLL_KEY => "screen_poll_key",
            GFX_TIME_MS => "screen_time_ms",
            GFX_SLEEP_MS => "screen_sleep_ms",
            _ => return None,
        })
    }

    fn handle(&self, code: u32, ctx: &mut SyscallCtx<B>) -> Option<Result<bool, FalconError>> {
        <Self as SyscallAbi<B>>::name(self, code)?;
        let cpu = &mut *ctx.cpu;
        let console = &mut *ctx.console;
        Some(match code {
            GFX_SCREEN_INIT => gfx_screen_init(cpu, console),
            GFX_SCREEN_CLEAR => gfx_with_screen(cpu, console, |screen, cpu| {
                screen.clear(cpu.read(10));
                cpu.write(10, 0);
            }),
            GFX_SET_PIXEL => gfx_with_screen(cpu, console, |screen, cpu| {
                let ok = screen.set_pixel(cpu.read(10), cpu.read(11), cpu.read(12));
                cpu.write(10, if ok { 0 } else { LINUX_EINVAL });
            }),
            GFX_FILL_RECT => gfx_with_screen(cpu, console, |screen, cpu| {
                screen.fill_rect(
                    cpu.read(10),
                    cpu.read(11),
                    cpu.read(12),
                    cpu.read(13),
                    cpu.read(14),
                );
                cpu.write(10, 0);
            }),
            GFX_PRESENT => gfx_with_screen(cpu, console, |screen, cpu| {
                screen.present();
                cpu.write(10, 0);
            }),
            GFX_POLL_KEY => gfx_with_screen(cpu, console, |screen, cpu| {
                cpu.write(10, screen.poll_key().unwrap_or(0));
            }),
            GFX_TIME_MS => gfx_with_screen(cpu, console, |screen, cpu| {
                cpu.write(10, screen.time_ms());
            }),
            GFX_SLEEP_MS => gfx_sleep_ms(cpu, console),
            _ => unreachable!("GfxAbi::handle called with unclaimed code"),
        })
    }
}

fn gfx_screen_init(cpu: &mut Cpu, console: &mut Console) -> Result<bool, FalconError> {
    use crate::ui::screen::{MAX_DIM, MIN_DIM, Screen, ScreenTarget};
    let w = cpu.read(10);
    let h = cpu.read(11);
    if !(MIN_DIM..=MAX_DIM).contains(&w) || !(MIN_DIM..=MAX_DIM).contains(&h) {
        console.push_error(format!(
            "screen_init: invalid size {w}x{h} (allowed {MIN_DIM}..={MAX_DIM})"
        ));
        cpu.write(10, LINUX_EINVAL);
        return Ok(true);
    }
    let mut screen = Screen::new(w, h);
    if console.screen_target == ScreenTarget::Window {
        if let Err(e) = screen.open_window() {
            console.push_colored(format!("screen: {e}"), ConsoleColor::Warning);
        }
    }
    console.screen = Some(screen);
    console.screen_uninit_warned = false;
    cpu.write(10, 0);
    Ok(true)
}

/// Runs `f` on the screen device; without a prior `screen_init` returns
/// `-EINVAL` in `a0` (warning once, so a broken frame loop can't flood the
/// console).
fn gfx_with_screen(
    cpu: &mut Cpu,
    console: &mut Console,
    f: impl FnOnce(&mut crate::ui::screen::Screen, &mut Cpu),
) -> Result<bool, FalconError> {
    match console.screen.as_mut() {
        Some(screen) => f(screen, cpu),
        None => {
            if !console.screen_uninit_warned {
                console.screen_uninit_warned = true;
                console.push_error("screen: graphics syscall before screen_init (2000)");
            }
            cpu.write(10, LINUX_EINVAL);
        }
    }
    Ok(true)
}

/// `screen_sleep_ms(a0=ms)` — parks this hart on its `ecall` without blocking
/// the host. First call arms `cpu.sleep_until` (and already writes `a0=0`, so
/// the JIT path — which never re-executes — is also correct); the interpreter
/// re-executes the `ecall` (see `exec.rs`) until the deadline passes and the
/// deadline is cleared.
fn gfx_sleep_ms(cpu: &mut Cpu, console: &mut Console) -> Result<bool, FalconError> {
    use std::time::{Duration, Instant};
    if console.screen.is_none() {
        return gfx_with_screen(cpu, console, |_, _| {});
    }
    match cpu.sleep_until {
        None => {
            let ms = cpu.read(10) as u64;
            cpu.sleep_until = Some(Instant::now() + Duration::from_millis(ms));
            cpu.write(10, 0);
        }
        Some(deadline) if Instant::now() >= deadline => {
            cpu.sleep_until = None;
        }
        Some(_) => {} // still parked; the ecall will re-execute
    }
    Ok(true)
}
