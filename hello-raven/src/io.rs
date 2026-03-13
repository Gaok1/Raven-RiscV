use core::fmt;
use crate::syscall::{sys_write};

// ── Writers ──────────────────────────────────────────────────────────────────

pub struct StdoutWriter;
pub struct StderrWriter;

impl fmt::Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { sys_write(1, s.as_ptr(), s.len()) };
        Ok(())
    }
}

impl fmt::Write for StderrWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe { sys_write(2, s.as_ptr(), s.len()) };
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

// ── Macros ────────────────────────────────────────────────────────────────────

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::io::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    ()            => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => { $crate::io::_eprint(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprintln {
    ()            => { $crate::eprint!("\n") };
    ($($arg:tt)*) => { $crate::eprint!("{}\n", format_args!($($arg)*)) };
}
