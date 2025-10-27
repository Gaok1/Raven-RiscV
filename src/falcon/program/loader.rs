use crate::falcon::{memory::Bus, errors::FalconError};

/// Loads words (u32 little-endian) at `base`, contiguously, as code.
pub fn load_words(mem: &mut impl Bus, base: u32, code: &[u32]) -> Result<(), FalconError> {
    let mut addr = base;
    for &w in code {
        mem.store32(addr, w)?;
        addr = addr.wrapping_add(4);
    }
    Ok(())
}

/// Loads raw bytes at `base`.
pub fn load_bytes(mem: &mut impl Bus, base: u32, bytes: &[u8]) -> Result<(), FalconError> {
    let mut addr = base;
    for &b in bytes {
        mem.store8(addr, b)?;
        addr = addr.wrapping_add(1);
    }
    Ok(())
}

/// Zeros a contiguous memory region of `size` bytes at `base`.
pub fn zero_bytes(mem: &mut impl Bus, base: u32, size: u32) -> Result<(), FalconError> {
    let mut addr = base;
    for _ in 0..size {
        mem.store8(addr, 0)?;
        addr = addr.wrapping_add(1);
    }
    Ok(())
}
