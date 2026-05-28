// falcon/mmu/ — Sv32 virtual memory + unified TLB
//
// Phase 1 status: scaffolding only. `Mmu::translate()` performs identity mapping
// regardless of `satp` until the Sv32 walker lands in Phase 2.

pub mod satp;
pub mod tlb;
pub mod walker;

pub use satp::{PrivMode, Satp, SatpMode};
pub use tlb::{PtePerms, Tlb, TlbConfig, TlbEntry, TlbStats};

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
        }
    }

    /// Translate a virtual address.
    ///
    /// Phase 1: always identity. Phase 2 will probe the TLB and walk the page
    /// table on miss.
    pub fn translate(
        &mut self,
        vaddr: u32,
        _access: AccessType,
    ) -> Result<(u32, u8), PageFault> {
        if !self.enabled || self.satp.mode() == SatpMode::Bare || self.priv_mode == PrivMode::M {
            return Ok((vaddr, 0));
        }
        // Phase 2 will replace this with TLB probe + page-table walk.
        Ok((vaddr, 0))
    }

    pub fn flush(&mut self) {
        self.tlb.flush();
    }
}
