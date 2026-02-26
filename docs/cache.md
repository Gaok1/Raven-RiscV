# Cache simulation

Falcon includes a simple **I-cache + D-cache** simulator with live stats and an interactive configuration UI.

Open the **Cache** tab to access three subtabs:

- **Stats** — inspect hit rate, miss patterns, RAM traffic, and cycle cost.
- **View** — inspect the live contents of every cache line (sets × ways matrix). Use ↑↓ to scroll vertically and ←→ (or horizontal touchpad scroll) to scroll horizontally when there are many ways.
- **Config** — tweak cache size/line size/associativity and policies.

---

## What is a cache? (core concept)

A **cache** is a small, fast memory that holds copies of recently-accessed RAM data. When the CPU accesses an address:

- **Hit** — data is in the cache → fast access (cost = `hit_latency` cycles)
- **Miss** — data is not in the cache → the entire line is fetched from RAM → slow (cost = `hit_latency + miss_penalty` cycles)

Cache effectiveness depends on the **principle of locality**:
- **Temporal**: if you accessed address X recently, you'll probably access X again soon.
- **Spatial**: if you accessed X, you'll probably access X+4, X+8, … soon.

---

## Address mapping (tag / index / offset)

This is the fundamental mechanism of any hardware cache.

### Address decomposition

Every 32-bit address is split into three fields:

```
 31                 ...        offset+index   offset    0
┌──────────────────────────────┬─────────────┬──────────┐
│             TAG              │    INDEX    │  OFFSET  │
└──────────────────────────────┴─────────────┴──────────┘
```

Given the cache parameters:

```
sets        = size / (line_size × associativity)
offset_bits = log₂(line_size)
index_bits  = log₂(sets)
tag_bits    = 32 - offset_bits - index_bits
```

- **OFFSET** (bits `[offset_bits-1 : 0]`) — selects the **byte within the line**. If `line_size=8` → offset_bits=3 → bits [2:0].
- **INDEX** (bits `[offset_bits+index_bits-1 : offset_bits]`) — selects **which set** in the cache. If `sets=4` → index_bits=2 → bits [4:3].
- **TAG** (bits `[31 : offset_bits+index_bits]`) — identifies **which memory block** is currently stored in that set/way.

### Numeric example

Config: `size=32, line_size=8, associativity=1`

```
sets        = 32 / (8 × 1) = 4
offset_bits = log₂(8) = 3  →  bits [2:0]
index_bits  = log₂(4) = 2  →  bits [4:3]
tag_bits    = 32 - 3 - 2   = 27  →  bits [31:5]
```

Address `0x1000` = `0001 0000 0000 0000` in binary:

```
TAG    = 0x1000 >> 5 = 0x80   (bits 31..5)
INDEX  = (0x1000 >> 3) & 0x3  = 0  → Set 0
OFFSET = 0x1000 & 0x7         = 0  → byte 0 within the line
```

The **loaded cache line** covers addresses `0x1000–0x1007` (all bytes sharing the same TAG+INDEX).

### When do two addresses conflict?

Two addresses conflict when they have the same INDEX but different TAGs — they want the **same set** but carry data from different memory regions.

```
stride_same_set = sets × line_size
```

Addresses `A` and `A + stride_same_set` always map to the same set. With `size=32, line_size=8, assoc=1`: stride = 4 × 8 = **32 bytes**. So `0x0000` and `0x0020` are rivals in the same set.

---

## Types of associativity

**Associativity** defines how many lines (ways) exist per set, which determines how the hardware handles conflicts.

### 1. Direct-mapped (`associativity = 1`)

Each set has **exactly 1 way**. Every address maps to exactly one slot in the cache. Two addresses with the same INDEX always evict each other.

```
Address A  → [ Set 2 | Way 0 ]  ← only option
Address B  → [ Set 2 | Way 0 ]  ← same option! → guaranteed miss if A and B are accessed alternately
```

**Pros:** simple, cheap hardware; single-cycle lookup (only one tag to compare).
**Cons:** suffers from **conflict misses** — two rival addresses evict each other in a loop, even if there is free capacity elsewhere.

Try it with `cache_conflict.fas` and `cache_direct_mapped_1kb.fcache`.

### 2. Set-associative (`associativity = N`, N > 1)

Each set has **N ways**. The hardware checks all N tags in parallel. A new address can go into any free way; the replacement policy picks the victim when the set is full.

```
Address A  → [ Set 2 | Way 0 ✓ ]  ← A placed in way 0
Address B  → [ Set 2 | Way 1 ✓ ]  ← B placed in way 1 → no conflict!
Address C  → [ Set 2 | ?       ]  ← set full → eviction by policy (LRU, FIFO…)
```

**Pros:** eliminates most conflict misses.
**Cons:** more complex hardware (compare N tags, need replacement policy).

Typical values: 2-way, 4-way, 8-way. Try `cache_large_4kb_4way.fcache`.

### 3. Fully associative (`associativity = sets` → `sets = 1`)

There is **one single set** with all ways. Any line can go anywhere. No INDEX field — the hardware compares the address against **all** tags in parallel.

```
sets = size / (line_size × associativity) = 1   →   index_bits = 0
```

**Pros:** no conflict misses at all.
**Cons:** very large comparison circuit for large caches; mainly used in TLBs and small special caches.

To simulate in Falcon: set `associativity` equal to the total number of lines (`size / line_size`), so `sets=1`.

---

## Types of misses

| Type | Also called | When it occurs |
|------|-------------|----------------|
| **Cold miss** | Compulsory miss | First time an address is accessed (the line was never loaded) |
| **Capacity miss** | — | The cache is too small for the working set; lines are evicted before they can be reused |
| **Conflict miss** | Interference miss | Two rival addresses (same INDEX) repeatedly evict each other, even though other sets have free ways |

**How to identify in the simulator:**
- Cold misses: run once; every first access to each line will be a miss.
- Capacity misses: increase `size` — if miss rate drops significantly, it was capacity-limited.
- Conflict misses: increase `associativity` (keep `size` the same) — if miss rate drops, conflicts were the issue.

---

## Replacement policies (`replacement`)

When a set is full and a miss occurs, the hardware must **choose a victim** (a line to evict). Available policies:

| Policy | Strategy | Typical use |
|--------|----------|-------------|
| `Lru` | Evict **least recently used** | Default in modern CPUs |
| `Fifo` | Evict **oldest installed** (First In, First Out) | Simpler than LRU |
| `Random` | **Pseudo-random** victim | Easy to implement in hardware |
| `Lfu` | Evict **least frequently used** | Good when access frequency is stable |
| `Clock` | Approximates LRU with a per-line ref bit; sweeps circularly | Used in OSes for TLB/page eviction |
| `Mru` | Evict **most recently used** | Useful for large sequential scans |

**LRU** is generally most efficient for typical access patterns. **MRU** can be surprisingly good for sequential scans: the most recent line is the one least likely to be reused.

In the Falcon **View** subtab, each line shows policy metadata:
- `r:0` = most recent position (LRU/FIFO) or to-be-evicted (MRU)
- `f:N` = access frequency (LFU)
- `>R` = clock pointer at this way + ref bit set (Clock)

---

## Write policies (`write_policy` and `write_alloc`)

These settings only affect the **D-cache** (the I-cache is read-only).

### Write-Back vs Write-Through

What happens when the CPU executes a **store**?

#### Write-Through

The write goes **simultaneously** to the cache AND to RAM.

```
CPU: sw t0, 0(t1)
  → cache line updated
  → RAM[address] updated immediately
```

`RAM W` increases on every store. Simple to implement but generates heavy RAM traffic.

#### Write-Back

The write goes **only to the cache**; the line is marked **dirty (D)**. RAM is updated only when the dirty line is eventually evicted.

```
CPU: sw t0, 0(t1)
  → cache line updated
  → line marked dirty (D shown in yellow in the View tab)
  → RAM untouched for now

  ... later, on eviction:
  → dirty line written back to RAM (writeback)
  → RAM W increases only now
```

**Pros:** much less RAM traffic when there is temporal locality.
**Cons:** more complex hardware (must track dirty bits).

### Write-Allocate vs No-Write-Allocate

What happens when a store causes a **miss**?

#### Write-Allocate

On a store miss: allocate the line in the cache (fill from RAM), then write to the cached line.

```
CPU: sw t0, 0(t1)  ← miss at address X
  → load line containing X from RAM into cache  (line fill)
  → update the byte in cache
  → (write-back: mark dirty; write-through: also write RAM)
```

**Advantage:** if the CPU writes to the same address again → hit.
**Typical combination:** `WriteBack + WriteAllocate`.

#### No-Write-Allocate (write-around)

On a store miss: **do not** allocate a line; write directly to RAM.

```
CPU: sw t0, 0(t1)  ← miss at address X
  → RAM[X] updated directly
  → cache unchanged
```

**Advantage:** does not pollute the cache with data that may not be read back.
**Typical combination:** `WriteThrough + NoWriteAllocate`.

### Common combinations

| `write_policy`  | `write_alloc`     | Behavior |
|-----------------|-------------------|----------|
| WriteBack       | WriteAllocate     | Modern default; minimizes RAM W; ideal for read+write loops |
| WriteThrough    | NoWriteAllocate   | Simple; high RAM W; cache stays clean (no dirty lines) |
| WriteThrough    | WriteAllocate     | Uncommon; cache fill + RAM write on every store |
| WriteBack       | NoWriteAllocate   | Uncommon; useful for write-only streams with no readback |

Try `cache_write_policy.fas` and the corresponding `.fcache` files to see the impact on `RAM W` and writeback counters.

---

## Cache → Stats

### Metrics (per cache)

- **Hit%** gauge — `hits / (hits + misses)`.
- **H / M / MR / MPKI**
  - `H`: hits (count)
  - `M`: misses (count)
  - `MR`: miss rate (%)
  - `MPKI`: misses per 1000 instructions (`misses / instructions × 1000`)
- **Acc / Evict / WB / Fills**
  - `Acc`: total accesses (`hits + misses`)
  - `Evict`: evictions (count)
  - `WB`: writebacks (D-cache only)
  - `Fills`: line fills (derived from `bytes_loaded / line_size`)
- **RAM R / RAM W**
  - `RAM R`: bytes loaded from RAM due to line fills (`bytes_loaded`)
  - `RAM W`: bytes actually written to RAM (`ram_write_bytes`)
- **CPU Stores** (D-cache only) — bytes written by the CPU via stores (`bytes_stored`).
- **Cycles / Avg / CPI**
  - `Cycles`: accumulated cycle cost for cache accesses
  - `Avg`: average cycles per access (`cycles / accesses`)
  - `CPI`: cycles per instruction (`cycles / instructions`) — "CPI contribution" of this cache

### Top Miss PCs (I-Cache)

The table shows which **fetch PCs** caused I-cache misses (sorted by miss count). Use Up/Down (or the mouse wheel) to scroll.

### Controls

- **Reset** (`r`) — clears cache stats (including `miss_pcs` and `ram_write_bytes`).
- **Pause/Resume** (`p`) — pauses/resumes the simulation (cache stats stop updating while paused).
- **View scope** (`i`/`d`/`b`) — show I-cache, D-cache, or both.

### What counts as "RAM W"?

`RAM W` counts **bytes written to RAM**, including:

- Write-through stores
- Write-back dirty line writebacks on eviction
- Write-back + no-write-allocate store misses that write directly to RAM

Dirty bytes still sitting in a write-back cache line are **not** counted until they are written back.

---

## Cache → Config

The Config subtab shows one panel for the **I-cache** and one for the **D-cache**.

### Editing

- Click a **numeric field**, type digits, `Backspace` to delete, `Enter` to confirm, `Esc` to cancel.
- For **enum fields**, click to cycle or use `◄/►` (Left/Right).
- Use `Tab` / `↑` / `↓` to move between fields while editing.
- Yellow values indicate **pending** changes (different from the active config).

### Presets

Use **Small / Medium / Large** to quickly load preset configurations.

### Apply

- **Apply + Reset Stats** — recreates caches with the pending config and resets cache statistics/history.
- **Apply Keep History** — recreates caches but keeps the **hit-rate history chart** (counters reset).

### Validation rules (must be satisfied)

- `line_size` is a power of two and `>= 4`
- `size` is a multiple of `(line_size × associativity)`
- `sets = size / (line_size × associativity)` is a power of two

Note: write policies are only meaningful for the **D-cache** (the I-cache is read-only).

### Validation rules (explained)

The UI enforces these rules so that the tag/index/offset mapping stays simple and hardware-like:

- `line_size` power of two → `offset_bits = log₂(line_size)` is an integer; lines are naturally aligned.
- `size` multiple of `(line_size × associativity)` → `sets` is an integer (no fractional sets).
- `sets` power of two → indexing can use a bitmask: `index = (addr >> offset_bits) & (sets - 1)`.

If a pending config violates any rule, **Apply** shows an error and will not recreate the caches.

---

## Configuration reference (.fcache)

Falcon can **import/export** cache configs as plain text files with `key=value` pairs.

- In the **Cache** tab: `Ctrl+L` imports a `.fcache`, `Ctrl+E` exports the current pending config.
- Lines starting with `#` are comments.
- Config keys are **case-sensitive** and must match the exact enum names below.

The file always contains two configs:

- `icache.*` — instruction cache (read-only; write policies are ignored)
- `dcache.*` — data cache

When importing, Falcon expects **all keys** for both caches; missing fields fail with a `Missing icache.<key>` / `Missing dcache.<key>` error.

### Keys (what each term means)

Numeric fields are base-10 integers (so `1024` is valid; `0x400` is not).

- `size` (bytes) — total cache capacity. Bigger `size` usually reduces **capacity misses**.
- `line_size` (bytes) — bytes per line (block size). A larger line helps **spatial locality** (sequential access) but may waste bandwidth on random access.
- `associativity` (ways) — ways per set. 1 = direct-mapped; `sets=1` = fully associative. More ways → fewer conflict misses.
- `replacement` (enum) — eviction policy. Values: `Lru`, `Fifo`, `Random`, `Lfu`, `Clock`, `Mru`.
- `write_policy` (D-cache only) — `WriteBack` or `WriteThrough`. See section above.
- `write_alloc` (D-cache only) — `WriteAllocate` or `NoWriteAllocate`. See section above.
- `hit_latency` (cycles) — cycle cost on a hit. Feeds `Cycles`, `Avg`, and `CPI`.
- `miss_penalty` (cycles) — extra cycle cost on a miss. A common teaching setup: `hit_latency=1`, `miss_penalty=50`.

### Worked example (1 KB, 16 B line, 2-way)

```
dcache.size=1024
dcache.line_size=16
dcache.associativity=2
```

Computing:

```
bytes_per_set    = line_size × associativity = 16 × 2 = 32 B
sets             = size / bytes_per_set = 1024 / 32 = 32
offset_bits      = log₂(16) = 4
index_bits       = log₂(32) = 5
tag_bits         = 32 - 4 - 5 = 23
stride_same_set  = sets × line_size = 32 × 16 = 512 B
```

Addresses 512 bytes apart compete for the same set — great for forcing conflict misses in class.

---

## Example files

Ready-to-run cache exploration programs live under `Program Examples/`:

- `cache_locality.fas` — small vs large array scans (sequential vs stride), with pauses so you can Reset stats between phases.
- `cache_conflict.fas` — 2-line vs 3-line same-set access pattern to demonstrate conflict misses on set-associative caches.
- `cache_write_policy.fas` — store-heavy loop to compare `WriteBack` vs `WriteThrough` and `NoWriteAllocate`.

Ready-to-import cache configs (`.fcache`):

- `cache_direct_mapped_1kb.fcache`
- `cache_large_4kb_4way.fcache`
- `cache_write_through.fcache`
- `cache_no_write_allocate.fcache`
