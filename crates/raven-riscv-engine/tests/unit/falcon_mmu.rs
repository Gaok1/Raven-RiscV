use super::*;
use crate::falcon::memory::Bus;

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

#[test]
fn identity_when_disabled() {
    let mut mmu = Mmu::default();
    let mut ram = Ram::new(0x1000);
    let (pa, stall) = mmu
        .translate(0xDEAD_BEEF, AccessType::Load, &mut ram)
        .unwrap();
    assert_eq!(pa, 0xDEAD_BEEF);
    assert_eq!(stall, 0);
}

#[test]
fn translates_4k_page_via_walker_and_caches_in_tlb() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let paddr = 0x0008_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, rwxu());
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(root, 1));

    let (pa1, stall1) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa1, paddr | 0x234);
    assert_eq!(stall1, mmu.tlb.config.miss_penalty);
    assert_eq!(mmu.tlb.stats.misses, 1);
    assert_eq!(mmu.tlb.stats.hits, 0);

    let (pa2, stall2) = mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa2, paddr | 0x234);
    assert_eq!(stall2, mmu.tlb.config.hit_latency);
    assert_eq!(mmu.tlb.stats.hits, 1);
}

#[test]
fn flush_invalidates_cached_translation() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let paddr = 0x0008_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, rwxu());
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(root, 1));

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
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(0x1, 1)); // empty root PT at 0x1000
    let err = mmu
        .translate(0x1234, AccessType::Load, &mut ram)
        .unwrap_err();
    assert_eq!(err.cause, 13);
    assert_eq!(mmu.tlb.stats.page_faults, 1);
}

#[test]
fn install_map_identity_megapages_translate_identity() {
    // Identity megapage map: VA == PA across superpages, including i>0.
    let mut ram = Ram::new(1 << 24); // 16 MiB
    let root_pa = (1u32 << 24) - 4096;
    Mmu::install_map(&mut ram, root_pa, PageMapSpec::default(), (0, 0));

    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.force_translate = true; // M-mode also translates (Auto)
    mmu.satp = Satp::new((1u32 << 31) | (root_pa >> 12));

    for &va in &[0x0000_1234u32, 0x0040_0010, 0x0080_0abc] {
        let (pa, _) = mmu.translate(va, AccessType::Load, &mut ram).unwrap();
        assert_eq!(pa, va, "identity map: PA must equal VA for 0x{va:08x}");
    }
}

#[test]
fn install_map_offset_shifts_physical_address() {
    let mut ram = Ram::new(1 << 24);
    let root_pa = (1u32 << 24) - 4096;
    // +4 MiB offset (one superpage), megapage granularity.
    let spec = PageMapSpec {
        kind: MapKind::Offset(4),
        perms: PtePerms {
            r: true,
            w: true,
            x: true,
            u: true,
        },
        ..PageMapSpec::default()
    };
    Mmu::install_map(&mut ram, root_pa, spec, (0, 0));

    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.force_translate = true;
    mmu.satp = Satp::new((1u32 << 31) | (root_pa >> 12));

    let va = 0x0000_1234u32;
    let (pa, _) = mmu.translate(va, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa, va + 0x0040_0000, "offset map shifts PA by +4 MiB");
}

#[test]
fn install_map_respects_permission_bits() {
    // A read-only map (no W) must fault on Store.
    let mut ram = Ram::new(1 << 24);
    let root_pa = (1u32 << 24) - 4096;
    let spec = PageMapSpec {
        kind: MapKind::Identity,
        perms: PtePerms {
            r: true,
            w: false,
            x: true,
            u: true,
        },
        ..PageMapSpec::default()
    };
    Mmu::install_map(&mut ram, root_pa, spec, (0, 0));

    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new((1u32 << 31) | (root_pa >> 12));

    let va = 0x0000_1000u32;
    assert!(mmu.translate(va, AccessType::Load, &mut ram).is_ok());
    let err = mmu.translate(va, AccessType::Store, &mut ram).unwrap_err();
    assert_eq!(err.cause, 15, "store to read-only page faults");
}

#[test]
fn tlb_disabled_never_hits() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1234;
    let paddr = 0x0008_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, rwxu());
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(root, 1));
    mmu.tlb_enabled = false;

    // Two reads of the same VPN both miss (no caching, no hits).
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
}

#[test]
fn store_on_clean_hit_re_walks_to_set_dirty() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let paddr = 0x0008_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, rwxu());
    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.priv_mode = PrivMode::U;
    mmu.satp = Satp::new(satp_value(root, 1));

    // Load installs entry with dirty=false.
    mmu.translate(vaddr, AccessType::Load, &mut ram).unwrap();
    let hits_before = mmu.tlb.stats.hits;
    let misses_before = mmu.tlb.stats.misses;
    mmu.translate(vaddr, AccessType::Store, &mut ram).unwrap();
    assert_eq!(
        mmu.tlb.stats.hits, hits_before,
        "store on clean entry must re-walk"
    );
    assert_eq!(mmu.tlb.stats.misses, misses_before + 1);

    // Subsequent Store hits the dirty entry.
    let hits_now = mmu.tlb.stats.hits;
    mmu.translate(vaddr, AccessType::Store, &mut ram).unwrap();
    assert_eq!(mmu.tlb.stats.hits, hits_now + 1);
}

// â”€â”€ Parametric paging â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn paging_scheme_sv32_shape() {
    let s = PagingScheme::sv32();
    assert!(s.is_valid());
    assert_eq!(s.total_bits(), 32);
    assert_eq!(s.num_levels(), 2);
    assert_eq!(s.shift_at(0), 22); // 4 MiB superpage
    assert_eq!(s.shift_at(1), 12); // 4 KiB page
    assert_eq!(s.leaf_masks(), vec![0, 10]);
    // Invalid: doesn't tile 32 bits.
    assert!(
        !PagingScheme {
            offset_bits: 12,
            level_bits: vec![10]
        }
        .is_valid()
    );
    // Valid 3-level scheme.
    let s3 = PagingScheme {
        offset_bits: 12,
        level_bits: vec![8, 6, 6],
    };
    assert!(s3.is_valid());
    assert_eq!(s3.leaf_masks(), vec![0, 6, 12]);
}

#[test]
fn make_satp_encoding() {
    let v = Mmu::make_satp(0x2000, 5);
    assert_eq!(v >> 31, 1, "Sv32 mode bit");
    assert_eq!((v >> 22) & 0x1FF, 5, "asid field");
    assert_eq!(v & 0x003F_FFFF, 0x2, "ppn = root_pa >> 12");
}

#[test]
fn install_map_sets_global_bit() {
    let mut ram = Ram::new(1 << 24);
    let root_pa = (1u32 << 24) - 4096;
    let spec = PageMapSpec {
        global: true,
        ..PageMapSpec::default()
    };
    Mmu::install_map(&mut ram, root_pa, spec, (0, 0));

    let mut mmu = Mmu::default();
    mmu.enabled = true;
    mmu.force_translate = true;
    mmu.satp = Satp::new(Mmu::make_satp(root_pa, 0));
    mmu.translate(0x1234, AccessType::Load, &mut ram).unwrap();
    assert!(
        mmu.tlb.entries.iter().any(|e| e.valid && e.global),
        "global map installs global TLB entries"
    );
}

#[test]
fn custom_three_level_scheme_translates() {
    // offset 12, levels [8,6,6] â†’ 3-level walk, 4 KiB leaf pages.
    let scheme = PagingScheme {
        offset_bits: 12,
        level_bits: vec![8, 6, 6],
    };
    let mut ram = Ram::new(1 << 24);
    let root_pa = scheme.root_pa(1 << 24);
    // Identity map, refine a small window around 0x1000 down to 4 KiB.
    Mmu::install_map_scheme(
        &mut ram,
        root_pa,
        &scheme,
        PageMapSpec::default(),
        (0x0, 0x4000),
    );

    let mut mmu = Mmu::default();
    mmu.set_scheme(scheme);
    mmu.enabled = true;
    mmu.force_translate = true;
    mmu.satp = Satp::new(Mmu::make_satp(root_pa, 0));

    // Inside the refined window: a real 3-level walk to a 4 KiB leaf.
    let (pa, _) = mmu.translate(0x1234, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa, 0x1234, "identity within refined 4 KiB window");
    // Outside the window: still identity via a top-level superpage leaf.
    let (pa2, _) = mmu
        .translate(0x0140_0000, AccessType::Load, &mut ram)
        .unwrap();
    assert_eq!(pa2, 0x0140_0000, "identity via superpage outside window");
}

#[test]
fn custom_offset_map_shifts_pa() {
    // Identity-vs-offset under a custom scheme: PA = VA + 8 MiB.
    let scheme = PagingScheme::sv32();
    let mut ram = Ram::new(1 << 25); // 32 MiB
    let root_pa = scheme.root_pa(1 << 25);
    let spec = PageMapSpec {
        kind: MapKind::Offset(8),
        ..PageMapSpec::default()
    };
    Mmu::install_map_scheme(&mut ram, root_pa, &scheme, spec, (0, 0));

    let mut mmu = Mmu::default();
    mmu.set_scheme(scheme);
    mmu.enabled = true;
    mmu.force_translate = true;
    mmu.satp = Satp::new(Mmu::make_satp(root_pa, 0));

    let (pa, _) = mmu.translate(0x1234, AccessType::Load, &mut ram).unwrap();
    assert_eq!(pa, 0x1234 + 0x0080_0000, "offset map shifts PA by +8 MiB");
}
