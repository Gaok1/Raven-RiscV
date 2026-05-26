# rust-to-raven

Minimal `no_std` support crate and examples for code that runs inside Raven.

## Coroutines

Stackful **cooperative** coroutines live in
[src/raven_api/coroutine.rs](/home/gaok1/rust/Raven/rust-to-raven/src/raven_api/coroutine.rs).
A `Coroutine` runs a closure on its own stack; the closure receives a `Yielder`
whose `suspend` hands control back to `resume`, keeping the stack alive so the
next `resume` continues exactly where it left off.

These are **single-hart** (one runs at a time) — distinct from the parallel hart
API. The switch is a pure user-space register/stack swap, no `ecall`. Unlike the
C SDK, the `Coroutine` allocates and owns its stack, freeing it on drop.

```rust
use crate::raven_api::Coroutine;

let mut counter = Coroutine::new(4096, |y| {
    for i in 1..=5usize {
        y.suspend(i);          // hand `i` back to resume; continues here next time
    }
});

while let Some(v) = counter.resume(0) {
    println!("yielded {v}");
}
// → yielded 1 .. yielded 5
```

`resume(send)` / `suspend(value)` exchange one `usize` in each direction (cast
pointers through it for richer payloads). Keep stacks modest — the default RAM
is 128 KB with no stack-overflow guard.

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
