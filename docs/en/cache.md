# Cache simulation

Falcon includes a built-in **I-cache + D-cache** simulator. As your program runs, you can watch the cache fill up, observe hits and misses in real time, and experiment with different configurations to understand their trade-offs.

Open the **Cache** tab for three subtabs:

- **Stats** — hit rate, miss count, RAM traffic, and cycle cost for each cache.
- **View** — a live map of every slot in the cache: which addresses are stored, which lines are dirty, and what the replacement policy is doing.
- **Config** — change size, line size, associativity, write policy, and latency parameters.

---

## What is a cache?

A **cache** is a small, fast memory that sits between the CPU and RAM. Instead of going all the way to RAM on every access, the processor checks the cache first.

- **Hit** — the data is already in the cache → fast (a few cycles)
- **Miss** — the data is not there → the processor loads an entire **cache line** from RAM → slow (dozens of cycles)

Caches are effective because programs tend to reuse data:

- **Temporal locality** — if you read an address, you'll likely read it again soon.
- **Spatial locality** — if you read address X, you'll likely read X+4, X+8, … soon (they share the same cache line).

Run `cache_locality.fas` with the default config to see both effects live.

---

## How the cache is organized

### Sets, ways, and lines

The cache is divided into **sets**, each containing **N ways** (slots). When the CPU accesses an address:

1. The address selects a specific **set** (a small group of slots).
2. All N ways in that set are checked simultaneously for a match.
3. **Hit** — found. **Miss** — not found: one slot is freed (eviction) and the new line is loaded from RAM.

```
Address → [ tag | set index | offset ]
                      ↓
            Set 5 → Way 0 | Way 1 | Way 2 | Way 3
                    check all 4 slots at once
```

- **Offset** — which byte within the line (depends on Line Size).
- **Set index** — which group of slots to look in (depends on number of sets).
- **Tag** — identifies which memory region the line came from.

You can see the tag (`T:XXXX`) and raw data bytes for every slot live in the **View** subtab.

### Line size

One miss loads an entire **line** — not just the requested byte. That's the power of spatial locality: if your code reads an array sequentially, the first element causes a miss but the next several are free hits from the same line.

- **Larger line** → better for sequential access; wastes bandwidth for random access.

### Number of sets

More sets → fewer addresses compete for the same slot → fewer **conflict misses**. Changing the total Size (while keeping Line Size and Associativity constant) changes the number of sets.

---

## Types of associativity

**Associativity** is the number of ways per set — how many lines can coexist at the same set.

### Direct-mapped (1-way)

Every address has **exactly one slot**. If two addresses map to the same set, they constantly kick each other out.

```
addr 0x0000 → set 0, way 0  (the only option)
addr 0x0400 → set 0, way 0  (same slot! → conflict)
```

Simple hardware, but vulnerable to **conflict misses**. Try `cache_conflict.fas` + `cache_direct_mapped_1kb.fcache`.

### Set-associative (N-way, N > 1)

Each set has N slots. A new line fills any free slot; only when the set is full does the replacement policy pick a victim.

```
addr 0x0000 → set 0, way 0  ✓
addr 0x0400 → set 0, way 1  ✓  (no conflict!)
addr 0x0800 → set 0, ?      → set full → eviction
```

More ways → fewer conflict misses, slightly more work per lookup. Try `cache_large_4kb_4way.fcache`.

### Fully associative

When Associativity equals the total number of lines, there is just one set and any line can go anywhere. No conflict misses, but too expensive for large caches — mainly used in small special-purpose caches.

---

## Types of misses

| Type | When it happens | How to investigate |
|------|-----------------|--------------------|
| **Cold miss** | First access to any address — the line was never loaded | Always happens at program start; unavoidable |
| **Capacity miss** | Working set is larger than the cache; lines are evicted before reuse | Increase **Size** — if miss rate drops, capacity was the bottleneck |
| **Conflict miss** | Two addresses sharing a set keep evicting each other | Increase **Associativity** (same Size) — if miss rate drops, conflicts were the issue |

---

## Replacement policies

When a set is full and a miss occurs, the hardware must pick a victim to evict. Available policies:

| Policy | Evicts... | Notes |
|--------|-----------|-------|
| **LRU** | Least recently used | Default in most CPUs; generally the best choice |
| **FIFO** | Oldest loaded line | Simpler than LRU; similar performance |
| **LFU** | Least frequently accessed | Good when access frequency is stable |
| **Clock** | A line not recently used (approximates LRU) | Used in OS page replacement |
| **MRU** | Most recently used | Surprisingly good for large sequential scans |
| **Random** | A random line | Simple hardware; decent average performance |

### Reading the View subtab

Each slot shows a small indicator that reveals what the replacement policy is "thinking." Colors do the main work: **cyan** = this slot is safe, **red** = this slot is next to be evicted.

| Policy | Indicator | What it means |
|--------|-----------|---------------|
| **LRU** | `r:N` | Recency rank — 0 = just used **(cyan, safe)**, highest = oldest **(red, evict next)** |
| **FIFO** | `r:N` | Arrival order — 0 = newest **(cyan, safe)**, highest = oldest **(red, evict next)** |
| **MRU** | `r:N` | Recency rank, meaning inverted — 0 = just used **(red, evict next!)**, highest = oldest **(cyan, safe)** |
| **LFU** | `f:N` | Access count — the slot with the **lowest count is red** (evict next) |
| **Clock** | `>` / `R` | `>` = clock pointer is here; `R` = recently used (protected); `>` without `R` = evict next |
| **Random** | `??` | No ordering — victim is chosen randomly |

---

## Write policies (D-cache only)

These settings control what happens when the CPU executes a **store** instruction.

### Write-Back (default)

The store updates **only the cache line**; the line is flagged **dirty** (shown as yellow `D` in the View tab). RAM is updated only when that dirty line is eventually evicted.

- Much less RAM traffic when the same variable is written many times.
- Watch the `D` flags in View and the `WB` counter in Stats.

### Write-Through

Every store immediately updates **both** the cache and RAM. No dirty lines exist.

- Simpler to reason about, but `RAM W` grows on every store.
- Use `cache_write_policy.fas` to compare `RAM W` between both policies.

### Write-Allocate vs No-Write-Allocate

What happens when a **store misses** (the target line is not in cache)?

- **Write-Allocate** — load the line into cache first, then write. Best when the same address will be read or written again.
- **No-Write-Allocate** — write directly to RAM, skip the cache. Good for write-only streams that won't be read back.

Common combinations: **Write-Back + Write-Allocate** (modern default) or **Write-Through + No-Write-Allocate**.

---

## Stats subtab

### Per-cache metrics

| Display | Meaning |
|---------|---------|
| Hit% gauge | Fraction of accesses that found data already in cache |
| `H` / `M` | Hit count / Miss count |
| `MR` | Miss rate (%) |
| `MPKI` | Misses per 1000 instructions — lower is better |
| `Acc` | Total accesses |
| `Evict` | Lines removed to make room for new ones |
| `WB` | Write-backs: dirty lines flushed to RAM on eviction (D-cache only) |
| `Fills` | Lines loaded from RAM (one per miss) |
| `RAM R` | Total bytes read from RAM (line fills) |
| `RAM W` | Total bytes written to RAM (write-through stores + dirty evictions) |
| `CPU Stores` | Bytes written by store instructions (D-cache only) |
| `Cycles` | Total clock cycles spent on cache operations |
| `Avg` | Average cycles per access |
| `CPI` | This cache's contribution to cycles-per-instruction |
| `Cost model` | Hit and miss cycle cost with the current config |

### Program summary bar

Shows totals across **both** caches: total cycles, overall CPI, instruction count, and the individual I-cache and D-cache contributions.

### Top Miss PCs

Lists which instruction addresses caused the most I-cache misses. Use ↑↓ to scroll.

### Controls

- `r` — **Reset** all counters and history.
- `p` — **Pause / Resume** the simulation.
- `i` / `d` / `b` — Switch view to I-cache only, D-cache only, or **B**oth.

---

## Config subtab

### How to edit

- **Click a number** to start editing; type digits, `Backspace` to correct, `Enter` to confirm, `Esc` to cancel.
- **◄ ► arrows** (or Left/Right keys) cycle through options for policy fields.
- `Tab` / `↑` / `↓` move between fields while editing.
- **Yellow** values indicate pending changes not yet applied to the active cache.

### Fields

| Field | What it controls |
|-------|-----------------|
| Size | Total capacity in bytes — larger cache → fewer capacity misses |
| Line Size | Bytes per cache line — larger → better spatial locality for sequential code; wastes bandwidth for random access. Must be a power of 2 and ≥ 4 |
| Associativity | Ways per set — 1 = direct-mapped; larger → fewer conflict misses |
| Replacement | Which line to evict when a set is full |
| Write Policy | Write-Back or Write-Through (D-cache only) |
| Write Alloc | What to do on a store miss: allocate in cache or write around it (D-cache only) |
| Hit Latency | Cycles for a cache hit — increase to model slower caches |
| Miss Penalty | Extra cycles waiting for RAM on a miss — typical range: 50–200 |
| Assoc Penalty | Extra cycles per additional way (cost of checking more tags) — default: 1 |
| Transfer Width | Data bus width in bytes — wider bus = fewer cycles to transfer a full line — default: 8 B |

### Presets

**Small / Medium / Large** load pre-built configurations — good starting points for comparison experiments.

### Apply

- **Apply + Reset Stats** — activates the config and clears all counters. Use this for clean before/after comparisons.
- **Apply Keep History** — activates the config but keeps the hit-rate chart for overlays.

### Validation

Line Size must be a power of 2, and the total Size must divide evenly into a whole number of sets. If a pending config is invalid, Apply shows an explanation of what needs to change.

---

## Saving and loading configs (.fcache)

Use **Ctrl+e** (export) and **Ctrl+l** (import) in the Cache tab to save and restore configurations as text files. The format is plain `key=value` — you can open and edit it in any text editor.

---

## Example programs

All examples are in `Program Examples/`:

- `cache_locality.fas` — sequential vs stride access; watch hit rate change as access patterns vary.
- `cache_conflict.fas` — two addresses sharing a set that evict each other even when free capacity exists elsewhere.
- `cache_write_policy.fas` — store-heavy loop; compare Write-Back vs Write-Through by watching `RAM W`.

Matching `.fcache` configs:

- `cache_direct_mapped_1kb.fcache`
- `cache_large_4kb_4way.fcache`
- `cache_write_through.fcache`
- `cache_no_write_allocate.fcache`
