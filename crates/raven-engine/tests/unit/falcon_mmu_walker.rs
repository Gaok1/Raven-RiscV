use super::*;
use crate::falcon::memory::Bus;

/// Layout helper: build a single-PTE root table and one leaf PTE at L0
/// mapping `vaddr` → `paddr` with `perms`. Returns the satp.ppn (root PPN).
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

fn walk_u(ram: &mut Ram, root: u32, vaddr: u32, access: AccessType) -> Walk {
    walk(root, vaddr, access, PrivMode::U, ram, true)
}

#[test]
fn walks_4k_page_happy() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x4000_1234;
    let paddr = 0x0003_0000;
    let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());
    let w = walk_u(&mut ram, root, vaddr, AccessType::Load);
    let leaf = w.result.unwrap();
    assert!(!leaf.megapage);
    assert_eq!(leaf.ppn, paddr >> 12);
    assert!(leaf.perms.r && leaf.perms.w && leaf.perms.x && leaf.perms.u);
}

#[test]
fn walk_reports_every_pte_it_read() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x4000_1234;
    let root = map_one_page(&mut ram, vaddr, 0x0003_0000, p_rwxu());
    let w = walk_u(&mut ram, root, vaddr, AccessType::Load);
    let steps = w.steps();
    assert_eq!(steps.len(), 2);
    // Root first, indexed by VPN[1]; then the leaf table by VPN[0].
    assert_eq!(steps[0].level, 1);
    assert_eq!(u32::from(steps[0].index), vaddr >> 22);
    assert_eq!(steps[0].pte_addr, 0x1000 + (vaddr >> 22) * 4);
    assert_eq!(steps[1].level, 0);
    assert_eq!(u32::from(steps[1].index), (vaddr >> 12) & 0x3FF);
    assert_eq!(steps[1].pte_addr, 0x2000 + ((vaddr >> 12) & 0x3FF) * 4);
}

#[test]
fn invalid_pte_faults_and_says_why() {
    let mut ram = Ram::new(1 << 20);
    // Root table all-zeros at 0x1000 → PTE.V=0.
    let w = walk_u(&mut ram, 0x1, 0x1234, AccessType::Load);
    assert_eq!(w.result, Err(FaultReason::NotValid));
    assert_eq!(w.steps().len(), 1, "the walk stops at the invalid root entry");
}

#[test]
fn store_to_readonly_page_is_not_permitted() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0080_0000;
    // R + U only (no W).
    let root = map_one_page(&mut ram, vaddr, 0x0005_0000, 0x2 | 0x10);
    let w = walk_u(&mut ram, root, vaddr, AccessType::Store);
    assert_eq!(w.result, Err(FaultReason::NotPermitted));
}

#[test]
fn fetch_to_non_x_page_is_not_permitted() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    // R + W + U (no X).
    let root = map_one_page(&mut ram, vaddr, 0x0006_0000, 0x2 | 0x4 | 0x10);
    let w = walk_u(&mut ram, root, vaddr, AccessType::Fetch);
    assert_eq!(w.result, Err(FaultReason::NotPermitted));
}

#[test]
fn u_mode_cannot_touch_supervisor_page() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    // R + W (no U).
    let root = map_one_page(&mut ram, vaddr, 0x0006_0000, 0x2 | 0x4);
    let w = walk_u(&mut ram, root, vaddr, AccessType::Load);
    assert_eq!(w.result, Err(FaultReason::SupervisorPage));
}

#[test]
fn s_mode_cannot_touch_user_page() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    let root = map_one_page(&mut ram, vaddr, 0x0006_0000, p_rwxu());
    let w = walk(root, vaddr, AccessType::Load, PrivMode::S, &mut ram, true);
    assert_eq!(w.result, Err(FaultReason::UserPage));
}

#[test]
fn megapage_l1_leaf_works() {
    let mut ram = Ram::new(1 << 23);
    let vaddr = 0x0080_1234; // vpn1=2, offset within 4MiB
    let megapage_pa = 0x0040_0000; // 4 MiB aligned (ppn0 = 0)
    let root_pt_pa: u32 = 0x1000;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let leaf = ((megapage_pa >> 12) << 10) | 0x2 | 0x4 | 0x10 | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, leaf).unwrap();
    let w = walk_u(&mut ram, root_pt_pa >> 12, vaddr, AccessType::Load);
    let leaf = w.result.unwrap();
    assert!(leaf.megapage);
    assert_eq!(leaf.ppn, megapage_pa >> 12);
    assert_eq!(w.steps().len(), 1, "a megapage ends the walk at the root");
}

#[test]
fn megapage_misaligned_faults() {
    let mut ram = Ram::new(1 << 23);
    let vaddr = 0x0080_1234;
    // PPN with ppn0 != 0 → misaligned superpage.
    let bad_ppn: u32 = (0x0040_0000 >> 12) | 0x1;
    let root_pt_pa: u32 = 0x1000;
    let vpn1 = (vaddr >> 22) & 0x3FF;
    let leaf = (bad_ppn << 10) | 0x2 | 0x4 | 0x10 | 0x1;
    ram.store32(root_pt_pa + vpn1 * 4, leaf).unwrap();
    let w = walk_u(&mut ram, root_pt_pa >> 12, vaddr, AccessType::Load);
    assert_eq!(w.result, Err(FaultReason::Misaligned));
}

#[test]
fn pt_out_of_ram_faults() {
    let mut ram = Ram::new(0x1000); // 4 KiB total
    let w = walk_u(&mut ram, 0x100, 0x1000, AccessType::Load);
    assert_eq!(w.result, Err(FaultReason::OutsideMemory));
}

#[test]
fn walker_sets_a_on_load_and_d_on_store() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1000;
    let root = map_one_page(&mut ram, vaddr, 0x0008_0000, p_rwxu());
    let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;

    let first = walk_u(&mut ram, root, vaddr, AccessType::Load).result.unwrap();
    assert!(first.first_touch, "A was clear before the first walk");
    assert!(first.set_accessed && !first.set_dirty);
    let pte = ram.load32(leaf_addr).unwrap();
    assert!(pte & 0x40 != 0, "A bit set");
    assert!(pte & 0x80 == 0, "D bit clear");

    let store = walk_u(&mut ram, root, vaddr, AccessType::Store).result.unwrap();
    assert!(!store.first_touch, "A was already set");
    assert!(store.set_dirty);
    assert!(ram.load32(leaf_addr).unwrap() & 0x80 != 0, "D bit set");
}

#[test]
fn a_read_only_walk_writes_nothing() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_1000;
    let root = map_one_page(&mut ram, vaddr, 0x0008_0000, p_rwxu());
    let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;
    let before = ram.load32(leaf_addr).unwrap();
    let w = walk(root, vaddr, AccessType::Store, PrivMode::U, &mut ram, false);
    assert!(w.result.is_ok());
    assert_eq!(ram.load32(leaf_addr).unwrap(), before);
}

#[test]
fn w_without_r_is_reserved_faults() {
    let mut ram = Ram::new(1 << 20);
    let vaddr = 0x0040_0000;
    // W=1 R=0 U=1 → reserved encoding.
    let root = map_one_page(&mut ram, vaddr, 0x0008_0000, 0x4 | 0x10);
    let w = walk_u(&mut ram, root, vaddr, AccessType::Load);
    assert_eq!(w.result, Err(FaultReason::Reserved));
}
