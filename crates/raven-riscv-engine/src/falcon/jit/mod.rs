//! Sistema JIT do Raven: compilaÃ§Ã£o de basic blocks para execuÃ§Ã£o nativa.
//!
//! # VisÃ£o geral
//!
//! O JIT Ã© implementado em fases incrementais, cada uma compilÃ¡vel e testÃ¡vel
//! independentemente:
//!
//! | Fase | O que adiciona                                      | Status |
//! |------|-----------------------------------------------------|--------|
//! | A    | Scaffolding: trait, factory, backends stub, flag CLI | âœ“      |
//! | B    | `scan_block`, `CompiledBlockCache`, codegen dynasm  | âœ“      |
//! | C    | Backends `hot` e `full` em produÃ§Ã£o                 | âœ“      |
//!
//! # OrganizaÃ§Ã£o dos mÃ³dulos
//!
//! ```text
//! jit/
//! â”œâ”€â”€ mod.rs        â€” este arquivo; re-exports pÃºblicos
//! â”œâ”€â”€ backend.rs    â€” trait ExecutionBackend + tipos ExecCtx, ExecOutcome
//! â”œâ”€â”€ interpreter.rs â€” backend de referÃªncia (wraps exec::step)
//! â”œâ”€â”€ profile.rs    â€” HotProfile: contador de alvos de desvios tomados
//! â”œâ”€â”€ block.rs      â€” scan_block: detecta basic blocks na memÃ³ria
//! â”œâ”€â”€ cache.rs      â€” CompiledBlockCache: armazena blocos compilados
//! â”œâ”€â”€ codegen.rs    â€” dynasm-rs codegen (feature "jit")
//! â””â”€â”€ factory.rs    â€” make_backend: constrÃ³i o backend selecionado pela CLI
//! ```
//!
//! # Por que o modo pipeline fica fora deste trait?
//!
//! O modo pipeline (`--pipeline`) Ã© um *simulador de stages*, nÃ£o um
//! interpretador de instruÃ§Ãµes individuais. Ele modela stalls, forwarding e
//! hazards para fins didÃ¡ticos. Plugar o JIT no pipeline inverteria a
//! arquitetura: o valor do pipeline estÃ¡ na simulaÃ§Ã£o dos stages, nÃ£o na
//! velocidade de execuÃ§Ã£o. Por isso `ui::pipeline::sim::pipeline_tick` nÃ£o
//! passa pelo trait `ExecutionBackend`.

pub mod backend;
pub mod block;
pub mod cache;
pub mod factory;
pub mod interpreter;
pub mod profile;

#[cfg(feature = "jit")]
pub mod codegen;
#[cfg(feature = "jit")]
pub mod hot;
#[cfg(feature = "jit")]
pub mod full;

pub use backend::{BackendKind, ExecCtx, ExecOutcome, ExecutionBackend};
pub use block::{scan_block, BasicBlock, BlockTerminator};
pub use factory::make_backend;
#[cfg(feature = "jit")]
pub use factory::make_full_backend;
pub use interpreter::InterpreterBackend;
pub use profile::HotProfile;

#[cfg(feature = "jit")]
pub use hot::HotBackend;
#[cfg(feature = "jit")]
pub use full::FullBackend;

#[cfg(test)]
#[path = "../../../tests/support/falcon_jit.rs"]
mod tests;

