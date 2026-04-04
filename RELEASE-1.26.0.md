# Raven v1.26.0

## New Instructions — RV32A Extension

Full **RV32A atomic** instruction support:

- `LR.W` / `SC.W` — load-reserved / store-conditional with per-hart reservation tracking
- `AMO` variants: `AMOSWAP`, `AMOADD`, `AMOXOR`, `AMOAND`, `AMOOR`, `AMOMAX`, `AMOMIN`, `AMOMAXU`, `AMOMINU`
- `FENCE` — now forwarded to the bus layer (was a no-op)
- `FENCE.I` — invalidates the I-cache, enabling self-modifying code to work correctly

Instruction panel type badges extended: `[A]` for atomic, `[F]` for float.

---

## Cache Coherence Bug Fixes

Four bugs that caused **silent data corruption** in multi-level cache (L1 D + L2) configurations, which could manifest as a program crash at `PC=0x00000000`:

1. **Blanket L2 invalidation after write-back stores removed** — on a write-allocate miss, L2 was being invalidated right after the dirty eviction wrote back to it, losing data in both cache levels.
2. **Write-through stores now correctly invalidate L2** — stale L2 copies are now dropped after write-through stores so future misses re-read the updated value from RAM.
3. **`sync_to_ram` writeback order fixed** — L2 (outer levels) is now flushed before D-cache to prevent stale L2 data from overwriting correct values in RAM.
4. **`flush_all` writeback order fixed** — same root cause as #3; applied when the user toggles the cache off.

---

## Pipeline Improvements

- **Per-FU stall counters** — stall counts broken down by functional unit (ALU, MUL, DIV, FPU, LSU, SYS)
- **Configurable FU counts** — set the number of each functional unit in `.pcfg`
- **Branch predictor expanded** — not-taken (default), always-taken, BTFNT, and 2-bit dynamic strategies
- **Per-hazard stall breakdown** in the pipeline footer
- **Speculative Gantt** — in-flight speculative instructions shown with distinct styling; flushed instructions marked
- `.pcfg` format updated: per-bypass-path flags replace the single `forwarding` boolean; `fu.*` count fields added

---

## rust-to-raven

- `raven_api` restructured: `hart.rs` moved to `hardware_thread/`
- New `atomic/` module: `Arc`, `AtomicBool`, `AtomicU32`, and friends backed by `LR.W`/`SC.W` — atomics actually work on the simulator now
- `main.rs` updated to use the new `HartTask` closure-based API
