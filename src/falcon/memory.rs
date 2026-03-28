// falcon/memory.rs
use crate::falcon::errors::FalconError;

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
}

pub struct Ram {
    data: Vec<u8>,
}

impl Ram {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
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
}

#[cfg(test)]
#[path = "../../tests/support/falcon_memory.rs"]
mod tests;
