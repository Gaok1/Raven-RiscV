# Multi-hart execution — optimization roadmap

## Problem statement

Running with 2+ harts is noticeably slower than single-hart execution, even at `Instant` speed. The slowdown compounds with more harts and becomes obvious to the user during any non-trivial program.

---

## Root cause analysis

The bottleneck lives in `step_all_cores_once` (src/ui/app/mod.rs), which is called on every `single_step` in `AllHarts` scope. For N running harts it executes:

```
for core_idx in 0..max_cores:
    switch_selected_core(core_idx)   ← sync_selected_core_to_runtime()
                                       sync_runtime_to_selected_core()
    pipeline_step() / single_step_selected_sequential()
sync_selected_core_to_runtime()
sync_runtime_to_selected_core()      ← restore original
```

Each `switch_selected_core` call clones the full logical state of one hart **into** `self.run` and **out of** it for the next hart. With 2 harts that is ≥ 4 full state copies per step. The state being cloned includes:

| Field                  | Type                              | Clone cost          |
|------------------------|-----------------------------------|---------------------|
| `exec_counts`          | `HashMap<u32, u64>`               | O(unique PCs) alloc |
| `exec_trace`           | `VecDeque<(u32, String)>`         | 200 × String alloc  |
| `PipelineSimState`     | struct with `VecDeque<GanttRow>`  | 12 rows × String + VecDeque per row |
| `cpu: Cpu`             | 64 × u32 registers                | cheap (256 B)       |
| `mem_access_log`       | `Vec<(u32, u32, u8)>`             | small               |

Additionally, `single_step` calls `step_all_cores_once` in a loop of up to **200 iterations** (to advance one pipeline instruction on the selected core). With 2 harts at `Instant` speed, each 8 ms budget frame may call this loop hundreds of times — resulting in tens of thousands of HashMap and String clones per frame.

A secondary cost: `single_step_selected_sequential` and `pipeline_step` both call `falcon::decoder::decode(word)` + `format!("{instr:?}")` on every executed instruction to build the exec_trace string. This is unnecessary when the trace is not visible.

---

## Phase 1 — Quick wins (no architectural change)

These are isolated, low-risk changes. Each one is independently mergeable.

### 1.1 Lazy disassembly in exec_trace

**Current:** `exec_trace: VecDeque<(u32, String)>` stores fully formatted instruction strings.
**Change:** Store `(u32, u32)` — PC and raw instruction word. Format the string only when rendering the trace view.

**Impact:** Eliminates `decode` + `format!` for every instruction executed by every hart. This is the single hottest allocation in the Instant run path.

**Files:** `src/ui/app/mod.rs` (both `single_step_selected_sequential` and `pipeline_step`), rendering code in the Run tab.

---

### 1.2 Skip Gantt updates outside the Pipeline tab

**Current:** `pipeline_tick` (in `src/ui/pipeline/sim.rs`) updates the Gantt history on every cycle, building `GanttRow` entries with `VecDeque<GanttCell>` and cloned `String` disasm fields.
**Change:** Add a `record_gantt: bool` flag to `PipelineSimState` (or pass it as a parameter). Set it to `true` only when the Pipeline tab is the active tab.

**Impact:** When running at `Instant` speed from the Run/Cache tabs, the Gantt VecDeque receives no pushes — no String clones, no VecDeque reallocations for 12 × 20 cells per pipeline tick per hart.

**Files:** `src/ui/pipeline/sim.rs`, `src/ui/app/mod.rs`.

---

### 1.3 Cap exec_counts growth in the hot path

**Current:** `exec_counts: HashMap<u32, u64>` grows unboundedly during long runs.
**Change:** No structural change needed — just stop cloning it in the context-switch path (see Phase 2). As a standalone improvement, pre-size the HashMap on first allocation to the instruction count of the loaded program.

---

### 1.4 Reduce PipelineSimState clone size

**Current:** `gantt: VecDeque<GanttRow>` is cloned (via `std::mem::replace`) on every `sync_selected_core_to_runtime` / `sync_runtime_to_selected_core`.
**Change:** Cap `MAX_GANTT_ROWS` to 8 (down from 12) for non-selected harts, or zero-fill the Gantt for harts that are not currently displayed (their history is not visible anyway until the user selects them).

---

## Phase 2 — Structural: eliminate per-step context switching

This is the correct long-term fix. It requires a moderate refactor but does not touch the simulation logic.

### The problem in one sentence

`step_all_cores_once` routes every hart's execution through `self.run` (the "selected core" proxy) because `pipeline_step` and `single_step_selected_sequential` operate on `self.run.*`. Switching to each hart requires syncing the full state in and out.

### The fix

Introduce a **direct step path** that operates on a `HartCoreRuntime` reference without touching `self.run`:

```rust
fn step_hart_direct(
    hart: &mut HartCoreRuntime,
    mem: &mut CacheController,
    console: &mut Console,
    cpi: &CpiConfig,
) -> StepOutcome
```

This function takes the hart by direct mutable reference. It:
- Calls `falcon::exec::step(&mut hart.cpu, mem, console)` (or `pipeline_tick` with the hart's own `PipelineSimState`)
- Updates `hart.exec_counts` in place
- Appends to `hart.exec_trace` in place
- Returns a `StepOutcome` enum (`Running`, `Exited`, `Paused`, `Faulted`)

`step_all_cores_once` then becomes:

```rust
fn step_all_cores_once(&mut self) -> bool {
    let mem = &mut self.run.mem;
    let console = &mut self.console;
    let cpi = &self.run.cpi_config;

    for hart in &mut self.harts {
        if hart.lifecycle != HartLifecycle::Running { continue; }
        let outcome = step_hart_direct(hart, mem, console, cpi);
        hart.lifecycle = outcome.into_lifecycle();
        // propagate global-exit / fault
    }

    // Sync selected core display state once, at the end
    self.sync_runtime_to_selected_core();
    // ...
}
```

This eliminates all per-hart context switches. The HashMap/VecDeque clones become **zero** for non-selected harts. The selected hart state is synced once per round of all harts, not N×2 times.

### Impact (estimated)

With 2 harts, non-pipeline mode:
- Before: 4 HashMap clones + 4 VecDeque clones per step
- After: 0 clones per step (exec_counts/exec_trace update in place)
- Expected speedup: 3–5× at Instant speed

With 2 harts, pipeline mode (Gantt also fixed via 1.2):
- Before: 4 HashMap clones + 4 VecDeque<GanttRow> clones per pipeline tick
- After: 0 HashMap clones, 0 Gantt clones for non-selected harts
- Expected speedup: 4–8×

### Memory sharing note

`CacheController` (`self.run.mem`) is already logically shared — all harts currently use the same instance via the `self.run.mem` reference threaded through each step. The direct step path preserves this by passing `mem` by `&mut` reference, accessed sequentially. No locking needed.

---

## Phase 3 — Advanced: parallel hart execution

Once Phase 2 is in place, the hart step loop becomes a simple sequential iteration over independent CPU states against a single shared memory. This is the precondition for parallelism.

### Option A: Rayon parallel iterator (speculative, may require memory refactor)

```rust
self.harts.par_iter_mut()
    .filter(|h| h.lifecycle == HartLifecycle::Running)
    .for_each(|hart| {
        step_hart_direct(hart, &shared_mem, ...);
    });
```

**Blocker:** `CacheController` is `&mut` — it must become `Arc<Mutex<CacheController>>` or a lock-free structure. Cache contention between harts would need modeling anyway (the educational value of showing cache interference between harts).

**Recommendation:** Defer unless hart count regularly reaches 4+. The sequential Phase 2 path is already a large win.

### Option B: Speculative execution for non-selected harts

For the focused hart (the one being displayed), track all state fully. For all other harts:
- Don't track `exec_counts` or `exec_trace` at all during `Instant` run
- Only capture them when the user switches to that hart's view

This is a UI-level optimization: the user cannot observe what they are not looking at. It drops the per-step cost for non-selected harts to essentially the raw instruction execution cost.

---

## Effort estimate and priority

| Phase | Effort | Risk | Speedup |
|-------|--------|------|---------|
| 1.1 Lazy disassembly             | 1–2h | Low | 1.5–2×  |
| 1.2 Skip Gantt outside Pipeline  | 1h   | Low | 1.5–3× (pipeline mode) |
| 1.4 Cap Gantt rows for non-selected harts | 30m | Low | small |
| 2   Direct step path             | 4–6h | Medium | 3–8×   |
| 3A  Rayon parallel               | 8–16h | High | 2–4× additional |
| 3B  Skip tracking non-selected   | 2–3h | Low | 2× additional |

**Recommended order:** 1.1 → 1.2 → 2 → 3B. Skip 3A unless the educational value of multi-hart cache interaction becomes a priority.

---

## Notes on correctness

- The round-robin order of hart execution must be preserved — hart 0 always steps before hart 1 in any given global cycle. Phase 2 keeps this by iterating `harts` in index order.
- `heap_break` propagation (all harts share the same `sbrk` pointer) must remain a post-step operation, not per-hart.
- `exit` / `exit_group` in any hart must still kill all harts immediately. The outcome return from `step_hart_direct` handles this.
- Breakpoints in non-selected harts should pause only that hart (already the behavior in `AllHarts` scope); Phase 2 preserves this via the `StepOutcome` enum.
