use super::*;
use crate::falcon::cache::{
    CacheConfig, CacheController, InclusionPolicy, ReplacementPolicy, WriteAllocPolicy,
    WritePolicy,
};
use crate::falcon::memory::{Bus, Ram};

fn map_one_page(ram: &mut Ram, vaddr: u32, paddr: u32, perms_bits: u32) -> u32 {
    let root_pt_pa: u32 = 0x1000;
    let leaf_pt_pa: u32 = 0x2000;
    let root_ppn = root_pt_pa >> 12;
    let leaf_ppn = leaf_pt_pa >> 12;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let vpn0 = (vaddr >> 12) & 0x3FF;
    let pte1 = (leaf_ppn << 10) | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, pte1).unwrap();
    let ppn = paddr >> 12;
    let pte0 = (ppn << 10) | perms_bits | 0x1;
    ram.store32(leaf_pt_pa + vpn0 * 4, pte0).unwrap();
    root_ppn
}

fn rwxu() -> u32 {
    0x2 | 0x4 | 0x8 | 0x10
}

/// Build a Sv32 satp value (mode=1, asid, ppn).
fn satp_value(ppn: u32, asid: u16) -> u32 {
    (1u32 << 31) | ((asid as u32 & 0x1FF) << 22) | (ppn & 0x003F_FFFF)
}

/// An MMU translating in U-mode through the table rooted at `root`.
fn user_mmu(root: u32) -> Mmu {
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(root, 1));
    mmu
}

/// An MMU in auto mode, translating through `root_pa` even in M-mode.
fn auto_mmu(root_pa: u32, asid: u16) -> Mmu {
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.force_translate = true;
    mmu.satp = Satp::new(Mmu::make_satp(root_pa, asid));
    mmu
}

#[test]
fn identity_when_disabled() {
    let mut mmu = Mmu::default();
    let mut ram = Ram::new(0x1000);
    let (pa, stall) = mmu
        .translate(0xDEAD_BEEF, AccessType::Load, &mut ram)
        .unwrap();
    assert_eq!(pa, 0xDEAD_BEEF);
    assert_eq!(stall, 0);
    assert!(mmu.trace.is_empty(), "an identity access leaves no record");
}

#[test]
fn translates_4k_page_via_walker_and_caches_in_tlb() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let paddr = 0x0008_0000;
    let mut mmu = user_mmu(map_one_page(&mut ram, vaddr, paddr, rwxu()));

    let (pa1, stall1) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa1, paddr | 0x234);
    assert_eq!(stall1, mmu.tlb.config.miss_penalty);
    assert_eq!(mmu.tlb.stats.misses, 1);
    assert_eq!(mmu.tlb.stats.hits, 0);

    let (pa2, stall2) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa2, paddr | 0x234);
    assert_eq!(stall2, mmu.tlb.config.hit_latency);
    assert_eq!(mmu.tlb.stats.hits, 1);
    assert_eq!(
        mmu.tlb.stats.total_cycles,
        u64::from(stall1) + u64::from(stall2)
    );
    assert_eq!(mmu.tlb.stats.walk_cycles, u64::from(stall1));
}

#[test]
fn flush_invalidates_cached_translation() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let mut mmu = user_mmu(map_one_page(&mut ram, vaddr, 0x0008_0000, rwxu()));
    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(mmu.tlb.stats.misses, 1);
    mmu.flush();
    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(
        mmu.tlb.stats.misses, 2,
        "after flush the second probe misses again"
    );
}

#[test]
fn page_fault_propagates() {
    let mut ram = Ram::new(0x4000);
    let mut mmu = user_mmu(0x1); // empty root PT at 0x1000
    let err = mmu
        .translate(0x1234, AccessType::Load, &mut ram)
        .unwrap_err();
    assert_eq!(err.cause, 13);
    assert_eq!(mmu.tlb.stats.page_faults, 1);
}

#[test]
fn tlb_disabled_never_hits() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let paddr = 0x0008_0000;
    let mut mmu = user_mmu(map_one_page(&mut ram, vaddr, paddr, rwxu()));
    mmu.tlb_enabled = false;

    let (pa1, stall1) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    let (pa2, stall2) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa1, paddr | 0x234);
    assert_eq!(pa2, paddr | 0x234);
    assert_eq!(stall1, mmu.tlb.config.miss_penalty);
    assert_eq!(stall2, mmu.tlb.config.miss_penalty);
    assert_eq!(mmu.tlb.stats.hits, 0, "disabled TLB never hits");
    assert_eq!(mmu.tlb.stats.misses, 2, "every access walks");
    assert!(
        mmu.tlb.entries.iter().all(|e| !e.valid),
        "disabled TLB installs nothing"
    );
    assert!(!mmu.trace.last().unwrap().tlb_used);
}

#[test]
fn store_on_clean_hit_re_walks_to_set_dirty() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let mut mmu = user_mmu(map_one_page(&mut ram, vaddr, 0x0008_0000, rwxu()));

    // Load installs the entry with dirty=false.
    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    let hits_before = mmu.tlb.stats.hits;
    let misses_before = mmu.tlb.stats.misses;
    mmu.translate(vaddr, AccessType::Store, &mut ram).unwrap();
    assert_eq!(
        mmu.tlb.stats.hits, hits_before,
        "store on clean entry must re-walk"
    );
    assert_eq!(mmu.tlb.stats.misses, misses_before + 1);
    let rec = mmu.trace.last().unwrap();
    assert!(rec.dirty_rewalk && rec.set_dirty);
    assert_eq!(
        rec.evicted, None,
        "the re-walk refreshes the same slot instead of evicting"
    );

    // Subsequent Store hits the dirty entry.
    let hits_now = mmu.tlb.stats.hits;
    mmu.translate(vaddr, AccessType::Store, &mut ram).unwrap();
    assert_eq!(mmu.tlb.stats.hits, hits_now + 1);
}

#[test]
fn make_satp_encoding() {
    let v = Mmu::make_satp(0x2000, 5);
    assert_eq!(v >> 31, 1, "Sv32 mode bit");
    assert_eq!((v >> 22) & 0x1FF, 5, "asid field");
    assert_eq!(v & 0x003F_FFFF, 0x2, "ppn = root_pa >> 12");
}

#[test]
fn vm_mode_reads_the_names_older_configs_wrote() {
    assert_eq!(VmMode::parse("sv32"), Some(VmMode::Auto));
    assert_eq!(VmMode::parse("CUSTOM"), Some(VmMode::Auto));
    assert_eq!(VmMode::parse("manual"), Some(VmMode::Program));
    assert_eq!(VmMode::parse("off"), Some(VmMode::Off));
    assert_eq!(VmMode::parse("bogus"), None);
    for mode in VmMode::ALL {
        assert_eq!(VmMode::parse(mode.as_str()), Some(mode));
    }
}

// ── The simulator's auto map ─────────────────────────────────────────────────

#[test]
fn auto_map_is_identity_with_4k_pages_over_the_window() {
    let mut ram = Ram::new(1 << 24); // 16 MiB
    let top = (1 << 24) - 0x1_0000;
    let layout =
        Mmu::install_auto_map(&mut ram, top, PageMapSpec::default(), (0x0, 0x5000)).unwrap();
    assert_eq!(layout.leaf_tables, 1, "the window fits in one 4 MiB region");
    assert_eq!(layout.tables_hi, top);
    assert_eq!(layout.bytes(), 2 * TABLE_BYTES);
    let mut mmu = auto_mmu(layout.root, 0);

    // Inside the window: a two-level walk to a 4 KiB page.
    let (pa, _) = mmu.translate(0x1234, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa, 0x1234);
    let rec = mmu.trace.last().unwrap();
    assert_eq!(rec.steps().len(), 2);
    assert!(!rec.megapage);

    // Outside it: one step to a megapage.
    for va in [0x0040_0010u32, 0x0080_0abc] {
        let (pa, _) = mmu.translate(va, AccessType::Load, &mut ram).unwrap();
        assert_eq!(pa, va, "identity map: PA must equal VA for 0x{va:08x}");
        let rec = mmu.trace.last().unwrap();
        assert_eq!(rec.steps().len(), 1);
        assert!(rec.megapage);
    }
}

#[test]
fn auto_map_respects_permission_bits() {
    let mut ram = Ram::new(1 << 24);
    let spec = PageMapSpec {
        perms: PtePerms {
            r: true,
            w: false,
            x: true,
            u: true,
        },
        ..PageMapSpec::default()
    };
    let layout = Mmu::install_auto_map(&mut ram, 1 << 24, spec, (0, 0)).unwrap();
    let mut mmu = auto_mmu(layout.root, 0);

    let va = 0x0000_1000u32;
    assert!(mmu.translate(va, AccessType::Load, &mut ram).is_ok());
    let err = mmu.translate(va, AccessType::Store, &mut ram).unwrap_err();
    assert_eq!(err.cause, 15, "store to read-only page faults");
    assert_eq!(
        mmu.trace.last().unwrap().fault(),
        Some(FaultReason::NotPermitted)
    );
}

#[test]
fn auto_map_sets_global_bit() {
    let mut ram = Ram::new(1 << 24);
    let spec = PageMapSpec {
        global: true,
        ..PageMapSpec::default()
    };
    let layout = Mmu::install_auto_map(&mut ram, 1 << 24, spec, (0, 0)).unwrap();
    let mut mmu = auto_mmu(layout.root, 0);
    mmu.translate(0x1234, AccessType::Load, &mut ram).unwrap();
    assert!(
        mmu.tlb.entries.iter().any(|e| e.valid && e.global),
        "global map installs global TLB entries"
    );
}

#[test]
fn auto_map_refuses_to_overlap_the_program() {
    let mut ram = Ram::new(1 << 16);
    let err = Mmu::install_auto_map(&mut ram, 0x8000, PageMapSpec::default(), (0, 0x7000))
        .unwrap_err();
    assert!(err.contains("overlap"), "{err}");
}

/// The tables used to sit in the top 4 KiB of RAM — where the stack starts.
/// Eight KiB of pushes zeroed every root entry, so the next page the program
/// reached faulted. Now they sit below the stack.
#[test]
fn a_deep_stack_leaves_the_auto_map_intact() {
    let src = "
.text
    li t1, 2048
fill:
    addi sp, sp, -4
    sw zero, 0(sp)
    addi t1, t1, -1
    bnez t1, fill
    li t2, 0x00800000
    lw t3, 0(t2)
    li a0, 7
    li a7, 93
    ecall
";
    let result = crate::Falcon::new()
        .asm(src)
        .vm(true)
        .max_cycles(1_000_000)
        .run()
        .expect("the program runs");
    assert_eq!(result.exit_code, Some(7));
}

// ── Through the cache controller ─────────────────────────────────────────────

fn write_back_dcache() -> CacheConfig {
    CacheConfig {
        size: 1024,
        line_size: 16,
        associativity: 2,
        replacement: ReplacementPolicy::Lru,
        write_policy: WritePolicy::WriteBack,
        write_alloc: WriteAllocPolicy::WriteAllocate,
        inclusion: InclusionPolicy::NonInclusive,
        hit_latency: 1,
        miss_penalty: 10,
        assoc_penalty: 0,
        transfer_width: 8,
    }
}

/// A PTE the program just stored can still sit dirty in a write-back D-cache,
/// with RAM holding the old value. The walker used to read RAM directly and
/// fault on a mapping that existed.
#[test]
fn the_walker_sees_a_pte_still_dirty_in_the_dcache() {
    let mut ctrl =
        CacheController::new(write_back_dcache(), write_back_dcache(), vec![], 1 << 20);
    let vaddr = 0x0040_0123;
    let root_pa = 0x1000;
    let leaf_table = 0x2000;
    let leaf_pte_addr = leaf_table + ((vaddr >> 12) & 0x3FF) * 4;
    let leaf_pte = ((0x8000u32 >> 12) << 10) | rwxu() | 0x1;

    // Paging off: these are plain stores, and the leaf lands in the D-cache.
    ctrl.store32(root_pa + (vaddr >> 22) * 4, ((leaf_table >> 12) << 10) | 0x1)
        .unwrap();
    ctrl.store32(leaf_pte_addr, leaf_pte).unwrap();
    assert_eq!(
        ctrl.ram.load32(leaf_pte_addr).unwrap(),
        0,
        "write-back: RAM has not seen the PTE yet"
    );

    ctrl.mmu = user_mmu(root_pa >> 12);
    let pa = ctrl
        .translate_for_access(vaddr, AccessType::Load)
        .expect("the mapping exists");
    assert_eq!(pa, 0x8123);

    // The A bit landed in the cached copy too, so writing the dirty line back
    // later cannot erase it.
    assert!(ctrl.effective_read32(leaf_pte_addr).unwrap() & 0x40 != 0);
}

#[test]
fn every_translation_leaves_a_record() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let mut mmu = user_mmu(map_one_page(&mut ram, vaddr, 0x0008_0000, rwxu()));
    mmu.note_instruction(0, 0x100);

    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    let walked = *mmu.trace.last().unwrap();
    assert_eq!(walked.outcome, TraceOutcome::Walked);
    assert_eq!(walked.pc, 0x100);
    assert_eq!(walked.steps().len(), 2);
    assert!(walked.set_accessed);
    assert_eq!(walked.paddr, Some(0x0008_0234));
    let slot = walked.installed_slot.expect("installed in the TLB");

    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    let hit = *mmu.trace.last().unwrap();
    assert_eq!(hit.outcome, TraceOutcome::Hit);
    assert_eq!(hit.hit_slot, Some(slot));
    assert_eq!(hit.seq, walked.seq + 1);

    mmu.translate(0x0080_0000, AccessType::Fetch, &mut ram)
        .unwrap_err();
    let fault = *mmu.trace.last().unwrap();
    assert_eq!(fault.fault(), Some(FaultReason::NotValid));
    assert_eq!(fault.pc, 0x0080_0000, "a fetch is its own pc");
}

#[test]
fn a_dry_run_changes_nothing() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let mmu = user_mmu(map_one_page(&mut ram, vaddr, 0x0008_0000, rwxu()));
    let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;
    let pte_before = ram.load32(leaf_addr).unwrap();

    let rec = mmu
        .dry_run(vaddr, AccessType::Store, &mut ram)
        .expect("translation applies");
    assert_eq!(rec.outcome, TraceOutcome::Walked);
    assert_eq!(rec.paddr, Some(0x0008_0234));
    assert_eq!(ram.load32(leaf_addr).unwrap(), pte_before, "no A/D written");
    assert_eq!(mmu.tlb.stats.misses, 0);
    assert!(mmu.trace.is_empty());
}

/// The walker's PTE reads are loads: a cold walk reaches memory for each, the
/// next walk of the same tables finds them in L1, and what each read cost lands
/// in the record and in the TLB's walk cycles.
#[test]
fn a_walk_pays_for_its_pte_reads_through_the_caches() {
    let mut ctrl =
        CacheController::new(write_back_dcache(), write_back_dcache(), vec![], 1 << 20);
    let vaddr = 0x0040_1234;
    let root = map_one_page(&mut ctrl.ram, vaddr, 0x0008_0000, rwxu());
    ctrl.mmu = user_mmu(root);
    let walker = u32::from(ctrl.mmu.tlb.config.miss_penalty);

    ctrl.translate_for_access(vaddr, AccessType::Load).unwrap();
    let cold = *ctrl.mmu.trace.last().unwrap();
    assert!(
        cold.steps().iter().all(|step| step.served_by.is_none()),
        "cold: memory serves both PTEs"
    );
    let cold_reads: u32 = cold.steps().iter().map(|step| u32::from(step.cycles)).sum();
    assert!(cold_reads > 0);
    assert_eq!(cold.cycles, walker + cold_reads);
    assert_eq!(ctrl.mmu.tlb.stats.walk_cycles, u64::from(walker + cold_reads));

    // Same tables, empty TLB: both PTEs sit in L1 now.
    ctrl.mmu.flush();
    ctrl.translate_for_access(vaddr, AccessType::Load).unwrap();
    let warm = *ctrl.mmu.trace.last().unwrap();
    assert!(warm.steps().iter().all(|step| step.served_by == Some(1)));
    let warm_reads: u32 = warm.steps().iter().map(|step| u32::from(step.cycles)).sum();
    assert!(warm_reads < cold_reads, "{warm_reads} vs {cold_reads}");
}

/// Each core translates through a TLB of its own: what one cached, the other
/// still walks for, and switching back finds it again.
#[test]
fn each_core_keeps_its_own_tlb() {
    use crate::capability::AddressTranslation;

    let mut ctrl =
        CacheController::new(write_back_dcache(), write_back_dcache(), vec![], 1 << 20);
    let vaddr = 0x0040_1234;
    let root = map_one_page(&mut ctrl.ram, vaddr, 0x0008_0000, rwxu());
    ctrl.mmu = user_mmu(root);
    ctrl.set_core_count(2);
    assert_eq!(ctrl.tlb_count(), 2);

    ctrl.translate_for_access(vaddr, AccessType::Load).unwrap();
    ctrl.set_active_core(1);
    ctrl.translate_for_access(vaddr, AccessType::Load).unwrap();
    assert_eq!(ctrl.tlb_of(1).stats.misses, 1, "core 0's entry is not core 1's");

    ctrl.set_active_core(0);
    ctrl.translate_for_access(vaddr, AccessType::Load).unwrap();
    assert_eq!(ctrl.tlb_of(0).stats.hits, 1, "core 0 kept its entry");
    assert_eq!(ctrl.tlb_of(1).stats.hits, 0);
    assert_eq!(ctrl.core_tlb_stats(1).misses, 1);
    assert!(ctrl.core_probe(1, u64::from(vaddr)).hit_entry.is_some());

    ctrl.flush_all_tlbs();
    for core in 0..2 {
        assert!(ctrl.tlb_of(core).entries.iter().all(|entry| !entry.valid));
    }
}
