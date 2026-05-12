//! Construção do backend de execução selecionado pela CLI.
//!
//! # Responsabilidade
//!
//! `make_backend` é o único ponto onde o código de fora do módulo `jit` decide
//! qual implementação concreta de [`ExecutionBackend`] usar. O restante do
//! sistema (CLI, TUI, testes) opera exclusivamente sobre o trait object
//! `Box<dyn ExecutionBackend<CacheController>>`, sem conhecer a variante concreta.
//!
//! Esse isolamento facilita a adição de novos backends (Fase C: `HotBackend`,
//! `FullBackend`) sem alterar nenhum call site fora deste arquivo.
//!
//! # Estado por fase
//!
//! | Fase | `None`  | `Hot`       | `Full`      |
//! |------|---------|-------------|-------------|
//! | A    | ✓       | Unsupported | Unsupported |
//! | B    | ✓       | Unsupported | Unsupported |
//! | C    | ✓       | ✓           | ✓           |
//!
//! Retornar `FalconError::Unsupported` com mensagem clara é melhor do que
//! `panic!` — a CLI pode exibir a mensagem ao usuário e sugerir `--jit=none`.

use crate::falcon::CacheController;
use crate::falcon::errors::FalconError;

use super::backend::{BackendKind, ExecutionBackend};
use super::interpreter::InterpreterBackend;

/// Constrói o backend de execução correspondente ao `kind` selecionado pela CLI.
///
/// `Hot` e `Full` requerem a cargo feature `jit` (dynasm-rs). Sem ela, retornam
/// `FalconError::Unsupported` com sugestão de uso.
pub fn make_backend(
    kind: BackendKind,
) -> Result<Box<dyn ExecutionBackend<CacheController>>, FalconError> {
    match kind {
        BackendKind::None => Ok(Box::new(InterpreterBackend::new())),

        #[cfg(feature = "jit")]
        BackendKind::Hot => Ok(Box::new(super::hot::HotBackend::new())),

        #[cfg(not(feature = "jit"))]
        BackendKind::Hot => Err(FalconError::Unsupported(
            "JIT 'hot' mode requires the 'jit' cargo feature. Rebuild with --features jit or use --jit=none.".into(),
        )),

        // Full requer cpu+mem para o scan eager; retorna Unsupported via make_backend.
        // Use make_full_backend para construir com estado inicial.
        BackendKind::Full => Err(FalconError::Unsupported(
            "Use make_full_backend(cpu, mem) para o modo --jit=full.".into(),
        )),
    }
}

/// Constrói o `FullBackend` com scan eager a partir do estado inicial do hart.
///
/// Separado de `make_backend` porque precisa de `cpu` e `mem` para o BFS inicial.
#[cfg(feature = "jit")]
pub fn make_full_backend(
    cpu: &crate::falcon::registers::Cpu,
    mem: &CacheController,
) -> Box<dyn ExecutionBackend<CacheController>> {
    Box::new(super::full::FullBackend::new(cpu, mem))
}
