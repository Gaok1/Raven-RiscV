use crate::falcon::memory::Bus;

/// Carrega palavras (u32 little-endian) em `base`, contíguas, como código.
pub fn load_words(mem: &mut impl Bus, base: u32, code: &[u32]) {
    let mut addr = base;
    for &w in code {
        mem.store32(addr, w);
        addr = addr.wrapping_add(4);
    }
}

/// Carrega bytes crus em `base`.
pub fn load_bytes(mem: &mut impl Bus, base: u32, bytes: &[u8]) {
    let mut addr = base;
    for &b in bytes {
        mem.store8(addr, b);
        addr = addr.wrapping_add(1);
    }
}
