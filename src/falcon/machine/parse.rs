//! Parse user-entered cell text into a width-bounded value.
//!
//! The one rule that matters for safety: a value that does not fit the target
//! cell is **rejected** (an [`EditError`]) — never silently truncated. The
//! format mirrors the Run tab's display toggles so what the user types is read
//! back the way they see it.

use super::types::{EditError, MemWidth};

/// How to interpret the typed text, matching the Run tab's `fmt_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellFormat {
    /// Base-16, with an optional `0x` / `0X` prefix.
    Hex,
    /// Base-10. Combined with `signed`, accepts a leading `-`.
    Dec,
    /// Raw bytes, packed little-endian (memory cells only).
    Str,
}

/// Parse `input` into the raw little-endian value to store in a `width`-byte
/// cell, rejecting anything that does not fit.
///
/// - **Hex** rejects values above [`MemWidth::max_unsigned`].
/// - **Dec, signed** parses an `i64`, rejects values outside
///   [`MemWidth::signed_range`], then encodes two's-complement into `width`.
/// - **Dec, unsigned** rejects values above [`MemWidth::max_unsigned`].
/// - **Str** packs the bytes little-endian, rejecting text longer than `width`.
pub fn parse_cell(
    input: &str,
    width: MemWidth,
    fmt: CellFormat,
    signed: bool,
) -> Result<u64, EditError> {
    let trimmed = input.trim();
    match fmt {
        CellFormat::Hex => parse_hex(trimmed, width),
        CellFormat::Dec if signed => parse_signed_dec(trimmed, width),
        CellFormat::Dec => parse_unsigned_dec(trimmed, width),
        CellFormat::Str => parse_str(input, width),
    }
}

fn parse_hex(input: &str, width: MemWidth) -> Result<u64, EditError> {
    let digits = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .unwrap_or(input);
    let value = u128::from_str_radix(&without_separators(digits), 16)
        .map_err(|_| EditError::ParseFailed { input: input.to_string() })?;
    fits_unsigned(value, width, false)
}

fn parse_unsigned_dec(input: &str, width: MemWidth) -> Result<u64, EditError> {
    let value: u128 = without_separators(input)
        .parse()
        .map_err(|_| EditError::ParseFailed { input: input.to_string() })?;
    fits_unsigned(value, width, false)
}

fn parse_signed_dec(input: &str, width: MemWidth) -> Result<u64, EditError> {
    let value: i64 = without_separators(input)
        .parse()
        .map_err(|_| EditError::ParseFailed { input: input.to_string() })?;
    let (lo, hi) = width.signed_range();
    if value < lo || value > hi {
        return Err(EditError::OutOfRange { width, signed: true });
    }
    // Two's-complement into the cell width: e.g. -128 in B1 → 0x80.
    Ok((value as u64) & mask(width))
}

fn parse_str(input: &str, width: MemWidth) -> Result<u64, EditError> {
    let bytes = input.as_bytes();
    if bytes.len() > width.bytes() as usize {
        return Err(EditError::OutOfRange { width, signed: false });
    }
    let mut value = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        value |= (b as u64) << (8 * i);
    }
    Ok(value)
}

/// Reject `value` if it exceeds the width's unsigned capacity, else narrow to
/// `u64`.
fn fits_unsigned(value: u128, width: MemWidth, signed: bool) -> Result<u64, EditError> {
    if value > width.max_unsigned() as u128 {
        Err(EditError::OutOfRange { width, signed })
    } else {
        Ok(value as u64)
    }
}

fn mask(width: MemWidth) -> u64 {
    width.max_unsigned()
}

/// Drop `_` digit-group separators so grouped input like `0x1_0000_0000` or
/// `1_000` parses the same way it reads on screen — mirroring Rust literals.
/// Numeric paths only; in a `Str` cell an underscore is a real byte.
fn without_separators(s: &str) -> String {
    s.replace('_', "")
}
