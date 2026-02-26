# Cache simulation

Falcon includes a simple **I-cache + D-cache** simulator with live stats and an interactive configuration UI.

Open the **Cache** tab to access two subtabs:

- **Stats** — inspect hit rate, miss patterns, RAM traffic, and cycle cost.
- **Config** — tweak cache size/line size/associativity and policies.

## Cache → Stats

### Metrics (per cache)

- **Hit%** gauge — `hits / (hits + misses)`.
- **H / M / MR / MPKI**
  - `H`: hits (count)
  - `M`: misses (count)
  - `MR`: miss rate (%)
  - `MPKI`: misses per 1000 instructions (`misses / instructions * 1000`)
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
  - `CPI`: cycles per instruction (`cycles / instructions`) — “CPI contribution” of this cache

### Top Miss PCs (I-Cache)

The table shows which **fetch PCs** caused I-cache misses (sorted by miss count). Use Up/Down (or the mouse wheel) to scroll.

### Controls

- **Reset** (`r`) — clears cache stats (including `miss_pcs` and `ram_write_bytes`).
- **Pause/Resume** (`p`) — pauses/resumes the simulation (cache stats stop updating while paused).
- **View scope** — show I-cache, D-cache, or both.

### What counts as “RAM W”?

`RAM W` counts **bytes written to RAM**, including:

- Write-through stores
- Write-back dirty line writebacks on eviction
- Write-back + no-write-allocate store misses that write directly to RAM

Dirty bytes still sitting in a write-back cache line are **not** counted until they are written back.

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
- `size` is a multiple of `(line_size * associativity)`
- `sets = size / (line_size * associativity)` is a power of two

Note: write policies are only meaningful for the **D-cache** (the I-cache is read-only).

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

- `size` (bytes)
  - Total cache capacity in bytes.
  - Bigger `size` usually reduces **capacity misses** (the cache can hold more lines).
- `line_size` (bytes)
  - Bytes per cache line (aka block size).
  - This simulator fetches the **whole line** on a miss, so a larger `line_size` can improve **spatial locality** (sequential access), but can waste bandwidth on random access.
- `associativity` (ways)
  - Number of lines per set (1 = direct-mapped; higher = set-associative; `sets=1` = fully-associative).
  - Higher associativity usually reduces **conflict misses**, at the cost of more metadata/complexity.
- `replacement` (enum)
  - Which line to evict when a set is full and a miss needs a new line.
  - Accepted values: `Lru`, `Fifo`, `Random`, `Lfu`, `Clock`, `Mru`
    - `Lru`: evict least-recently used (good general default)
    - `Fifo`: evict oldest inserted line
    - `Random`: pseudo-random victim
    - `Lfu`: evict least-frequently used
    - `Clock`: second-chance/clock algorithm (approximate LRU)
    - `Mru`: evict most-recently used (can be good for scans)
- `write_policy` (D-cache only; enum)
  - What happens on stores.
  - Accepted values: `WriteBack`, `WriteThrough`
    - `WriteThrough`: every store updates RAM immediately (so `RAM W` tends to be high).
    - `WriteBack`: stores update the cache line and mark it **dirty**; RAM is updated later on eviction (so `RAM W` can be much lower until writebacks happen).
- `write_alloc` (D-cache only; enum)
  - What happens on a **store miss**.
  - Accepted values: `WriteAllocate`, `NoWriteAllocate`
    - `WriteAllocate` (write-allocate): on a store miss, the cache allocates/fills the line, then performs the write into the cache.
    - `NoWriteAllocate` (write-around): on a store miss, do not fill the cache line (store goes straight to RAM).
- `hit_latency` (cycles)
  - Cycle cost added to cache stats on a **hit**.
  - This feeds the `Cycles`, `Avg`, and `CPI` cache metrics.
- `miss_penalty` (cycles)
  - Extra cycle cost added on a **miss** (stall waiting for RAM).
  - A common teaching setup is something like `hit_latency=1` and `miss_penalty=50` (hit=1 cyc, miss≈51 cyc).

### Mapping logic (tag / index / offset)

This is the core “why do conflicts happen?” part.

Given:

- `sets = size / (line_size * associativity)`
- `offset_bits = log2(line_size)`
- `index_bits = log2(sets)`

An address is split like this:

```
[   tag   |   index   |  offset  ]
```

- `offset` selects a byte *inside* a line
- `index` selects the set
- `tag` identifies which line is currently stored in that set/way

Two different addresses **conflict** if they have the same `index` but different `tag` (they compete for the same set).

Handy trick:

- `stride_same_set = sets * line_size` bytes

Addresses `A` and `A + stride_same_set` land in the **same set** (different tag). This is exactly what `cache_conflict.fas` demonstrates with the default config.

### Worked example (default 1 KB, 16 B, 2-way)

Default D-cache config (the built-in preset is close to this):

```
dcache.size=1024
dcache.line_size=16
dcache.associativity=2
```

Compute:

- `bytes_per_set = line_size * associativity = 16 * 2 = 32 B`
- `sets = size / bytes_per_set = 1024 / 32 = 32` (power of 2: OK)
- `offset_bits = log2(16) = 4`
- `index_bits = log2(32) = 5`
- `stride_same_set = sets * line_size = 32 * 16 = 512 B`

So addresses 512 bytes apart compete for the same set (great to force conflicts in class).

### Validation rules (explained)

The UI enforces a few rules so the mapping above stays simple and “hardware-like”:

- `line_size` must be a power of two and `>= 4`
  - so `offset_bits = log2(line_size)` is an integer and lines are naturally aligned
- `size` must be a multiple of `(line_size * associativity)`
  - so `sets` is an integer (no fractional sets)
- `sets` must be a power of two
  - so indexing can be done with a bitmask (`index = (addr >> offset_bits) & (sets - 1)`)

If a pending config violates any rule, **Apply** shows an error and won’t recreate the caches.

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
