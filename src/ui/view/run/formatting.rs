use super::{App, FormatMode};

/// The cell as the *program* would read it: `MemoryInspect::peek` answers for
/// whichever backend is running, and on one with a write-back cache it reports
/// the dirty line rather than the stale word still in RAM.
pub(super) fn format_memory_value(app: &App, addr: u32) -> String {
    let bytes = app.run.mem_view_bytes;
    let word = app
        .memory()
        .map_or(0, |memory| memory.peek_word(u64::from(addr), bytes as usize));
    match bytes {
        4 => format_u32_value(word as u32, app.run.fmt_mode, app.run.show_signed),
        2 => format_u16_value(word as u16, app.run.fmt_mode, app.run.show_signed),
        _ => format_u8_value(word as u8, app.run.fmt_mode, app.run.show_signed),
    }
}

/// What is still in RAM behind a dirty cache line, shown beside the live value
/// so a write-back is visible as a difference. Only a backend whose cache the
/// host can see through has one; the rest never reach this, because nothing
/// reports a dirty line for them.
fn stale_word(app: &App, addr: u32, bytes: u32) -> u64 {
    app.rv32().map_or(0, |rv32| match bytes {
        4 => u64::from(rv32.mem().peek32(addr).unwrap_or(0)),
        2 => u64::from(rv32.mem().peek16(addr).unwrap_or(0)),
        _ => u64::from(rv32.mem().peek8(addr).unwrap_or(0)),
    })
}

/// Read the raw RAM value (ignoring any dirty cache), for showing the stale value.
pub(super) fn format_stale_value(app: &App, addr: u32) -> String {
    match app.run.mem_view_bytes {
        4 => format_u32_value(
            stale_word(app, addr, 4) as u32,
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        2 => format_u16_value(
            stale_word(app, addr, 2) as u16,
            app.run.fmt_mode,
            app.run.show_signed,
        ),
        _ => format_u8_value(
            stale_word(app, addr, 1) as u8,
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
