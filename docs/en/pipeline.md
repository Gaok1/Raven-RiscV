# Pipeline simulation

Raven includes a cycle-by-cycle educational pipeline simulator for RV32I/M plus the Falcon extensions already supported by the project. The goal is not just to execute code, but to make visible why an instruction advanced, stalled, flushed, or received a forwarded value.

This document describes the current model implemented in the TUI pipeline tab.

---

## Overview

The pipeline is a classic 5-stage design:

1. `IF` — fetch instruction from the current `fetch_pc`
2. `ID` — decode, read registers, and compute early control information when configured
3. `EX` — ALU, branch condition, address generation, MUL/DIV/FP execution, functional-unit latency
4. `MEM` — loads, stores, atomics, cache latency
5. `WB` — write back results and retire the instruction

Each visible pipeline step corresponds to exactly one CPU clock cycle. Cache latency is folded into pipeline stalls, so the user never has to "step inside" a cache separately.

---

## Stage behavior

### IF

- Fetches through the I-cache, not raw RAM
- Stores per-fetch latency in the IF slot
- Holds the instruction in `IF` while remaining I-cache cycles are consumed
- Marks fetched instructions as speculative when a control instruction is already in flight

### ID

- Decodes the instruction word
- Reads integer or floating-point source registers
- Applies forwarding into decoded operands when enabled
- Re-reads operands when an instruction stays stalled in `ID`
- Can resolve branches in `ID` when branch resolution is configured there

### EX

- Computes ALU results
- Evaluates branches and jumps
- Computes effective addresses for loads/stores/atomics
- Applies forwarding again for EX-stage consumers
- Holds instructions for the configured `CPIConfig` latency in both pipeline modes
- Always renders the functional-unit panel in the TUI so execution opportunities stay visible

### MEM

- Executes memory access for loads/stores/atomics
- Uses D-cache timing for each access
- Converts total access latency into visible pipeline stall cycles
- Applies store-data forwarding before the memory access when needed

### WB

- Writes the final result to integer or FP register files
- Handles `ecall`, `halt`, and `ebreak`
  `exit/exit_group` stop the whole program; `halt` stops the current hart permanently, while `ebreak`
  is a resumable debug stop
- Counts the instruction as retired only here

---

## Hazards Raven models

### RAW hazards

Read-after-write hazards are detected between in-flight producer and consumer instructions.

- With forwarding enabled:
  - Raven emits a forwarding trace and lets the consumer proceed when the value is already available
  - A true load-use case still stalls until the value exists
- With forwarding disabled:
  - Raven stalls `ID` until the producer has safely written back

### Syscall ABI barrier (`ecall`)

`ecall` is modeled conservatively as a privileged ABI boundary for the integer argument/result registers:

- it is treated as reading all of `a0..a7`
- younger instructions that consume `a0..a7` must wait until the `ecall` retires
- older instructions that are still producing `a0..a7` can also stall an `ecall` in `ID`

This rule is intentionally stronger than minimal forwarding because syscall handlers may:

- consume arguments from `a0..a7`
- return values in `a0`
- update simulator-visible state in ways that should not race with nearby instructions

In practice, this makes `ecall` behave like a conservative pipeline barrier around the ABI argument bank, avoiding stale-operand bugs in syscall-heavy or runtime-generated code.

### Load-use hazards

These are treated specially because a loaded value only becomes available after `MEM`.

Covered instructions currently include:

- `lb`, `lh`, `lw`, `lbu`, `lhu`
- `flw`
- `lr.w`

### WAW and WAR hazards

Raven reports `WAW` and `WAR` hazards as informational traces so the user can see overlapping name dependencies even when they do not force a stall in this in-order design.

### Control hazards

Branches and jumps can:

- redirect fetch through prediction
- mark younger instructions as speculative
- flush younger stages on mispredict

The Gantt/history view distinguishes a flushed instruction from a normal bubble.

### Cache stalls

- I-cache latency stalls `IF`
- D-cache latency stalls `MEM`
- If `MEM` is already blocking the pipe, `IF` does not silently burn its own pending stall cycles in the background
- The UI labels these separately from data stalls: a valid instruction can be shown as waiting in `IF`/`MEM`, while `ID` can show an upstream/front-end wait if no new instruction arrived

---

## Forwarding model

When forwarding is enabled, Raven can bypass values from older stages into younger consumers:

- `EX/MEM/WB -> ID`
- `MEM/WB -> EX`
- `WB -> MEM` for store-data cases

Forwarding is tracked in two places:

- stage badges and warnings inside the pipeline panel
- the lower Hazard / Forwarding Map, which shows producer and consumer stages explicitly

Raven also distinguishes a forwarding-covered RAW from a true stall-producing RAW.

---

## Branch prediction and flush behavior

Static prediction is configurable in the pipeline settings.

Current behavior:

- branch/jump prediction is attached to the instruction once it reaches `ID`
- predicted-taken control flow redirects `fetch_pc`
- younger wrong-path instructions are marked speculative
- on mismatch, Raven flushes younger stages and redirects to the real target

Visual markers:

- predicted instructions receive prediction badges
- squashed instructions receive flush/squash markers
- front-end bubbles and fetch waits are labeled separately from instruction stalls
- the hazard map shows control-flush paths separately from data hazards

---

## Execution model

The pipeline config currently exposes two execution models:

- `Serialized`
- `Parallel UFs`

Both use the same functional-unit panel in the UI. The difference is semantic, not cosmetic.

In the current implementation, execution still behaves like a single in-order EX path, and `EX` can remain busy for multiple cycles depending on class:

- `ALU`
- `MUL`
- `DIV`
- `LOAD`
- `STORE`
- `BRANCH`
- `JUMP`
- `SYSTEM`
- `FP`

While a long-latency instruction holds `EX`, the front of the pipe remains blocked and Raven keeps that state visible without letting unrelated IF latency progress incorrectly. The functional-unit panel breaks that latency down by FU so the user can see which resource is active and where parallelism could exist once the execution model allows it.

---

## Cache interaction

The pipeline and cache now share one clock model:

- an access returns a total latency
- the pipeline turns that latency into `N` visible stall cycles
- per-level stats remain accumulated in the cache model as local service cost
- timing stays visible in the pipeline model

This applies to:

- I-cache fetch stalls in `IF`
- D-cache stalls in `MEM`
- extra cache levels when configured
- all latency paid on the access path, including outer levels and writeback/fill work

The RAM sidebar also distinguishes cache presence by source:

- `I1` = L1 instruction cache
- `D1` = L1 data cache
- `U2`, `U3`, ... = unified outer cache levels

---

## Recommended example programs

The following example programs are intended specifically for pipeline inspection:

- `Program Examples/pipeline_forwarding_demo.fas`
- `Program Examples/pipeline_load_use_demo.fas`
- `Program Examples/pipeline_branch_flush_demo.fas`
- `Program Examples/pipeline_cache_stall_demo.fas`

Suggested workflow:

1. Open the program in the editor
2. Assemble and load it
3. Switch to the Pipeline tab
4. Step cycle by cycle
5. Watch the Hazard / Forwarding Map and the Gantt history together

---

## Notes and limits

- The simulator is intentionally didactic, not a claim of cycle-accurate silicon behavior
- `WAW` and `WAR` are reported visually even when no stall is required
- Unified outer cache levels are shared between instruction and data traffic, so the RAM sidebar labels them as `U2+`
- CLI execution validates the same assembler/execution path, but the graphical pipeline traces are TUI-only
