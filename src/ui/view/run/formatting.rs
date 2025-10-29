use crate::falcon::memory::Bus;

use super::{App, FormatMode};

pub(super) fn format_memory_value(app: &App, addr: u32) -> String {
    match app.mem_view_bytes {
        4 => format_u32_value(
            app.mem.load32(addr).unwrap_or(0),
            app.fmt_mode,
            app.show_signed,
        ),
        2 => format_u16_value(
            app.mem.load16(addr).unwrap_or(0),
            app.fmt_mode,
            app.show_signed,
        ),
        _ => format_u8_value(
            app.mem.load8(addr).unwrap_or(0),
            app.fmt_mode,
            app.show_signed,
        ),
    }
}

pub(super) fn format_u32_value(value: u32, fmt: FormatMode, show_signed: bool) -> String {
    match fmt {
        FormatMode::Hex => format!("0x{value:08x}"),
        FormatMode::Dec => match show_signed {
            true => format!("{}", value as i32),
            false => format!("{value}"),
        },
        FormatMode::Str => ascii_bytes(&value.to_le_bytes()),
    }
}

pub(super) fn format_u16_value(value: u16, fmt: FormatMode, show_signed: bool) -> String {
    match fmt {
        FormatMode::Hex => format!("0x{value:04x}"),
        FormatMode::Dec => match show_signed {
            true => format!("{}", value as i16),
            false => format!("{value}"),
        },
        FormatMode::Str => ascii_bytes(&value.to_le_bytes()),
    }
}

pub(super) fn format_u8_value(value: u8, fmt: FormatMode, show_signed: bool) -> String {
    match fmt {
        FormatMode::Hex => format!("0x{value:02x}"),
        FormatMode::Dec => match show_signed {
            true => format!("{}", value as i8),
            false => format!("{value}"),
        },
        FormatMode::Str => ascii_bytes(&[value]),
    }
}

pub(super) fn ascii_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            b if b.is_ascii_graphic() || b == b' ' => b as char,
            _ => '.',
        })
        .collect()
}
