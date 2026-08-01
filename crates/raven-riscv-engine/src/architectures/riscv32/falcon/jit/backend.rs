//! Contrato central do sistema JIT: trait `ExecutionBackend` e tipos auxiliares.
//!
//! # Por que um trait?
//!
//! O Raven suporta trÃªs modos de execuÃ§Ã£o selecionados pela flag `--jit`:
//!
//! | Flag          | Backend              | Status   |
//! |---------------|----------------------|----------|
//! | `--jit=none`  | `InterpreterBackend` | Fase A âœ“ |
//! | `--jit=hot`   | `HotBackend`         | Fase C   |
//! | `--jit=full`  | `FullBackend`        | Fase C   |
//!
//! O trait `ExecutionBackend<B>` permite que o driver de execuÃ§Ã£o (CLI e TUI)
//! opere sobre qualquer implementaÃ§Ã£o sem saber qual estÃ¡ em uso â€” o backend Ã©
//! escolhido uma vez por [`crate::falcon::jit::factory::make_backend`] e
//! armazenado como `Box<dyn ExecutionBackend<CacheController>>`.
//!
//! # `ExecCtx` â€” por que um struct e nÃ£o parÃ¢metros separados?
//!
//! `run_until_yield` precisa de `&mut Cpu`, `&mut B` e `&mut Console`
//! simultaneamente. Passar os trÃªs como parÃ¢metros distintos tornaria a
//! assinatura do trait mais verbosa e dificultaria futuros acrÃ©scimos.
//! `ExecCtx` agrupa os emprÃ©stimos mutÃ¡veis em um Ãºnico struct de vida curta
//! (lifetime `'a`), sem alocar.
//!
//! # `ExecOutcome` â€” o que o driver precisa saber apÃ³s cada passo
//!
//! O driver precisa distinguir:
//! - `Stepped { instructions }` â€” execuÃ§Ã£o normal; `instructions` indica
//!   quantas instruÃ§Ãµes foram processadas (Ãºtil quando o JIT executa um bloco
//!   inteiro por chamada).
//! - `AwaitingInput` â€” o programa fez uma leitura de console e estÃ¡ bloqueado.
//! - `Halted` â€” o programa encerrou (`halt` ou `exit` syscall).

use crate::falcon::errors::FalconError;
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

use super::profile::HotProfile;

/// Identifica qual backend estÃ¡ ativo. Usado para exibiÃ§Ã£o na TUI e logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    None,
    Hot,
    Full,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::None => "none",
            BackendKind::Hot => "hot",
            BackendKind::Full => "full",
        }
    }
}

impl Default for BackendKind {
    fn default() -> Self {
        BackendKind::None
    }
}

/// Resultado de uma chamada a [`ExecutionBackend::run_until_yield`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecOutcome {
    /// ExecuÃ§Ã£o avanÃ§ou normalmente. `instructions` Ã© a contagem de instruÃ§Ãµes
    /// executadas nesta chamada (1 para o interpretador, N para um bloco JIT).
    Stepped { instructions: u32 },
    /// O programa aguarda entrada do console (`read` syscall bloqueante).
    Halted,
    /// O programa encerrou (instruÃ§Ã£o `halt` ou syscall de saÃ­da).
    AwaitingInput,
}

/// Contexto de execuÃ§Ã£o: referÃªncias mutÃ¡veis ao estado do hart.
///
/// Emprestado por uma Ãºnica chamada a `run_until_yield`; nÃ£o sobrevive alÃ©m disso.
pub struct ExecCtx<'a, B: Bus> {
    pub cpu: &'a mut Cpu,
    pub mem: &'a mut B,
    pub console: &'a mut Console,
}

impl<'a, B: Bus> ExecCtx<'a, B> {
    pub fn new(cpu: &'a mut Cpu, mem: &'a mut B, console: &'a mut Console) -> Self {
        Self { cpu, mem, console }
    }
}

/// AbstraÃ§Ã£o sobre a estratÃ©gia de execuÃ§Ã£o de instruÃ§Ãµes RISC-V.
///
/// ImplementaÃ§Ãµes devem ser `Send` para que o driver TUI possa mover o backend
/// entre threads (hartthread model futuro).
///
/// MÃ©todos com implementaÃ§Ã£o padrÃ£o (`invalidate`, `hot_profile`) tÃªm comportamento
/// no-op/None para backends que nÃ£o os usam, evitando que implementaÃ§Ãµes simples
/// precisem sobrescrever tudo.
pub trait ExecutionBackend<B: Bus>: Send {
    /// Retorna o tipo deste backend â€” usado para logging e display na TUI.
    fn kind(&self) -> BackendKind;

    /// Executa atÃ© um ponto de rendimento natural do backend:
    /// - Interpretador: 1 instruÃ§Ã£o.
    /// - Bloco JIT: todas as instruÃ§Ãµes do basic block atÃ© o terminador.
    fn run_until_yield(&mut self, ctx: &mut ExecCtx<'_, B>) -> Result<ExecOutcome, FalconError>;

    /// Invalida blocos compilados cujo intervalo [start_pc, end_pc) intersecta
    /// [start, end). Chamado pelo driver quando `cpu.pending_exec_map` sinaliza
    /// Self-Modifying Code. Backends sem cache compilado ignoram (no-op padrÃ£o).
    fn invalidate(&mut self, _start: u32, _end: u32) {}

    /// Acesso ao perfil de branches quentes, se o backend o mantÃ©m.
    /// Retorna `None` para backends que nÃ£o rastreiam frequÃªncia de branches.
    fn hot_profile(&self) -> Option<&HotProfile> {
        None
    }
}

