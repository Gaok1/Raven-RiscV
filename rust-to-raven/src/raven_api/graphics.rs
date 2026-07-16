//! Raven graphics API (syscalls 2000+).
//!
//! The framebuffer lives on the host. Drawing calls write to a back buffer;
//! [`screen_present`] publishes it. Colors are `0x00RRGGBB`.

pub const KEY_ENTER: u32 = 13;
pub const KEY_BACKSPACE: u32 = 8;
pub const KEY_ESC: u32 = 27;
pub const KEY_UP: u32 = 256;
pub const KEY_DOWN: u32 = 257;
pub const KEY_LEFT: u32 = 258;
pub const KEY_RIGHT: u32 = 259;

#[inline(always)]
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Rgb { r: u8, g: u8, b: u8 },
    Bytes([u8; 3]),
    Raw(u32),
}

impl Color {
    #[inline(always)]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }

    #[inline(always)]
    pub const fn bytes(bytes: [u8; 3]) -> Self {
        Self::Bytes(bytes)
    }

    #[inline(always)]
    pub const fn raw(rgb: u32) -> Self {
        Self::Raw(rgb)
    }

    #[inline(always)]
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Rgb { r, g, b } => rgb(r, g, b),
            Self::Bytes([r, g, b]) => rgb(r, g, b),
            Self::Raw(v) => v & 0x00FF_FFFF,
        }
    }
}

impl From<u32> for Color {
    #[inline(always)]
    fn from(value: u32) -> Self {
        Self::Raw(value)
    }
}

impl From<[u8; 3]> for Color {
    #[inline(always)]
    fn from(value: [u8; 3]) -> Self {
        Self::Bytes(value)
    }
}

impl From<(u8, u8, u8)> for Color {
    #[inline(always)]
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Self::Rgb { r, g, b }
    }
}

/// Create a host-side framebuffer of `width` x `height` pixels.
/// Returns 0 on success, or a negative Linux-style errno value.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_init(width: u32, height: u32) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2000_u32,
            in("a0") width,
            in("a1") height,
            lateout("a0") ret,
        );
    }
    ret
}

/// Fill the entire back buffer with `rgb` (`0x00RRGGBB`).
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_clear(rgb: u32) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2001_u32,
            in("a0") rgb,
            lateout("a0") ret,
        );
    }
    ret
}

#[inline(always)]
pub fn screen_clear_color(color: Color) -> i32 {
    screen_clear(color.to_u32())
}

/// Set one pixel. Returns 0 on success, or -EINVAL when out of bounds.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_set_pixel(x: u32, y: u32, rgb: u32) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2002_u32,
            in("a0") x,
            in("a1") y,
            in("a2") rgb,
            lateout("a0") ret,
        );
    }
    ret
}

#[inline(always)]
pub fn screen_set_pixel_color(x: u32, y: u32, color: Color) -> i32 {
    screen_set_pixel(x, y, color.to_u32())
}

/// Fill a rectangle in the back buffer. The rectangle is clipped to screen bounds.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_fill_rect(x: u32, y: u32, width: u32, height: u32, rgb: u32) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2003_u32,
            in("a0") x,
            in("a1") y,
            in("a2") width,
            in("a3") height,
            in("a4") rgb,
            lateout("a0") ret,
        );
    }
    ret
}

#[inline(always)]
pub fn screen_fill_rect_color(x: u32, y: u32, width: u32, height: u32, color: Color) -> i32 {
    screen_fill_rect(x, y, width, height, color.to_u32())
}

/// Publish the back buffer to the display.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_present() -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2004_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// Non-blocking key poll.
///
/// Returns 0 when no key is pending; printable keys are lowercase ASCII and
/// arrows are [`KEY_UP`], [`KEY_DOWN`], [`KEY_LEFT`], [`KEY_RIGHT`].
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_poll_key() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2005_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// Wall-clock milliseconds since [`screen_init`].
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_time_ms() -> u32 {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2006_u32,
            lateout("a0") ret,
        );
    }
    ret
}

/// Frame pacing. Parks the current hart until the wall-clock deadline.
#[unsafe(no_mangle)]
#[inline(always)]
pub fn screen_sleep_ms(ms: u32) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2007_u32,
            in("a0") ms,
            lateout("a0") ret,
        );
    }
    ret
}
