//! ConstruÃ§Ã£o do backend de execuÃ§Ã£o selecionado pela CLI.
//!
//! # Responsabilidade
//!
//! `make_backend` Ã© o Ãºnico ponto onde o cÃ³digo de fora do mÃ³dulo `jit` decide
//! qual implementaÃ§Ã£o concreta de [`ExecutionBackend`] usar. O restante do
//! sistema (CLI, TUI, testes) opera exclusivamente sobre o trait object
//! `Box<dyn ExecutionBackend<CacheController>>`, sem conhecer a variante concreta.
//!
//! Esse isolamento facilita a adiÃ§Ã£o de novos backends (Fase C: `HotBackend`,
//! `FullBackend`) sem alterar nenhum call site fora deste arquivo.
//!
//! # Estado por fase
//!
//! | Fase | `None`  | `Hot`       | `Full`      |
//! |------|---------|-------------|-------------|
//! | A    | âœ“       | Unsupported | Unsupported |
//! | B    | âœ“       | Unsupported | Unsupported |
//! | C    | âœ“       | âœ“           | âœ“           |
//!
//! Retornar `FalconError::Unsupported` com mensagem clara Ã© melhor do que
//! `panic!` â€” a CLI pode exibir a mensagem ao usuÃ¡rio e sugerir `--jit=none`.

use crate::falcon::CacheController;
use crate::falcon::errors::FalconError;

use super::backend::{BackendKind, ExecutionBackend};
use super::interpreter::InterpreterBackend;

/// ConstrÃ³i o backend de execuÃ§Ã£o correspondente ao `kind` selecionado pela CLI.
///
/// `Hot` e `Full` requerem a cargo feature `jit` (dynasm-rs). Sem ela, retornam
/// `FalconError::Unsupported` com sugestÃ£o de uso.
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

/// ConstrÃ³i o `FullBackend` com scan eager a partir do estado inicial do hart.
///
/// Separado de `make_backend` porque precisa de `cpu` e `mem` para o BFS inicial.
#[cfg(feature = "jit")]
pub fn make_full_backend(
    cpu: &crate::falcon::registers::Cpu,
    mem: &CacheController,
) -> Box<dyn ExecutionBackend<CacheController>> {
    Box::new(super::full::FullBackend::new(cpu, mem))
}

