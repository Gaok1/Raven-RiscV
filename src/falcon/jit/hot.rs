//! Backend `--jit=hot`: compilação seletiva de blocos quentes — Fase C.
//!
//! # Política de compilação
//!
//! O `HotBackend` mantém um `HotProfile` que o `InterpreterBackend` interno
//! atualiza a cada branch/jump tomado. Quando um PC acumula ≥ `threshold`
//! entradas no perfil, o basic block a partir daquele PC é compilado e
//! inserido no `CompiledBlockCache`.
//!
//! Threshold padrão: **500**. O interpretador do Raven é propositalmente lento
//! (fins didáticos), então 500 entradas já caracteriza um loop quente.
//!
//! # Fluxo por step
//!
//! ```text
//! run_until_yield(ctx):
//!   1. cache.get(cpu.pc) → hit  → executa bloco compilado, retorna
//!   2. profile.get(cpu.pc) >= threshold → compile_block, insere, retry
//!   3. fallthrough → InterpreterBackend::run_until_yield (perfil continua crescendo)
//! ```
//!
//! # Amortização do custo de compilação
//!
//! Para um loop quente de 100 000 iterações:
//! - 500 iterações interpretadas (acumulando perfil)
//! - 1 compilação síncrona (custo único)
//! - 99 499 iterações compiladas
//!
//! O break-even é imediato dado o speedup ≥ 2× esperado por bloco.

#[cfg(feature = "jit")]
mod inner {
    use std::collections::HashSet;

    use crate::falcon::CacheController;
    use crate::falcon::errors::FalconError;

    use super::super::backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
    use super::super::block::scan_block;
    use super::super::cache::{CompiledBlockCache, exit};
    use super::super::codegen::compile_block;
    use super::super::interpreter::InterpreterBackend;
    use super::super::profile::HotProfile;

    const DEFAULT_THRESHOLD: u32 = 50;

    pub struct HotBackend {
        interpreter: InterpreterBackend,
        cache: CompiledBlockCache,
        /// PCs que já foram avaliados e são pequenos demais para compilar.
        /// Evita chamar scan_block em toda iteração após o threshold ser atingido.
        skip_pcs: HashSet<u32>,
        threshold: u32,
    }

    impl HotBackend {
        pub fn new() -> Self {
            Self {
                interpreter: InterpreterBackend::new(),
                cache: CompiledBlockCache::new(),
                skip_pcs: HashSet::new(),
                threshold: DEFAULT_THRESHOLD,
            }
        }
    }

    impl Default for HotBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    // SAFETY: HotBackend é usado single-threaded no modelo de hart atual.
    // ExecutableBuffer é !Send em dynasmrt, mas o acesso é sempre serializado
    // pelo driver de execução.
    unsafe impl Send for HotBackend {}

    impl ExecutionBackend<CacheController> for HotBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Hot
        }

        fn run_until_yield(
            &mut self,
            ctx: &mut ExecCtx<'_, CacheController>,
        ) -> Result<ExecOutcome, FalconError> {
            let pc = ctx.cpu.pc;

            // --- Cache hit: executar bloco compilado ---
            if let Some(block) = self.cache.get(pc) {
                let exit_code = unsafe {
                    let f = block.as_fn();
                    f(ctx.cpu as *mut _, ctx.mem as *mut _, ctx.console as *mut _)
                };
                return map_exit(exit_code, block.instruction_count);
            }

            // --- Threshold atingido: compilar se não estiver na skip list ---
            let profile_count = self.interpreter.profile().get(pc);
            if profile_count >= self.threshold && !self.skip_pcs.contains(&pc) {
                let basic_block = scan_block(ctx.mem, pc);
                if basic_block.words.len() >= 3 {
                    if let Some(compiled) = compile_block(&basic_block) {
                        self.cache.insert(compiled);
                        if let Some(block) = self.cache.get(pc) {
                            let exit_code = unsafe {
                                let f = block.as_fn();
                                f(ctx.cpu as *mut _, ctx.mem as *mut _, ctx.console as *mut _)
                            };
                            return map_exit(exit_code, block.instruction_count);
                        }
                    }
                } else {
                    // Bloco pequeno demais — marcar para não tentar novamente.
                    self.skip_pcs.insert(pc);
                }
            }

            // --- Fallthrough: interpretar e continuar acumulando perfil ---
            self.interpreter.run_until_yield(ctx)
        }

        fn invalidate(&mut self, start: u32, end: u32) {
            self.cache.invalidate_range(start, end);
            self.skip_pcs.retain(|&pc| pc < start || pc >= end);
        }

        fn hot_profile(&self) -> Option<&HotProfile> {
            Some(self.interpreter.profile())
        }
    }

    fn map_exit(exit_code: u32, instruction_count: u32) -> Result<ExecOutcome, FalconError> {
        match exit_code {
            exit::CONTINUE => Ok(ExecOutcome::Stepped { instructions: instruction_count }),
            exit::AWAIT_INPUT => Ok(ExecOutcome::AwaitingInput),
            exit::HALTED => Ok(ExecOutcome::Halted),
            exit::FAULT => Err(FalconError::Bus("JIT fault: invalid memory access".into())),
            _ => Ok(ExecOutcome::Stepped { instructions: instruction_count }),
        }
    }
}

#[cfg(feature = "jit")]
pub use inner::HotBackend;
