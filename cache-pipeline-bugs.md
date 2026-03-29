# Cache and Pipeline Bug Audit

Date: 2026-03-28

This file records concrete bugs found while reviewing the cache hierarchy and pipeline simulator.

## ✅ 1. Lower cache levels become stale after L1 D-cache stores

**FIXED** — `dcache_store_bytes` now calls `level.invalidate_line(addr)` on all
`extra_levels` at the end of every write, preventing stale L2/L3 hits.

Files:
- `src/falcon/cache.rs:1073`
- `src/falcon/cache.rs:1276`
- `src/falcon/cache.rs:1508`

What happens:
- `dcache_store_bytes()` updates only L1 D-cache and sometimes RAM.
- It never updates or invalidates `extra_levels` entries for the same line.
- Later, `fetch_line()` can satisfy an L1 miss from a stale lower-level cache hit instead of RAM.

Why this is a bug:
- A write-back L1 line can hold the newest value while L2/L3 still keep an older clean copy.
- When L1 evicts that line, `install_dcache_line()` writes it back straight to RAM, but the lower cache line is still stale.
- A future L1 miss will reload the stale L2/L3 line and effectively resurrect old data.

## ✅ 2. `fetch_line()` cannot build an upper-level line when lower levels use smaller line sizes

**FIXED** — `fetch_line` now loops assembling chunks from the next level until
`needed_size` bytes are collected, instead of returning a single possibly-truncated slice.

Files:
- `src/falcon/cache.rs:1276`
- `src/falcon/cache.rs:1313`
- `src/falcon/cache.rs:1330`
- `src/falcon/cache.rs:1366`

What happens:
- `fetch_line(addr, needed_size, from_level)` assumes one lower-level access can provide all `needed_size` bytes.
- On a hit, it returns `line_data[byte_offset..end]`, where `end` is clamped to the lower level line length.
- On a miss, it recursively fetches only `level_line_size` bytes from the next level.

## ✅ 3. AMAT is underreported whenever extra cache levels exist

**FIXED** — `icache_amat`, `dcache_amat`, and `extra_level_amat` now include each
level's `miss_penalty + line_transfer_cycles` in the miss-cost term before recursing.

Files:
- `src/falcon/cache.rs:1382`
- `src/falcon/cache.rs:1401`
- `src/falcon/cache.rs:1420`

What happens:
- `icache_amat()` and `dcache_amat()` use:
- `hit_latency + miss_rate * extra_level_amat(0)`
- When extra levels exist, the L1 miss penalty and L1 line transfer cost disappear from the formula.
- `extra_level_amat()` repeats the same pattern for lower levels, so intermediate miss penalties are also skipped.

## ✅ 4. Pipeline retired-instruction counting diverges from sequential execution

**FIXED** — `stage_wb` now increments `cpu.instr_count` before the early-return
paths for `Ecall`, `Ebreak`, and `Halt`, matching sequential mode.

Files:
- `src/falcon/exec.rs:23`
- `src/ui/pipeline/sim.rs:1360`
- `src/ui/pipeline/sim.rs:1383`
- `src/ui/pipeline/sim.rs:1405`

What happens:
- Sequential execution increments `cpu.instr_count` before executing the decoded instruction.
- Pipeline execution increments `cpu.instr_count` only at the bottom of `stage_wb()`.
- `stage_wb()` returns early for `ecall`, `ebreak`, and `halt`, so those instructions are never counted in pipeline mode.

## ✅ 5. Pipeline WAW/WAR reporting confuses integer and float register files

**FIXED** — `detect_name_hazards` now collects `(RegFile, rd)` pairs via
`slot_destination`; WAW compares both fields and uses float register names for FP
instructions.

Files:
- `src/ui/pipeline/mod.rs:439`
- `src/ui/pipeline/sim.rs:1882`
- `src/ui/pipeline/sim.rs:1925`
- `src/ui/pipeline/sim.rs:1961`

What happens:
- `PipeSlot` stores only a numeric `rd`.
- `detect_name_hazards()` builds its writer list from raw `rd` values only.
- WAW checks compare register number alone, and the displayed name always comes from `reg_name()`.

## ✅ 6. UI sequential mode double-counts executed instructions

**FIXED** — removed the explicit `mem.instruction_count += 1` from the sequential
path in `hart.rs`; `fetch32` inside `exec::step` already increments it once.

Files:
- `src/falcon/cache.rs:1550`
- `src/ui/app/hart.rs:134`
- `src/ui/app/hart.rs:158`

What happens:
- `CacheController::fetch32()` increments `self.instruction_count` on every instruction fetch.
- The sequential UI path then calls `mem.instruction_count += 1` again after `exec::step()`.

## ✅ 7. Pipeline memory access faults are logged but do not fault or stop execution

**FIXED** — `stage_mem` now returns `(latency, faulted)`; both call sites set
`state.faulted = true` when `faulted` is returned as `true`.

Files:
- `src/ui/pipeline/sim.rs:962`
- `src/ui/pipeline/sim.rs:990`
- `src/ui/pipeline/sim.rs:1051`
- `src/ui/pipeline/sim.rs:1413`

What happens:
- `stage_mem()` catches load/store errors and only calls `console.push_error(...)`.
- It does not mark the slot invalid, does not set `state.faulted`, and does not stop the pipeline.
- `commit_wb()` will still retire the instruction later unless something else stops execution.

## ✅ 8. Pipeline fetch faults are dropped silently

**FIXED** — `fetch_slot` now returns `(Option<PipeSlot>, bool)`; a fetch error
returns `(None, true)` and the caller sets `state.faulted = true`.

Files:
- `src/ui/pipeline/sim.rs:2244`

What happens:
- `fetch_slot()` converts any fetch error into `None`.
- The caller cannot distinguish "no slot because fetch failed" from "no slot because nothing was fetched this cycle."

## ✅ 9. `inclusion` is user-configurable but functionally ignored

**FIXED** — `CacheConfig::validate()` now rejects `Inclusive` and `Exclusive`
configs with a clear error until the feature is implemented.

Files:
- `src/falcon/cache.rs:39`
- `src/falcon/cache.rs:67`
- `docs/cache-config.md:146`

What happens:
- `CacheConfig` exposes `inclusion: InclusionPolicy`.
- Docs describe `Inclusive` and `Exclusive` as real hierarchy behaviors.
- The cache implementation never branches on `config.inclusion`.
