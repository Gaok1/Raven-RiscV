use super::{App, FormatMode};

pub(super) fn format_memory_value(app: &App, addr: u32) -> String {
    // Use effective_read* which returns dirty D-cache values if present,
    // so write-back stores are visible in the RUN tab memory view.
    match app.run.mem_view_bytes {
        4 => format_u32_value(
            app.run.mem().effective_read32(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        2 => format_u16_value(
            app.run.mem().effective_read16(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        _ => format_u8_value(
            app.run.mem().effective_read8(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
    }
}

/// Read the raw RAM value (ignoring any dirty cache), for showing the stale value.
pub(super) fn format_stale_value(app: &App, addr: u32) -> String {
    match app.run.mem_view_bytes {
        4 => format_u32_value(
            app.run.mem().peek32(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        2 => format_u16_value(
            app.run.mem().peek16(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        _ => format_u8_value(
            app.run.mem().peek8(addr).unwrap_or(0),
            app.run.fmt_mode,
            app.run.show_signed,
        ),
    }
}

/// Format a register of any declared width, so the sidebar can draw an 8-bit
/// SAP accumulator and a 32-bit RV32 register with the same code.
///
/// Widths are padded to their own size rather than a fixed 32 bits — an 8-bit
/// register shown as `0x0000002a` would misstate how wide it is.
pub(super) fn format_register_value(
    value: u64,
    bits: u8,
    fmt: FormatMode,
    show_signed: bool,
) -> String {
    let bits = bits.clamp(1, 64);
    let hex_width = usize::from(bits).div_ceil(4);
    let width = usize::from(bits);
    match fmt {
        FormatMode::Hex => format!("0x{value:0hex_width$x}"),
        FormatMode::Dec if show_signed => format!("{}", sign_extend(value, bits)),
        FormatMode::Dec => format!("{value}"),
        FormatMode::Bin => format!("0b{value:0width$b}"),
        FormatMode::Str => ascii_bytes(&value.to_le_bytes()[..usize::from(bits).div_ceil(8)]),
    }
}

/// Reinterpret the low `bits` of `value` as a two's-complement signed number.
fn sign_extend(value: u64, bits: u8) -> i64 {
    if bits >= 64 {
        return value as i64;
    }
    let shift = 64 - u32::from(bits);
    ((value << shift) as i64) >> shift
}

pub(super) fn format_u32_value(value: u32, fmt: FormatMode, show_signed: bool) -> String {
    match fmt {
        FormatMode::Hex => format!("0x{value:08x}"),
        FormatMode::Dec => match show_signed {
            true => format!("{}", value as i32),
            false => format!("{value}"),
        },
        FormatMode::Bin => format!("0b{value:032b}"),
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
        FormatMode::Bin => format!("0b{value:016b}"),
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
        FormatMode::Bin => format!("0b{value:08b}"),
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
