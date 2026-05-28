//! Sistema JIT do Raven: compilação de basic blocks para execução nativa.
//!
//! # Visão geral
//!
//! O JIT é implementado em fases incrementais, cada uma compilável e testável
//! independentemente:
//!
//! | Fase | O que adiciona                                      | Status |
//! |------|-----------------------------------------------------|--------|
//! | A    | Scaffolding: trait, factory, backends stub, flag CLI | ✓      |
//! | B    | `scan_block`, `CompiledBlockCache`, codegen dynasm  | ✓      |
//! | C    | Backends `hot` e `full` em produção                 | ✓      |
//!
//! # Organização dos módulos
//!
//! ```text
//! jit/
//! ├── mod.rs        — este arquivo; re-exports públicos
//! ├── backend.rs    — trait ExecutionBackend + tipos ExecCtx, ExecOutcome
//! ├── interpreter.rs — backend de referência (wraps exec::step)
//! ├── profile.rs    — HotProfile: contador de alvos de desvios tomados
//! ├── block.rs      — scan_block: detecta basic blocks na memória
//! ├── cache.rs      — CompiledBlockCache: armazena blocos compilados
//! ├── codegen.rs    — dynasm-rs codegen (feature "jit")
//! └── factory.rs    — make_backend: constrói o backend selecionado pela CLI
//! ```
//!
//! # Por que o modo pipeline fica fora deste trait?
//!
//! O modo pipeline (`--pipeline`) é um *simulador de stages*, não um
//! interpretador de instruções individuais. Ele modela stalls, forwarding e
//! hazards para fins didáticos. Plugar o JIT no pipeline inverteria a
//! arquitetura: o valor do pipeline está na simulação dos stages, não na
//! velocidade de execução. Por isso `ui::pipeline::sim::pipeline_tick` não
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
