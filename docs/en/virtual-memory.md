# Virtual memory (Sv32)

> [Leia em Português](../pt-BR/virtual-memory.md)

Raven implements the RISC-V **Sv32** virtual memory scheme: a 2-level page table walked in software by the CPU's MMU, fronted by a configurable TLB. Translation is **off by default** so legacy programs run unchanged; turn it on from **Settings → Virtual Memory** (also persisted in `.rcfg`).

When enabled, every fetch and every load/store goes through the MMU. M-mode keeps physical addressing always; U-mode goes through translation as soon as `satp.MODE = Sv32`. The TLB has its own subtab inside the **Cache** tab where you can configure size, associativity, replacement policy, hit latency, and miss penalty — and watch hits/misses live.

---

## When to enable it

| Use case | VM on? |
|----------|--------|
| Plain RV32IMAF program, single flat memory | off — same behavior as before |
| Studying page-table walks, A/D bits, page faults | **on** |
| Comparing TLB hit/miss penalties across replacement policies | **on** |
| Running an OS-style kernel (M-mode setup + U-mode user code) | **on** |

The toggle is in `Settings → Virtual Memory`. Default is **off**. With VM off, the MMU is bypassed entirely — no TLB lookups, no walker, no extra cycles.

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
| Machine | M | Always physical, MMU bypassed |
| User    | U | Always goes through translation; `U=0` PTEs fault |

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

## A minimal U-mode boot sequence

```asm
# Build a single 4 KiB mapping VA 0x10_0000 → PA 0x8_0000 (R|W|U), then drop
# into U-mode at the freshly-mapped page.

.text
boot:
    # ... build root + leaf PTEs in RAM (see tests/mmu_traps.rs for the layout)

    li   t0, 0x80000000     # satp.MODE = Sv32 (bit 31) | PPN of root PT
    la   t1, root_pt
    srli t1, t1, 12
    or   t0, t0, t1
    csrw satp, t0           # writes here flush the TLB

    la   t0, user_entry
    csrw mepc, t0
    li   t0, 0              # mstatus.MPP = U
    csrw mstatus, t0
    mret                    # drop to U-mode at user_entry
```

The end-to-end shape — PTE layout, fault routing through `mtvec`, `mret` back to U-mode — is exercised in `tests/mmu_traps.rs`.

---

## See also

- [Memory map](memory-allocation.md) — physical address layout used as the backing store
- [Cache config](cache-config.md) — `.fcache` / `.rcfg` fields including the `[tlb]` block
- [Pipeline simulation](pipeline.md) — where MMU stalls show up in the Gantt view
