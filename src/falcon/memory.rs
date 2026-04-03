// falcon/memory.rs
use crate::falcon::errors::FalconError;
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
/// ## Contrato de leitura (por que load* é cache-aware)
///
/// `load*` retorna o valor *mais atual* no endereço — idêntico ao que o
/// software veria ao ler aquele endereço. Para `Ram` isso é simplesmente a
/// RAM. Para `CacheController`, linhas sujas da D-cache têm prioridade sobre
/// a RAM (write-back): `load*` delega a `effective_read*`, sem side-effects
/// de stats.
///
/// ## Separação de responsabilidades
///
/// | Método          | Semântica                                      | Quem usa          |
/// |-----------------|------------------------------------------------|-------------------|
/// | `load*`         | Leitura cache-aware, sem stats                 | syscalls, decoder |
/// | `store*`        | Escrita via D-cache                            | exec.rs stores    |
/// | `fetch32`       | Busca de instrução via I-cache                 | exec.rs fetch     |
/// | `dcache_read*`  | Leitura com tracking de stats de D-cache       | exec.rs loads     |
/// | `peek*` (CC)    | RAM bruta — apenas no `CacheController`, para UI |                 |
pub trait Bus {
    /// Leitura cache-aware: retorna o valor mais atual no endereço.
    /// Implementações com cache devem verificar linhas sujas antes da RAM.
    fn load8(&self, addr: u32) -> Result<u8, FalconError>;
    fn load16(&self, addr: u32) -> Result<u16, FalconError>;
    fn load32(&self, addr: u32) -> Result<u32, FalconError>;

    fn store8(&mut self, addr: u32, val: u8) -> Result<(), FalconError>;
    fn store16(&mut self, addr: u32, val: u16) -> Result<(), FalconError>;
    fn store32(&mut self, addr: u32, val: u32) -> Result<(), FalconError>;

    /// Busca de instrução — sobrescrever para rotear pela I-cache.
    fn fetch32(&mut self, addr: u32) -> Result<u32, FalconError> {
        self.load32(addr)
    }

    /// Leitura de dado com tracking de stats de D-cache — sobrescrever em CacheController.
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
}

pub struct Ram {
    data: Vec<u8>,
    reservations: HashMap<u32, Reservation>,
}

impl Ram {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
            reservations: HashMap::new(),
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
}

impl Bus for Ram {
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
