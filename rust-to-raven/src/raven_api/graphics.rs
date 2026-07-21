//! Raven graphics API (syscalls 2000+).
//!
//! The framebuffer lives on the host. Drawing calls write to a back buffer;
//! [`screen_present`] publishes it. Colors are `0x00RRGGBB`.

/// No key is pending in [`screen_poll_key`]'s queue.
pub const KEY_NONE: u32 = 0;

/// Common ASCII control key code constants used by [`screen_poll_key`].
pub const KEY_BACKSPACE: u32 = 8;
pub const KEY_TAB: u32 = 9;
pub const KEY_ENTER: u32 = 13;
pub const KEY_ESC: u32 = 27;

/// Printable ASCII key code constants used by [`screen_poll_key`].
///
/// Raven lowercases alphabetic input before placing it in the graphics key
/// queue, so `KEY_A` through `KEY_Z` have the same values as `b'a'..=b'z'`.
pub const KEY_SPACE: u32 = b' ' as u32;
pub const KEY_EXCLAMATION: u32 = b'!' as u32;
pub const KEY_DOUBLE_QUOTE: u32 = b'"' as u32;
pub const KEY_HASH: u32 = b'#' as u32;
pub const KEY_DOLLAR: u32 = b'$' as u32;
pub const KEY_PERCENT: u32 = b'%' as u32;
pub const KEY_AMPERSAND: u32 = b'&' as u32;
pub const KEY_APOSTROPHE: u32 = b'\'' as u32;
pub const KEY_LEFT_PAREN: u32 = b'(' as u32;
pub const KEY_RIGHT_PAREN: u32 = b')' as u32;
pub const KEY_ASTERISK: u32 = b'*' as u32;
pub const KEY_PLUS: u32 = b'+' as u32;
pub const KEY_COMMA: u32 = b',' as u32;
pub const KEY_MINUS: u32 = b'-' as u32;
pub const KEY_PERIOD: u32 = b'.' as u32;
pub const KEY_SLASH: u32 = b'/' as u32;
pub const KEY_0: u32 = b'0' as u32;
pub const KEY_1: u32 = b'1' as u32;
pub const KEY_2: u32 = b'2' as u32;
pub const KEY_3: u32 = b'3' as u32;
pub const KEY_4: u32 = b'4' as u32;
pub const KEY_5: u32 = b'5' as u32;
pub const KEY_6: u32 = b'6' as u32;
pub const KEY_7: u32 = b'7' as u32;
pub const KEY_8: u32 = b'8' as u32;
pub const KEY_9: u32 = b'9' as u32;
pub const KEY_COLON: u32 = b':' as u32;
pub const KEY_SEMICOLON: u32 = b';' as u32;
pub const KEY_LESS_THAN: u32 = b'<' as u32;
pub const KEY_EQUALS: u32 = b'=' as u32;
pub const KEY_GREATER_THAN: u32 = b'>' as u32;
pub const KEY_QUESTION: u32 = b'?' as u32;
pub const KEY_AT: u32 = b'@' as u32;
pub const KEY_A: u32 = b'a' as u32;
pub const KEY_B: u32 = b'b' as u32;
pub const KEY_C: u32 = b'c' as u32;
pub const KEY_D: u32 = b'd' as u32;
pub const KEY_E: u32 = b'e' as u32;
pub const KEY_F: u32 = b'f' as u32;
pub const KEY_G: u32 = b'g' as u32;
pub const KEY_H: u32 = b'h' as u32;
pub const KEY_I: u32 = b'i' as u32;
pub const KEY_J: u32 = b'j' as u32;
pub const KEY_K: u32 = b'k' as u32;
pub const KEY_L: u32 = b'l' as u32;
pub const KEY_M: u32 = b'm' as u32;
pub const KEY_N: u32 = b'n' as u32;
pub const KEY_O: u32 = b'o' as u32;
pub const KEY_P: u32 = b'p' as u32;
pub const KEY_Q: u32 = b'q' as u32;
pub const KEY_R: u32 = b'r' as u32;
pub const KEY_S: u32 = b's' as u32;
pub const KEY_T: u32 = b't' as u32;
pub const KEY_U: u32 = b'u' as u32;
pub const KEY_V: u32 = b'v' as u32;
pub const KEY_W: u32 = b'w' as u32;
pub const KEY_X: u32 = b'x' as u32;
pub const KEY_Y: u32 = b'y' as u32;
pub const KEY_Z: u32 = b'z' as u32;
pub const KEY_LEFT_BRACKET: u32 = b'[' as u32;
pub const KEY_BACKSLASH: u32 = b'\\' as u32;
pub const KEY_RIGHT_BRACKET: u32 = b']' as u32;
pub const KEY_CARET: u32 = b'^' as u32;
pub const KEY_UNDERSCORE: u32 = b'_' as u32;
pub const KEY_BACKTICK: u32 = b'`' as u32;
pub const KEY_LEFT_BRACE: u32 = b'{' as u32;
pub const KEY_PIPE: u32 = b'|' as u32;
pub const KEY_RIGHT_BRACE: u32 = b'}' as u32;
pub const KEY_TILDE: u32 = b'~' as u32;

/// Non-ASCII special key codes delivered by [`screen_poll_key`].
pub const KEY_UP: u32 = 256;
pub const KEY_DOWN: u32 = 257;
pub const KEY_LEFT: u32 = 258;
pub const KEY_RIGHT: u32 = 259;

#[inline(always)]
pub const fn is_ascii_key(key: u32) -> bool {
    key <= 0x7F
}

#[inline(always)]
pub const fn is_printable_ascii_key(key: u32) -> bool {
    key >= KEY_SPACE && key <= KEY_TILDE
}

#[inline(always)]
pub const fn key_to_ascii(key: u32) -> Option<u8> {
    if is_ascii_key(key) {
        Some(key as u8)
    } else {
        None
    }
}

#[inline(always)]
pub const fn key_to_printable_char(key: u32) -> Option<char> {
    if is_printable_ascii_key(key) {
        Some(key as u8 as char)
    } else {
        None
    }
}

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

/// Fill the entire back buffer with a color.
///
/// Accepts `Color`, raw `u32` (`0x00RRGGBB`), `[u8; 3]`, or `(u8, u8, u8)`.
#[inline(always)]
pub fn screen_clear<C: Into<Color>>(color: C) -> i32 {
    let rgb = color.into().to_u32();
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
    screen_clear(color)
}

/// Set one pixel. Returns 0 on success, or -EINVAL when out of bounds.
///
/// Accepts `Color`, raw `u32` (`0x00RRGGBB`), `[u8; 3]`, or `(u8, u8, u8)`.
#[inline(always)]
pub fn screen_set_pixel<C: Into<Color>>(x: u32, y: u32, color: C) -> i32 {
    let rgb = color.into().to_u32();
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
    screen_set_pixel(x, y, color)
}

/// Fill a rectangle in the back buffer. The rectangle is clipped to screen bounds.
///
/// Accepts `Color`, raw `u32` (`0x00RRGGBB`), `[u8; 3]`, or `(u8, u8, u8)`.
#[inline(always)]
pub fn screen_fill_rect<C: Into<Color>>(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: C,
) -> i32 {
    let rgb = color.into().to_u32();
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
    screen_fill_rect(x, y, width, height, color)
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
/// Returns [`KEY_NONE`] when no key is pending. Printable keys are ASCII
/// characters (alphabetic keys are normalized to lowercase), and arrows are
/// [`KEY_UP`], [`KEY_DOWN`], [`KEY_LEFT`], [`KEY_RIGHT`].
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

/// Poll the key queue and return a printable ASCII character, if the pending
/// key is printable. Non-printable/special keys are consumed and return `None`.
#[inline(always)]
pub fn screen_poll_printable_char() -> Option<char> {
    key_to_printable_char(screen_poll_key())
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
