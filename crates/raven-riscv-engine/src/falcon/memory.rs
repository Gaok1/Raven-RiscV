// falcon/memory.rs
use crate::falcon::errors::FalconError;
use crate::falcon::mmu::{AccessType, PageFault, PrivMode};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmoOp {
    Swap,
    Add,
    Xor,
    And,
    Or,
    Max,
    Min,
    MaxU,
    MinU,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reservation {
    addr: u32,
}

fn reservation_addr(addr: u32) -> u32 {
    addr & !0x3
}

fn overlaps_reserved_word(addr: u32, size: usize, reserved_addr: u32) -> bool {
    let start = addr as u64;
    let end = start.saturating_add(size as u64);
    let rstart = reserved_addr as u64;
    let rend = rstart + 4;
    start < rend && rstart < end
}

fn invalidate_reservations(reservations: &mut HashMap<u32, Reservation>, addr: u32, size: usize) {
    reservations.retain(|_, res| !overlaps_reserved_word(addr, size, res.addr));
}

/// Shared helper for `Bus::user_load*` / `Bus::user_store*` default impls:
/// translate or convert a `PageFault` into the matching `FalconError::Trap`.
fn translate_or_trap<B: Bus + ?Sized>(
    bus: &mut B,
    vaddr: u32,
    access: AccessType,
) -> Result<u32, FalconError> {
    match bus.translate(vaddr, access) {
        Ok((pa, _stall)) => Ok(pa),
        Err(pf) => Err(FalconError::Trap {
            cause: pf.cause,
            tval: pf.vaddr,
            vaddr: pf.vaddr,
        }),
    }
}

fn amo_apply(op: AmoOp, old: u32, operand: u32) -> u32 {
    match op {
        AmoOp::Swap => operand,
        AmoOp::Add => old.wrapping_add(operand),
        AmoOp::Xor => old ^ operand,
        AmoOp::And => old & operand,
        AmoOp::Or => old | operand,
        AmoOp::Max => (old as i32).max(operand as i32) as u32,
        AmoOp::Min => (old as i32).min(operand as i32) as u32,
        AmoOp::MaxU => old.max(operand),
        AmoOp::MinU => old.min(operand),
    }
}

/// Memory bus abstraction.
///
/// ## Contrato de leitura (por que load* Ã© cache-aware)
///
/// `load*` retorna o valor *mais atual* no endereÃ§o â€” idÃªntico ao que o
/// software veria ao ler aquele endereÃ§o. Para `Ram` isso Ã© simplesmente a
/// RAM. Para `CacheController`, linhas sujas da D-cache tÃªm prioridade sobre
/// a RAM (write-back): `load*` delega a `effective_read*`, sem side-effects
/// de stats.
///
/// ## SeparaÃ§Ã£o de responsabilidades
///
/// | MÃ©todo          | SemÃ¢ntica                                      | Quem usa          |
/// |-----------------|------------------------------------------------|-------------------|
/// | `load*`         | Leitura cache-aware, sem stats                 | syscalls, decoder |
/// | `store*`        | Escrita via D-cache                            | exec.rs stores    |
/// | `fetch32`       | Busca de instruÃ§Ã£o via I-cache                 | exec.rs fetch     |
/// | `dcache_read*`  | Leitura com tracking de stats de D-cache       | exec.rs loads     |
/// | `peek*` (CC)    | RAM bruta â€” apenas no `CacheController`, para UI |                 |
pub trait Bus {
    /// Total addressable RAM bytes behind this bus.
    fn mem_len(&self) -> u32;

    /// Leitura cache-aware: retorna o valor mais atual no endereÃ§o.
    /// ImplementaÃ§Ãµes com cache devem verificar linhas sujas antes da RAM.
    fn load8(&self, addr: u32) -> Result<u8, FalconError>;
    fn load16(&self, addr: u32) -> Result<u16, FalconError>;
    fn load32(&self, addr: u32) -> Result<u32, FalconError>;

    /// Translating cache-aware read. Used by syscalls and other "user space"
    /// code paths that receive a virtual address from the guest and must
    /// honour the MMU (when enabled). The default impl translates via the
    /// `translate()` hook then forwards to `load*`, which makes it a no-op for
    /// buses without an MMU (the trait's default `translate` is identity).
    fn user_load8(&mut self, vaddr: u32) -> Result<u8, FalconError> {
        let pa = translate_or_trap(self, vaddr, AccessType::Load)?;
        self.load8(pa)
    }
    fn user_load16(&mut self, vaddr: u32) -> Result<u16, FalconError> {
        let pa = translate_or_trap(self, vaddr, AccessType::Load)?;
        self.load16(pa)
    }
    fn user_load32(&mut self, vaddr: u32) -> Result<u32, FalconError> {
        let pa = translate_or_trap(self, vaddr, AccessType::Load)?;
        self.load32(pa)
    }

    fn store8(&mut self, addr: u32, val: u8) -> Result<(), FalconError>;
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError>;
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError>;

    /// Busca de instruÃ§Ã£o â€” sobrescrever para rotear pela I-cache.
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        self.load32(addr)
    }

    /// Leitura de dado com tracking de stats de D-cache â€” sobrescrever em CacheController.
    fn dcache_read8(&mut self, addr: u32) -> Result<u8, FalconError> {
        self.load8(addr)
    }
    fn dcache_read16(&mut self, addr: u32) -> Result<u16, FalconError> {
        self.load16(addr)
    }
    fn dcache_read32(&mut self, addr: u32) -> Result<u32, FalconError> {
        self.load32(addr)
    }

    /// Total simulated cycles (instruction cycles + cache penalties).
    /// CacheController overrides this; other Bus impls return 0.
    fn total_cycles(&self) -> u64 {
        0
    }

    fn fence(&mut self) -> Result<(), FalconError> {
        Ok(())
    }

    fn fence_i(&mut self) -> Result<(), FalconError> {
        Ok(())
    }

    fn lr_w(&mut self, hart_id: u32, addr: u32) -> Result<u32, FalconError>;
    fn sc_w(&mut self, hart_id: u32, addr: u32, val: u32) -> Result<bool, FalconError>;
    fn amo_w(
        &mut self,
        hart_id: u32,
        addr: u32,
        op: AmoOp,
        operand: u32,
    ) -> Result<u32, FalconError>;

    /// Translate a virtual address to a physical address.
    ///
    /// Returns `(paddr, extra_stall_cycles)`. The default impl is identity with
    /// zero stall â€” the no-MMU path used by `Ram` and by `CacheController`
    /// when `vm_enabled` is off. The MMU-aware impl on `CacheController`
    /// overrides this once Phase 2 wires the Sv32 walker.
    fn translate(
        &mut self,
        vaddr: u32,
        _access: AccessType,
    ) -> Result<(u32, u8), PageFault> {
        Ok((vaddr, 0))
    }

    /// Flush every TLB entry (invoked on `satp` write and `sfence.vma`).
    fn tlb_flush(&mut self) {}

    /// Flush the TLB entry that maps `vaddr` (invoked on `sfence.vma rs1, _`
    /// with a non-zero `rs1`). Default no-op for buses without a real MMU.
    fn tlb_flush_vaddr(&mut self, _vaddr: u32) {}

    /// Push a new `satp` value to the MMU. Default no-op for buses without a
    /// real MMU. Implementations should also flush the TLB.
    fn set_satp(&mut self, _val: u32) {}

    /// Update the hart's current privilege level (used by `mret`/trap entry).
    fn set_priv_mode(&mut self, _mode: PrivMode) {}
}

pub struct Ram {
    data: Vec<u8>,
    reservations: HashMap<u32, Reservation>,
    /// When `Some`, every byte mutated by `store8` first appends its
    /// `(addr, old_byte)` pre-image here. This is the single chokepoint for
    /// *all* runtime RAM writes â€” direct stores, write-through, and dirty-line
    /// writebacks all funnel through `store8` â€” so the `Machine` journal can
    /// rewind a step by replaying these pre-images in reverse. `None` (the
    /// default) means recording is off and `store8` pays nothing.
    write_log: Option<Vec<(u32, u8)>>,
}

impl Ram {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
            reservations: HashMap::new(),
            write_log: None,
        }
    }

    /// Number of bytes in this RAM.
    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    /// Copy `len` bytes from `src` into this RAM starting at byte offset 0.
    pub fn copy_from_slice(&mut self, src: &[u8], len: usize) {
        let len = len.min(self.data.len()).min(src.len());
        self.data[..len].copy_from_slice(&src[..len]);
    }

    /// Return a read-only view of the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    // â”€â”€ Step-journal recording (see the `falcon::machine` module) â”€â”€

    /// Start capturing `(addr, old_byte)` pre-images for every subsequent
    /// `store8`. Replaces any in-flight log.
    pub fn begin_recording(&mut self) {
        self.write_log = Some(Vec::new());
    }

    /// Stop recording and return the pre-images in write order (oldest first).
    /// Replay them in *reverse* â€” each via [`Ram::poke8`] â€” to undo the writes.
    pub fn take_recording(&mut self) -> Vec<(u32, u8)> {
        self.write_log.take().unwrap_or_default()
    }

    /// Raw byte write that bypasses both recording and reservation tracking.
    /// Used only to restore pre-images during a step-back; out-of-bounds
    /// addresses are silently ignored (the address was valid when captured).
    pub fn poke8(&mut self, addr: u32, val: u8) {
        if let Some(slot) = self.data.get_mut(addr as usize) {
            *slot = val;
        }
    }
}

impl Bus for Ram {
    fn mem_len(&self) -> u32 {
        self.data.len().min(u32::MAX as usize) as u32
    }

    fn load8(&self, a: u32) -> Result<u8, FalconError> {
        self.data
            .get(a as usize)
            .copied()
            .ok_or_else(|| FalconError::Bus(format!("address 0x{a:08X} out of bounds")))
    }
    fn load16(&self, a: u32) -> Result<u16, FalconError> {
        Ok(u16::from_le_bytes([self.load8(a)?, self.load8(a + 1)?]))
    }
    fn load32(&self, a: u32) -> Result<u32, FalconError> {
        Ok(u32::from_le_bytes([
            self.load8(a)?,
            self.load8(a + 1)?,
            self.load8(a + 2)?,
            self.load8(a + 3)?,
        ]))
    }
    fn store8(&mut self, a: u32, v: u8) -> Result<(), FalconError> {
        if let Some(slot) = self.data.get_mut(a as usize) {
            if let Some(log) = self.write_log.as_mut() {
                log.push((a, *slot));
            }
            *slot = v;
            invalidate_reservations(&mut self.reservations, a, 1);
            Ok(())
        } else {
            Err(FalconError::Bus(format!("address 0x{a:08X} out of bounds")))
        }
    }
    fn store16(&mut self, a: u32, v: u16) -> Result<(), FalconError> {
        let b = v.to_le_bytes();
        self.store8(a, b[0])?;
        self.store8(a + 1, b[1])
    }
    fn store32(&mut self, a: u32, v: u32) -> Result<(), FalconError> {
        let b = v.to_le_bytes();
        for i in 0..4 {
            self.store8(a + i as u32, b[i])?;
        }
        Ok(())
    }

    fn lr_w(&mut self, hart_id: u32, addr: u32) -> Result<u32, FalconError> {
        let aligned = reservation_addr(addr);
        let val = self.load32(aligned)?;
        self.reservations
            .insert(hart_id, Reservation { addr: aligned });
        Ok(val)
    }

    fn sc_w(&mut self, hart_id: u32, addr: u32, val: u32) -> Result<bool, FalconError> {
        let aligned = reservation_addr(addr);
        let success = self
            .reservations
            .get(&hart_id)
            .is_some_and(|res| res.addr == aligned);
        self.reservations.remove(&hart_id);
        if success {
            self.store32(aligned, val)?;
        }
        Ok(success)
    }

    fn amo_w(
        &mut self,
        _hart_id: u32,
        addr: u32,
        op: AmoOp,
        operand: u32,
    ) -> Result<u32, FalconError> {
        let aligned = reservation_addr(addr);
        let old = self.load32(aligned)?;
        let new = amo_apply(op, old, operand);
        self.store32(aligned, new)?;
        Ok(old)
    }
}

#[cfg(test)]
#[path = "../../tests/support/falcon_memory.rs"]
mod tests;

