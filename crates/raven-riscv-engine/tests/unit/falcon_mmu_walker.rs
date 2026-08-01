use super::*;
use crate::falcon::memory::Bus;
use crate::falcon::mmu::PagingScheme;

/// Layout helper: build a single-PTE root table and one leaf PTE at L0
/// mapping `vaddr` â†’ `paddr` with `perms`. Returns the satp.ppn (root PPN).
fn map_one_page(ram: &mut Ram, vaddr: u32, paddr: u32, perms_bits: u32) -> u32 {
    // Place root PT at page 1 (paddr 0x1000), leaf PT at page 2 (0x2000).
    // Caller must avoid overlap with `paddr`.
    let root_pt_pa: u32 = 0x1000;
    let leaf_pt_pa: u32 = 0x2000;
    let root_ppn = root_pt_pa >> 12;
    let leaf_ppn = leaf_pt_pa >> 12;

    let vpn1 = (vaddr >> 22) & 0x3FF;
    let vpn0 = (vaddr >> 12) & 0x3FF;

    // Non-leaf PTE points to leaf table: V=1, R=W=X=0.
    let pte1 = (leaf_ppn << 10) | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, pte1).unwrap();

    // Leaf PTE: V=1 + perms + ppn.
    let ppn = paddr >> 12;
    let pte0 = (ppn << 10) | perms_bits | 0x1;
    ram.store32(leaf_pt_pa + vpn0 * 4, pte0).unwrap();

    root_ppn
}

fn p_rwxu() -> u32 {
    0x2 | 0x4 | 0x8 | 0x10
}

#[test]
fn walks_4k_page_happy() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x4000_1234;
    let paddr = 0x0003_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());
    let r = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap();
    assert_eq!(r.page_bits, 12);
    assert_eq!(r.ppn, paddr >> 12);
    assert!(r.perms.r && r.perms.w && r.perms.x && r.perms.u);
}

#[test]
fn invalid_pte_faults_with_load_cause() {
    let mut ram = Ram::new(1 << 20);
    // Root table all-zeros at 0x1000 â†’ PTE.V=0.
    let root = 0x1000 >> 12;
    let err = walk(
        &PagingScheme::sv32(),
        root,
        0x1234,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 13);
    assert_eq!(err.vaddr, 0x1234);
}

#[test]
fn store_to_readonly_page_faults_with_store_cause() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0080_0000;
    let paddr = 0x0005_0000;
    // R + U only (no W).
    let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x10);
    let err = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Store,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 15);
}

#[test]
fn fetch_to_non_x_page_faults() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let paddr = 0x0006_0000;
    // R + W + U (no X).
    let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4 | 0x10);
    let err = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Fetch,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 12);
}

#[test]
fn u_mode_cannot_touch_supervisor_page() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let paddr = 0x0006_0000;
    // R + W (no U).
    let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4);
    let err = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 13);
}

#[test]
fn megapage_l1_leaf_works() {
    let mut ram = Ram::new(1 << 23);
    let vaddr = 0x0080_1234; // vpn1=2, offset within 4MiB
    let megapage_pa = 0x0040_0000; // 4 MiB aligned (ppn0 = 0)
    let root_pt_pa: u32 = 0x1000;
    let root = root_pt_pa >> 12;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    // Leaf PTE at L1 with R|W|U + V, ppn = megapage_pa >> 12.
    let leaf = ((megapage_pa >> 12) << 10) | 0x2 | 0x4 | 0x10 | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, leaf).unwrap();
    let r = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap();
    assert_eq!(r.page_bits, 22);
    assert_eq!(r.ppn, megapage_pa >> 12);
}

#[test]
fn megapage_misaligned_faults() {
    let mut ram = Ram::new(1 << 23);
    let vaddr = 0x0080_1234;
    // PPN with ppn0 != 0 â†’ misaligned superpage.
    let bad_ppn: u32 = (0x0040_0000 >> 12) | 0x1; // ppn0 = 1
    let root_pt_pa: u32 = 0x1000;
    let root = root_pt_pa >> 12;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let leaf = (bad_ppn << 10) | 0x2 | 0x4 | 0x10 | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, leaf).unwrap();
    let err = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 13);
}

#[test]
fn pt_out_of_ram_faults() {
    let mut ram = Ram::new(0x1000); // 4 KiB total
    // root_ppn points past RAM.
    let err = walk(
        &PagingScheme::sv32(),
        0x100,
        0x1000,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 13);
}

#[test]
fn walker_sets_a_on_load_and_d_on_store() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1000;
    let paddr = 0x0008_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());

    // Load: A is set, D stays clear.
    walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap();
    let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;
    let pte_after_load = ram.load32(leaf_addr).unwrap();
    assert!(pte_after_load & 0x40 != 0, "A bit set");
    assert!(pte_after_load & 0x80 == 0, "D bit clear");

    // Store: D becomes set too.
    walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Store,
        PrivMode::U,
    )
    .unwrap();
    let pte_after_store = ram.load32(leaf_addr).unwrap();
    assert!(pte_after_store & 0x80 != 0, "D bit set");
}

#[test]
fn w_without_r_is_reserved_faults() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let paddr = 0x0008_0000;
    // W=1 R=0 U=1 â†’ reserved encoding.
    let root = map_one_page(&mut ram, vaddr, paddr, 0x4 | 0x10);
    let err = walk(
        &PagingScheme::sv32(),
        root,
        vaddr,
        &mut ram,
        AccessType::Load,
        PrivMode::U,
    )
    .unwrap_err();
    assert_eq!(err.cause, 13);
}
