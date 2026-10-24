use super::*;
use crate::falcon::Ram;

fn elf_path() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rels = [
        "rust-to-raven/rust-to-raven.elf",
        "c-to-raven/c-to-raven.elf",
    ];

    for root in manifest_dir.ancestors() {
        for rel in rels {
            let path = root.join(rel);
            if path.exists() {
                return path;
            }
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

/// Code a tiny ELF loads at its entry point: `li a0, 5; halt`.
const TINY_CODE: [u32; 2] = [0x0050_0513, 0x0020_0073];

fn push_shdr(out: &mut Vec<u8>, name: u32, sh_type: u32, offset: usize, size: usize, link: u32) {
    let entsize = if sh_type == 2 { 16 } else { 0 };
    for field in [name, sh_type, 0, 0, offset as u32, size as u32, link, 0, 1, entsize] {
        out.extend_from_slice(&field.to_le_bytes());
    }
}

/// A minimal ELF32 RISC-V executable built in place, so these tests need no
/// fixture: one loadable code segment at `0x1000` and a symbol table holding
/// `symbols`, each a plain label (`STT_NOTYPE`) like riscv-tests' `tohost`.
fn tiny_elf(symbols: &[(&str, u32)]) -> Vec<u8> {
    let mut strtab = vec![0u8];
    let mut symtab = vec![0u8; 16]; // the null symbol
    for &(name, value) in symbols {
        symtab.extend_from_slice(&(strtab.len() as u32).to_le_bytes());
        symtab.extend_from_slice(&value.to_le_bytes());
        symtab.extend_from_slice(&8u32.to_le_bytes()); // st_size
        symtab.extend_from_slice(&[0x10, 0]); // STB_GLOBAL | STT_NOTYPE, st_other
        symtab.extend_from_slice(&1u16.to_le_bytes()); // st_shndx
        strtab.extend_from_slice(name.as_bytes());
        strtab.push(0);
    }
    let shstrtab = b"\0.symtab\0.strtab\0.shstrtab\0";

    // ELF header (52 bytes) and one program header (32), then the contents.
    let mut out = vec![0u8; 52 + 32];
    let text_off = out.len();
    for word in TINY_CODE {
        out.extend_from_slice(&word.to_le_bytes());
    }
    let symtab_off = out.len();
    out.extend_from_slice(&symtab);
    let strtab_off = out.len();
    out.extend_from_slice(&strtab);
    let shstrtab_off = out.len();
    out.extend_from_slice(shstrtab);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    let shoff = out.len();
    push_shdr(&mut out, 0, 0, 0, 0, 0);
    push_shdr(&mut out, 1, 2, symtab_off, symtab.len(), 2); // .symtab → .strtab
    push_shdr(&mut out, 9, 3, strtab_off, strtab.len(), 0); // .strtab
    push_shdr(&mut out, 17, 3, shstrtab_off, shstrtab.len(), 0); // .shstrtab

    let put16 = |out: &mut Vec<u8>, off: usize, v: u16| {
        out[off..off + 2].copy_from_slice(&v.to_le_bytes())
    };
    let put32 = |out: &mut Vec<u8>, off: usize, v: u32| {
        out[off..off + 4].copy_from_slice(&v.to_le_bytes())
    };
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = 1; // ELFCLASS32
    out[5] = 1; // ELFDATA2LSB
    out[6] = 1; // EV_CURRENT
    put16(&mut out, 16, 2); // ET_EXEC
    put16(&mut out, 18, 0xF3); // EM_RISCV
    put32(&mut out, 20, 1);
    put32(&mut out, 24, 0x1000); // e_entry
    put32(&mut out, 28, 52); // e_phoff
    put32(&mut out, 32, shoff as u32);
    put16(&mut out, 40, 52); // e_ehsize
    put16(&mut out, 42, 32); // e_phentsize
    put16(&mut out, 44, 1); // e_phnum
    put16(&mut out, 46, 40); // e_shentsize
    put16(&mut out, 48, 4); // e_shnum
    put16(&mut out, 50, 3); // e_shstrndx

    let code_len = (TINY_CODE.len() * 4) as u32;
    put32(&mut out, 52, 1); // PT_LOAD
    put32(&mut out, 56, text_off as u32);
    put32(&mut out, 60, 0x1000); // p_vaddr
    put32(&mut out, 64, 0x1000); // p_paddr
    put32(&mut out, 68, code_len); // p_filesz
    put32(&mut out, 72, code_len); // p_memsz
    put32(&mut out, 76, 0x5); // PF_R | PF_X
    put32(&mut out, 80, 4); // p_align
    out
}

#[test]
fn tohost_symbol_address_is_reported() {
    let bytes = tiny_elf(&[("_start", 0x1000), ("tohost", 0x1040)]);
    let mut mem = Ram::new(0x4000);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");
    assert_eq!(info.entry, 0x1000);
    assert_eq!(mem.load32(0x1000).unwrap(), TINY_CODE[0]);
    assert_eq!(info.tohost, Some(0x1040));
}

#[test]
fn an_elf_without_tohost_reports_none() {
    let bytes = tiny_elf(&[("_start", 0x1000), ("fromhost", 0x1080)]);
    let mut mem = Ram::new(0x4000);
    let info = load_elf(&bytes, &mut mem).expect("load_elf failed");
    assert!(info.symbols.contains_key(&0x1080));
    assert_eq!(info.tohost, None);
}
