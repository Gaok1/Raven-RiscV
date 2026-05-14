//! Basic-block detection for the JIT compiler.
//!
//! # O que é um basic block?
//!
//! Um *basic block* é a maior sequência contígua de instruções que:
//! - tem exatamente **um ponto de entrada** (a primeira instrução),
//! - tem exatamente **um ponto de saída** (a última instrução, chamada de *terminador*),
//! - não contém saltos internos nem é alvo de salto no meio.
//!
//! Em outras palavras: se a execução entrar no basic block, ela obrigatoriamente
//! executará todas as instruções até o terminador, sem desvios.
//!
//! # Como `scan_block` funciona
//!
//! A função lê palavras de 32 bits da memória via [`CacheController::peek32`], que
//! acessa a **RAM diretamente** sem registrar no I-cache (importante: o scan não
//! deve poluir as estatísticas de cache). Cada palavra é decodificada com
//! [`crate::falcon::decoder::decode`] e classificada como terminador ou não.
//!
//! O scan para quando encontra:
//! - Um terminador explícito (`Branch`, `Jal`, `Jalr`, `Ecall`, `Ebreak`, `Halt`, `Fence`).
//! - 64 instruções sem terminador — o bloco é encerrado com [`BlockTerminator::FallThrough`]
//!   para evitar blocos patológicos em código denso.
//! - Erro de leitura de memória ou instrução ilegal — também vira `FallThrough`.
//!
//! # Invariante de `end_pc`
//!
//! `BasicBlock::end_pc` é **sempre o PC da última instrução do bloco**:
//! - Para terminadores reais: é o PC do terminador.
//! - Para `FallThrough`: é o PC da 64ª instrução.
//!
//! Isso garante que `block.end_pc + 4` aponte para o início do próximo bloco
//! sequencial, o que a Fase C (backends `hot` e `full`) usa ao construir o grafo
//! de fluxo de controle.

use crate::falcon::cache::CacheController;
use crate::falcon::decoder::decode;
use crate::falcon::instruction::Instruction;

/// Categoriza o tipo de instrução que encerra um basic block.
///
/// O discriminante determina como o backend JIT deve atualizar `cpu.pc`
/// ao final da execução do bloco compilado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockTerminator {
    /// Branch condicional (Beq, Bne, Blt, Bge, Bltu, Bgeu).
    /// O PC seguinte depende do valor dos registradores — resolvido em runtime.
    Branch,

    /// Salto incondicional com destino codificado no imediato (Jal).
    /// O destino pode ser calculado estaticamente em tempo de scan.
    Jal,

    /// Salto indireto via registrador (Jalr).
    /// O destino é dinâmico — não pode ser resolvido em tempo de scan.
    Jalr,

    /// System call. Controle passa ao sistema; bloco encerra aqui.
    Ecall,

    /// Breakpoint. Normalmente chama o depurador ou gera uma exceção.
    Ebreak,

    /// Instrução `halt` do Raven (pseudoinstrução para encerrar a simulação).
    Halt,

    /// `fence` ou `fence.i` — barreiras de memória/instrução.
    /// Encerram o bloco porque podem invalidar suposições sobre o estado da memória.
    Fence,

    /// O scan atingiu 64 instruções sem encontrar um terminador explícito,
    /// ou encontrou memória ilegível / instrução inválida.
    /// O próximo bloco começa em `end_pc + 4`.
    FallThrough,
}

/// Descreve um basic block: intervalo de PCs, palavras brutas e tipo de saída.
///
/// `words[i]` é a instrução encodada no PC `start_pc + i * 4`.
/// `end_pc` é o PC da última instrução (= `start_pc + (words.len() - 1) * 4`).
#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub start_pc: u32,
    pub end_pc: u32,
    pub words: Vec<u32>,
    pub terminator: BlockTerminator,
}

const MAX_INSTRS: usize = 64;

/// Varre a memória a partir de `start_pc` e retorna o primeiro basic block.
///
/// Usa `mem.peek32` para ler da RAM sem afetar estatísticas do I-cache.
/// Consulte o doc do módulo para detalhes sobre o algoritmo e a invariante
/// de `end_pc`.
pub fn scan_block(mem: &CacheController, start_pc: u32) -> BasicBlock {
    let mut words = Vec::with_capacity(MAX_INSTRS);
    let mut pc = start_pc;

    loop {
        let word = match mem.peek32(pc) {
            Ok(w) => w,
            Err(_) => {
                let end_pc = if words.is_empty() { start_pc } else { pc.wrapping_sub(4) };
                return BasicBlock {
                    start_pc,
                    end_pc,
                    words,
                    terminator: BlockTerminator::FallThrough,
                };
            }
        };

        words.push(word);

        let terminator = match decode(word) {
            Ok(instr) => classify_terminator(&instr),
            Err(_) => Some(BlockTerminator::FallThrough),
        };

        if let Some(term) = terminator {
            return BasicBlock { start_pc, end_pc: pc, words, terminator: term };
        }

        if words.len() >= MAX_INSTRS {
            return BasicBlock {
                start_pc,
                end_pc: pc,
                words,
                terminator: BlockTerminator::FallThrough,
            };
        }

        pc = pc.wrapping_add(4);
    }
}

/// Retorna o [`BlockTerminator`] correspondente à instrução, ou `None` se a
/// instrução não encerra um basic block.
fn classify_terminator(instr: &Instruction) -> Option<BlockTerminator> {
    match instr {
        Instruction::Beq { .. }
        | Instruction::Bne { .. }
        | Instruction::Blt { .. }
        | Instruction::Bge { .. }
        | Instruction::Bltu { .. }
        | Instruction::Bgeu { .. } => Some(BlockTerminator::Branch),
        Instruction::Jal { .. } => Some(BlockTerminator::Jal),
        Instruction::Jalr { .. } => Some(BlockTerminator::Jalr),
        Instruction::Ecall => Some(BlockTerminator::Ecall),
        Instruction::Ebreak => Some(BlockTerminator::Ebreak),
        Instruction::Halt => Some(BlockTerminator::Halt),
        Instruction::Fence | Instruction::FenceI => Some(BlockTerminator::Fence),
        _ => None,
    }
}
