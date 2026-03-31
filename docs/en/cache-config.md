# Raven — Cache Configuration File (`.fcache`)

This guide explains every field in a Raven cache configuration file so you (or an LLM) can write valid configs from scratch. Import any `.fcache` into Raven via **Cache → ↓ Import cfg**.

---

## File format

Plain text, one `key=value` pair per line.

```
# Lines starting with # are comments — ignored by Raven
key=value
```

- Whitespace around `=` is trimmed.
- Unknown keys are silently ignored (forward-compatible).
- Enum values are **case-sensitive PascalCase** (e.g. `Lru`, not `lru`).
- File extension must be `.fcache`.

---

## Header

Every file must declare how many extra levels (beyond L1) exist.

```
# Raven Cache Config v2
levels=N
```

| Key | Type | Meaning |
|-----|------|---------|
| `levels` | integer ≥ 0 | Number of unified cache levels added beyond L1 (`0` = L1 only, `1` = L1+L2, `2` = L1+L2+L3, …) |

> If `levels` is absent the parser assumes `0` (L1 only — v1 compatibility).

---

## Level prefixes

Each level is identified by a prefix that is prepended to every field name with a dot.

| Prefix | Level | Type |
|--------|-------|------|
| `icache` | L1 Instruction Cache | Split (always present) |
| `dcache` | L1 Data Cache | Split (always present) |
| `l2` | L2 Unified Cache | Extra (requires `levels≥1`) |
| `l3` | L3 Unified Cache | Extra (requires `levels≥2`) |
| `l4` | L4 Unified Cache | Extra (requires `levels≥3`) |
| `lN` | LN Unified Cache | Extra (requires `levels≥N-1`) |

So `icache.size=4096` sets L1-I size to 4 KB, and `l2.line_size=64` sets the L2 line size to 64 bytes.

---

## Fields

Every level (icache, dcache, l2, l3, …) accepts the same set of fields.

### Geometry

| Key | Type | Valid range | Notes |
|-----|------|-------------|-------|
| `size` | integer (bytes) | 64 – 1 048 576, **power of 2** | Total cache capacity. Must equal `line_size × associativity × num_sets` where `num_sets` is also a power of 2. |
| `line_size` | integer (bytes) | 4 – 512, **power of 2** | Size of one cache block / line. Larger lines reduce miss rate on sequential access but increase miss penalty traffic. |
| `associativity` | integer | 1 – 16 | Number of ways per set. `1` = direct-mapped, `N` = N-way set-associative. Must satisfy `associativity × line_size ≤ size`. |

**Derived (read-only, not in the file):**
`num_sets = size / (line_size × associativity)` — must be a power of 2 or Raven will reject the config.

**Quick sanity check formula:**
```
sets = size / (line_size * associativity)   → must be a power of 2
```

Example: `size=4096, line_size=32, associativity=4` → `sets = 4096/128 = 32` ✓

---

### Timing

| Key | Type | Valid range | Default | Notes |
|-----|------|-------------|---------|-------|
| `hit_latency` | integer (cycles) | 1 – 999 | — | Cycles consumed on every cache hit. |
| `miss_penalty` | integer (cycles) | 0 – 9999 | — | **Extra** stall cycles added on a cache miss (on top of `hit_latency`). Models the time to fetch from the next level or RAM. |
| `assoc_penalty` | integer (cycles) | 0 – 99 | `1` | Extra cycles per additional way during tag search. `(associativity - 1) × assoc_penalty` is added to `hit_latency`. Set to `0` to model fully-parallel tag lookup. |
| `transfer_width` | integer (bytes) | 1 – 512 | `8` | Bus width between this level and the one below. Transfer cost = `ceil(line_size / transfer_width)` cycles, added to the miss penalty automatically. |

> **AMAT formula used by Raven:**
> `AMAT = hit_latency + assoc_penalty*(associativity-1) + miss_rate * (miss_penalty + ceil(line_size/transfer_width))`

---

### Replacement policy

Key: `replacement`
Type: enum (one exact string from the table below)

| Value | Eviction rule | Best for |
|-------|--------------|----------|
| `Lru` | Least Recently Used — evicts the way not accessed for the longest time. | General-purpose workloads |
| `Mru` | Most Recently Used — evicts the most recently accessed line. | Scan/streaming patterns that should not pollute cache |
| `Fifo` | First In First Out — evicts the line that was installed earliest. | Predictable, hardware-simple |
| `Lfu` | Least Frequently Used — evicts the way with the fewest accesses (ties broken by LRU). | Frequency-skewed access patterns |
| `Clock` | Clock / Second-Chance — circular pointer with a reference bit per line. | Approximation of LRU with lower hardware cost |
| `Random` | Pseudo-random via LCG. | Worst-case analysis; avoids pathological LRU thrashing |

---

### Write policy

Key: `write_policy`
Type: enum

| Value | Meaning |
|-------|---------|
| `WriteBack` | Writes stay in the cache and are propagated to the next level only on eviction. Reduces write traffic; requires dirty bits. |
| `WriteThrough` | Every write is immediately forwarded to the next level. Simpler; no dirty bits; higher write traffic. |

---

### Write allocate policy

Key: `write_alloc`
Type: enum

| Value | Meaning |
|-------|---------|
| `WriteAllocate` | On a write miss, a new line is fetched into the cache before writing. Works naturally with `WriteBack`. |
| `NoWriteAllocate` | On a write miss, the write is sent directly to the next level without allocating a line. Common with `WriteThrough`. |

**Conventional pairings:**

| `write_policy` | `write_alloc` | Notes |
|----------------|---------------|-------|
| `WriteBack` | `WriteAllocate` | Standard for L1/L2 in modern CPUs |
| `WriteThrough` | `NoWriteAllocate` | Common for simple or small L1 caches |
| `WriteBack` | `NoWriteAllocate` | Unusual but valid |
| `WriteThrough` | `WriteAllocate` | Unusual; high traffic |

---

### Inclusion policy (L2 and above only)

Key: `inclusion`
Type: enum
Default: `NonInclusive`

| Value | Meaning |
|-------|---------|
| `NonInclusive` | No constraint — a line may or may not exist in both levels simultaneously. Default for most configs. |
| `Inclusive` | Every line in this level is **guaranteed** to also exist in the level below. Simplifies coherence; wastes capacity. |
| `Exclusive` | A line lives in **exactly one** level. When fetched into L1 it is evicted from L2 (victim cache model). |

> `inclusion` is only meaningful for L2 and higher. On `icache`/`dcache` it is parsed but has no effect.

---

## CPI config (optional)

Controls the per-instruction-class latency model. If omitted, Raven uses defaults.

- Sequential mode: these values contribute to the serial CPI/total-cycle model.
- Pipeline mode: these values become stage latency and stall behavior inside the pipeline wall-clock.

```
# --- CPI Config ---
cpi.alu=1
cpi.mul=3
cpi.div=20
cpi.load=0
cpi.store=0
cpi.branch_taken=3
cpi.branch_not_taken=1
cpi.jump=2
cpi.system=10
cpi.fp=5
```

| Key | Default | Meaning |
|-----|---------|---------|
| `cpi.alu` | `1` | Integer ALU instructions (add, sub, and, or, …) |
| `cpi.mul` | `3` | Integer multiply (mul, mulh, …) |
| `cpi.div` | `20` | Integer divide (div, rem, …) |
| `cpi.load` | `0` | Extra overhead per load beyond cache AMAT |
| `cpi.store` | `0` | Extra overhead per store beyond cache cost |
| `cpi.branch_taken` | `3` | Pipeline flush cost when branch is taken |
| `cpi.branch_not_taken` | `1` | Cost when branch falls through |
| `cpi.jump` | `2` | jal / jalr cost |
| `cpi.system` | `10` | ecall / ebreak |
| `cpi.fp` | `5` | Floating-point instructions (if emulated) |

All values are unsigned integers ≥ 0.

---

## Validation rules

Raven will reject the config and show an error if any of these fail:

1. `line_size` must be a power of 2 and ≥ 4.
2. `size` must be a power of 2.
3. `associativity ≥ 1`.
4. `associativity × line_size ≤ size` (at least one set must exist).
5. `num_sets = size / (line_size × associativity)` must be a power of 2.

Powers of 2 to remember: 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576.

---

## Complete examples

### L1 only (minimal)

```
# Raven Cache Config v2
levels=0

icache.size=1024
icache.line_size=16
icache.associativity=2
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=50
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=1024
dcache.line_size=16
dcache.associativity=2
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=50
dcache.assoc_penalty=1
dcache.transfer_width=8
```

---

### L1 + L2

```
# Raven Cache Config v2
levels=1

icache.size=4096
icache.line_size=32
icache.associativity=4
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=10
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=4096
dcache.line_size=32
dcache.associativity=4
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=10
dcache.assoc_penalty=1
dcache.transfer_width=8

l2.size=131072
l2.line_size=64
l2.associativity=8
l2.replacement=Lru
l2.write_policy=WriteBack
l2.write_alloc=WriteAllocate
l2.inclusion=NonInclusive
l2.hit_latency=10
l2.miss_penalty=200
l2.assoc_penalty=2
l2.transfer_width=16
```

---

### L1 + L2 + L3

```
# Raven Cache Config v2
levels=2

icache.size=4096
icache.line_size=32
icache.associativity=4
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=10
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=4096
dcache.line_size=32
dcache.associativity=4
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=10
dcache.assoc_penalty=1
dcache.transfer_width=8

l2.size=131072
l2.line_size=64
l2.associativity=8
l2.replacement=Lru
l2.write_policy=WriteBack
l2.write_alloc=WriteAllocate
l2.inclusion=NonInclusive
l2.hit_latency=10
l2.miss_penalty=30
l2.assoc_penalty=2
l2.transfer_width=16

l3.size=4194304
l3.line_size=64
l3.associativity=16
l3.replacement=Lru
l3.write_policy=WriteBack
l3.write_alloc=WriteAllocate
l3.inclusion=Inclusive
l3.hit_latency=30
l3.miss_penalty=300
l3.assoc_penalty=3
l3.transfer_width=32
```

---

## Quick-reference card

```
Field            Type        Valid values
───────────────  ──────────  ──────────────────────────────────────────
size             integer     power of 2, 64–1048576
line_size        integer     power of 2, 4–512
associativity    integer     1–16
replacement      enum        Lru | Mru | Fifo | Lfu | Clock | Random
write_policy     enum        WriteBack | WriteThrough
write_alloc      enum        WriteAllocate | NoWriteAllocate
inclusion        enum        NonInclusive | Inclusive | Exclusive
hit_latency      integer     1–999
miss_penalty     integer     0–9999
assoc_penalty    integer     0–99  (default 1)
transfer_width   integer     1–512 (default 8)
```
