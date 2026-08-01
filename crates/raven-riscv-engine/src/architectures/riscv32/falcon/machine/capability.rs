//! The engine-level capability contract, implemented on the runtime that owns
//! the state.
//!
//! These live on [`Machine`] rather than on the `Architecture` adapter so that
//! *any* holder of the RV32 runtime — the adapter, the TUI, a test — sees the
//! same registers and the same memory through the same traits. Without that,
//! a host driving the runtime directly would have to reach for `cpu()` and
//! re-derive everything the capability layer already spells out.

use super::types::{FRegId, MemWidth, RegId, RegTarget};
use super::{JournaledPipeline, Machine};
use crate::MachineError;
use crate::capability::{
    MemoryInspect, MemoryRegion, RegisterBank, RegisterFile, RegisterFormat, RegisterId,
};

/// Two banks: the integer file every RV32 program uses, and the float file the
/// F extension adds. Both are 32 registers of 32 bits, but nothing outside this
/// module needs to know that.
pub(crate) static BANKS: [RegisterBank; 2] = [
    RegisterBank::integer("x", "Integer", 32, 32),
    RegisterBank {
        prefix: "f",
        label: "Float",
        count: 32,
        bits: 32,
        format: RegisterFormat::Float,
    },
];

pub(crate) const INTEGER_BANK: usize = 0;
pub(crate) const FLOAT_BANK: usize = 1;

/// RISC-V calling-convention names, indexed by register number.
pub(crate) const INTEGER_ALIASES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

pub(crate) const FLOAT_ALIASES: [&str; 32] = [
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2",
    "fa3", "fa4", "fa5", "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9",
    "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

/// Resolve a register name the way the assembler does, so the Run tab accepts
/// exactly the names a source file may use — including `fp` for `x8`, an alias
/// the generated list does not carry.
pub(crate) fn resolve_register(name: &str) -> Option<RegisterId> {
    if let Some(index) = crate::falcon::asm::utils::parse_reg(name) {
        return Some(RegisterId::new(INTEGER_BANK, usize::from(index)));
    }
    crate::falcon::asm::utils::parse_freg(name)
        .map(|index| RegisterId::new(FLOAT_BANK, usize::from(index)))
}

impl<P: JournaledPipeline> RegisterFile for Machine<P> {
    fn banks(&self) -> &[RegisterBank] {
        &BANKS
    }

    fn read(&self, id: RegisterId) -> Option<u64> {
        let index = u8::try_from(id.index).ok()?;
        match id.bank {
            INTEGER_BANK if id.index < 32 => Some(u64::from(self.cpu().read(index))),
            FLOAT_BANK if id.index < 32 => Some(u64::from(self.cpu().fread_bits(index))),
            _ => None,
        }
    }

    /// Journaled, so a host's step-back undoes an edit the same way it undoes
    /// an instruction. `write_reg` is also where x0's immutability is enforced,
    /// so that rule cannot drift from the runtime's own.
    fn write(&mut self, id: RegisterId, value: u64) -> Result<(), MachineError> {
        let value = u32::try_from(value)
            .map_err(|_| MachineError::new("register value exceeds 32 bits"))?;
        let index =
            u8::try_from(id.index).map_err(|_| MachineError::new("no such RV32 register"))?;
        match id.bank {
            INTEGER_BANK if id.index < 32 => {
                let target = RegId::new(index)
                    .map(RegTarget::X)
                    .ok_or_else(|| MachineError::new("no such RV32 register"))?;
                self.write_reg(target, value)
                    .map_err(|e| MachineError::new(e.to_string()))
            }
            FLOAT_BANK if id.index < 32 => {
                let freg =
                    FRegId::new(index).ok_or_else(|| MachineError::new("no such RV32 register"))?;
                self.write_freg(freg, value);
                Ok(())
            }
            _ => Err(MachineError::new("no such RV32 register")),
        }
    }

    fn program_counter(&self) -> u64 {
        u64::from(self.cpu().pc)
    }

    fn set_program_counter(&mut self, value: u64) -> Result<(), MachineError> {
        let pc = u32::try_from(value).map_err(|_| MachineError::new("PC exceeds RV32"))?;
        self.write_reg(RegTarget::Pc, pc)
            .map_err(|e| MachineError::new(e.to_string()))
    }

    fn alias(&self, id: RegisterId) -> Option<&'static str> {
        match id.bank {
            INTEGER_BANK => INTEGER_ALIASES.get(id.index).copied(),
            FLOAT_BANK => FLOAT_ALIASES.get(id.index).copied(),
            _ => None,
        }
    }

    fn resolve(&self, name: &str) -> Option<RegisterId> {
        resolve_register(name)
    }
}

impl<P: JournaledPipeline> MemoryInspect for Machine<P> {
    fn size(&self) -> u64 {
        self.mem().ram().data_len() as u64
    }

    /// Reads through `effective_read8`, so a byte still sitting dirty in a
    /// cache line shows its real value rather than the stale one in RAM.
    fn peek(&self, address: u64, bytes: usize) -> Vec<u8> {
        let Ok(start) = u32::try_from(address) else {
            return Vec::new();
        };
        (0..bytes)
            .map_while(|offset| {
                let at = start.checked_add(u32::try_from(offset).ok()?)?;
                self.mem().effective_read8(at).ok()
            })
            .collect()
    }

    fn poke(&mut self, address: u64, bytes: &[u8]) -> Result<(), MachineError> {
        let start =
            u32::try_from(address).map_err(|_| MachineError::new("address exceeds RV32"))?;
        for (offset, value) in bytes.iter().copied().enumerate() {
            let at = start
                .checked_add(u32::try_from(offset).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            self.write_mem(at, MemWidth::B1, u64::from(value))
                .map_err(|e| MachineError::new(e.to_string()))?;
        }
        Ok(())
    }

    /// Heap comes from the program break and the stack from the pointer, both
    /// clamped to the last readable byte — RV32 starts `sp` one past the top of
    /// RAM to mean "empty". A runtime holding no image cannot name where `.data`
    /// begins, so the caller's PC stands in; the [`crate::Machine`] adapter,
    /// which does keep the image, reports the real segment.
    fn regions(&self) -> Vec<MemoryRegion> {
        let top = MemoryInspect::size(self).saturating_sub(1);
        vec![
            MemoryRegion::new("Data", u64::from(self.cpu().pc).min(top)),
            MemoryRegion::new("Heap", u64::from(self.cpu().heap_break).min(top)),
            MemoryRegion::new("Stack", u64::from(self.cpu().read(2)).min(top)),
        ]
    }
}

fn overflow() -> MachineError {
    MachineError::new("address overflow")
}
