//! Backend `--jit=full`: scan eager de todos os blocos acessÃ­veis â€” Fase C.
//!
//! # DiferenÃ§a em relaÃ§Ã£o ao `hot`
//!
//! O `HotBackend` compila sob demanda ao atingir o threshold. O `FullBackend`
//! compila **todos os blocos acessÃ­veis estaticamente** antes de executar a
//! primeira instruÃ§Ã£o, fazendo um BFS a partir de `cpu.pc`.
//!
//! Isso elimina o warm-up de 500 iteraÃ§Ãµes e Ã© ideal para benchmarks onde
//! o tempo total importa (inclusive os primeiros ciclos).
//!
//! # Scan format-agnostic
//!
//! O scan comeÃ§a em `cpu.pc` â€” nunca lÃª `ElfInfo`. O loader (ELF/FALC/ASM/flat)
//! jÃ¡ depositou o cÃ³digo em `mem.ram` antes de `FullBackend::new` ser chamado.
//! Blocos nÃ£o descobertos em tempo de scan (alvos de JALR dinÃ¢mico) sÃ£o tratados
//! como cache miss e caem para o interpretador em runtime.
//!
//! # PolÃ­tica de JALR
//!
//! JALR tem alvo dinÃ¢mico â€” nÃ£o pode ser resolvido em tempo de scan. O bloco
//! Ã© encerrado com `BlockTerminator::Jalr` e o compilado nÃ£o inclui o bloco
//! seguinte. Em runtime, o miss aciona `InterpreterBackend::run_until_yield`
//! para aquele passo, e a prÃ³xima iteraÃ§Ã£o tentarÃ¡ novamente o cache.
//!
//! # SMC
//!
//! `invalidate` delega para `CompiledBlockCache::invalidate_range`. O prÃ³ximo
//! dispatch de um PC invalidado Ã© um miss â†’ interpretador â†’ recompila se
//! necessÃ¡rio.

#[cfg(feature = "jit")]
mod inner {
    use std::collections::{HashSet, VecDeque};
    use crate::falcon::CacheController;
    use crate::falcon::errors::FalconError;
    use crate::falcon::registers::Cpu;

    use super::super::backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
    use super::super::block::{BasicBlock, BlockTerminator, scan_block};
    use super::super::cache::{CompiledBlockCache, exit};
    use super::super::codegen::compile_block;
    use super::super::interpreter::InterpreterBackend;
    use super::super::profile::HotProfile;

    pub struct FullBackend {
        cache: CompiledBlockCache,
        interpreter: InterpreterBackend,
    }

    impl FullBackend {
        /// ConstrÃ³i o backend fazendo o scan eager a partir de `cpu.pc`.
        pub fn new(cpu: &Cpu, mem: &CacheController) -> Self {
            let cache = eager_compile(cpu.pc, mem);
            Self {
                cache,
                interpreter: InterpreterBackend::new(),
            }
        }
    }

    // SAFETY: igual ao HotBackend â€” uso single-threaded serializado pelo driver.
    unsafe impl Send for FullBackend {}

    impl ExecutionBackend<CacheController> for FullBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Full
        }

        fn run_until_yield(
            &mut self,
            ctx: &mut ExecCtx<'_, CacheController>,
        ) -> Result<ExecOutcome, FalconError> {
            let pc = ctx.cpu.pc;

            if let Some(block) = self.cache.get(pc) {
                let exit_code = unsafe {
                    let f = block.as_fn();
                    f(ctx.cpu as *mut _, ctx.mem as *mut _, ctx.console as *mut _)
                };
                return map_exit(exit_code, block.instruction_count);
            }

            // Miss (JALR dinÃ¢mico ou bloco nÃ£o alcanÃ§ado no scan inicial):
            // interpretar um passo e tentar compilar oportunisticamente.
            let result = self.interpreter.run_until_yield(ctx)?;
            // Compilar o bloco recÃ©m-visitado para futuras passagens.
            let new_pc = ctx.cpu.pc;
            if self.cache.get(new_pc).is_none() {
                let basic_block = scan_block(ctx.mem, new_pc);
                if let Some(compiled) = compile_block(&basic_block) {
                    self.cache.insert(compiled);
                }
            }
            Ok(result)
        }

        fn invalidate(&mut self, start: u32, end: u32) {
            self.cache.invalidate_range(start, end);
        }

        fn hot_profile(&self) -> Option<&HotProfile> {
            Some(self.interpreter.profile())
        }
    }

    /// NÃºmero mÃ¡ximo de blocos compilados pelo scan eager.
    /// Limita o tempo de startup em programas grandes (evita compilar cÃ³digo morto).
    const MAX_EAGER_BLOCKS: usize = 200;

    /// BFS a partir de `start_pc`, compilando blocos acessÃ­veis estaticamente
    /// atÃ© o limite de MAX_EAGER_BLOCKS. Blocos alÃ©m desse limite sÃ£o compilados
    /// oportunisticamente em runtime (igual ao HotBackend).
    fn eager_compile(start_pc: u32, mem: &CacheController) -> CompiledBlockCache {
        let mut cache = CompiledBlockCache::new();
        let mut visited: HashSet<u32> = HashSet::new();
        let mut queue: VecDeque<u32> = VecDeque::new();
        queue.push_back(start_pc);

        while let Some(pc) = queue.pop_front() {
            if cache.len() >= MAX_EAGER_BLOCKS {
                break;
            }
            if !visited.insert(pc) {
                continue;
            }

            let block = scan_block(mem, pc);
            enqueue_successors(&block, &mut queue);

            if let Some(compiled) = compile_block(&block) {
                cache.insert(compiled);
            }
        }

        cache
    }

    fn enqueue_successors(block: &BasicBlock, queue: &mut VecDeque<u32>) {
        let fall_through = block.end_pc.wrapping_add(4);

        match block.terminator {
            BlockTerminator::FallThrough => {
                queue.push_back(fall_through);
            }
            BlockTerminator::Jal => {
                // O PC de destino do JAL estÃ¡ no Ãºltimo word do bloco.
                // O codegen jÃ¡ o resolveu; aqui precisamos redecodificÃ¡-lo.
                if let Some(&word) = block.words.last() {
                    if let Some(target) = jal_target(block.end_pc, word) {
                        queue.push_back(target);
                    }
                }
                queue.push_back(fall_through);
            }
            BlockTerminator::Branch => {
                // Branch condicional: alvo tomado + fall-through.
                if let Some(&word) = block.words.last() {
                    if let Some(target) = branch_target(block.end_pc, word) {
                        queue.push_back(target);
                    }
                }
                queue.push_back(fall_through);
            }
            // JALR: alvo dinÃ¢mico â€” nÃ£o pode ser resolvido aqui.
            // Ecall/Ebreak/Halt/Fence: terminadores absolutos.
            _ => {}
        }
    }

    /// Extrai o PC alvo de uma instruÃ§Ã£o JAL: `pc + sign_extend(imm21)`.
    fn jal_target(pc: u32, word: u32) -> Option<u32> {
        use crate::falcon::decoder::decode;
        use crate::falcon::instruction::Instruction;
        match decode(word) {
            Ok(Instruction::Jal { imm, .. }) => Some(pc.wrapping_add(imm as u32)),
            _ => None,
        }
    }

    /// Extrai o PC alvo tomado de um branch B-type: `pc + sign_extend(imm13)`.
    fn branch_target(pc: u32, word: u32) -> Option<u32> {
        use crate::falcon::decoder::decode;
        use crate::falcon::instruction::Instruction;
        match decode(word) {
            Ok(Instruction::Beq { imm, .. })
            | Ok(Instruction::Bne { imm, .. })
            | Ok(Instruction::Blt { imm, .. })
            | Ok(Instruction::Bge { imm, .. })
            | Ok(Instruction::Bltu { imm, .. })
            | Ok(Instruction::Bgeu { imm, .. }) => Some(pc.wrapping_add(imm as u32)),
            _ => None,
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
pub use inner::FullBackend;

