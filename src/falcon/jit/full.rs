//! Backend `--jit=full`: scan eager de todos os blocos acessíveis — Fase C.
//!
//! # Diferença em relação ao `hot`
//!
//! O `HotBackend` compila sob demanda ao atingir o threshold. O `FullBackend`
//! compila **todos os blocos acessíveis estaticamente** antes de executar a
//! primeira instrução, fazendo um BFS a partir de `cpu.pc`.
//!
//! Isso elimina o warm-up de 500 iterações e é ideal para benchmarks onde
//! o tempo total importa (inclusive os primeiros ciclos).
//!
//! # Scan format-agnostic
//!
//! O scan começa em `cpu.pc` — nunca lê `ElfInfo`. O loader (ELF/FALC/ASM/flat)
//! já depositou o código em `mem.ram` antes de `FullBackend::new` ser chamado.
//! Blocos não descobertos em tempo de scan (alvos de JALR dinâmico) são tratados
//! como cache miss e caem para o interpretador em runtime.
//!
//! # Política de JALR
//!
//! JALR tem alvo dinâmico — não pode ser resolvido em tempo de scan. O bloco
//! é encerrado com `BlockTerminator::Jalr` e o compilado não inclui o bloco
//! seguinte. Em runtime, o miss aciona `InterpreterBackend::run_until_yield`
//! para aquele passo, e a próxima iteração tentará novamente o cache.
//!
//! # SMC
//!
//! `invalidate` delega para `CompiledBlockCache::invalidate_range`. O próximo
//! dispatch de um PC invalidado é um miss → interpretador → recompila se
//! necessário.

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
        /// Constrói o backend fazendo o scan eager a partir de `cpu.pc`.
        pub fn new(cpu: &Cpu, mem: &CacheController) -> Self {
            let cache = eager_compile(cpu.pc, mem);
            Self {
                cache,
                interpreter: InterpreterBackend::new(),
            }
        }
    }

    // SAFETY: igual ao HotBackend — uso single-threaded serializado pelo driver.
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

            // Miss (JALR dinâmico ou bloco não alcançado no scan inicial):
            // interpretar um passo e tentar compilar oportunisticamente.
            let result = self.interpreter.run_until_yield(ctx)?;
            // Compilar o bloco recém-visitado para futuras passagens.
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

    /// Número máximo de blocos compilados pelo scan eager.
    /// Limita o tempo de startup em programas grandes (evita compilar código morto).
    const MAX_EAGER_BLOCKS: usize = 200;

    /// BFS a partir de `start_pc`, compilando blocos acessíveis estaticamente
    /// até o limite de MAX_EAGER_BLOCKS. Blocos além desse limite são compilados
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
                // O PC de destino do JAL está no último word do bloco.
                // O codegen já o resolveu; aqui precisamos redecodificá-lo.
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
            // JALR: alvo dinâmico — não pode ser resolvido aqui.
            // Ecall/Ebreak/Halt/Fence: terminadores absolutos.
            _ => {}
        }
    }

    /// Extrai o PC alvo de uma instrução JAL: `pc + sign_extend(imm21)`.
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
