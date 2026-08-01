//! Backend de referÃªncia: interpretador puro wrappando `falcon::exec::step`.
//!
//! # Por que existe este wrapper?
//!
//! A Fase A do JIT introduziu o trait [`ExecutionBackend`] para que todos os
//! call sites de execuÃ§Ã£o (CLI e TUI) sejam independentes da implementaÃ§Ã£o
//! concreta. `InterpreterBackend` Ã© o adaptador que conecta o interpretador
//! original (`exec.rs`) a esse trait.
//!
//! **Regra importante:** `exec.rs` nÃ£o foi modificado. Isso garante que a
//! semÃ¢ntica de execuÃ§Ã£o do interpretador â€” cada instruÃ§Ã£o, cada efeito
//! colateral â€” seja preservada exatamente como estava antes do refactor JIT.
//! Qualquer divergÃªncia futura entre o JIT e o interpretador pode ser
//! diagnosticada comparando com este backend.
//!
//! # `HotProfile` no interpretador
//!
//! ApÃ³s cada passo, se o PC mudou por mais do que os 4 bytes normais de
//! incremento sequencial, o novo PC Ã© registrado em [`HotProfile`] como alvo
//! de um desvio tomado. O backend `hot` da Fase C consulta esse contador para
//! decidir quais PCs compilar (threshold 500 entradas).
//!
//! Nota: o `HotProfile` aqui Ã© **diferente** do `exec_counts` exibido na TUI
//! (`app.run.exec_counts: HashMap<u32, u64>`). O `exec_counts` da TUI conta
//! toda instruÃ§Ã£o executada em qualquer PC; o `HotProfile` conta apenas alvos
//! de desvios tomados (loop heads), que Ã© o que o JIT precisa.

use crate::falcon::errors::FalconError;
use crate::falcon::memory::Bus;

use super::backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
use super::profile::HotProfile;

/// Backend de execuÃ§Ã£o que delega cada passo a `falcon::exec::step`.
///
/// Ã‰ o Ãºnico backend disponÃ­vel na Fase A e permanece como referÃªncia de
/// correÃ§Ã£o para validaÃ§Ã£o do cÃ³digo JIT nas fases seguintes.
pub struct InterpreterBackend {
    profile: HotProfile,
}

impl InterpreterBackend {
    pub fn new() -> Self {
        Self {
            profile: HotProfile::new(),
        }
    }

    /// Acesso direto ao `HotProfile` sem passar pelo trait, evitando
    /// ambiguidade de inferÃªncia de tipo quando o chamador tem um
    /// `InterpreterBackend` concreto (nÃ£o `dyn ExecutionBackend`).
    pub fn profile(&self) -> &HotProfile {
        &self.profile
    }
}

impl Default for InterpreterBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bus> ExecutionBackend<B> for InterpreterBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::None
    }

    fn run_until_yield(&mut self, ctx: &mut ExecCtx<'_, B>) -> Result<ExecOutcome, FalconError> {
        let pc_before = ctx.cpu.pc;
        let alive = crate::falcon::exec::step(ctx.cpu, ctx.mem, ctx.console)?;

        if alive && ctx.cpu.pc != pc_before.wrapping_add(4) {
            self.profile.record_target(ctx.cpu.pc);
        }

        if !alive {
            return Ok(if ctx.console.reading {
                ExecOutcome::AwaitingInput
            } else {
                ExecOutcome::Halted
            });
        }
        Ok(ExecOutcome::Stepped { instructions: 1 })
    }

    fn hot_profile(&self) -> Option<&HotProfile> {
        Some(&self.profile)
    }
}

