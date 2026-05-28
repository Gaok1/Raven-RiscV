// falcon/mmu/satp.rs — RV32 satp CSR + privilege mode
//
// Sv32 satp layout (32 bits):
//   bit 31    : MODE (0 = Bare, 1 = Sv32)
//   bits 30-22: ASID (9 bits)
//   bits 21-0 : PPN  (22 bits, root page-table physical page number)

#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PrivMode {
    #[default]
    M,
    S,
    U,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SatpMode {
    Bare,
    Sv32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Satp {
    pub raw: u32,
}

impl Satp {
    pub fn new(raw: u32) -> Self {
        Self { raw }
    }

    pub fn mode(self) -> SatpMode {
        if (self.raw >> 31) & 1 == 1 {
            SatpMode::Sv32
        } else {
            SatpMode::Bare
        }
    }

    pub fn asid(self) -> u16 {
        ((self.raw >> 22) & 0x1FF) as u16
    }

    /// Root page-table PPN (22 bits). The byte address of the root PT is
    /// `ppn() << 12`.
    pub fn ppn(self) -> u32 {
        self.raw & 0x003F_FFFF
    }
}
