# Virtual memory (Sv32)

> [Leia em Português](../pt-BR/virtual-memory.md)

Raven implements the RISC-V **Sv32** virtual memory scheme: a 2-level page table walked in software by the CPU's MMU, fronted by a configurable TLB. Translation is **off by default** so programs run unchanged; turn it on from **Settings → Virtual Memory** (also persisted in `.rcfg`).

When enabled, the simulator enters **standard VM mode**: it automatically installs an identity page map (VA = PA for the full address space) and activates the TLB — so any program, even a simple loop, immediately shows TLB hits and misses with **no setup code required**. The TLB has its own subtab inside the **Cache** tab where you can configure size, associativity, replacement policy, hit latency, and miss penalty — and watch hits/misses live.

If you want to study custom address layouts, page faults, or OS-style privilege transitions, you can still write your own page tables and `csrw satp` — your mapping will replace the auto-installed one automatically.

---

## When to enable it

| Use case | VM on? |
|----------|--------|
| Plain RV32IMAF program, single flat memory | off — same behavior as before |
| Observing TLB activity on any program (standard mode) | **on** |
| Studying page-table walks, A/D bits, page faults | **on** |
| Comparing TLB hit/miss penalties across replacement policies | **on** |
| Running an OS-style kernel (M-mode setup + U-mode user code) | **on** |

The toggle is in `Settings → Virtual Memory`. Default is **off**. With VM off, the MMU is bypassed entirely — no TLB lookups, no walker, no extra cycles.

### Standard mode (auto identity map)

Turning on VM is enough. On every assembly, Raven writes a 1024-entry root page table at the last 4 KiB of RAM where each entry is a **megapage** (4 MiB) with identity mapping and full R/W/X/U permissions. The `satp` CSR is set to Sv32 pointing at that table. From that point every fetch and load/store goes through the TLB.

You can still override any part: write a custom `csrw satp` to switch to your own page table; the auto-installed one is just the starting point.

---

## Address translation

Sv32 splits a 32-bit virtual address into two 10-bit page-table indices and a 12-bit page offset:

```
 31         22 21         12 11          0
┌─────────────┬─────────────┬─────────────┐
│   VPN[1]    │   VPN[0]    │   offset    │
└─────────────┴─────────────┴─────────────┘
```

Translation walks two levels of PTEs starting at `satp.PPN << 12`:

1. **L1 PTE** at `(satp.PPN << 12) + VPN[1] * 4` — if it's a leaf (R/W/X set), the page is a 4 MiB **megapage** and `VPN[0]` becomes part of the offset.
2. **L0 PTE** at `(L1.PPN << 12) + VPN[0] * 4` — must be a leaf. The final 4 KiB page's physical address is `(L0.PPN << 12) | offset`.

`A` (accessed) and `D` (dirty) bits are auto-set by the walker when an access succeeds — Raven does **not** trap to let the OS update them, so you can experiment without writing a fault handler.

---

## Privilege levels

| Mode | Notation | Behavior under `satp.MODE = Sv32` |
|------|----------|-----------------------------------|
| Machine | M | Physical by default; translated in standard mode (didactic override) |
| User    | U | Always goes through translation; `U=0` PTEs fault |

> **Standard mode note:** In real RISC-V hardware, M-mode always uses physical addresses and ignores `satp`. Raven's standard VM mode deliberately overrides this so that programs written in M-mode (the default) also produce TLB activity — which is the common case for didactic examples. When you write your own page tables and drop into U-mode via `mret`, the behavior matches hardware exactly.

A trap (page fault, ecall, ebreak) switches the CPU to M-mode and saves the previous mode in `mstatus.MPP`. `mret` restores the saved mode and resumes at `mepc`.

---

## Page-fault traps

When translation fails, the CPU raises one of:

| Cause | Meaning |
|-------|---------|
| `12` | Instruction page fault — fetch could not be translated |
| `13` | Load page fault — `lw`/`lh`/`lb` could not be translated |
| `15` | Store page fault — `sw`/`sh`/`sb` could not be translated |

The trap fills `mcause`, `mtval` (faulting virtual address), `mepc` (faulting PC), sets `mstatus.MPP` to the previous mode, switches to M-mode, and jumps to `mtvec & ~3`. With `mtvec = 0`, Raven prints the fault to the console and halts — handy when you forget to install a handler.

---

## CSRs and system instructions

Raven implements just enough Zicsr + privileged ops to drive Sv32:

| CSR    | Number | Use |
|--------|--------|-----|
| `satp` | `0x180` | Root page-table PPN + ASID + MODE (1 = Sv32, 0 = Bare) |
| `mstatus` | `0x300` | Saved privilege bits (`MPP`) on trap entry/exit |
| `mtvec` | `0x305` | Trap-vector base address |
| `mepc`  | `0x341` | PC saved on trap |
| `mcause`| `0x342` | Trap cause |
| `mtval` | `0x343` | Trap-specific value (faulting vaddr for page faults) |

Instructions: `csrrw / csrrs / csrrc` (and the `i` immediate variants), `mret`, `sfence.vma`. Writing `satp` flushes the TLB; `sfence.vma` also flushes (ignoring its `rs1`/`rs2` filters in this release).

---

## Configuring the TLB

The TLB UI lives at **Cache → TLB** with three subviews:

- **Stats** — hit rate, total hits/misses, page faults, and a rolling 300-cycle hit-rate history.
- **Config** — entries (power of two), associativity, replacement policy (LRU / FIFO / Random / Clock / LFU / MRU), hit latency, miss penalty. Apply to commit.
- **Entries** — per-entry table: VPN | PPN | RWXU | ASID | V | G | A | D | megapage.

Configuration persists in `.rcfg` (via Cache export/import) so you can ship a TLB layout next to your CPI and cache configs.

---

## Performance impact

Every fetch and load/store gets two pieces of latency from the MMU:

- **Hit:** `tlb.hit_latency` cycles (default `1`).
- **Miss:** `tlb.miss_penalty` cycles for the walk (default `20`), plus any extra cycles the RAM walker spends fetching PTEs.

In **pipeline mode** the MMU stall lands in `if_stall_cycles` or `mem_stall_cycles` on the corresponding pipeline slot — visible as red MEM/IF stretches in the Gantt view. In **interpreter mode** the stall is added to `extra_cycles` and shows up in `total_program_cycles` / CPI.

---

## PTE format

Each 32-bit PTE stores the **Physical Page Number** in the upper bits and flags in the lower bits:

```
 31                  10  9  8  7  6  5  4  3  2  1  0
┌───────────────────────┬────┬──┬──┬──┬──┬──┬──┬──┬──┐
│        PPN (22 b)     │RSW │ D│ A│ G│ U│ X│ W│ R│ V│
└───────────────────────┴────┴──┴──┴──┴──┴──┴──┴──┴──┘
```

**Encoding formula:**

```
PTE value = (ppn << 10) | flags
```

Where `ppn = physical_address >> 12` (physical page number of the target page).

| PTE type | R W X | Meaning |
|----------|-------|---------|
| Pointer (non-leaf) | 0 0 0 | Points to the next-level table |
| Leaf | at least one set | Maps a page; subject to permission checks |

**Common flag combinations:**

| Hex | Bits set | Typical use |
|-----|----------|-------------|
| `0x01` | V | Non-leaf pointer to next-level table |
| `0x0F` | R\|W\|X\|V | Kernel code+data page (no U) |
| `0x1F` | R\|W\|X\|U\|V | User code+data page |
| `0x17` | R\|X\|U\|V | User read-execute (no W) |

> **Common mistake:** do not confuse the physical *address* of a table with the PTE *value* that points to it.
> A leaf PT at physical address `0x2000` has `ppn = 0x2000 >> 12 = 2`, so the non-leaf PTE value is
> `(2 << 10) | 0x1 = 0x801` — **not** `0x2001`.

---

## Standard mode — minimal observable example

With VM enabled, **no setup code is needed**. This is enough to see TLB activity:

```asm
.text
    li   t0, 0
    li   t1, 100
loop:
    addi t0, t0, 1
    blt  t0, t1, loop   # every load/store and fetch hits the TLB
    li   a0, 0
    li   a7, 93
    ecall               # exit
```

Enable **Settings → Virtual Memory** before assembling and watch the TLB Stats subtab fill up.

---

## Custom page tables (advanced)

If you want to study custom address layouts, page faults, or a real OS-style privilege transition, you can set up your own page tables. Write `csrw satp` to install them; the TLB is flushed automatically and your mapping takes over.

```asm
# Maps VA 0x0000 → PA 0x0000 (R|W|X|U, 4 KiB), then drops into U-mode.
#
# Memory layout (chosen to avoid overlap with code at 0x0000):
#   0x1000 — root page table   (root PT: PPN = 1)
#   0x2000 — leaf page table   (leaf PT: PPN = 2)

.text
boot:
    # ── 1. Write root PTE ──────────────────────────────────────────────
    # Non-leaf pointer: PPN=2 (leaf PT @ 0x2000), V=1
    # Value = (2 << 10) | 0x1 = 0x801
    li   t0, 0x801
    li   t1, 0x1000          # root PT lives at PA 0x1000
    sw   t0, 0(t1)           # root_pt[VPN1=0] = 0x801

    # ── 2. Write leaf PTE ──────────────────────────────────────────────
    # Leaf: PPN=0 (PA 0x0000), R|W|X|U|V = 0x1F
    # Value = (0 << 10) | 0x1F = 0x1F
    li   t2, 0x1F
    li   t3, 0x2000          # leaf PT lives at PA 0x2000
    sw   t2, 0(t3)           # leaf_pt[VPN0=0] = 0x1F

    # ── 3. Install satp: Sv32 (bit 31), ASID=0, root PPN=1 ────────────
    li   t0, 0x80000001      # bit31=Sv32 | PPN=1
    csrw satp, t0            # writing satp flushes the TLB

    # ── 4. Set up mret to drop into U-mode ────────────────────────────
    la   t0, user_entry
    csrw mepc, t0
    li   t0, 0               # mstatus.MPP = 0b00 = U
    csrw mstatus, t0
    mret                     # privilege → U, pc → user_entry

user_entry:
    # Translation active (satp=Sv32, priv=U) — hardware-accurate behavior.
    # Every fetch and load/store goes through the TLB.
    nop
    halt
```

The end-to-end shape — PTE layout, fault routing through `mtvec`, `mret` back to U-mode — is exercised in `tests/mmu_traps.rs`.

---

## See also

- [Memory map](memory-allocation.md) — physical address layout used as the backing store
- [Cache config](cache-config.md) — `.fcache` / `.rcfg` fields including the `[tlb]` block
- [Pipeline simulation](pipeline.md) — where MMU stalls show up in the Gantt view
