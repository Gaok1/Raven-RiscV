// loaders.rs — file format detection and program loading helpers

use crate::falcon;
use crate::falcon::program::{load_bytes, load_words, zero_bytes};
use crate::falcon::{CacheController, Cpu};

// ── Loaders ───────────────────────────────────────────────────────────────────

pub(super) fn is_elf(b: &[u8]) -> bool {
    b.len() >= 4 && &b[0..4] == b"\x7fELF"
}

pub(super) fn is_falc(b: &[u8]) -> bool {
    b.len() >= 16 && &b[0..4] == b"FALC"
}

pub(super) fn looks_like_text(b: &[u8]) -> bool {
    // Heuristic: if >85% of bytes are printable ASCII or common control chars → text
    if b.is_empty() {
        return false;
    }
    let printable = b
        .iter()
        .filter(|&&c| c >= 32 || c == b'\n' || c == b'\r' || c == b'\t')
        .count();
    printable * 100 / b.len() >= 85
}

pub(super) fn load_asm_text(bytes: &[u8], cpu: &mut Cpu, mem: &mut CacheController) -> Result<(), String> {
    let text = String::from_utf8_lossy(bytes).to_string();
    let prog = falcon::asm::assemble(&text, 0x0)
        .map_err(|e| format!("Assembly error at line {}: {}", e.line + 1, e.msg))?;

    load_words(&mut mem.ram, 0x0, &prog.text).map_err(|e| format!("Load error: {e}"))?;

    if !prog.data.is_empty() {
        load_bytes(&mut mem.ram, prog.data_base, &prog.data)
            .map_err(|e| format!("Data load error: {e}"))?;
    }

    let bss_base = prog.data_base.wrapping_add(prog.data.len() as u32);
    if prog.bss_size > 0 {
        zero_bytes(&mut mem.ram, bss_base, prog.bss_size).map_err(|e| format!("BSS error: {e}"))?;
    }

    cpu.pc = 0x0;
    let bss_end = bss_base.wrapping_add(prog.bss_size);
    cpu.heap_break = (bss_end.wrapping_add(15)) & !15;
    Ok(())
}

pub(super) fn load_falc(
    bytes: &[u8],
    cpu: &mut Cpu,
    mem: &mut CacheController,
    mem_size: usize,
) -> Result<(), String> {
    let text_sz = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let data_sz = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let bss_sz = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let body = &bytes[16..];

    if body.len() < text_sz + data_sz {
        return Err("FALC binary truncated or corrupt".to_string());
    }

    let text_bytes = &body[..text_sz];
    let data_bytes = &body[text_sz..text_sz + data_sz];

    // Text at 0x0, data right after (4-byte aligned)
    let data_base: u32 = ((text_sz as u32).wrapping_add(3)) & !3;

    load_bytes(&mut mem.ram, 0, text_bytes).map_err(|e| format!("Load error: {e}"))?;

    if !data_bytes.is_empty() {
        load_bytes(&mut mem.ram, data_base, data_bytes)
            .map_err(|e| format!("Data load error: {e}"))?;
    }

    if bss_sz > 0 {
        let bss_base = data_base.wrapping_add(data_bytes.len() as u32);
        zero_bytes(&mut mem.ram, bss_base, bss_sz).map_err(|e| format!("BSS error: {e}"))?;
    }

    cpu.pc = 0;
    cpu.write(2, mem_size as u32);

    let bss_end = data_base
        .wrapping_add(data_bytes.len() as u32)
        .wrapping_add(bss_sz);
    cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

    Ok(())
}
