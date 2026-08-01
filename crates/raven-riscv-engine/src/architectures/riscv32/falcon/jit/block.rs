//! Basic-block detection for the JIT compiler.
//!
//! # O que Ã© um basic block?
//!
//! Um *basic block* Ã© a maior sequÃªncia contÃ­gua de instruÃ§Ãµes que:
//! - tem exatamente **um ponto de entrada** (a primeira instruÃ§Ã£o),
//! - tem exatamente **um ponto de saÃ­da** (a Ãºltima instruÃ§Ã£o, chamada de *terminador*),
//! - nÃ£o contÃ©m saltos internos nem Ã© alvo de salto no meio.
//!
//! Em outras palavras: se a execuÃ§Ã£o entrar no basic block, ela obrigatoriamente
//! executarÃ¡ todas as instruÃ§Ãµes atÃ© o terminador, sem desvios.
//!
//! # Como `scan_block` funciona
//!
//! A funÃ§Ã£o lÃª palavras de 32 bits da memÃ³ria via [`CacheController::peek32`], que
//! acessa a **RAM diretamente** sem registrar no I-cache (importante: o scan nÃ£o
//! deve poluir as estatÃ­sticas de cache). Cada palavra Ã© decodificada com
//! [`crate::falcon::decoder::decode`] e classificada como terminador ou nÃ£o.
//!
//! O scan para quando encontra:
//! - Um terminador explÃ­cito (`Branch`, `Jal`, `Jalr`, `Ecall`, `Ebreak`, `Halt`, `Fence`).
//! - 64 instruÃ§Ãµes sem terminador â€” o bloco Ã© encerrado com [`BlockTerminator::FallThrough`]
//!   para evitar blocos patolÃ³gicos em cÃ³digo denso.
//! - Erro de leitura de memÃ³ria ou instruÃ§Ã£o ilegal â€” tambÃ©m vira `FallThrough`.
//!
//! # Invariante de `end_pc`
//!
//! `BasicBlock::end_pc` Ã© **sempre o PC da Ãºltima instruÃ§Ã£o do bloco**:
//! - Para terminadores reais: Ã© o PC do terminador.
//! - Para `FallThrough`: Ã© o PC da 64Âª instruÃ§Ã£o.
//!
//! Isso garante que `block.end_pc + 4` aponte para o inÃ­cio do prÃ³ximo bloco
//! sequencial, o que a Fase C (backends `hot` e `full`) usa ao construir o grafo
//! de fluxo de controle.

use crate::falcon::cache::CacheController;
use crate::falcon::decoder::decode;
use crate::falcon::instruction::Instruction;

/// Categoriza o tipo de instruÃ§Ã£o que encerra um basic block.
///
/// O discriminante determina como o backend JIT deve atualizar `cpu.pc`
/// ao final da execuÃ§Ã£o do bloco compilado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockTerminator {
    /// Branch condicional (Beq, Bne, Blt, Bge, Bltu, Bgeu).
    /// O PC seguinte depende do valor dos registradores â€” resolvido em runtime.
    Branch,

    /// Salto incondicional com destino codificado no imediato (Jal).
    /// O destino pode ser calculado estaticamente em tempo de scan.
    Jal,

    /// Salto indireto via registrador (Jalr).
    /// O destino Ã© dinÃ¢mico â€” nÃ£o pode ser resolvido em tempo de scan.
    Jalr,

    /// System call. Controle passa ao sistema; bloco encerra aqui.
    Ecall,

    /// Breakpoint. Normalmente chama o depurador ou gera uma exceÃ§Ã£o.
    Ebreak,

    /// InstruÃ§Ã£o `halt` do Raven (pseudoinstruÃ§Ã£o para encerrar a simulaÃ§Ã£o).
    Halt,

    /// `fence` ou `fence.i` â€” barreiras de memÃ³ria/instruÃ§Ã£o.
    /// Encerram o bloco porque podem invalidar suposiÃ§Ãµes sobre o estado da memÃ³ria.
    Fence,

    /// O scan atingiu 64 instruÃ§Ãµes sem encontrar um terminador explÃ­cito,
    /// ou encontrou memÃ³ria ilegÃ­vel / instruÃ§Ã£o invÃ¡lida.
    /// O prÃ³ximo bloco comeÃ§a em `end_pc + 4`.
    FallThrough,
}

/// Descreve um basic block: intervalo de PCs, palavras brutas e tipo de saÃ­da.
///
/// `words[i]` Ã© a instruÃ§Ã£o encodada no PC `start_pc + i * 4`.
/// `end_pc` Ã© o PC da Ãºltima instruÃ§Ã£o (= `start_pc + (words.len() - 1) * 4`).
#[derive(Clone, Debug)]
pub struct BasicBlock {
    pub start_pc: u32,
    pub end_pc: u32,
    pub words: Vec<u32>,
    pub terminator: BlockTerminator,
}

const MAX_INSTRS: usize = 64;

/// Varre a memÃ³ria a partir de `start_pc` e retorna o primeiro basic block.
///
/// Usa `mem.peek32` para ler da RAM sem afetar estatÃ­sticas do I-cache.
/// Consulte o doc do mÃ³dulo para detalhes sobre o algoritmo e a invariante
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

/// Retorna o [`BlockTerminator`] correspondente Ã  instruÃ§Ã£o, ou `None` se a
/// instruÃ§Ã£o nÃ£o encerra um basic block.
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

