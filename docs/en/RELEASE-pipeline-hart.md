# Raven — Pipeline & Hardware Threads

This release adds two interconnected features that make the inner mechanics of a RISC-V processor visible in a way that no static textbook diagram can match: a **cycle-by-cycle pipeline simulator** and **multi-hart (hardware thread) execution**.

Both features work with every program that already runs in Raven. No changes to your assembly are required to observe the pipeline. Hart spawning is opt-in via the `hart_start` syscall.

---

## Pipeline simulation

### What it is

A classic **5-stage in-order pipeline** running alongside every execution in Raven. When the Pipeline tab is open, you watch the machine tick one clock cycle at a time.

```
IF  →  ID  →  EX  →  MEM  →  WB
```

Each stage does exactly what a real core does:

| Stage | Work                                                           |
|-------|----------------------------------------------------------------|
| `IF`  | Fetch from I-cache. Stall here when the cache misses.         |
| `ID`  | Decode, read registers, apply forwarding, attach prediction.  |
| `EX`  | ALU, branch evaluation, address generation, FU latency.       |
| `MEM` | Load/store through D-cache. Stall here on cache miss.         |
| `WB`  | Write result, retire instruction, handle syscalls.            |

### Hazards — made visible

Every hazard that would stall or flush your program is labeled and shown in the **Hazard / Forwarding Map** below the pipeline:

| Hazard       | Color       | What it means                                      |
|--------------|-------------|----------------------------------------------------|
| `RAW`        | amber       | Read-after-write: consumer needs a value not yet written |
| `LOAD`       | amber       | Load-use: load result is only available after MEM  |
| `CTRL`       | red         | Branch/jump: wrong-path instructions were flushed  |
| `FWD`        | blue        | Forwarding bypassed the hazard — no stall needed   |
| `FU`         | purple      | Functional unit busy (multi-cycle mode)            |
| `MEM`        | steel blue  | Cache latency converted to pipeline stall cycles   |
| `WAW`/`WAR`  | —           | Informational: name dependency, no stall required  |

### Gantt history

The Gantt chart to the right of the stage view shows the last 12 instructions as a timeline. Each row is one instruction; each column is one clock cycle. You can see at a glance where an instruction stalled, which cycle it committed, and how many bubbles a branch flush introduced.

Cell types: `IF` `ID` `EX` `MEM` `WB` `──` (stall) `·` (bubble) `✕` (flush)

### Forwarding

With forwarding enabled, Raven bypasses results from older to younger stages along three paths:

```
EX/MEM/WB  →  ID
   MEM/WB  →  EX
       WB  →  MEM  (store data)
```

The map distinguishes a forwarding-covered RAW (no stall, shown as `FWD`) from a true load-use stall.

### Branch prediction

Prediction is configurable: **not-taken** (default), **always-taken**, **BTFNT**, or **2-bit Dynamic**. The pipeline attaches a prediction badge when the branch reaches `ID`. If the prediction is wrong, younger speculative instructions are flushed and the fetch redirects.

You can also choose where branches resolve: `ID` (1 bubble), `EX` (2 bubbles, default), or `MEM` (3 bubbles).

### Functional-unit mode

Switch from *Single-cycle* to *Functional Units* in the pipeline config. Each instruction class then has its own configurable latency — MUL holds EX for 3 cycles, DIV for 20, FP for 5, and so on. The pipe stalls visibly while the unit is occupied.

### Cache integration

The pipeline and cache share one clock model. An IF or MEM access pays the full latency returned by the cache hierarchy, including outer levels and any writeback/fill work triggered by that access. The cache stats tab and the pipeline Gantt stay in sync: the pipeline pays the wall-clock time, while each cache level keeps its own local service-cost stats.

### Configuration file

Pipeline settings persist in a `.pcfg` file:

```
# Raven Pipeline Config v1
enabled=true
bypass.ex_to_ex=true
bypass.mem_to_ex=true
bypass.wb_to_id=true
bypass.store_to_load=false
mode=SingleCycle
fu.alu=1
fu.mul=1
fu.div=1
fu.fpu=1
fu.lsu=1
fu.sys=1
branch_resolve=Ex
predict=NotTaken
speed=Normal
```

---

## Hardware threads (harts)

### What a hart is

A **hart** is an independent RISC-V execution context — its own PC, register file, and stack — running concurrently with other harts. All harts share the same flat address space and cache hierarchy. There is no memory protection between harts.

In Raven, each hart maps to one simulated core slot. The maximum number of simultaneous harts is set in Settings (`max_cores`).

### Spawning a hart

```asm
# Assembly: syscall 1100
la   a0, worker       # entry PC
la   a1, stack_top    # stack pointer (top/high address, 16-byte aligned)
li   a2, 42           # argument passed to the new hart in a0
li   a7, 1100
ecall                 # returns hart id in a0 (< 0 = error)
```

```c
// C: c-to-raven
static char stack[4096];
SPAWN_HART(worker, stack, /*arg=*/42);
```

```rust
// Rust: rust-to-raven
static mut STACK: [u8; 4096] = [0; 4096];
spawn_hart_fn(worker, unsafe { &mut STACK }, 42);
// or with a closure:
spawn_hart(move || { /* ... */ hart_exit() }, unsafe { &mut STACK });
```

### Terminating a hart

| Syscall      | Number | Effect                                          |
|--------------|--------|-------------------------------------------------|
| `hart_exit`  | `1101` | This hart only. Others keep running.            |
| `exit`       | `93`   | All harts. Global program exit.                 |
| `exit_group` | `94`   | All harts. Identical to `exit` in Raven.        |

### Hart lifecycle states

`FREE` → `RUN` → `EXIT` / `BRK` / `FAULT`

A fault in any hart stops the entire simulation. A breakpoint pauses only that hart; the rest continue if the run scope is `ALL`.

### Pipeline per hart

Every spawned hart has its own pipeline state. When you switch cores in the Pipeline tab, you see that hart's in-flight stages, Gantt history, hazard map, and statistics — independent of every other hart.

### Execution model

Harts advance in round-robin lockstep: each global step advances all running harts by one cycle (or one instruction in non-pipeline mode). The shared memory means cache effects from one hart are immediately visible to all others.

---

## New example programs

Five programs in `Program Examples/` are designed specifically for these features:

| Program                              | What to observe                                |
|--------------------------------------|------------------------------------------------|
| `pipeline_forwarding_demo.fas`       | Chained RAW hazards, forwarding bypass paths   |
| `pipeline_load_use_demo.fas`         | Load-use stalls on every access                |
| `pipeline_branch_flush_demo.fas`     | Wrong-path squashes on every loop iteration    |
| `pipeline_cache_stall_demo.fas`      | Streaming access pattern, MEM and IF stalls    |
| `hart_spawn_visual_demo.fas`         | Three concurrent harts, core selector in action|

Suggested workflow for the pipeline demos:

1. Open a program in the Editor tab and assemble (`Ctrl+Enter`)
2. Switch to the Pipeline tab
3. Press `Space` to run — watch the Gantt fill up in real time
4. Slow down with the speed control to step cycle by cycle
5. Toggle forwarding off in the Config subtab and compare CPI

---

## Interactive tutorial

The pipeline tutorial (click `[?]` in the Pipeline tab) walks through every part of the interface: the stage view, the hazard map, the Gantt chart, and the config panel. It points to the exact UI element it is describing at each step.

---

## Stats

The pipeline tab footer shows live metrics:

```
Cycle  342   Instr  211   CPI  1.62   Stalls  89   Flushes  12
```

The per-hazard breakdown is reported as **stall tags**. A single stalled cycle can contribute to more than one tag when multiple causes overlap.

---

## Summary of new keys and controls

| Control                          | Action                                  |
|----------------------------------|-----------------------------------------|
| Pipeline tab → Config subtab     | Toggle forwarding, mode, prediction     |
| Speed button in Pipeline toolbar | Slow / Normal / Fast / Instant          |
| Core button in Pipeline/Run toolbar | Switch between hart core slots       |
| Settings → `max_cores`           | Set number of concurrent hart slots     |
| Settings → Run scope `ALL/FOCUS` | Step all harts or only the focused one  |
| `Ctrl+Shift+P`                   | Save / load pipeline config (`.pcfg`)   |

---
