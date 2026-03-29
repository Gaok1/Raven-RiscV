use super::{load_bytes, zero_bytes};
use crate::falcon::{errors::FalconError, memory::Bus};
use std::collections::HashMap;

/// A data/rodata/bss section extracted from an ELF for the sections viewer.
pub struct ElfSection {
    pub name: String,
    pub addr: u32,
    pub size: u32,
    /// Raw bytes from the file (empty for .bss).
    pub bytes: Vec<u8>,
}

/// Information about a loaded ELF32 image.
pub struct ElfInfo {
    /// Virtual address of the entry point.
    pub entry: u32,
    /// Virtual address of the first executable PT_LOAD segment (used as base_pc).
    pub text_base: u32,
    /// Raw bytes of the executable segment (for the disassembler).
    pub text_bytes: Vec<u8>,
    /// Virtual address of the first non-executable PT_LOAD segment (used as data_base).
    /// Falls back to `entry` when there is no writable segment.
    pub data_base: u32,
    /// Total file bytes loaded into RAM.
    pub total_bytes: usize,
    /// First address past all loaded segments — use as initial heap break.
    pub heap_start: u32,
    /// Symbol table: addr → list of names (STT_FUNC / STT_OBJECT, non-empty, non-`$`-prefixed).
    pub symbols: HashMap<u32, Vec<String>>,
    /// Data/rodata/bss sections for the sections viewer.
    pub sections: Vec<ElfSection>,
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
        return Err(FalconError::Decode(
            "only ELF32 is supported (EI_CLASS != 1)",
        ));
    }
    if bytes[5] != 1 {
        return Err(FalconError::Decode(
            "only little-endian ELF is supported (EI_DATA != 1)",
        ));
    }

    let u16le = |o: usize| u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap());
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());

    let e_machine = u16le(18);
    if e_machine != 0xF3 {
        return Err(FalconError::Decode("not a RISC-V ELF (e_machine != 0xF3)"));
    }

    let e_entry = u32le(24);
    let e_phoff = u32le(28) as usize;
    let e_phentsize = u16le(42) as usize;
    let e_phnum = u16le(44) as usize;

    // Section header fields (non-fatal if missing/zero)
    let e_shoff = u32le(32) as usize;
    let e_shentsize = u16le(46) as usize;
    let e_shnum = u16le(48) as usize;
    let e_shstrndx = u16le(50) as usize;

    if e_phentsize < 32 {
        return Err(FalconError::Decode("ELF program header entry too small"));
    }

    // ── iterate PT_LOAD segments ─────────────────────────────────────────
    const PT_LOAD: u32 = 1;
    const PF_X: u32 = 1;

    let mut text_bytes = Vec::<u8>::new();
    let mut text_base = e_entry;
    let mut data_base = e_entry; // fallback
    let mut total_bytes = 0usize;
    let mut seg_end_max = 0u32; // highest byte past all PT_LOAD segments

    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        if ph + 32 > bytes.len() {
            return Err(FalconError::Decode("ELF program header out of bounds"));
        }

        let p_type = u32le(ph);
        let p_offset = u32le(ph + 4) as usize;
        let p_vaddr = u32le(ph + 8);
        let p_filesz = u32le(ph + 16) as usize;
        let p_memsz = u32le(ph + 20) as usize;
        let p_flags = u32le(ph + 24);

        if p_type != PT_LOAD {
            continue;
        }

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
            text_base = p_vaddr;
        } else if data_base == e_entry && p_filesz > 0 {
            data_base = p_vaddr;
        }

        // Track the end of this segment in virtual memory (for heap_start)
        let end = p_vaddr.saturating_add(p_memsz as u32);
        if end > seg_end_max {
            seg_end_max = end;
        }
    }

    // Heap starts right after all segments, 16-byte aligned
    let heap_start = (seg_end_max.wrapping_add(15)) & !15;

    // ── Parse section headers (best-effort, non-fatal) ───────────────────
    let (symbols, sections) = parse_sections(bytes, e_shoff, e_shentsize, e_shnum, e_shstrndx);

    Ok(ElfInfo {
        entry: e_entry,
        text_base,
        text_bytes,
        data_base,
        total_bytes,
        heap_start,
        symbols,
        sections,
    })
}

/// Parse section headers to extract the symbol table and data/rodata/bss sections.
/// Returns empty maps on any structural problem (non-fatal).
fn parse_sections(
    bytes: &[u8],
    e_shoff: usize,
    e_shentsize: usize,
    e_shnum: usize,
    e_shstrndx: usize,
) -> (HashMap<u32, Vec<String>>, Vec<ElfSection>) {
    let mut symbols: HashMap<u32, Vec<String>> = HashMap::new();
    let mut sections: Vec<ElfSection> = Vec::new();

    if e_shoff == 0 || e_shentsize < 40 || e_shnum == 0 {
        return (symbols, sections);
    }

    let u32le = |o: usize| -> Option<u32> {
        bytes
            .get(o..o + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    };
    let u32le_r = |o: usize| -> u32 { u32le(o).unwrap_or(0) };

    // ── Read all section headers into a lightweight cache ─────────────────
    struct Shdr {
        name_off: u32,
        sh_type: u32,
        addr: u32,
        file_off: usize,
        size: usize,
        link: u32,
    }
    let mut shdrs: Vec<Shdr> = Vec::with_capacity(e_shnum);
    for i in 0..e_shnum {
        let base = e_shoff + i * e_shentsize;
        if base + 40 > bytes.len() {
            return (symbols, sections);
        }
        shdrs.push(Shdr {
            name_off: u32le_r(base),
            sh_type: u32le_r(base + 4),
            addr: u32le_r(base + 12),
            file_off: u32le_r(base + 16) as usize,
            size: u32le_r(base + 20) as usize,
            link: u32le_r(base + 24),
        });
    }

    // ── Locate shstrtab (section name string table) ───────────────────────
    let shstrtab: &[u8] = if e_shstrndx < shdrs.len() {
        let s = &shdrs[e_shstrndx];
        if s.file_off + s.size <= bytes.len() {
            &bytes[s.file_off..s.file_off + s.size]
        } else {
            &[]
        }
    } else {
        &[]
    };

    let cstr = |strtab: &[u8], off: usize| -> String {
        let slice = strtab.get(off..).unwrap_or(&[]);
        let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        String::from_utf8_lossy(&slice[..end]).into_owned()
    };

    // ── Find .symtab section ──────────────────────────────────────────────
    const SHT_SYMTAB: u32 = 2;
    const SHT_STRTAB: u32 = 3;
    const STT_NOTYPE: u8 = 0; // plain assembly labels — no type annotation
    const STT_OBJECT: u8 = 1;
    const STT_FUNC: u8 = 2;

    for (i, sh) in shdrs.iter().enumerate() {
        if sh.sh_type == SHT_SYMTAB {
            // Linked .strtab
            let strtab: &[u8] = if (sh.link as usize) < shdrs.len() {
                let st = &shdrs[sh.link as usize];
                if st.sh_type == SHT_STRTAB && st.file_off + st.size <= bytes.len() {
                    &bytes[st.file_off..st.file_off + st.size]
                } else {
                    &[]
                }
            } else {
                &[]
            };

            if sh.file_off + sh.size > bytes.len() {
                continue;
            }
            let sym_data = &bytes[sh.file_off..sh.file_off + sh.size];
            // ELF32 symbol entry = 16 bytes
            let n = sym_data.len() / 16;
            for j in 0..n {
                let o = j * 16;
                let st_name = u32::from_le_bytes(sym_data[o..o + 4].try_into().unwrap()) as usize;
                let st_value = u32::from_le_bytes(sym_data[o + 4..o + 8].try_into().unwrap());
                let st_info = sym_data[o + 12];
                let sym_type = st_info & 0x0F;
                // Include STT_NOTYPE (plain asm labels), STT_OBJECT, STT_FUNC
                if sym_type != STT_FUNC && sym_type != STT_OBJECT && sym_type != STT_NOTYPE {
                    continue;
                }
                if st_value == 0 {
                    continue;
                }
                let name = cstr(strtab, st_name);
                if name.is_empty() || name.starts_with('$') || name.starts_with(".L") {
                    continue;
                }
                symbols.entry(st_value).or_default().push(name);
            }
            let _ = i; // suppress unused warning
        }
    }

    // ── Collect data/rodata/bss sections for the viewer ───────────────────
    for sh in &shdrs {
        if sh.addr == 0 || sh.size == 0 {
            continue;
        }
        let name = cstr(shstrtab, sh.name_off as usize);
        if !is_viewer_section(&name) {
            continue;
        }
        // .bss sections have sh_type=SHT_NOBITS(8), no file bytes
        const SHT_NOBITS: u32 = 8;
        let sec_bytes: Vec<u8> = if sh.sh_type == SHT_NOBITS {
            Vec::new()
        } else if sh.file_off + sh.size <= bytes.len() {
            bytes[sh.file_off..sh.file_off + sh.size].to_vec()
        } else {
            Vec::new()
        };
        sections.push(ElfSection {
            name,
            addr: sh.addr,
            size: sh.size as u32,
            bytes: sec_bytes,
        });
    }
    // Sort by address for stable display
    sections.sort_by_key(|s| s.addr);

    (symbols, sections)
}

/// Returns true for sections that should appear in the sections viewer:
/// .data, .rodata, .bss, and any .data.* / .rodata.* subsections.
/// .text is excluded (already shown in disassembly).
fn is_viewer_section(name: &str) -> bool {
    name == ".data"
        || name.starts_with(".data.")
        || name == ".rodata"
        || name.starts_with(".rodata.")
        || name == ".bss"
        || name.starts_with(".bss.")
}

#[cfg(test)]
#[path = "../../../tests/support/falcon_program_elf.rs"]
mod tests;
