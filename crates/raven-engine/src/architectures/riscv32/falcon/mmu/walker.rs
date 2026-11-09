// falcon/mmu/walker.rs — Sv32 2-level page-table walker.
//
// Sv32 (RISC-V volume 2, §4.3): a two-level table, a 10+10+12 split, 4 KiB
// pages and 4 MiB megapages. `walk` runs on a TLB miss and reports every PTE it
// read, so a viewer can replay the walk. On success it sets A (and D on a
// store) in the leaf PTE itself — the hardware-managed flavour the spec allows,
// which spares the program a trap round-trip.

use super::tlb::PtePerms;
use super::trace::{FaultReason, WalkStep};
use super::{AccessType, LEVELS, PAGE_BITS, PrivMode, VPN_BITS};
use crate::falcon::memory::{Bus, Ram};

/// Sv32 PTE bit layout (32 bits):
///   bits 31-10: PPN (22 bits, `PPN[1]:PPN[0]`) ← `paddr >> 12`
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
/// A pointer PTE has R=W=X=0 and V=1; a leaf has at least one of R/W/X set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pte {
    pub raw: u32,
}

pub const PTE_V: u32 = 1 << 0;
pub const PTE_R: u32 = 1 << 1;
pub const PTE_W: u32 = 1 << 2;
pub const PTE_X: u32 = 1 << 3;
pub const PTE_U: u32 = 1 << 4;
pub const PTE_G: u32 = 1 << 5;
pub const PTE_A: u32 = 1 << 6;
pub const PTE_D: u32 = 1 << 7;

impl Pte {
    pub fn new(raw: u32) -> Self {
        Self { raw }
    }
    pub fn valid(self) -> bool {
        self.raw & PTE_V != 0
    }
    pub fn perms(self) -> PtePerms {
        PtePerms {
            r: self.raw & PTE_R != 0,
            w: self.raw & PTE_W != 0,
            x: self.raw & PTE_X != 0,
            u: self.raw & PTE_U != 0,
        }
    }
    pub fn global(self) -> bool {
        self.raw & PTE_G != 0
    }
    pub fn accessed(self) -> bool {
        self.raw & PTE_A != 0
    }
    pub fn dirty(self) -> bool {
        self.raw & PTE_D != 0
    }
    /// Combined 22-bit PPN (`PPN[1]:PPN[0]`).
    pub fn ppn(self) -> u32 {
        (self.raw >> 10) & 0x003F_FFFF
    }
    /// A leaf PTE has at least one of R/W/X set.
    pub fn is_leaf(self) -> bool {
        self.raw & (PTE_R | PTE_W | PTE_X) != 0
    }
    /// W without R is reserved for future use, and faults.
    pub fn is_reserved(self) -> bool {
        self.raw & (PTE_R | PTE_W) == PTE_W
    }
    /// Physical address of the table a pointer PTE names, or of a leaf's frame.
    pub fn target(self) -> u32 {
        self.ppn() << PAGE_BITS
    }
}

/// Where the walker reads and writes PTEs. Plain [`Ram`] for tests; the cache
/// controller answers with the value the caches hold, so a PTE the program
/// just stored is seen even while it still sits dirty in the D-cache.
pub trait PteMemory {
    fn read_pte(&mut self, pa: u32) -> Option<u32>;
    fn write_pte(&mut self, pa: u32, value: u32) -> bool;
}

impl PteMemory for Ram {
    fn read_pte(&mut self, pa: u32) -> Option<u32> {
        self.load32(pa).ok()
    }
    fn write_pte(&mut self, pa: u32, value: u32) -> bool {
        self.store32(pa, value).is_ok()
    }
}

/// Whether a page with `perms` may be touched by `access` from `priv_mode`.
/// One check for both a TLB hit and a fresh walk. `mstatus.SUM`/`MXR` are not
/// modelled: S-mode never touches U pages.
pub fn check_access(
    perms: PtePerms,
    access: AccessType,
    priv_mode: PrivMode,
) -> Result<(), FaultReason> {
    let allowed = match access {
        AccessType::Fetch => perms.x,
        AccessType::Load => perms.r,
        AccessType::Store => perms.w,
    };
    if !allowed {
        return Err(FaultReason::NotPermitted);
    }
    match priv_mode {
        PrivMode::U if !perms.u => Err(FaultReason::SupervisorPage),
        PrivMode::S if perms.u => Err(FaultReason::UserPage),
        _ => Ok(()),
    }
}

/// The leaf a successful walk ended on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// Leaf PPN as the PTE stores it. A megapage's low 10 bits are zero.
    pub ppn: u32,
    pub perms: PtePerms,
    pub global: bool,
    pub megapage: bool,
    /// A was clear before this walk: the first touch since the PTE was written.
    pub first_touch: bool,
    /// D is set in the PTE now (it was, or this walk set it).
    pub dirty: bool,
    pub set_accessed: bool,
    pub set_dirty: bool,
}

/// Everything one walk did: the PTEs it read, outermost first, and where it
/// ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    pub steps: [WalkStep; LEVELS],
    pub step_count: u8,
    pub result: Result<Leaf, FaultReason>,
}

impl Walk {
    pub fn steps(&self) -> &[WalkStep] {
        &self.steps[..self.step_count as usize]
    }
}

/// Walk the Sv32 table rooted at `root_ppn` for `vaddr`.
///
/// With `update` set, a successful walk writes A (and D on a store) back into
/// the leaf PTE; a read-only "what if" walk for a viewer passes `false`.
pub fn walk<M: PteMemory + ?Sized>(
    root_ppn: u32,
    vaddr: u32,
    access: AccessType,
    priv_mode: PrivMode,
    mem: &mut M,
    update: bool,
) -> Walk {
    let mut out = Walk {
        steps: [WalkStep::default(); LEVELS],
        step_count: 0,
        result: Err(FaultReason::NoLeaf),
    };
    let mut table = root_ppn << PAGE_BITS;

    // Level 1 is the root (VPN[1]); level 0 is the leaf table (VPN[0]).
    for level in (0..LEVELS).rev() {
        let shift = PAGE_BITS + VPN_BITS * level as u32;
        let index = (vaddr >> shift) & ((1 << VPN_BITS) - 1);
        let pte_addr = table.wrapping_add(index * 4);
        let Some(raw) = mem.read_pte(pte_addr) else {
            out.result = Err(FaultReason::OutsideMemory);
            out.steps[out.step_count as usize] = WalkStep {
                level: level as u8,
                index: index as u16,
                pte_addr,
                ..WalkStep::default()
            };
            out.step_count += 1;
            return out;
        };
        let pte = Pte::new(raw);
        out.steps[out.step_count as usize] = WalkStep {
            level: level as u8,
            index: index as u16,
            pte_addr,
            pte: raw,
            ..WalkStep::default()
        };
        out.step_count += 1;

        if !pte.valid() {
            out.result = Err(FaultReason::NotValid);
            return out;
        }
        if pte.is_reserved() {
            out.result = Err(FaultReason::Reserved);
            return out;
        }
        if !pte.is_leaf() {
            if level == 0 {
                out.result = Err(FaultReason::NoLeaf);
                return out;
            }
            table = pte.target();
            continue;
        }

        // A leaf at level 1 is a megapage: its frame must be 4 MiB-aligned.
        let megapage = level == 1;
        if megapage && pte.ppn() & ((1 << VPN_BITS) - 1) != 0 {
            out.result = Err(FaultReason::Misaligned);
            return out;
        }
        if let Err(reason) = check_access(pte.perms(), access, priv_mode) {
            out.result = Err(reason);
            return out;
        }

        let set_accessed = !pte.accessed();
        let set_dirty = matches!(access, AccessType::Store) && !pte.dirty();
        if update && (set_accessed || set_dirty) {
            let mut updated = raw;
            if set_accessed {
                updated |= PTE_A;
            }
            if set_dirty {
                updated |= PTE_D;
            }
            if !mem.write_pte(pte_addr, updated) {
                out.result = Err(FaultReason::OutsideMemory);
                return out;
            }
        }
        out.result = Ok(Leaf {
            ppn: pte.ppn(),
            perms: pte.perms(),
            global: pte.global(),
            megapage,
            first_touch: set_accessed,
            dirty: pte.dirty() || set_dirty,
            set_accessed: update && set_accessed,
            set_dirty: update && set_dirty,
        });
        return out;
    }
    out
}

#[cfg(test)]
#[path = "../../../../../tests/unit/falcon_mmu_walker.rs"]
mod tests;
