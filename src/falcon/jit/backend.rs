//! Contrato central do sistema JIT: trait `ExecutionBackend` e tipos auxiliares.
//!
//! # Por que um trait?
//!
//! O Raven suporta três modos de execução selecionados pela flag `--jit`:
//!
//! | Flag          | Backend              | Status   |
//! |---------------|----------------------|----------|
//! | `--jit=none`  | `InterpreterBackend` | Fase A ✓ |
//! | `--jit=hot`   | `HotBackend`         | Fase C   |
//! | `--jit=full`  | `FullBackend`        | Fase C   |
//!
//! O trait `ExecutionBackend<B>` permite que o driver de execução (CLI e TUI)
//! opere sobre qualquer implementação sem saber qual está em uso — o backend é
//! escolhido uma vez por [`crate::falcon::jit::factory::make_backend`] e
//! armazenado como `Box<dyn ExecutionBackend<CacheController>>`.
//!
//! # `ExecCtx` — por que um struct e não parâmetros separados?
//!
//! `run_until_yield` precisa de `&mut Cpu`, `&mut B` e `&mut Console`
//! simultaneamente. Passar os três como parâmetros distintos tornaria a
//! assinatura do trait mais verbosa e dificultaria futuros acréscimos.
//! `ExecCtx` agrupa os empréstimos mutáveis em um único struct de vida curta
//! (lifetime `'a`), sem alocar.
//!
//! # `ExecOutcome` — o que o driver precisa saber após cada passo
//!
//! O driver precisa distinguir:
//! - `Stepped { instructions }` — execução normal; `instructions` indica
//!   quantas instruções foram processadas (útil quando o JIT executa um bloco
//!   inteiro por chamada).
//! - `AwaitingInput` — o programa fez uma leitura de console e está bloqueado.
//! - `Halted` — o programa encerrou (`halt` ou `exit` syscall).

use crate::falcon::errors::FalconError;
use crate::falcon::memory::Bus;
use crate::falcon::registers::Cpu;
use crate::ui::Console;

use super::profile::HotProfile;

/// Identifica qual backend está ativo. Usado para exibição na TUI e logs.
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
    /// Execução avançou normalmente. `instructions` é a contagem de instruções
    /// executadas nesta chamada (1 para o interpretador, N para um bloco JIT).
    Stepped { instructions: u32 },
    /// O programa aguarda entrada do console (`read` syscall bloqueante).
    Halted,
    /// O programa encerrou (instrução `halt` ou syscall de saída).
    AwaitingInput,
}

/// Contexto de execução: referências mutáveis ao estado do hart.
///
/// Emprestado por uma única chamada a `run_until_yield`; não sobrevive além disso.
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

/// Abstração sobre a estratégia de execução de instruções RISC-V.
///
/// Implementações devem ser `Send` para que o driver TUI possa mover o backend
/// entre threads (hartthread model futuro).
///
/// Métodos com implementação padrão (`invalidate`, `hot_profile`) têm comportamento
/// no-op/None para backends que não os usam, evitando que implementações simples
/// precisem sobrescrever tudo.
pub trait ExecutionBackend<B: Bus>: Send {
    /// Retorna o tipo deste backend — usado para logging e display na TUI.
    fn kind(&self) -> BackendKind;

    /// Executa até um ponto de rendimento natural do backend:
    /// - Interpretador: 1 instrução.
    /// - Bloco JIT: todas as instruções do basic block até o terminador.
    fn run_until_yield(&mut self, ctx: &mut ExecCtx<'_, B>) -> Result<ExecOutcome, FalconError>;

    /// Invalida blocos compilados cujo intervalo [start_pc, end_pc) intersecta
    /// [start, end). Chamado pelo driver quando `cpu.pending_exec_map` sinaliza
    /// Self-Modifying Code. Backends sem cache compilado ignoram (no-op padrão).
    fn invalidate(&mut self, _start: u32, _end: u32) {}

    /// Acesso ao perfil de branches quentes, se o backend o mantém.
    /// Retorna `None` para backends que não rastreiam frequência de branches.
    fn hot_profile(&self) -> Option<&HotProfile> {
        None
    }
}
