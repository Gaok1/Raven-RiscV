use core::fmt;

use crate::raven_api::syscall::{read, write};
use crate::raven_api::syscall::RavenFD;

// ── Writers ──────────────────────────────────────────────────────────────────

pub struct StdoutWriter;
pub struct StderrWriter;

impl fmt::Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { write(super::syscall::RavenFD::STDOUT, s.as_ptr(), s.len()) };
        Ok(())
    }
}

impl fmt::Write for StderrWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { write(super::syscall::RavenFD::STDERR, s.as_ptr(), s.len()) };
        Ok(())
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    StdoutWriter.write_fmt(args).ok();
}

#[doc(hidden)]
pub fn _eprint(args: fmt::Arguments) {
    use fmt::Write;
    StderrWriter.write_fmt(args).ok();
}

// ── Reader ────────────────────────────────────────────────────────────────────

/// Reads bytes from stdin into `buf` until a newline (`\n`) or the buffer is
/// full. The newline is **not** included. Returns the number of bytes written
/// into `buf`.
#[doc(hidden)]
pub fn _read_line(buf: &mut [u8]) -> usize {
    let mut n = 0;
    for slot in buf.iter_mut() {
        let mut byte = 0u8;
        let ret = unsafe { read(RavenFD::STDIN, &mut byte as *mut u8, 1) };
        if ret <= 0 || byte == b'\n' {
            break;
        }
        *slot = byte;
        n += 1;
    }
    n
}

// ── Integer readers ───────────────────────────────────────────────────────────

/// Parse a signed decimal integer from one line of stdin.
#[doc(hidden)]
pub fn _read_int() -> i32 {
    let mut buf = [0u8; 24];
    let n = _read_line(&mut buf);
    let s = &buf[..n];
    let (neg, digits) = if s.first() == Some(&b'-') { (true, &s[1..]) } else { (false, s) };
    let v = digits.iter().take_while(|&&b| b.is_ascii_digit())
        .fold(0i32, |acc, &b| acc * 10 + (b - b'0') as i32);
    if neg { -v } else { v }
}

/// Parse an unsigned decimal integer from one line of stdin.
#[doc(hidden)]
pub fn _read_uint() -> u32 {
    let mut buf = [0u8; 24];
    let n = _read_line(&mut buf);
    buf[..n].iter().take_while(|&&b| b.is_ascii_digit())
        .fold(0u32, |acc, &b| acc * 10 + (b - b'0') as u32)
}

// ── Macros ────────────────────────────────────────────────────────────────────

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::raven_api::io::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    ()            => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

/// Reads a line from stdin into a `&mut [u8]` buffer.
/// Returns the number of bytes read (newline excluded).
///
/// ```no_run
/// let mut buf = [0u8; 64];
/// let n = read_line!(buf);
/// let s = core::str::from_utf8(&buf[..n]).unwrap_or("");
/// ```
#[macro_export]
macro_rules! read_line {
    ($buf:expr) => { $crate::raven_api::io::_read_line(&mut $buf) };
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => { $crate::raven_api::io::_eprint(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprintln {
    ()            => { $crate::eprint!("\n") };
    ($($arg:tt)*) => { $crate::eprint!("{}\n", format_args!($($arg)*)) };
}

/// Reads a signed decimal integer from one line of stdin.
#[macro_export]
macro_rules! read_int {
    () => { $crate::raven_api::io::_read_int() };
}

/// Reads an unsigned decimal integer from one line of stdin.
#[macro_export]
macro_rules! read_uint {
    () => { $crate::raven_api::io::_read_uint() };
}
