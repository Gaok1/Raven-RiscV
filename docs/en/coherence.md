# Cache coherence

> [Leia em Português](../pt-BR/coherence.md)

When several cores each have a cache of their own, the same memory line can sit in more than one of them. The moment one core writes its copy, the others hold stale data. A **coherence protocol** is the set of rules the caches follow so that every core still reads the latest value.

RAVEN lets you turn private caches and a protocol on and off, and shows every step: which core asked for a line, what each cache did with its copy, and what it cost.

---

## Contents

1. [The cache model](#1-the-cache-model)
2. [Configuring coherence](#2-configuring-coherence)
3. [Line states](#3-line-states)
4. [Bus requests](#4-bus-requests)
5. [The three protocols](#5-the-three-protocols)
6. [Special cases](#6-special-cases)
7. [What it costs](#7-what-it-costs)
8. [Watching it in the TUI](#8-watching-it-in-the-tui)
9. [Counters in the CLI output](#9-counters-in-the-cli-output)
10. [Experiments](#10-experiments)
11. [Limits of the model](#11-limits-of-the-model)

---

## 1. The cache model

There are two shapes:

```
 coherence = shared (default)          coherence = none | msi | mesi | moesi

  core 0   core 1   core 2              core 0      core 1      core 2
     \       |       /                  ┌──────┐    ┌──────┐    ┌──────┐
      ┌─────────────┐                   │L1 I/D│    │L1 I/D│    │L1 I/D│
      │   L1 I / D  │                   └──┬───┘    └──┬───┘    └──┬───┘
      └──────┬──────┘                   ═══╪═══ snooping bus ══════╪═══
          L2, L3…                               L2, L3… (shared)
            RAM                                       RAM
```

- **`shared`** — every core uses one L1 I-cache and one L1 D-cache. Each line has a single copy, so there is nothing to keep coherent. This is what RAVEN always did, and it is still the default.
- **Any other value** — each core gets its own L1 I-cache and D-cache, all with the shape set in the Cache tab. L2 and below, and RAM, stay shared. The D-caches sit on a **snooping bus**: every miss, and every write to a line another cache may hold, is a request all the other D-caches see and answer.

Harts take turns: each round runs one instruction on every running core (see [hart.md](hart.md)). A request is answered before the next instruction runs, so the machine is always sequentially consistent: coherence here is about *which copy* a core reads and *what it costs*, not about reordering.

---

## 2. Configuring coherence

| Where | How |
|---|---|
| TUI | **Settings → MULTI-CORE CACHES**: `coherence` (←/→ or Enter cycles), `snoop latency`, `cache-to-cache`. Raise `max cores` to 2 or more and set `run scope` to `ALL`. |
| `.rcfg` | In the `[cache]` section: `coherence=`, `coherence.snoop_latency=`, `coherence.c2c_latency=` (see [cache-config.md](cache-config.md#coherence-keys)). |
| CLI | `raven run prog.s --cores 2 --coherence mesi` — overrides the config's `coherence=` (see [cli.md](cli.md)). |

| `coherence=` | Private L1s | Protocol | What it shows |
|---|---|---|---|
| `shared` | no | — | one copy per line; the baseline |
| `none` | yes | none | stale data: a core keeps reading its own old copy |
| `msi` | yes | MSI | the minimal invalidation protocol |
| `mesi` | yes | MESI | + Exclusive: the first write to an unshared line is free |
| `moesi` | yes | MOESI | + Owned: a dirty line is shared without a write-back |

Changing the protocol restarts the simulation, since the private caches are built with the machine. The two latencies only set what later requests cost and take effect at once.

---

## 3. Line states

Each line in a private D-cache is in one state:

| State | Letter | Valid | Dirty | Other copies | Meaning |
|---|---|---|---|---|---|
| Modified | `M` | yes | yes | none | The only copy, newer than memory. |
| Owned | `O` | yes | yes | maybe, all `S` | Newer than memory and shared. This cache answers for the line and writes it back when it leaves. MOESI only. |
| Exclusive | `E` | yes | no | none | The only copy, same as memory. A write needs no bus request. MESI and MOESI. |
| Shared | `S` | yes | no | maybe | One of possibly several read-only copies. |
| Invalid | `I` | no | — | — | Not held. |

MSI uses `M S I`, MESI `M E S I`, MOESI all five. In the Cache tab's **View** subtab the D-cache column that showed the dirty bit shows the letter instead, in the state's colour.

The I-caches hold read-only copies the protocol does not track; their lines show `-` as before.

---

## 4. Bus requests

| Request | Sent when | What the other caches do |
|---|---|---|
| `BusRd` | read miss | A dirty copy (`M`/`O`) writes itself back (MSI, MESI) or sends the data straight across (MOESI). `M`/`E` copies become `S` (MOESI: `M` → `O`). |
| `BusRdX` | write miss (read for ownership); also the read half of an AMO | Every copy becomes `I`; a dirty one is written back first (MSI, MESI) or sent across (MOESI). |
| `BusUpgr` | write hit on an `S` or `O` line | Every other copy becomes `I`. No data moves: the writer already has it. |
| `BusWr` | a write that goes on the bus as it is: every write-through store, and a write-back miss with no write-allocate | Every copy becomes `I`; a dirty one is written back first, so its later write-back cannot undo this write. |
| `IFetch` | an I-cache miss | A dirty data copy of the line writes itself back and becomes `S`, so the fetch reads the current bytes. |
| `WriteBack` | a dirty line leaves its cache (eviction or `fence.i`) | Nothing; logged so you can see the line go. |

---

## 5. The three protocols

Processor events (`PrRd`, `PrWr`) come from the core that owns the cache; bus events come from the others.

### MSI

| State | PrRd | PrWr | snooped BusRd | snooped BusRdX / BusUpgr / BusWr |
|---|---|---|---|---|
| `I` | `BusRd` → `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | stay `S` | → `I` |
| `M` | hit | hit | write back → `S` | write back → `I` |

A line that was read with no other copy around is still `S`, so its first write costs a `BusUpgr`.

### MESI

| State | PrRd | PrWr | snooped BusRd | snooped BusRdX / BusUpgr / BusWr |
|---|---|---|---|---|
| `I` | `BusRd` → `E` if no other copy, else `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | stay `S` | → `I` |
| `E` | hit | **silent** → `M` | → `S` | → `I` |
| `M` | hit | hit | write back → `S` | write back → `I` |

### MOESI

| State | PrRd | PrWr | snooped BusRd | snooped BusRdX / BusUpgr / BusWr |
|---|---|---|---|---|
| `I` | `BusRd` → `E` or `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | stay `S` | → `I` |
| `E` | hit | silent → `M` | → `S` | → `I` |
| `O` | hit | `BusUpgr` → `M` | **send data**, stay `O` | send data → `I` (`BusWr`: write back → `I`) |
| `M` | hit | hit | **send data** → `O` | send data → `I` (`BusWr`: write back → `I`) |

The owner sends the line cache to cache instead of writing it back and letting the requester read it from L2 or RAM. Memory stays stale until the `O` line is evicted.

### `none`

No requests at all. Each private L1 fills from L2/RAM on a miss and writes back on eviction, and never learns what the other cores did. This is the case the protocols exist to fix.

---

## 6. Special cases

- **Write-through.** Every store already goes to memory, so it goes on the bus as a `BusWr` and invalidates the other copies. Lines are never dirty, so the states reduce to `S` and `I`; there is no `E`, because a write would need the bus anyway.
- **No write-allocate.** A write-back store that misses is a `BusWr`: a dirty copy elsewhere is written back first, then the bytes go to the shared level (L2 if there is one, else RAM).
- **Atomics.** An AMO (`amoadd.w`, …) reads for ownership: one `BusRdX` on a miss, then the write hits the `M` line. `lr.w`/`sc.w` keep using RAVEN's reservation table, which every store from any core clears; that is independent of the protocol. With `coherence=none` an AMO works on the core's private copy, so atomics across cores break too — part of what that setting shows.
- **Instruction fetch and `fence.i`.** An I-cache miss snoops the other cores' D-caches, so code another core wrote is fetched current. A core's own stores still need `fence.i` to reach its own I-cache, as the RISC-V spec requires: `fence.i` writes the core's dirty D-cache lines to the shared level (`M` → `E`, or `S` under MSI; `O` → `S`) and drops its I-cache.
- **Memory a syscall or the debugger reads.** It is read as the running core would see it: its own copy first, then (under a protocol) the one dirty copy another core owns, then L2/RAM. With `none` a core does not see other cores' private dirty data.

---

## 7. What it costs

| Event | Cycles, charged to the core that made the request |
|---|---|
| any request put on the bus (and the snoop behind an I-cache miss) | `coherence.snoop_latency` (default 4) |
| a dirty copy written back for it | the L1's miss penalty + line transfer, as an eviction |
| a line sent cache to cache (MOESI) | `coherence.c2c_latency` (default 10), *instead of* the trip to L2/RAM |

These cycles go into the core's L1 D-cache (or I-cache) totals, so they show up in AMAT, CPI and the program total like any other cache cost.

---

## 8. Watching it in the TUI

1. Settings: `max cores` = 2, `run scope` = `ALL`, `coherence` = `MESI`.
2. Open `Program Examples/harts/coherence_flag_handoff.s`, assemble, and step or run.
3. **Cache → view** shows the L1 of the selected core (`[` / `]` switches core). The panel title says `Core N D-Cache`, and each line's state letter replaces the dirty bit.
4. **Cache → coherence** shows:
   - the protocol and its latencies;
   - **Bus traffic per core**: `BusRd`, `BusRdX`, `BusUpgr`, `BusWr`, invalidations received, flushes (write-backs another core caused), cache-to-cache transfers, coherence misses and bus cycles;
   - **Recent lines**: the state of each recently requested line in every core's D-cache;
   - **Bus requests**, newest first, for example
     `c1  BusRd  0x00008010  c1 I→S, c0 M→S  written back first`.

---

## 9. Counters in the CLI output

With private L1s, every results format adds one L1 pair and one set of bus counters per core:

- **JSON** — a `"coherence"` object: `protocol`, `snoop_latency`, `c2c_latency`, and `cores[]`, each with the counters and its own `icache`/`dcache` objects.
- **rstats** — `coherence.*` keys, then `coreN.<counter>` and `coreN.icache.*` / `coreN.dcache.*`.
- **CSV** — `Core N I-Cache L1` / `Core N D-Cache L1` rows in the cache table, and a `COHERENCE (<protocol>)` table.

The top-level `icache`/`dcache` entries are core 0's.

| Counter | Meaning |
|---|---|
| `bus_reads`, `bus_read_exclusive`, `bus_upgrades`, `bus_writes` | requests this core sent |
| `bus_transactions` | their sum |
| `invalidations_received` | this core's lines another core's request invalidated |
| `flushes` | dirty lines this core wrote back because another core asked |
| `cache_to_cache` | lines this core sent straight to another cache (MOESI) |
| `coherence_misses` | misses on a line another core's write had invalidated here |
| `bus_cycles` | cycles this core waited on the bus |

---

## 10. Experiments

### Stale data without a protocol

```bash
raven run "Program Examples/harts/coherence_flag_handoff.s" --cores 2 --coherence none --max-cycles 200000
raven run "Program Examples/harts/coherence_flag_handoff.s" --cores 2 --coherence mesi
```

With `none` the run never ends: core 1 read `flag` into its L1 before core 0 set it and keeps spinning on that copy. With any protocol it exits with 42.

| `coherence_flag_handoff.s` | cycles | bus requests | write-backs | cache-to-cache | bus cycles |
|---|---|---|---|---|---|
| `shared` | 772 | — | — | — | — |
| `msi` | 1376 | 14 | 4 | 0 | 292 |
| `mesi` | 1376 | 14 | 4 | 0 | 292 |
| `moesi` | 1000 | 14 | 0 | 4 | 124 |

MOESI sends the dirty lines cache to cache instead of writing them back, which is where its cycles go.

### False sharing

`coherence_false_sharing.s` has two cores each increment their own counter 200 times. With the counters 4 bytes apart they share a 16-byte line; with 16 bytes apart they do not (change `STRIDE`).

| `mesi` | cycles | bus requests | invalidations | coherence misses |
|---|---|---|---|---|
| same line (`STRIDE` 4) | 50444 | 607 | 400 | 400 |
| separate lines (`STRIDE` 16) | 6558 | 8 | 0 | 0 |

Neither core reads the other's counter, yet the line bounces between the two caches on every write.

### MSI against MESI

With the counters on separate lines, MSI needs a `BusUpgr` for each core's first write (2 in total, 10 requests), MESI none (8 requests): the line was read while no one else held it, so it was `E` and the write was silent.

---

## 11. Limits of the model

- A snooping bus only; there is no directory.
- One request at a time and sequential consistency: no transient states, write buffers or memory reordering.
- I-caches are not invalidated by other cores' writes; RISC-V programs use `fence.i` for that.
- The page-table walker reads RAM directly (see [virtual-memory.md](virtual-memory.md)), as it already did with one L1.
- The JIT runs whole blocks per turn, so cores interleave more coarsely than with the interpreter. Use the interpreter (`--jit none`, the default) to study interleavings.
- Headless `--pipeline` runs a single core; multi-core pipelines run in the TUI.
