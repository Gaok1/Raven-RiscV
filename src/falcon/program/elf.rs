use crate::falcon::{errors::FalconError, memory::Bus};
use super::{load_bytes, zero_bytes};

/// Information about a loaded ELF32 image.
pub struct ElfInfo {
    /// Virtual address of the entry point.
    pub entry:      u32,
    /// Virtual address of the first executable PT_LOAD segment (used as base_pc).
    pub text_base:  u32,
    /// Raw bytes of the executable segment (for the disassembler).
    pub text_bytes: Vec<u8>,
    /// Virtual address of the first non-executable PT_LOAD segment (used as data_base).
    /// Falls back to `entry` when there is no writable segment.
    pub data_base:  u32,
    /// Total file bytes loaded into RAM.
    pub total_bytes: usize,
}

/// Parse and load an ELF32 LE RISC-V executable into `mem`.
///
/// Returns `Err` with a human-readable message on any structural violation.
/// Segment bytes are written to their virtual addresses directly; BSS tails
/// (p_memsz > p_filesz) are zeroed.
pub fn load_elf<B: Bus>(bytes: &[u8], mem: &mut B) -> Result<ElfInfo, FalconError> {
    // ── magic / class / data ─────────────────────────────────────────────
    if bytes.len() < 52 {
        return Err(FalconError::Decode("file too small for ELF32 header"));
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err(FalconError::Decode("not an ELF file (bad magic)"));
    }
    if bytes[4] != 1 {
        return Err(FalconError::Decode("only ELF32 is supported (EI_CLASS != 1)"));
    }
    if bytes[5] != 1 {
        return Err(FalconError::Decode("only little-endian ELF is supported (EI_DATA != 1)"));
    }

    let u16le = |o: usize| u16::from_le_bytes(bytes[o..o+2].try_into().unwrap());
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o+4].try_into().unwrap());

    let e_machine   = u16le(18);
    if e_machine != 0xF3 {
        return Err(FalconError::Decode("not a RISC-V ELF (e_machine != 0xF3)"));
    }

    let e_entry     = u32le(24);
    let e_phoff     = u32le(28) as usize;
    let e_phentsize = u16le(42) as usize;
    let e_phnum     = u16le(44) as usize;

    if e_phentsize < 32 {
        return Err(FalconError::Decode("ELF program header entry too small"));
    }

    // ── iterate PT_LOAD segments ─────────────────────────────────────────
    const PT_LOAD: u32 = 1;
    const PF_X:    u32 = 1;

    let mut text_bytes  = Vec::<u8>::new();
    let mut text_base   = e_entry;
    let mut data_base   = e_entry; // fallback
    let mut total_bytes = 0usize;

    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        if ph + 32 > bytes.len() {
            return Err(FalconError::Decode("ELF program header out of bounds"));
        }

        let p_type   = u32le(ph);
        let p_offset = u32le(ph +  4) as usize;
        let p_vaddr  = u32le(ph +  8);
        let p_filesz = u32le(ph + 16) as usize;
        let p_memsz  = u32le(ph + 20) as usize;
        let p_flags  = u32le(ph + 24);

        if p_type != PT_LOAD { continue; }

        if p_filesz > 0 {
            if p_offset + p_filesz > bytes.len() {
                return Err(FalconError::Decode("ELF segment extends past end of file"));
            }
            let seg = &bytes[p_offset..p_offset + p_filesz];
            load_bytes(mem, p_vaddr, seg)?;
            total_bytes += p_filesz;
        }

        if p_memsz > p_filesz {
            let bss_base = p_vaddr + p_filesz as u32;
            zero_bytes(mem, bss_base, (p_memsz - p_filesz) as u32)?;
        }

        if p_flags & PF_X != 0 && p_filesz > 0 {
            text_bytes = bytes[p_offset..p_offset + p_filesz].to_vec();
            text_base  = p_vaddr;
        } else if data_base == e_entry && p_filesz > 0 {
            data_base = p_vaddr;
        }
    }

    Ok(ElfInfo { entry: e_entry, text_base, text_bytes, data_base, total_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::falcon::Ram;

    fn elf_bytes() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/elf-test/no_std"
        ))
        .expect("elf-test/no_std.bin not found")
    }

    #[test]
    fn elf_header_parsed_correctly() {
        let bytes = elf_bytes();
        let mut mem = Ram::new(128 * 1024);
        let info = load_elf(&bytes, &mut mem).expect("load_elf failed");

        assert_eq!(info.entry, 0x110d4, "wrong entry point");
        // executable segment starts at 0x110d4 (the .text LOAD segment)
        assert_eq!(info.text_base, 0x110d4);
        // first non-exec LOAD segment is at 0x10000
        assert_eq!(info.data_base, 0x10000);
    }

    #[test]
    fn elf_text_segment_loaded_into_ram() {
        let bytes = elf_bytes();
        let mut mem = Ram::new(128 * 1024);
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
        bytes[4] = 1; bytes[5] = 1; // ELFCLASS32, ELFDATA2LSB
        bytes[18..20].copy_from_slice(&0x28u16.to_le_bytes()); // EM_ARM
        let mut mem = Ram::new(64);
        assert!(load_elf(&bytes, &mut mem).is_err());
    }
}
