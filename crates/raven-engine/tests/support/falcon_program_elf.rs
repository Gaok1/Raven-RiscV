use super::*;
use crate::falcon::Ram;
use crate::falcon::decoder::decode;
use crate::falcon::instruction::Instruction;

/// The ELF fixture committed under tests/fixtures/elf (its README says how to
/// rebuild it). It lives in the repository so these tests run on a clean
/// checkout, without a rust-to-raven or c-to-raven build lying around.
fn elf_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/elf/fixture.elf")
}

fn elf_bytes() -> Vec<u8> {
    let path = elf_path();
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("failed to read ELF fixture {}: {e}", path.display()))
}

/// Address of `name` in the symbol table `load_elf` returned.
fn symbol_addr(info: &ElfInfo, name: &str) -> u32 {
    info.symbols
        .iter()
        .find(|(_, names)| names.iter().any(|n| n == name))
        .map(|(&addr, _)| addr)
        .unwrap_or_else(|| panic!("symbol `{name}` missing from the ELF symbol table"))
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

    // The disassembler gets the same bytes as RAM, and the word at the entry
    // point is the fixture's first instruction, `li a0, 1`.
    let off = (info.entry - info.text_base) as usize;
    assert_eq!(info.text_bytes[off..off + 4], word.to_le_bytes());
    assert!(
        matches!(decode(word), Ok(Instruction::Addi { rd: 10, rs1: 0, imm: 1 })),
        "entry word 0x{word:08x} is not `li a0, 1`"
    );
}

#[test]
fn elf_data_segment_loaded_and_bss_tail_zeroed() {
    let bytes = elf_bytes();
    let mut mem = Ram::new(1024 * 1024);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");
    let counter = symbol_addr(&info, "counter");
    let scratch = symbol_addr(&info, "scratch");

    // Load again over RAM that is not zero, so a bss tail left alone shows up.
    let mut mem = Ram::new(1024 * 1024);
    for i in 0..16 {
        mem.store8(scratch + i, 0xAA).unwrap();
    }
    load_elf(&bytes, &mut mem).expect("load_elf failed");

    assert_eq!(mem.load32(counter).unwrap(), 41, ".data word not loaded");
    for i in 0..16 {
        assert_eq!(mem.load8(scratch + i).unwrap(), 0, "bss byte {i} not zeroed");
    }
}

#[test]
fn elf_symbols_and_sections_parsed() {
    let bytes = elf_bytes();
    let mut mem = Ram::new(1024 * 1024);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");

    assert_eq!(symbol_addr(&info, "_start"), info.entry);
    assert_eq!(symbol_addr(&info, "bump"), info.text_base);
    assert_eq!(symbol_addr(&info, "greeting"), info.data_base);
    assert!(
        info.symbols.values().flatten().all(|n| !n.starts_with('$')),
        "mapping symbols ($x, $d) should be filtered out"
    );

    let names: Vec<&str> = info.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, [".rodata", ".data", ".bss"]);
    let rodata = &info.sections[0];
    assert_eq!(rodata.addr, info.data_base);
    assert_eq!(rodata.bytes, b"hello from the ELF fixture\n");
    let bss = &info.sections[2];
    assert_eq!(bss.addr, symbol_addr(&info, "scratch"));
    assert_eq!(bss.size, 16);
    assert!(bss.bytes.is_empty(), ".bss has no file bytes");
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
