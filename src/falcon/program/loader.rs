use crate::falcon::memory::Bus;

/// Loads words (u32 little-endian) at `base`, contiguously, as code.
pub fn load_words(mem: &mut impl Bus, base: u32, code: &[u32]) {
    let mut addr = base;
    for &w in code {
        mem.store32(addr, w);
        addr = addr.wrapping_add(4);
    }
}

/// Loads raw bytes at `base`.
pub fn load_bytes(mem: &mut impl Bus, base: u32, bytes: &[u8]) {
    let mut addr = base;
    for &b in bytes {
        mem.store8(addr, b);
        addr = addr.wrapping_add(1);
    }
}
