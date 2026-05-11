use crate::falcon::CacheController;
use crate::falcon::errors::FalconError;

use super::backend::{BackendKind, ExecutionBackend};
use super::interpreter::InterpreterBackend;

/// Construct the execution backend selected by the CLI.
///
/// Phase A only implements `BackendKind::None`. `Hot` and `Full` return a
/// clean `FalconError::Unsupported` so the CLI can surface a useful message
/// instead of panicking. The boxed trait object is parameterized on
/// `CacheController` — the only `Bus` type used in execution paths today.
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
