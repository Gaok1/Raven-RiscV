// falcon/mmu/ — Sv32 virtual memory + unified TLB
//
// `Mmu::translate` is the single seam through which every cache-aware access
// (fetch, load, store) reaches RAM. When VM is disabled, or satp=Bare, or the
// hart is in M-mode, it short-circuits to identity. Otherwise it probes the
// TLB; on a miss it walks the Sv32 page table, installs the resulting entry,
// and surfaces page faults to the caller.

pub mod satp;
pub mod tlb;
pub mod walker;

pub use satp::{PrivMode, Satp, SatpMode};
pub use tlb::{PtePerms, Tlb, TlbConfig, TlbEntry, TlbStats};

use crate::falcon::memory::{Bus, Ram};

/// What kind of access is being translated. Determines which permission bit is
/// checked and which fault code is raised on failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessType {
    Fetch,
    Load,
    Store,
}

impl AccessType {
    /// RISC-V cause code for a page fault on this access type.
    pub fn page_fault_cause(self) -> u32 {
        match self {
            AccessType::Fetch => 12,
            AccessType::Load => 13,
            AccessType::Store => 15,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFault {
    pub cause: u32,
    pub vaddr: u32,
}

pub struct Mmu {
    pub tlb: Tlb,
    pub satp: Satp,
    pub priv_mode: PrivMode,
    /// Mirror of `vm_enabled`. When false, `translate()` is pure identity and
    /// no TLB state is touched (zero overhead path).
    pub enabled: bool,
    /// When true, translation is applied even in M-mode. Used by the
    /// didactic standard mode so any program sees TLB activity without
    /// needing explicit page-table setup code.
    pub force_translate: bool,
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new(TlbConfig::default())
    }
}

impl Mmu {
    pub fn new(cfg: TlbConfig) -> Self {
        Self {
            tlb: Tlb::new(cfg),
            satp: Satp::default(),
            priv_mode: PrivMode::M,
            enabled: false,
            force_translate: false,
        }
    }

    /// Write 1024 Sv32 megapage PTEs at `root_pa`, creating an identity map
    /// that covers the full 4 GiB address space (VA == PA, RWX+U+Valid).
    /// Used by the standard VM mode so programs see TLB activity without
    /// manual page-table setup.
    pub fn install_identity_megapages(ram: &mut Ram, root_pa: u32) {
        for i in 0u32..1024 {
            let pte = (i << 10) | 0x1F; // PPN=i, R|W|X|U|V
            let _ = ram.store32(root_pa + i * 4, pte);
        }
    }

    /// Translate a virtual address.
    ///
    /// Returns `(paddr, extra_stall_cycles)`. The stall is `hit_latency` on a
    /// TLB hit and `miss_penalty` on a walk. RAM is mutable because the walker
    /// auto-sets A/D on the leaf PTE.
    pub fn translate(
        &mut self,
        vaddr: u32,
        access: AccessType,
        ram: &mut Ram,
    ) -> Result<(u32, u8), PageFault> {
        // Short-circuit: VM off or satp=Bare → identity (no TLB touch).
        if !self.enabled || self.satp.mode() == SatpMode::Bare {
            return Ok((vaddr, 0));
        }
        // M-mode bypasses the MMU on real hardware. In the didactic standard
        // mode (force_translate), we skip this bypass so TLB activity is
        // visible for any program without privilege-level boilerplate.
        if self.priv_mode == PrivMode::M && !self.force_translate {
            return Ok((vaddr, 0));
        }

        let vpn = vaddr >> 12;
        let asid = self.satp.asid();

        // TLB probe. A Store on a non-dirty entry is forced through the walker
        // so the PTE gets its D bit set in RAM — mirrors real hardware.
        if let Some(entry) = self.tlb.probe(vpn, asid) {
            let needs_d_writeback = matches!(access, AccessType::Store) && !entry.dirty;
            if !needs_d_writeback && self.check_perms(&entry, access).is_ok() {
                self.tlb.stats.hits += 1;
                let paddr = build_paddr(&entry, vaddr);
                return Ok((paddr, self.tlb.config.hit_latency));
            }
            // Otherwise fall through and re-walk — either to fault on perms or
            // to set the D bit. The walker will reinstall the entry.
            if self.check_perms(&entry, access).is_err() && !needs_d_writeback {
                self.tlb.stats.page_faults += 1;
                return Err(PageFault {
                    cause: access.page_fault_cause(),
                    vaddr,
                });
            }
        }

        self.tlb.stats.misses += 1;
        let res = match walker::walk(self.satp.ppn(), vaddr, ram, access, self.priv_mode) {
            Ok(r) => r,
            Err(e) => {
                self.tlb.stats.page_faults += 1;
                return Err(e);
            }
        };

        let entry = TlbEntry {
            valid: true,
            vpn: if res.megapage { vpn & !0x3FF } else { vpn },
            ppn: res.ppn,
            asid,
            perms: res.perms,
            global: res.global,
            accessed: true,
            dirty: matches!(access, AccessType::Store),
            megapage: res.megapage,
            age: 0,
            ref_bit: false,
        };
        self.tlb.install(entry);

        let paddr = build_paddr(&entry, vaddr);
        Ok((paddr, self.tlb.config.miss_penalty))
    }

    fn check_perms(&self, entry: &TlbEntry, access: AccessType) -> Result<(), ()> {
        match access {
            AccessType::Fetch if !entry.perms.x => return Err(()),
            AccessType::Load if !entry.perms.r => return Err(()),
            AccessType::Store if !entry.perms.w => return Err(()),
            _ => {}
        }
        match self.priv_mode {
            PrivMode::U if !entry.perms.u => Err(()),
            PrivMode::S if entry.perms.u => Err(()), // ignore mstatus.SUM in Phase 2
            _ => Ok(()),
        }
    }

    pub fn flush(&mut self) {
        self.tlb.flush();
    }
}

fn build_paddr(entry: &TlbEntry, vaddr: u32) -> u32 {
    if entry.megapage {
        // PPN[1] from entry (PPN[0] is zero by Sv32 superpage rule); 22 low bits
        // from vaddr.
        (entry.ppn << 12) | (vaddr & 0x003F_FFFF)
    } else {
        (entry.ppn << 12) | (vaddr & 0xFFF)
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(mmu.tlb.stats.misses, 2, "after flush the second probe misses again");
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
        assert_eq!(mmu.tlb.stats.hits, hits_before, "store on clean entry must re-walk");
        assert_eq!(mmu.tlb.stats.misses, misses_before + 1);

        // Subsequent Store hits the dirty entry.
        let hits_now = mmu.tlb.stats.hits;
        mmu.translate(vaddr, AccessType::Store, &mut ram).unwrap();
        assert_eq!(mmu.tlb.stats.hits, hits_now + 1);
    }
}
