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
