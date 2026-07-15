//! Unit tests for `falcon::machine` — included from `machine/mod.rs`.
//!
//! These prove the journal round-trips (step-back is the exact inverse of a
//! step or edit) and that overflowing edits are rejected, all before any of
//! this is wired into the UI.

use super::parse::{parse_cell, CellFormat};
use super::types::{EditError, FRegId, MemWidth, RegId, RegTarget};
use super::{Machine, NoPipeline};

use crate::falcon::cache::{CacheConfig, CacheController};
use crate::falcon::encoder::encode;
use crate::falcon::instruction::Instruction;
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

const RAM_SIZE: usize = 256;
/// Data scratch address used by the load/store programs below.
const DATA_ADDR: u32 = 0x40;

/// Build a machine with `program` loaded at PC 0 and the given D-cache config.
/// Instructions are written straight to RAM (as a loader would), so the
/// I-cache fetches them cleanly.
fn machine_with(program: &[Instruction], dcfg: CacheConfig) -> Machine {
    let mut mem = CacheController::new(CacheConfig::default(), dcfg, vec![], RAM_SIZE);
    for (i, inst) in program.iter().enumerate() {
        let word = encode(*inst).expect("encodable instruction");
        mem.ram_mut()
            .store32(i as u32 * 4, word)
            .expect("in-bounds program word");
    }
    Machine::new(Cpu::default(), mem, NoPipeline)
}

/// A small program: set a base pointer, compute a value, store it, load it back.
fn load_store_program() -> Vec<Instruction> {
    vec![
        Instruction::Addi { rd: 2, rs1: 0, imm: DATA_ADDR as i32 }, // x2 = 0x40
        Instruction::Addi { rd: 1, rs1: 0, imm: 5 },                // x1 = 5
        Instruction::Sw { rs2: 1, rs1: 2, imm: 0 },                 // mem[x2] = x1
        Instruction::Lw { rd: 3, rs1: 2, imm: 0 },                  // x3 = mem[x2]
    ]
}

/// A comparable view of architectural CPU state.
fn fingerprint(cpu: &Cpu) -> (u32, [u32; 32], u64) {
    let mut regs = [0u32; 32];
    for (i, slot) in regs.iter_mut().enumerate() {
        *slot = cpu.read(i as u8);
    }
    (cpu.pc, regs, cpu.instr_count)
}

fn write_through_dcache() -> CacheConfig {
    CacheConfig {
        write_policy: crate::falcon::cache::WritePolicy::WriteThrough,
        ..CacheConfig::default()
    }
}

#[test]
fn step_then_stepback_is_identity() {
    let program = load_store_program();
    let mut m = machine_with(&program, CacheConfig::default());
    let mut console = Console::default();

    let cpu0 = fingerprint(m.cpu());
    let mem0 = m.mem().effective_read32(DATA_ADDR).unwrap();

    for _ in 0..program.len() {
        m.step_interpreted(&mut console).unwrap();
    }
    // The program actually changed something.
    assert_eq!(m.cpu().read(3), 5, "lw should have loaded the stored value");
    assert_ne!(fingerprint(m.cpu()), cpu0);

    while m.can_stepback() {
        assert!(m.stepback().is_some());
    }
    assert_eq!(fingerprint(m.cpu()), cpu0, "CPU must round-trip to its start");
    assert_eq!(
        m.mem().effective_read32(DATA_ADDR).unwrap(),
        mem0,
        "effective memory must round-trip to its start"
    );
}

#[test]
fn stepback_restores_memory_store() {
    // Write-through so the store reaches RAM and `peek32` (raw RAM) observes it
    // — this exercises the byte-level pre-image log specifically.
    let program = load_store_program();
    let mut m = machine_with(&program, write_through_dcache());
    let mut console = Console::default();

    for _ in 0..program.len() {
        m.step_interpreted(&mut console).unwrap();
    }
    assert_eq!(m.mem().peek32(DATA_ADDR).unwrap(), 5, "store reached RAM");

    while m.can_stepback() {
        m.stepback();
    }
    assert_eq!(m.mem().peek32(DATA_ADDR).unwrap(), 0, "RAM reverted to zero");
}

#[test]
fn write_reg_journaled_undo() {
    let mut m = machine_with(&[], CacheConfig::default());
    let x5 = RegTarget::X(RegId::new(5).unwrap());

    m.write_reg(x5, 0xDEAD).unwrap();
    assert_eq!(m.cpu().read(5), 0xDEAD);

    assert!(m.stepback().is_some());
    assert_eq!(m.cpu().read(5), 0, "register reverted");
    assert!(!m.can_stepback());
}

#[test]
fn write_pc_and_freg_journaled_undo() {
    let mut m = machine_with(&[], CacheConfig::default());

    m.write_reg(RegTarget::Pc, 0x80).unwrap();
    m.write_freg(FRegId::new(3).unwrap(), 0x4048_F5C3); // 3.14f bits
    assert_eq!(m.cpu().pc, 0x80);
    assert_eq!(m.cpu().fread_bits(3), 0x4048_F5C3);

    m.stepback(); // undo freg
    assert_eq!(m.cpu().fread_bits(3), 0);
    m.stepback(); // undo pc
    assert_eq!(m.cpu().pc, 0);
}

#[test]
fn write_reg_x0_rejected() {
    let mut m = machine_with(&[], CacheConfig::default());
    let x0 = RegTarget::X(RegId::new(0).unwrap());

    assert_eq!(m.write_reg(x0, 0xFFFF), Err(EditError::X0Immutable));
    assert_eq!(m.cpu().read(0), 0, "x0 stays zero");
    assert!(!m.can_stepback(), "a rejected edit journals nothing");
}

#[test]
fn write_mem_edit_journaled_undo() {
    let mut m = machine_with(&[], write_through_dcache());

    m.write_mem(DATA_ADDR, MemWidth::B4, 0xCAFE_BABE).unwrap();
    assert_eq!(m.mem().effective_read32(DATA_ADDR).unwrap(), 0xCAFE_BABE);

    m.stepback();
    assert_eq!(m.mem().effective_read32(DATA_ADDR).unwrap(), 0, "edit undone");
}

#[test]
fn instr_word_edit_visible_to_fetch() {
    // Editing the instruction word in memory changes what a later fetch decodes.
    let original = encode(Instruction::Addi { rd: 1, rs1: 0, imm: 5 }).unwrap();
    let replacement = encode(Instruction::Addi { rd: 1, rs1: 0, imm: 9 }).unwrap();
    let mut m = machine_with(&[Instruction::Addi { rd: 1, rs1: 0, imm: 5 }], write_through_dcache());

    assert_eq!(m.mem().peek32(0).unwrap(), original);
    m.write_mem(0, MemWidth::B4, replacement as u64).unwrap();
    assert_eq!(m.mem().effective_read32(0).unwrap(), replacement, "fetch sees new word");

    let mut console = Console::default();
    m.step_interpreted(&mut console).unwrap();
    assert_eq!(m.cpu().read(1), 9, "edited immediate took effect");
}

#[test]
fn instr_word_edit_stepback_restores_original() {
    // The imem inline editor commits through `write_mem`, so undoing past the
    // edit must bring back the original instruction word and its effects.
    let original = encode(Instruction::Addi { rd: 1, rs1: 0, imm: 5 }).unwrap();
    let replacement = encode(Instruction::Addi { rd: 1, rs1: 0, imm: 9 }).unwrap();
    let mut m = machine_with(&[Instruction::Addi { rd: 1, rs1: 0, imm: 5 }], write_through_dcache());
    let mut console = Console::default();

    m.write_mem(0, MemWidth::B4, replacement as u64).unwrap();
    m.step_interpreted(&mut console).unwrap();
    assert_eq!(m.cpu().read(1), 9, "edited instruction executed");

    m.stepback(); // undo the step
    m.stepback(); // undo the edit
    assert_eq!(m.mem().peek32(0).unwrap(), original, "original word restored");
    assert_eq!(m.cpu().read(1), 0, "register effect of the step undone");
    assert_eq!(m.cpu().pc, 0);
}

#[test]
fn checkpoint_restores_full_state() {
    // A checkpoint is the only way to rewind writes that bypass the byte log.
    let mut m = machine_with(&[], write_through_dcache());
    m.write_mem(DATA_ADDR, MemWidth::B4, 0x1111_1111).unwrap();

    m.checkpoint();
    // Mutate via the unjournaled hatch (as a GO burst would).
    m.mem_mut_unjournaled()
        .ram_mut()
        .store32(DATA_ADDR, 0x2222_2222)
        .unwrap();
    assert_eq!(m.mem().peek32(DATA_ADDR).unwrap(), 0x2222_2222);

    assert!(m.stepback().is_some(), "step back to the checkpoint");
    assert_eq!(m.mem().peek32(DATA_ADDR).unwrap(), 0x1111_1111);
}

#[test]
fn stepback_reports_what_it_undid() {
    // The UI keys its post-undo bookkeeping off this kind, so each path must
    // report itself: a step, an edit, and a checkpoint, newest-first.
    use super::StepbackKind;
    let mut m = machine_with(&load_store_program(), write_through_dcache());
    let mut console = Console::default();

    m.step_interpreted(&mut console).unwrap(); // one instruction
    m.write_reg(RegTarget::X(RegId::new(5).unwrap()), 0xABCD).unwrap(); // an edit
    m.checkpoint(); // a burst boundary

    assert_eq!(m.stepback(), Some(StepbackKind::Checkpoint));
    assert_eq!(m.stepback(), Some(StepbackKind::Edit));
    assert_eq!(m.stepback(), Some(StepbackKind::Step));
    assert_eq!(m.stepback(), None, "journal is empty");
}

#[test]
fn journal_ring_bounded() {
    let mut m = machine_with(&[], CacheConfig::default());
    let x5 = RegTarget::X(RegId::new(5).unwrap());
    for i in 0..2000u32 {
        m.write_reg(x5, i).unwrap();
    }
    assert_eq!(m.journal_depth(), 1024, "oldest entries evicted at capacity");
}

#[test]
fn parse_cell_overflow_matrix() {
    use CellFormat::{Dec, Hex};
    use MemWidth::{B1, B4};

    assert_eq!(
        parse_cell("0x1FF", B1, Hex, false),
        Err(EditError::OutOfRange { width: B1, signed: false })
    );
    assert_eq!(
        parse_cell("256", B1, Dec, false),
        Err(EditError::OutOfRange { width: B1, signed: false })
    );
    assert_eq!(
        parse_cell("-129", B1, Dec, true),
        Err(EditError::OutOfRange { width: B1, signed: true })
    );
    assert_eq!(parse_cell("-128", B1, Dec, true), Ok(0x80));
    assert_eq!(parse_cell("0xFF", B1, Hex, false), Ok(0xFF));
    // `_` digit-group separators are accepted in numeric input.
    assert_eq!(parse_cell("1_000", B4, Dec, false), Ok(1000));
    assert_eq!(parse_cell("0xFFFFFFFF", B4, Hex, false), Ok(0xFFFF_FFFF));
    assert!(matches!(
        parse_cell("0x1_0000_0000", B4, Hex, false),
        Err(EditError::OutOfRange { .. })
    ));
    assert!(matches!(parse_cell("xyz", B4, Hex, false), Err(EditError::ParseFailed { .. })));
}

#[test]
fn parse_cell_binary() {
    use CellFormat::Bin;
    use MemWidth::{B1, B4};

    assert_eq!(parse_cell("1010", B1, Bin, false), Ok(0b1010));
    assert_eq!(parse_cell("0b1111_1111", B1, Bin, false), Ok(0xFF));
    assert_eq!(
        parse_cell("1_0000_0000", B1, Bin, false), // 9 bits → too wide for B1
        Err(EditError::OutOfRange { width: B1, signed: false })
    );
    assert_eq!(parse_cell("0b0", B4, Bin, false), Ok(0));
    assert!(matches!(
        parse_cell("012", B4, Bin, false), // '2' is not a binary digit
        Err(EditError::ParseFailed { .. })
    ));
}

#[test]
fn parse_cell_str_packs_little_endian() {
    assert_eq!(parse_cell("AB", MemWidth::B2, CellFormat::Str, false), Ok(0x4241));
    assert!(matches!(
        parse_cell("ABC", MemWidth::B2, CellFormat::Str, false),
        Err(EditError::OutOfRange { .. })
    ));
}
