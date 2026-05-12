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
/// Parameterizado em `CacheController` — o único tipo `Bus` usado nos caminhos
/// de execução hoje.
pub fn make_backend(
    kind: BackendKind,
) -> Result<Box<dyn ExecutionBackend<CacheController>>, FalconError> {
    match kind {
        BackendKind::None => Ok(Box::new(InterpreterBackend::new())),
        BackendKind::Hot => Err(FalconError::Unsupported(
            "JIT 'hot' mode is not yet implemented (Phase B). Use --jit=none.".into(),
        )),
        BackendKind::Full => Err(FalconError::Unsupported(
            "JIT 'full' mode is not yet implemented (Phase B). Use --jit=none.".into(),
        )),
    }
}
