// falcon/mmu/walker.rs — Sv32 2-level page-table walker.
//
// Sv32 (RISC-V volume 2, §4.3): two-level table, 10+10+12 split, 4 KiB / 4 MiB
// page sizes. `walk` is invoked by `Mmu::translate` on a TLB miss. On success
// the walker auto-sets A (and D on Store) directly in the PTE — a pedagogical
// shortcut that avoids a second trap round-trip.

#![allow(dead_code)]

use super::tlb::PtePerms;
use super::{AccessType, PageFault, PrivMode};
use crate::falcon::memory::Ram;

/// Sv32 PTE bit layout (32 bits):
///   bit 0 : V (valid)
///   bit 1 : R
///   bit 2 : W
///   bit 3 : X
///   bit 4 : U
///   bit 5 : G (global)
///   bit 6 : A (accessed)
///   bit 7 : D (dirty)
///   bits 9-8  : RSW (software reserved)
///   bits 19-10: PPN[0]
///   bits 31-20: PPN[1]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pte {
    pub raw: u32,
}

impl Pte {
    pub fn new(raw: u32) -> Self {
        Self { raw }
    }
    pub fn valid(self) -> bool {
        self.raw & 0x1 != 0
    }
    pub fn perms(self) -> PtePerms {
        PtePerms {
            r: self.raw & 0x2 != 0,
            w: self.raw & 0x4 != 0,
            x: self.raw & 0x8 != 0,
            u: self.raw & 0x10 != 0,
        }
    }
    pub fn global(self) -> bool {
        self.raw & 0x20 != 0
    }
    pub fn accessed(self) -> bool {
        self.raw & 0x40 != 0
    }
    pub fn dirty(self) -> bool {
        self.raw & 0x80 != 0
    }
    /// Combined 22-bit PPN (PPN[1]:PPN[0]).
    pub fn ppn(self) -> u32 {
        (self.raw >> 10) & 0x003F_FFFF
    }
    pub fn ppn0(self) -> u32 {
        (self.raw >> 10) & 0x3FF
    }
    pub fn ppn1(self) -> u32 {
        (self.raw >> 20) & 0xFFF
    }
    /// A leaf PTE has at least one of R/W/X set.
    pub fn is_leaf(self) -> bool {
        let p = self.perms();
        p.r || p.w || p.x
    }
}

/// Result of a successful Sv32 walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalkResult {
    /// Final-level PPN. For a megapage (L1 leaf) the lower 10 bits are zero and
    /// the offset is `vpn[0]:page_offset`; the TLB entry stores `ppn` as-is and
    /// uses the `megapage` flag when reassembling the physical address.
    pub ppn: u32,
    pub perms: PtePerms,
    pub global: bool,
    pub megapage: bool,
}

/// Walk the Sv32 page table rooted at `root_ppn` for `vaddr`.
///
/// On success the leaf PTE has its A bit set, and D set on `Store`. RAM is
/// updated in place; the caller does not need to do a separate writeback.
pub fn walk(
    root_ppn: u32,
    vaddr: u32,
    ram: &mut Ram,
    access: AccessType,
    priv_mode: PrivMode,
) -> Result<WalkResult, PageFault> {
    let cause = access.page_fault_cause();
    let fault = |_| PageFault { cause, vaddr };

    let vpn1 = (vaddr >> 22) & 0x3FF;
    let vpn0 = (vaddr >> 12) & 0x3FF;

    // ── Level 1 ──
    let pte1_addr = (root_ppn << 12).wrapping_add(vpn1 * 4);
    let pte1 = Pte::new(load_pte(ram, pte1_addr).map_err(fault)?);

    if !pte1.valid() {
        return Err(PageFault { cause, vaddr });
    }
    let p1 = pte1.perms();
    if p1.w && !p1.r {
        // Reserved encoding (W=1,R=0).
        return Err(PageFault { cause, vaddr });
    }

    let (leaf, leaf_addr, megapage) = if pte1.is_leaf() {
        // Megapage (4 MiB). PPN[0] must be zero (misaligned superpage check).
        if pte1.ppn0() != 0 {
            return Err(PageFault { cause, vaddr });
        }
        (pte1, pte1_addr, true)
    } else {
        // Walk to level 0.
        let pte0_addr = (pte1.ppn() << 12).wrapping_add(vpn0 * 4);
        let pte0 = Pte::new(load_pte(ram, pte0_addr).map_err(fault)?);
        if !pte0.valid() {
            return Err(PageFault { cause, vaddr });
        }
        let p0 = pte0.perms();
        if p0.w && !p0.r {
            return Err(PageFault { cause, vaddr });
        }
        if !pte0.is_leaf() {
            // PTE at the last level must be a leaf.
            return Err(PageFault { cause, vaddr });
        }
        (pte0, pte0_addr, false)
    };

    let perms = leaf.perms();

    // ── Permission check ──
    match access {
        AccessType::Fetch => {
            if !perms.x {
                return Err(PageFault { cause, vaddr });
            }
        }
        AccessType::Load => {
            if !perms.r {
                return Err(PageFault { cause, vaddr });
            }
        }
        AccessType::Store => {
            if !perms.w {
                return Err(PageFault { cause, vaddr });
            }
        }
    }

    // ── Privilege check ──
    match priv_mode {
        PrivMode::U => {
            if !perms.u {
                return Err(PageFault { cause, vaddr });
            }
        }
        PrivMode::S => {
            // Phase 2 ignores mstatus.SUM: S-mode cannot touch U pages.
            if perms.u {
                return Err(PageFault { cause, vaddr });
            }
        }
        PrivMode::M => {
            // Should not happen — Mmu::translate short-circuits in M-mode.
        }
    }

    // ── A / D writeback (pedagogical: walker does it) ──
    let need_a = !leaf.accessed();
    let need_d = matches!(access, AccessType::Store) && !leaf.dirty();
    if need_a || need_d {
        let mut raw = leaf.raw;
        if need_a {
            raw |= 0x40;
        }
        if need_d {
            raw |= 0x80;
        }
        store_pte(ram, leaf_addr, raw).map_err(fault)?;
    }

    Ok(WalkResult {
        ppn: leaf.ppn(),
        perms,
        global: leaf.global(),
        megapage,
    })
}

fn load_pte(ram: &Ram, addr: u32) -> Result<u32, ()> {
    use crate::falcon::memory::Bus;
    ram.load32(addr).map_err(|_| ())
}

fn store_pte(ram: &mut Ram, addr: u32, val: u32) -> Result<(), ()> {
    use crate::falcon::memory::Bus;
    ram.store32(addr, val).map_err(|_| ())
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn walks_4k_page_happy() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x4000_1234;
        let paddr = 0x0003_0000;
        let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());
        let r = walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
        assert!(!r.megapage);
        assert_eq!(r.ppn, paddr >> 12);
        assert!(r.perms.r && r.perms.w && r.perms.x && r.perms.u);
    }

    #[test]
    fn invalid_pte_faults_with_load_cause() {
        let mut ram = Ram::new(1 << 20);
        // Root table all-zeros at 0x1000 → PTE.V=0.
        let root = 0x1000 >> 12;
        let err = walk(root, 0x1234, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
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
        let err = walk(root, vaddr, &mut ram, AccessType::Store, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 15);
    }

    #[test]
    fn fetch_to_non_x_page_faults() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_0000;
        let paddr = 0x0006_0000;
        // R + W + U (no X).
        let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4 | 0x10);
        let err = walk(root, vaddr, &mut ram, AccessType::Fetch, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 12);
    }

    #[test]
    fn u_mode_cannot_touch_supervisor_page() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_0000;
        let paddr = 0x0006_0000;
        // R + W (no U).
        let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4);
        let err = walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
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
        let r = walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
        assert!(r.megapage);
        assert_eq!(r.ppn, megapage_pa >> 12);
    }

    #[test]
    fn megapage_misaligned_faults() {
        let mut ram = Ram::new(1 << 23);
        let vaddr = 0x0080_1234;
        // PPN with ppn0 != 0 → misaligned superpage.
        let bad_ppn: u32 = (0x0040_0000 >> 12) | 0x1; // ppn0 = 1
        let root_pt_pa: u32 = 0x1000;
        let root = root_pt_pa >> 12;
        let vpn1 = (vaddr >> 22) & 0x3FF;
        let leaf = (bad_ppn << 10) | 0x2 | 0x4 | 0x10 | 0x1;
        ram.store32(root_pt_pa + vpn1 * 4, leaf).unwrap();
        let err = walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }

    #[test]
    fn pt_out_of_ram_faults() {
        let mut ram = Ram::new(0x1000); // 4 KiB total
        // root_ppn points past RAM.
        let err = walk(0x100, 0x1000, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }

    #[test]
    fn walker_sets_a_on_load_and_d_on_store() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_1000;
        let paddr = 0x0008_0000;
        let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());

        // Load: A is set, D stays clear.
        walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
        let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;
        let pte_after_load = ram.load32(leaf_addr).unwrap();
        assert!(pte_after_load & 0x40 != 0, "A bit set");
        assert!(pte_after_load & 0x80 == 0, "D bit clear");

        // Store: D becomes set too.
        walk(root, vaddr, &mut ram, AccessType::Store, PrivMode::U).unwrap();
        let pte_after_store = ram.load32(leaf_addr).unwrap();
        assert!(pte_after_store & 0x80 != 0, "D bit set");
    }

    #[test]
    fn w_without_r_is_reserved_faults() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_0000;
        let paddr = 0x0008_0000;
        // W=1 R=0 U=1 → reserved encoding.
        let root = map_one_page(&mut ram, vaddr, paddr, 0x4 | 0x10);
        let err = walk(root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }
}
