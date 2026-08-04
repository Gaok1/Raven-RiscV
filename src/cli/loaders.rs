// loaders.rs — file format detection and program loading helpers

use crate::falcon::{CacheController, Cpu};
use crate::riscv32;
use raven_engine::ProgramImage;

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

pub(super) fn load_asm_text(
    bytes: &[u8],
    cpu: &mut Cpu,
    mem: &mut CacheController,
) -> Result<(), String> {
    let text = String::from_utf8_lossy(bytes);
    let image = riscv32::architecture()
        .assembler()
        .assemble(&text, 0)
        .map_err(|e| format!("Assembly error at {e}"))?;
    let mem_size = mem.ram.data_len();
    install(&image, cpu, mem, mem_size)
}

/// Load a FALC container (either version). Images built for another backend are
/// rejected by name so the user knows which `--arch` to pass.
pub(super) fn load_falc(
    bytes: &[u8],
    cpu: &mut Cpu,
    mem: &mut CacheController,
    mem_size: usize,
) -> Result<(), String> {
    let image = riscv32::image_from_binary(bytes, 0).map_err(|e| e.to_string())?;
    install(&image, cpu, mem, mem_size)
}

/// Place `image` in memory and hand the CPU a stack pointer, an entry point and
/// a heap break — the same placement the engine's own RV32 machine uses.
fn install(
    image: &ProgramImage,
    cpu: &mut Cpu,
    mem: &mut CacheController,
    mem_size: usize,
) -> Result<(), String> {
    riscv32::install_image(&mut mem.ram, image).map_err(|e| e.to_string())?;
    cpu.pc = u32::try_from(image.entry).map_err(|_| "entry point exceeds RV32".to_string())?;
    cpu.write(2, mem_size as u32);
    cpu.heap_break = riscv32::heap_break_after(image);
    Ok(())
}
