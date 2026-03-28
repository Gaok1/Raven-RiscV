
use super::*;
use crate::falcon::Ram;

fn elf_path() -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for rel in [
        "rust-to-raven/rust-to-raven.elf",
        "c-to-raven/c-to-raven.elf",
    ] {
        let path = root.join(rel);
        if path.exists() {
            return path;
        }
    }
    panic!("no versioned ELF fixture found under rust-to-raven/ or c-to-raven/");
}

fn elf_bytes() -> Vec<u8> {
    std::fs::read(elf_path()).expect("failed to read ELF fixture")
}

fn read_u16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

#[test]
fn elf_header_parsed_correctly() {
    let bytes = elf_bytes();
    let mut mem = Ram::new(1024 * 1024);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");

    let e_entry = read_u32(&bytes, 24);
    let e_phoff = read_u32(&bytes, 28) as usize;
    let e_phentsize = read_u16(&bytes, 42) as usize;
    let e_phnum = read_u16(&bytes, 44) as usize;

    let mut expected_text_base = None;
    let mut expected_data_base = None;
    for i in 0..e_phnum {
        let off = e_phoff + i * e_phentsize;
        let p_type = read_u32(&bytes, off);
        let p_vaddr = read_u32(&bytes, off + 8);
        let p_flags = read_u32(&bytes, off + 24);
        if p_type != 1 {
            continue;
        }
        let is_exec = (p_flags & 0x1) != 0;
        if is_exec && expected_text_base.is_none() {
            expected_text_base = Some(p_vaddr);
        }
        if !is_exec && expected_data_base.is_none() {
            expected_data_base = Some(p_vaddr);
        }
    }

    assert_eq!(info.entry, e_entry, "wrong entry point");
    assert_eq!(
        info.text_base,
        expected_text_base.expect("missing executable PT_LOAD")
    );
    assert_eq!(
        info.data_base,
        expected_data_base.expect("missing non-executable PT_LOAD")
    );
    assert!(
        info.data_base < info.text_base,
        "expected rodata/data PT_LOAD before text PT_LOAD"
    );
}

#[test]
fn elf_text_segment_loaded_into_ram() {
    let bytes = elf_bytes();
    let mut mem = Ram::new(1024 * 1024);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");

    assert!(info.total_bytes > 0);
    // first word at entry point must be readable and non-zero
    let word = mem.load32(info.entry).expect("load32 at entry failed");
    assert_ne!(word, 0, "entry point word is zero — segment not loaded");
}

#[test]
fn bad_magic_returns_error() {
    let mut bytes = vec![0u8; 64];
    bytes[0..4].copy_from_slice(b"NOPE");
    let mut mem = Ram::new(64);
    assert!(load_elf(&bytes, &mut mem).is_err());
}

#[test]
fn wrong_machine_returns_error() {
    // Build a minimal ELF32 LE header with e_machine = 0x28 (ARM)
    let mut bytes = vec![0u8; 52];
    bytes[0..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 1;
    bytes[5] = 1; // ELFCLASS32, ELFDATA2LSB
    bytes[18..20].copy_from_slice(&0x28u16.to_le_bytes()); // EM_ARM
    let mut mem = Ram::new(64);
    assert!(load_elf(&bytes, &mut mem).is_err());
}
