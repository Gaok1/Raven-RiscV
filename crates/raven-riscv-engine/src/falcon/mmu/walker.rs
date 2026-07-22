// falcon/mmu/walker.rs â€” Sv32 2-level page-table walker.
//
// Sv32 (RISC-V volume 2, Â§4.3): two-level table, 10+10+12 split, 4 KiB / 4 MiB
// page sizes. `walk` is invoked by `Mmu::translate` on a TLB miss. On success
// the walker auto-sets A (and D on Store) directly in the PTE â€” a pedagogical
// shortcut that avoids a second trap round-trip.

#![allow(dead_code)]

use super::tlb::PtePerms;
use super::{AccessType, PageFault, PrivMode};
use crate::falcon::memory::Ram;

/// Sv32 PTE bit layout (32 bits):
///   bits 31-10: PPN (22 bits, combined PPN[1]:PPN[0])  â† `paddr >> 12`
///   bits  9- 8: RSW (software reserved)
///   bit 7 : D (dirty)
///   bit 6 : A (accessed)
///   bit 5 : G (global)
///   bit 4 : U (user-accessible)
///   bit 3 : X (execute)
///   bit 2 : W (write)
///   bit 1 : R (read)
///   bit 0 : V (valid)
///
/// Encoding: `PTE = (ppn << 10) | flags`, where `ppn = physical_address >> 12`.
/// A non-leaf (pointer) PTE has R=W=X=0 and V=1; a leaf has at least one of R/W/X set.
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

/// Result of a successful page-table walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalkResult {
    /// Leaf PPN (frame number = `paddr >> 12`), as stored in the PTE. For a
    /// superpage the low `(page_bits-12)` bits are zero; the caller reassembles
    /// the physical address using `page_bits`.
    pub ppn: u32,
    pub perms: PtePerms,
    pub global: bool,
    /// log2 of the page size in bytes (e.g. 12 = 4 KiB, 22 = 4 MiB superpage).
    pub page_bits: u8,
    /// Level at which the leaf PTE was found (0 = top).
    pub leaf_level: u8,
}

/// Walk the page table rooted at `root_ppn` for `vaddr`, using the parametric
/// [`PagingScheme`](super::PagingScheme) (number of levels, per-level index
/// widths and page-offset width â€” Sv32 is the `offset=12, levels=[10,10]`
/// preset). A leaf PTE (R|W|X set) may occur at any level, yielding a superpage.
///
/// On success the leaf PTE has its A bit set, and D set on `Store`. RAM is
/// updated in place; the caller does not need to do a separate writeback.
pub fn walk(
    scheme: &super::PagingScheme,
    root_ppn: u32,
    vaddr: u32,
    ram: &mut Ram,
    access: AccessType,
    priv_mode: PrivMode,
) -> Result<WalkResult, PageFault> {
    let cause = access.page_fault_cause();
    let fault = |_| PageFault { cause, vaddr };

    let offset_bits = scheme.offset_bits as u32;
    let n = scheme.level_bits.len();
    // `shift` tracks the low bit position of the current level's index. It
    // starts at the top (offset + all index bits == 32) and descends.
    let mut shift = offset_bits + scheme.level_bits.iter().map(|b| *b as u32).sum::<u32>();
    // Page tables are frame-granular (4 KiB) regardless of the page offset.
    let mut table_pa = root_ppn << 12;

    for level in 0..n {
        let lb = scheme.level_bits[level] as u32;
        shift -= lb;
        let idx = (vaddr >> shift) & ((1u32 << lb) - 1);
        let pte_addr = table_pa.wrapping_add(idx * 4);
        let pte = Pte::new(load_pte(ram, pte_addr).map_err(fault)?);

        if !pte.valid() {
            return Err(PageFault { cause, vaddr });
        }
        let p = pte.perms();
        if p.w && !p.r {
            // Reserved encoding (W=1, R=0).
            return Err(PageFault { cause, vaddr });
        }

        if !pte.is_leaf() {
            // Pointer PTE â†’ descend to the next level.
            table_pa = pte.ppn() << 12;
            continue;
        }

        // â”€â”€ Leaf (possibly a superpage at this level) â”€â”€
        // `page_bits == shift`: offset + the index bits of all lower levels.
        let page_bits = shift;
        // Misaligned superpage: the frame must be aligned to the page size, i.e.
        // the low `(page_bits - 12)` bits of the PPN must be zero.
        if page_bits > 12 {
            let align_mask = (1u32 << (page_bits - 12)) - 1;
            if pte.ppn() & align_mask != 0 {
                return Err(PageFault { cause, vaddr });
            }
        }

        // â”€â”€ Permission check â”€â”€
        match access {
            AccessType::Fetch => {
                if !p.x {
                    return Err(PageFault { cause, vaddr });
                }
            }
            AccessType::Load => {
                if !p.r {
                    return Err(PageFault { cause, vaddr });
                }
            }
            AccessType::Store => {
                if !p.w {
                    return Err(PageFault { cause, vaddr });
                }
            }
        }

        // â”€â”€ Privilege check â”€â”€
        match priv_mode {
            PrivMode::U => {
                if !p.u {
                    return Err(PageFault { cause, vaddr });
                }
            }
            PrivMode::S => {
                // Phase 2 ignores mstatus.SUM: S-mode cannot touch U pages.
                if p.u {
                    return Err(PageFault { cause, vaddr });
                }
            }
            PrivMode::M => {
                // Should not happen â€” Mmu::translate short-circuits in M-mode.
            }
        }

        // â”€â”€ A / D writeback (pedagogical: walker does it) â”€â”€
        let need_a = !pte.accessed();
        let need_d = matches!(access, AccessType::Store) && !pte.dirty();
        if need_a || need_d {
            let mut raw = pte.raw;
            if need_a {
                raw |= 0x40;
            }
            if need_d {
                raw |= 0x80;
            }
            store_pte(ram, pte_addr, raw).map_err(fault)?;
        }

        return Ok(WalkResult {
            ppn: pte.ppn(),
            perms: p,
            global: pte.global(),
            page_bits: page_bits as u8,
            leaf_level: level as u8,
        });
    }

    // Ran out of levels without finding a leaf (last-level PTE was a pointer).
    Err(PageFault { cause, vaddr })
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
        let r = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
        assert_eq!(r.page_bits, 12);
        assert_eq!(r.ppn, paddr >> 12);
        assert!(r.perms.r && r.perms.w && r.perms.x && r.perms.u);
    }

    #[test]
    fn invalid_pte_faults_with_load_cause() {
        let mut ram = Ram::new(1 << 20);
        // Root table all-zeros at 0x1000 â†’ PTE.V=0.
        let root = 0x1000 >> 12;
        let err = walk(&PagingScheme::sv32(), root, 0x1234, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
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
        let err = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Store, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 15);
    }

    #[test]
    fn fetch_to_non_x_page_faults() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_0000;
        let paddr = 0x0006_0000;
        // R + W + U (no X).
        let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4 | 0x10);
        let err = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Fetch, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 12);
    }

    #[test]
    fn u_mode_cannot_touch_supervisor_page() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_0000;
        let paddr = 0x0006_0000;
        // R + W (no U).
        let root = map_one_page(&mut ram, vaddr, paddr, 0x2 | 0x4);
        let err = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
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
        let r = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
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
        let err = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }

    #[test]
    fn pt_out_of_ram_faults() {
        let mut ram = Ram::new(0x1000); // 4 KiB total
        // root_ppn points past RAM.
        let err = walk(&PagingScheme::sv32(), 0x100, 0x1000, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }

    #[test]
    fn walker_sets_a_on_load_and_d_on_store() {
        let mut ram = Ram::new(1 << 20);
        let vaddr = 0x0040_1000;
        let paddr = 0x0008_0000;
        let root = map_one_page(&mut ram, vaddr, paddr, p_rwxu());

        // Load: A is set, D stays clear.
        walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap();
        let leaf_addr = 0x2000 + ((vaddr >> 12) & 0x3FF) * 4;
        let pte_after_load = ram.load32(leaf_addr).unwrap();
        assert!(pte_after_load & 0x40 != 0, "A bit set");
        assert!(pte_after_load & 0x80 == 0, "D bit clear");

        // Store: D becomes set too.
        walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Store, PrivMode::U).unwrap();
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
        let err = walk(&PagingScheme::sv32(), root, vaddr, &mut ram, AccessType::Load, PrivMode::U).unwrap_err();
        assert_eq!(err.cause, 13);
    }
}

