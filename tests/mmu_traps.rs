// MMU integration tests — end-to-end CPU stepping through Sv32 translation,
// page faults, traps, mret, and sfence.vma.

use raven::falcon::cache::{CacheConfig, CacheController};
use raven::falcon::encoder::encode;
use raven::falcon::exec::step;
use raven::falcon::instruction::Instruction;
use raven::falcon::memory::Bus;
use raven::falcon::mmu::PrivMode;
use raven::falcon::registers::Cpu;
use raven::ui::Console;

const RAM_SIZE: usize = 1 << 20; // 1 MiB

fn fresh_setup() -> (Cpu, CacheController, Console) {
    let icfg = CacheConfig::default();
    let dcfg = CacheConfig::default();
    let mem = CacheController::new(icfg, dcfg, vec![], RAM_SIZE);
    (Cpu::default(), mem, Console::default())
}

/// Map a single 4 KiB virtual page `vaddr` → `paddr` with the given perm bits
/// (R=2, W=4, X=8, U=16). Returns the satp value (mode=Sv32 + ppn).
fn install_one_page(mem: &mut CacheController, vaddr: u32, paddr: u32, perms_bits: u32) -> u32 {
    let root_pt_pa: u32 = 0x1000;
    let leaf_pt_pa: u32 = 0x2000;
    let root_ppn = root_pt_pa >> 12;
    let leaf_ppn = leaf_pt_pa >> 12;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let vpn0 = (vaddr >> 12) & 0x3FF;
    let pte1 = (leaf_ppn << 10) | 0x1;
    mem.ram_mut().store32(root_pt_pa + vpn1 * 4, pte1).unwrap();
    let ppn = paddr >> 12;
    let pte0 = (ppn << 10) | perms_bits | 0x1;
    mem.ram_mut().store32(leaf_pt_pa + vpn0 * 4, pte0).unwrap();
    (1u32 << 31) | root_ppn
}

/// Write a 4-byte instruction to physical memory.
fn write_instr(mem: &mut CacheController, addr: u32, w: u32) {
    mem.ram_mut().store32(addr, w).unwrap();
}

#[test]
fn u_mode_load_to_unmapped_page_traps_to_mtvec() {
    let (mut cpu, mut mem, mut console) = fresh_setup();
    mem.mmu_mut().enabled = true;

    // No mapping installed — root PT at 0x1000 is all zeros.
    let satp = (1u32 << 31) | (0x1000 >> 12);
    cpu.satp = satp;
    mem.set_satp(satp);

    // Place a handler word at 0x3000 (won't actually execute; we only check
    // pc/mcause). Use a halt to terminate cleanly if we step into it.
    write_instr(&mut mem, 0x3000, encode(Instruction::Halt).unwrap());
    cpu.mtvec = 0x3000;

    // Hand-craft a U-mode load: lw x5, 0(x6) with x6 = 0x5000 (unmapped).
    let load = encode(Instruction::Lw { rd: 5, rs1: 6, imm: 0 }).unwrap();
    // The fault is on the load itself, not on the fetch; put the load in a
    // mapped page so fetch succeeds. Map VA 0 → PA 0 with RX+U.
    let _ = install_one_page(&mut mem, 0, 0, 0x2 | 0x8 | 0x10);
    write_instr(&mut mem, 0, load);

    cpu.write(6, 0x5000);
    cpu.pc = 0;
    cpu.priv_mode = PrivMode::U;
    mem.set_priv_mode(PrivMode::U);

    // First step should fault and redirect to mtvec.
    let cont = step(&mut cpu, &mut mem, &mut console).unwrap();
    assert!(cont);
    assert_eq!(cpu.pc, 0x3000, "trap vectors through mtvec");
    assert_eq!(cpu.mcause, 13, "load page fault cause");
    assert_eq!(cpu.mtval, 0x5000, "tval == faulting vaddr");
    assert_eq!(cpu.mepc, 0, "mepc == faulting PC");
    assert_eq!(cpu.priv_mode, PrivMode::M, "trap enters M-mode");
}

#[test]
fn u_mode_store_to_read_only_page_faults() {
    let (mut cpu, mut mem, mut console) = fresh_setup();
    mem.mmu_mut().enabled = true;

    // Map VA 0 → PA 0 RX+U (code), and VA 0x4000 → PA 0x8000 R+U only (no W).
    let satp = install_one_page(&mut mem, 0, 0, 0x2 | 0x8 | 0x10);
    let _ = install_one_page(&mut mem, 0x4000, 0x8000, 0x2 | 0x10);
    cpu.satp = satp;
    mem.set_satp(satp);
    // map needs to re-bind both; rerun for code page since install_one_page
    // overwrites a shared leaf table — both vpn[1]s here are 0 so they share
    // the leaf table, and the second call overwrites the first leaf entry.
    // Reinstall the code mapping after the data mapping.
    let _ = install_one_page(&mut mem, 0, 0, 0x2 | 0x8 | 0x10);

    write_instr(&mut mem, 0x3000, encode(Instruction::Halt).unwrap());
    cpu.mtvec = 0x3000;

    let store = encode(Instruction::Sw { rs2: 5, rs1: 6, imm: 0 }).unwrap();
    write_instr(&mut mem, 0, store);

    cpu.write(5, 0xCAFE_BABE);
    cpu.write(6, 0x4000);
    cpu.pc = 0;
    cpu.priv_mode = PrivMode::U;
    mem.set_priv_mode(PrivMode::U);

    step(&mut cpu, &mut mem, &mut console).unwrap();
    assert_eq!(cpu.mcause, 15, "store page fault cause");
    assert_eq!(cpu.pc, 0x3000);
}

#[test]
fn mret_returns_to_user_mode_and_resumes_at_mepc() {
    let (mut cpu, mut mem, mut console) = fresh_setup();
    // No VM here — just exercise the mret semantics.
    cpu.priv_mode = PrivMode::M;
    cpu.mepc = 0x100;
    cpu.mstatus = 0; // MPP = 0 → U
    let mret = encode(Instruction::Mret).unwrap();
    write_instr(&mut mem, 0, mret);
    cpu.pc = 0;
    step(&mut cpu, &mut mem, &mut console).unwrap();
    assert_eq!(cpu.pc, 0x100);
    assert_eq!(cpu.priv_mode, PrivMode::U);
}

#[test]
fn satp_csr_write_flushes_tlb() {
    let (mut cpu, mut mem, mut console) = fresh_setup();
    mem.mmu_mut().enabled = true;

    let satp = install_one_page(&mut mem, 0x1_0000, 0x8000, 0x2 | 0x4 | 0x10);
    cpu.write(5, satp);
    let csrrw = encode(Instruction::Csrrw {
        rd: 0,
        rs1: 5,
        csr: 0x180,
    })
    .unwrap();
    write_instr(&mut mem, 0, csrrw);
    cpu.pc = 0;
    cpu.priv_mode = PrivMode::M;
    mem.set_priv_mode(PrivMode::M);

    step(&mut cpu, &mut mem, &mut console).unwrap();
    assert_eq!(cpu.satp, satp);
    // The MMU's satp mirror must agree.
    assert_eq!(mem.mmu().satp.raw, satp);
    // TLB must be empty after satp write.
    assert!(mem.mmu().tlb.entries.iter().all(|e| !e.valid));
}

#[test]
fn satp_bare_with_vm_on_is_identity_equivalent_to_vm_off() {
    // Run the same straight-line program twice — once with VM fully off, once
    // with VM on but satp.MODE=Bare. Final architectural state must agree.
    fn run(vm_on: bool) -> (u32, u32, u64) {
        let (mut cpu, mut mem, mut console) = fresh_setup();
        mem.mmu_mut().enabled = vm_on;
        // Tiny program: x5 = 1 + 2 + 3 + 4 + 5, then halt.
        let prog = [
            encode(Instruction::Addi { rd: 5, rs1: 0, imm: 1 }).unwrap(),
            encode(Instruction::Addi { rd: 5, rs1: 5, imm: 2 }).unwrap(),
            encode(Instruction::Addi { rd: 5, rs1: 5, imm: 3 }).unwrap(),
            encode(Instruction::Addi { rd: 5, rs1: 5, imm: 4 }).unwrap(),
            encode(Instruction::Addi { rd: 5, rs1: 5, imm: 5 }).unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];
        for (i, w) in prog.iter().enumerate() {
            write_instr(&mut mem, (i * 4) as u32, *w);
        }
        cpu.pc = 0;
        // Bare mode: satp.MODE=0, all other bits ignored.
        cpu.satp = 0;
        mem.set_satp(0);
        cpu.priv_mode = PrivMode::M;
        mem.set_priv_mode(PrivMode::M);
        while step(&mut cpu, &mut mem, &mut console).unwrap() {}
        (cpu.read(5), cpu.pc, cpu.instr_count)
    }
    let off = run(false);
    let bare = run(true);
    assert_eq!(off, bare);
    assert_eq!(off.0, 15);
}

#[test]
fn sfence_vma_flushes_tlb() {
    let (mut cpu, mut mem, mut console) = fresh_setup();
    mem.mmu_mut().enabled = true;

    let satp = install_one_page(&mut mem, 0x1_0000, 0x8000, 0x2 | 0x4 | 0x10);
    cpu.satp = satp;
    mem.set_satp(satp);
    cpu.priv_mode = PrivMode::U;
    mem.set_priv_mode(PrivMode::U);

    // Populate the TLB by routing a load through the Bus trait, which uses
    // the controller's internal MMU + RAM and avoids the dual-borrow issue.
    let _ = mem.translate(0x1_0000, raven::falcon::mmu::AccessType::Load);
    assert!(mem.mmu().tlb.entries.iter().any(|e| e.valid));

    // Execute sfence.vma from M-mode.
    cpu.priv_mode = PrivMode::M;
    mem.set_priv_mode(PrivMode::M);
    let sfence = encode(Instruction::SfenceVma { rs1: 0, rs2: 0 }).unwrap();
    write_instr(&mut mem, 0, sfence);
    cpu.pc = 0;
    step(&mut cpu, &mut mem, &mut console).unwrap();
    assert!(mem.mmu().tlb.entries.iter().all(|e| !e.valid));
}
