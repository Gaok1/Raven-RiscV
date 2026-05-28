// falcon/mmu/walker.rs — Sv32 2-level page-table walker.
//
// Phase 1 scaffolding: walker is not yet wired in — `Mmu::translate()` is
// identity. This module defines the PTE layout helpers that Phase 2 will use
// to read the root PT from RAM.

#![allow(dead_code)]

use super::tlb::PtePerms;

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
