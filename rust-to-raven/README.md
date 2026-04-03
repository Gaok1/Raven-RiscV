# rust-to-raven

Minimal `no_std` support crate and examples for code that runs inside Raven.

## Atomic wrappers

The atomic API lives in [src/raven_api/atomic](/home/gaok1/rust/Raven/rust-to-raven/src/raven_api/atomic).
It provides:

- `AtomicBool`
- `AtomicU32`
- `AtomicI32`
- `AtomicUsize`
- `Ordering`

These wrappers are implemented directly on top of Raven's RV32A instructions:

- `lr.w`
- `sc.w`
- `amoadd.w`
- `amoand.w`
- `amoor.w`
- `amoxor.w`
- `amoswap.w`

That means the behavior follows the simulator's multi-hart atomic semantics,
instead of depending on a single-core host-side atomic fallback.

## Example

```rust
use crate::raven_api::atomic::{AtomicBool, AtomicU32, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU32 = AtomicU32::new(0);

fn publish_work() {
    COUNT.fetch_add(1, Ordering::AcqRel);
    READY.store(true, Ordering::Release);
}

fn try_consume() -> Option<u32> {
    if READY.load(Ordering::Acquire) {
        Some(COUNT.load(Ordering::Acquire))
    } else {
        None
    }
}
```

## Guidance

- Use `Release` when publishing data another hart will read.
- Use `Acquire` when consuming data that was published by another hart.
- Use `AcqRel` for read-modify-write operations that both consume and publish.
- Use `SeqCst` only when you really need the strongest global ordering.
- Prefer these wrappers over open-coded inline assembly in application code.

## Current scope

- The wrapper is built for 32-bit Raven targets.
- It covers the common integer atomic operations needed by hart coordination.
- It does not try to emulate a full host `std::sync::atomic` surface.
