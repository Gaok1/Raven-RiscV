//! Cache de blocos compilados pelo JIT.
//!
//! # Duas implementações condicionais
//!
//! Este arquivo define `CompiledBlockCache` em duas variantes:
//!
//! - **Com `--features jit`:** implementação real com `HashMap<u32, Arc<CompiledBlock>>`.
//!   `CompiledBlock` contém o buffer de código nativo (`dynasmrt::ExecutableBuffer`),
//!   o `AssemblyOffset` do ponto de entrada e metadados do bloco.
//!
//! - **Sem a feature:** stub no-op. O trait `ExecutionBackend::invalidate` pode ser
//!   chamado pelo driver sem que nenhum backend JIT real esteja ativo.
//!
//! # `invalidate_range`
//!
//! Remove todos os blocos cujo intervalo `[start_pc, end_pc)` intersecta `[start, end)`.
//! Necessário para Self-Modifying Code: quando o programa escreve em uma região
//! marcada como executável via `SYS_RAVEN_MAP_EXEC`, o driver notifica o backend
//! via `ExecutionBackend::invalidate`, que delega para este método.
//!
//! **Implementação:** `retain(|_, b| b.end_pc < start || b.start_pc >= end)` — O(N)
//! sobre os blocos em cache. Aceitável porque SMC denso não é o caso de uso real.

// ---------------------------------------------------------------------------
// Implementação real — apenas com cargo feature "jit"
// ---------------------------------------------------------------------------

#[cfg(feature = "jit")]
mod real {
    use std::collections::HashMap;
    use std::sync::Arc;

    use dynasmrt::{AssemblyOffset, ExecutableBuffer};

    use crate::falcon::CacheController;
    use crate::falcon::registers::Cpu;
    use crate::ui::Console;

    /// Assinatura C da função compilada pelo JIT.
    ///
    /// Argumentos (System V x86_64 ABI):
    /// - rdi = `*mut Cpu`
    /// - rsi = `*mut CacheController`
    /// - rdx = `*mut Console`
    ///
    /// Retorno em eax: discriminante de [`ExitKind`].
    pub type BlockFn = unsafe extern "C" fn(
        cpu: *mut Cpu,
        mem: *mut CacheController,
        console: *mut Console,
    ) -> u32;

    /// Discriminantes retornados por blocos compilados (eax no retorno).
    pub mod exit {
        /// Execução normal — dispatcher deve continuar.
        pub const CONTINUE: u32 = 0;
        /// Programa aguarda entrada do console.
        pub const AWAIT_INPUT: u32 = 1;
        /// Programa encerrou (`halt` ou syscall de saída).
        pub const HALTED: u32 = 2;
        /// Instrução inválida ou acesso de memória com falha.
        pub const FAULT: u32 = 3;
    }

    /// Um basic block compilado para código nativo x86_64.
    pub struct CompiledBlock {
        /// PC da primeira instrução do bloco.
        pub start_pc: u32,
        /// PC da última instrução do bloco (inclusive).
        pub end_pc: u32,
        /// Número de instruções compiladas neste bloco.
        pub instruction_count: u32,
        /// Buffer de memória executável (alocado via mmap).
        pub code: ExecutableBuffer,
        /// Offset dentro de `code` do ponto de entrada da função.
        pub entry: AssemblyOffset,
    }

    impl CompiledBlock {
        /// Retorna o ponteiro de função para a execução nativa do bloco.
        ///
        /// # Safety
        /// O buffer deve conter código x86_64 válido com assinatura [`BlockFn`].
        pub unsafe fn as_fn(&self) -> BlockFn {
            let ptr = self.code.ptr(self.entry);
            unsafe { std::mem::transmute(ptr) }
        }
    }

    /// Cache de basic blocks compilados, indexado por `start_pc`.
    pub struct CompiledBlockCache {
        blocks: HashMap<u32, Arc<CompiledBlock>>,
    }

    impl CompiledBlockCache {
        pub fn new() -> Self {
            Self { blocks: HashMap::new() }
        }

        /// Retorna o bloco compilado para o PC dado, se houver.
        pub fn get(&self, pc: u32) -> Option<Arc<CompiledBlock>> {
            self.blocks.get(&pc).cloned()
        }

        /// Insere um bloco recém-compilado no cache.
        pub fn insert(&mut self, block: Arc<CompiledBlock>) {
            self.blocks.insert(block.start_pc, block);
        }

        /// Remove blocos cujo `[start_pc, end_pc)` intersecta `[start, end)`.
        pub fn invalidate_range(&mut self, start: u32, end: u32) {
            self.blocks.retain(|_, b| b.end_pc < start || b.start_pc >= end);
        }

        /// Número de blocos no cache.
        pub fn len(&self) -> usize {
            self.blocks.len()
        }

        pub fn is_empty(&self) -> bool {
            self.blocks.is_empty()
        }
    }

    impl Default for CompiledBlockCache {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Stub — quando a feature "jit" não está ativa
// ---------------------------------------------------------------------------

#[cfg(not(feature = "jit"))]
mod stub {
    /// Stub sem armazenamento real. Ver doc do módulo para a versão real.
    pub struct CompiledBlockCache {
        _private: (),
    }

    impl CompiledBlockCache {
        pub fn new() -> Self {
            Self { _private: () }
        }

        /// No-op. Fase B com `--features jit` implementa a invalidação real.
        pub fn invalidate_range(&mut self, _start: u32, _end: u32) {}
    }

    impl Default for CompiledBlockCache {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

#[cfg(feature = "jit")]
pub use real::{BlockFn, CompiledBlock, CompiledBlockCache, exit};

#[cfg(not(feature = "jit"))]
pub use stub::CompiledBlockCache;
