# Virtual Memory and the TLB (Sv32)

> [Leia em Português](../pt-BR/virtual-memory.md)

This is study material on **virtual memory in RISC-V** (the **Sv32** scheme) and the **TLB** the simulator puts in front of it. The goal is to teach from the ground up: *why* virtual memory exists, *how* translation happens, *what* the TLB solves, and how you can experiment with all of it in Raven's Virtual Memory tab.

You don't need to open any code to read this. Everything is presented in terms of concepts, diagrams, and example assembly programs.

---

## Contents

1. [Why virtual memory exists](#1-why-virtual-memory-exists)
2. [The conceptual model: VA → PA](#2-the-conceptual-model-va--pa)
3. [Page tables: from a giant array to a multi-level tree](#3-page-tables-from-a-giant-array-to-a-multi-level-tree)
4. [Sv32 in detail](#4-sv32-in-detail)
5. [The PTE format](#5-the-pte-format)
6. [Megapages (superpages)](#6-megapages-superpages)
7. [The A and D bits](#7-the-a-and-d-bits)
8. [Permissions and privilege levels](#8-permissions-and-privilege-levels)
9. [Why the TLB exists](#9-why-the-tlb-exists)
10. [TLB organization: sets, ways, indexing](#10-tlb-organization-sets-ways-indexing)
11. [Replacement policies](#11-replacement-policies)
12. [ASIDs and the global flag](#12-asids-and-the-global-flag)
13. [`sfence.vma` and TLB coherence](#13-sfencevma-and-tlb-coherence)
14. [Page faults: causes and trap flow](#14-page-faults-causes-and-trap-flow)
15. [The VM modes: standard, Custom and Manual](#15-the-vm-modes-standard-custom-and-manual)
16. [Performance impact](#16-performance-impact)
17. [The Virtual Memory tab](#17-the-virtual-memory-tab)
18. [Minimal example — standard mode](#18-minimal-example--standard-mode)
19. [Advanced example — custom table](#19-advanced-example--custom-table)
20. [Trap delegation and demand paging](#20-trap-delegation-and-demand-paging)
21. [See also](#21-see-also)

---

## 1. Why virtual memory exists

Imagine a computer with no virtual memory. Every program sees physical RAM directly: address `0x1000` in your program is byte `0x1000` on the memory chip. Everything works until you try to run **two programs at once**.

Four practical problems appear:

1. **Isolation.** Process A can read or write process B's `0x1000` just by accessing `0x1000`. There is no barrier — a bug in A can corrupt B.
2. **Relocation.** Each program is linked to start at some fixed address (say `0x10000`). If two programs chose the same address, they collide — and even if they don't, it's the OS that decides *where* in RAM each program fits, not the linker.
3. **Fragmentation.** As processes come and go, free memory turns into a jigsaw of holes. There may be 200 MiB free in total but no contiguous 100 MiB block for the next process.
4. **Controlled sharing.** Two instances of `bash` should share the same copy of the code in RAM (saving memory) but need separate stacks (isolation). Without a layer of indirection, it's all or nothing.

**Virtual memory** solves all four by inserting an indirection between the address the program uses (the **virtual address**, or VA) and the real address in RAM (the **physical address**, or PA). The hardware translates VA → PA on every access. The translation table is controlled by the operating system. As a result:

- Each process gets its own **virtual address space**. A's `0x1000` translates to a different physical page than B's `0x1000`.
- The linker can assume a fixed address; the OS places the physical pages wherever it likes.
- The "holes" only exist in the physical world. Virtually, each process sees a contiguous space.
- Mapping the *same* physical page at different VAs implements sharing. Marking one copy read-only implements copy-on-write.

The indirection has a cost: each memory access becomes potentially several accesses (reading the translation table). The next section shows how the hardware structures that table; later we'll see how the **TLB** cuts the cost to nearly zero in the common case.

---

## 2. The conceptual model: VA → PA

The unit of translation isn't a byte; it's a **page** (typically 4 KiB). Addresses within the same page are translated together.

```
virtual address (VA)          physical address (PA)
┌──────────────┬──────┐       ┌──────────────┬──────┐
│ Virtual Page │offset│  ─→   │ Physical Page│offset│
│   Number     │      │       │   Number     │      │
│  (VPN)       │      │       │  (PPN)       │      │
└──────────────┴──────┘       └──────────────┴──────┘
```

- The **offset** (low bits) passes straight through: byte 7 of virtual page `X` is byte 7 of the matching physical page.
- The hardware only needs to translate VPN → PPN. That translation lives in a structure called the **page table**.

With 4 KiB pages, the offset is 12 bits (`2^12 = 4096`). The remaining bits form the VPN. In RV32 that leaves 20 bits of VPN, so there are `2^20 ≈ 1 million` possible virtual pages per process.

---

## 3. Page tables: from a giant array to a multi-level tree

### 3.1 The naive approach

The simplest form would be an array indexed by VPN: given the VPN, read the entry and find the PPN.

```
page_table[VPN] = PPN
```

For RV32 (20 bits of VPN) that's `2^20` entries × 4 bytes = **4 MiB per process** — and almost all of it zero, because a typical process uses only a fraction of the address space. On 64-bit systems it explodes: billions of entries, terabytes of table.

### 3.2 The solution: a hierarchy

Instead of one giant array, we use a **tree**: one table points to subtables, which point to the final pages. Only the branches actually used take up space.

In Sv32 the hierarchy has 2 levels: a **root table** of 1024 entries, each pointing either to a subtable of 1024 entries (which points to the final pages) or directly to a **megapage** of 4 MiB.

```
        satp points to the root
        ┌─────────────────┐
        │  root PT (1024) │   ← one 4 KiB page
        └─┬─────┬─────────┘
          │     │
          ▼     ▼
       ┌────┐ ┌────┐         ← leaf PTs (one per used region)
       │1024│ │1024│
       └─┬──┘ └─┬──┘
         ▼     ▼
       physical 4 KiB pages
```

- Small processes: 1 root + 1–2 leaves = 8–12 KiB of overhead, not 4 MiB.
- Huge processes: pay only for the regions actually populated.

Walking that tree is called a **page-table walk**, and the hardware does the walk on every access that isn't in the TLB.

---

## 4. Sv32 in detail

**Sv32** is RISC-V's 32-bit paging scheme. The name means "Supervisor virtual addressing, 32 bits".

### 4.1 Splitting the virtual address

The 32-bit VA is cut into three fields:

```
 31         22 21         12 11           0
┌─────────────┬─────────────┬─────────────┐
│   VPN[1]    │   VPN[0]    │   offset    │
│  (10 bits)  │  (10 bits)  │  (12 bits)  │
└─────────────┴─────────────┴─────────────┘
```

- `offset` (12 bits) → byte within the page (0..4095).
- `VPN[0]` (10 bits) → index into the **level-0** table (leaf PT).
- `VPN[1]` (10 bits) → index into the **level-1** table (root PT).

`2^10 = 1024` entries per table × 4 bytes/entry = **4 KiB per table** — exactly the size of a page. Convenient: each table fits in one page and is addressed by a single PPN.

### 4.2 The walk algorithm

Given a virtual address `vaddr` and the `satp` CSR pointing at the root:

```
1. pte1_addr = (satp.PPN << 12) + VPN[1] * 4
   pte1 = mem[pte1_addr]
   if !pte1.V          → page fault
   if pte1 is a leaf   → MEGAPAGE (4 MiB), jump straight to the checks
   else                → continue to level 0

2. pte0_addr = (pte1.PPN << 12) + VPN[0] * 4
   pte0 = mem[pte0_addr]
   if !pte0.V          → page fault
   if !pte0 is a leaf  → page fault (the last-level PTE must be a leaf)

3. check permissions (R/W/X for the access type) and privilege (U)
4. paddr = (pte0.PPN << 12) | offset
```

Note: **two RAM accesses per translation**, plus the original access. Without a TLB, each `lw` becomes potentially 3 memory accesses.

### 4.3 The `satp` CSR

The **S**upervisor **A**ddress **T**ranslation and **P**rotection register controls translation:

```
 31  30          22 21                          0
┌────┬─────────────┬────────────────────────────┐
│MODE│    ASID     │           PPN              │
│ 1b │   9 bits    │         22 bits            │
└────┴─────────────┴────────────────────────────┘
```

- **MODE**: `0` = Bare (no translation, VA = PA), `1` = Sv32 (translation active).
- **ASID** (Address Space IDentifier): identifies the process. The TLB uses the ASID to avoid mixing translations from different processes (see §12).
- **PPN**: the physical page number where the root table lives. Byte address = `PPN << 12`.

Writing `satp` **flushes the TLB**, because switching tables would invalidate every cached translation.

---

## 5. The PTE format

Each **P**age **T**able **E**ntry is 32 bits:

```
 31                  10  9  8  7  6  5  4  3  2  1  0
┌───────────────────────┬────┬──┬──┬──┬──┬──┬──┬──┬──┐
│        PPN (22 b)     │RSW │ D│ A│ G│ U│ X│ W│ R│ V│
└───────────────────────┴────┴──┴──┴──┴──┴──┴──┴──┴──┘
```

| Bit | Name | Function |
|-----|------|----------|
| 0 | V | **Valid**. If 0, the entry doesn't count — any walk reaching it page-faults. |
| 1 | R | **Read**. Page can be read (loads). |
| 2 | W | **Write**. Page can be written (stores). |
| 3 | X | **eXecute**. Page can be fetched as instructions. |
| 4 | U | **User**. Page is accessible in U-mode. Without this bit, only S and M may touch it. |
| 5 | G | **Global**. The entry has no ASID — it matches in any address space. |
| 6 | A | **Accessed**. Some access has touched this page since the last clear. |
| 7 | D | **Dirty**. Some store has touched this page. |
| 8–9 | RSW | Reserved for software (the OS may use it freely). |
| 10–31 | PPN | Physical page number (or a pointer to a subtable, if non-leaf). |

### 5.1 Encoding the value

The formula is always:

```
PTE = (ppn << 10) | flags
```

where `ppn = physical_address >> 12`. Classic mistake: **do not confuse the physical address of a table with the PTE value that points to it.** A leaf PT at address `0x2000` has `ppn = 0x2000 >> 12 = 2`, so the non-leaf PTE pointing to it is `(2 << 10) | 0x1 = 0x801` — **not** `0x2001`.

### 5.2 Leaf vs non-leaf

The distinction is in the R, W, X bits:

- **Non-leaf (pointer)**: R=W=X=0, V=1. The PPN points to a subtable.
- **Leaf**: at least one of R/W/X set. The PPN points to a data/code page.

Common encodings:

| Hex | Bits | Meaning |
|-----|------|---------|
| `0x01` | V | Pointer to a subtable |
| `0x0F` | V\|R\|W\|X | Kernel page (code+data, no U) |
| `0x1F` | V\|R\|W\|X\|U | User page (code+data) |
| `0x17` | V\|R\|X\|U | User read-execute page |
| `0x0B` | V\|R\|X | Kernel read-execute page |

### 5.3 Reserved encoding: W=1, R=0

The combination `W=1, R=0` is **reserved** and page-faults during the walk. Intuition: it makes little sense to allow writes without reads — code that writes usually wants to read back. The architecture keeps the encoding for future extensions.

---

## 6. Megapages (superpages)

A level-1 PTE may be a **leaf**. In that case it maps a contiguous **4 MiB** region (1024 × 4 KiB) with a single PTE. We call this a **megapage** (or superpage).

```
VA with a megapage:
 31         22 21                       0
┌─────────────┬─────────────────────────┐
│   VPN[1]    │     offset (22 bits)    │
└─────────────┴─────────────────────────┘
                ↑
                vpn[0] + page offset become one offset
```

Why does it matter?

- **Shorter walk**: 1 RAM access instead of 2.
- **Smaller TLB footprint**: 1 entry covers 4 MiB. Without megapages, mapping 4 MiB would need 1024 entries — more than the whole TLB has.
- **Good for large contiguous regions**: kernel text, frame buffers, identity maps.

Constraint: the megapage's PPN must be **4 MiB-aligned** (its low 10 bits zero). Otherwise the architecture treats it as "superpage misaligned" and page-faults.

Raven's standard mode (§15) uses megapages to cover the whole address space with 1024 single entries, all identity-mapped.

---

## 7. The A and D bits

Every time a page is touched, the hardware should signal it somehow — otherwise the OS has no way to know:

- *Which pages can I safely evict to swap?* (needs the A bit for approximate LRU)
- *Has this page been modified since the last write to disk?* (needs the D bit)

The RISC-V spec leaves **two implementation options**:

1. **Hardware sets the bits directly in RAM** when the walker finishes successfully (`A` always, `D` on a store). Simple for the OS.
2. **Hardware raises a trap** when A or D are clear and the OS updates them. More complex, but spares the hardware from writing the PT.

Raven chooses option 1, for two reasons:

- It is didactically simpler: you run a program and watch the A/D bits appear in the PTEs without writing a handler.
- It avoids a second trip through the trap just to update a flag.

Observable consequence: if you inspect the page table in RAM after running a program, you'll see the A and D bits set on the pages that were actually used and written. That's information a real OS would use to decide what goes to swap first.

---

## 8. Permissions and privilege levels

### 8.1 The R/W/X + U bits

Each leaf PTE carries four protection flags:

| Bit | Access allowed if set |
|-----|-----------------------|
| R | Loads (`lw`, `lh`, `lb`, …) |
| W | Stores (`sw`, `sh`, `sb`, …) |
| X | Instruction fetches |
| U | User mode may touch the page |

Combined, they give fine-grained access control: code pages running in userland are usually `R + X + U`; a stack is `R + W + U`; kernel code is `R + X` (no U).

### 8.2 Privilege: M, S, U

| Mode | Notation | What it can do |
|------|----------|----------------|
| Machine | M | Full access; ignores translation on real hardware |
| Supervisor | S | Kernel; sees pages without `U`. With SUM=0 it can't see `U` pages. |
| User | U | Application; only sees pages with `U=1` |

Rules under `satp.MODE = Sv32`:

- **U**: the PTE must have `U=1`, otherwise page fault.
- **S**: the PTE must have `U=0`.
- **M**: real hardware **bypasses** the MMU entirely in M-mode.

> **Raven's didactic override:** in the standard (Sv32) and Custom modes, the MMU also translates in M-mode. This is deliberate: most educational programs run in M (there's no privilege setup), so without this override the TLB would stay silent. When you install your own tables and drop into U via `mret` (Manual mode), behavior matches real hardware again.

### 8.3 The page-fault cause

The trap cause depends on the access type:

| Cause | Type |
|-------|------|
| 12 | Instruction page fault (fetch failed) |
| 13 | Load page fault (load failed) |
| 15 | Store page fault (store failed) |

---

## 9. Why the TLB exists

Let's count memory accesses without a TLB.

A simple `lw t0, 0(t1)`:

1. **Fetch** of the instruction: 1 RAM read → but the PC must be translated → 2 PT accesses + 1 read = **3 accesses**.
2. **Load** of the operand: 1 RAM read → translate `t1` → 2 PT accesses + 1 read = **3 accesses**.

Total: **6 RAM accesses** to run an instruction that originally cost 2. **A 3× slowdown.**

And it isn't just slowness: each PT access is also a cache-miss candidate, and the PT tree competes with program data for the D-cache.

### 9.1 The observation that saves everything

Programs have **locality**: the same VPN repeats over and over in short time windows (a loop over an array, sequential instruction fetch, etc.). If we cache the `VPN → PPN` translation, the walk happens **once** and the next N accesses to that page are essentially free.

That cache is called the **Translation Lookaside Buffer (TLB)**. It is the single most important structure for the performance of any system with paging.

### 9.2 Metrics to watch

- **Hit rate**: the fraction of translations served from the TLB. Well-behaved programs stay above 99%.
- **Miss penalty**: cycles to do a walk. In Raven, default `20` cycles.
- **Hit latency**: cycles to confirm a hit. Default `1` cycle.

The TLB's stats subtab plots the hit rate in a rolling 300-cycle window — useful for spotting distinct phases (warmup vs steady state, a working-set change, etc.).

---

## 10. TLB organization: sets, ways, indexing

The TLB is a small cache, so it inherits the same terminology as regular caches: **sets**, **ways**, **associativity**, **replacement policies**.

### 10.1 Set-associative

Each VPN maps to **one specific set** (via a hash). Within the set, any of the **N ways** (slots) may hold the entry.

```
                       ┌── way 0 ──┬── way 1 ──┬── way 2 ──┬── way 3 ──┐
VPN → hash → set k →   │   entry   │   entry   │   entry   │   entry   │
                       └───────────┴───────────┴───────────┴───────────┘
                       all N ways compared in parallel
```

- **Hit**: some way in the set has a matching `vpn` and compatible `asid`.
- **Miss**: no way matches → walk → install into some way (evicting someone if the set is full).

### 10.2 Megapages and indexing

A megapage covers 1024 consecutive VPNs. If it were placed only in the set for the *starting* VPN, any probe for a VPN in the middle of the megapage would miss. To avoid that, megapages use a different indexing scheme than 4 KiB pages — effectively living in "their own" sets — and the TLB consults both schemes on every lookup, so large entries are found by all the VPNs they cover.

You don't have to think about this when writing programs. The detail matters for understanding why a single megapage in standard mode can serve a whole program with a hit rate near 100%.

### 10.3 Associativity trade-offs

| Associativity | Advantage | Cost |
|---------------|-----------|------|
| 1-way (direct-mapped) | Simplest hardware | Vulnerable to conflict misses |
| N-way (typically 4–8) | Tolerates collisions | More parallel comparators |
| Fully associative | No conflict misses | Expensive to scale |

Raven's default is **32 entries, 4-way** → 8 sets.

---

## 11. Replacement policies

When a set is full, which entry is evicted? Raven offers the same six policies as the D-cache:

| Policy | Evicts | Best for… |
|--------|--------|-----------|
| **LRU** (Least Recently Used) | The least recently accessed | Default; good at almost everything |
| **FIFO** | The longest-installed entry | Simple hardware, low overhead |
| **LFU** (Least Frequently Used) | The least frequently accessed | Stable working sets |
| **Clock** (second-chance) | Approximates LRU with 1 bit per entry | Classic LRU-vs-FIFO compromise |
| **MRU** (Most Recently Used) | The most recently accessed | Large sequential streams |
| **Random** | Random | Baseline; surprisingly OK |

Changing the policy at runtime: go to **Virtual Memory → settings**, pick the policy, Apply. Apply resets the TLB (all entries become invalid), so the next run starts cold.

Experiment tip: run the same program twice — once with LRU, once with MRU. Compare the hit rate in the stats subtab. For common access patterns (loops, arrays) LRU wins by a lot. For large sequential scans bigger than the TLB, MRU can surprise you.

---

## 12. ASIDs and the global flag

### 12.1 The problem

Imagine two processes, A and B. Both have a translation for VPN `0x1000`, but to different physical pages. If the TLB caches A's translation and the OS switches to B, the next time B accesses `0x1000` it will *hit A's entry* — and read/write the wrong physical page. Catastrophe.

### 12.2 The traditional fix: full flush

On every process switch, flush the whole TLB. It works, but throws away useful work — entries for shared pages (kernel, libc) would have to be re-validated.

### 12.3 The better fix: ASIDs

Each process gets a unique number (**Address Space ID**) — 9 bits in Sv32. The current ASID lives in `satp`. Each TLB entry carries the ASID it was installed with, and a match is valid only if both `vpn` AND `asid` match.

Result: a process switch *doesn't need* to flush the TLB. Old entries sleep until they are naturally evicted.

### 12.4 The G (global) flag

Pages used by all processes (kernel mappings, for example) can have `G=1`. Global entries **ignore the ASID** on a match — matching the VPN is enough. This saves TLB space because you don't need one entry per ASID.

---

## 13. `sfence.vma` and TLB coherence

The TLB is a cache of something that lives in RAM (the page table). If the OS modifies the PT — installs a new page, changes permissions, pages something out — old TLB entries become **stale**.

The **`sfence.vma`** instruction signals the hardware: "invalidate the cached translations; I touched the PT". Variants:

- `sfence.vma rs1=x0, rs2=x0` → flush everything.
- `sfence.vma rs1=vaddr, rs2=x0` → flush only the `vaddr` entry.
- `sfence.vma rs1=x0, rs2=asid` → flush only the given ASID.
- `sfence.vma rs1=vaddr, rs2=asid` → flush the specific entry.

Raven implements `sfence.vma` as a **full flush** in this release (`rs1`/`rs2` ignored), which is correct but not optimized. Writing `satp` also flushes, since switching tables invalidates everything.

---

## 14. Page faults: causes and trap flow

When translation fails — invalid PTE, wrong permission, insufficient privilege, misaligned megapage, reserved encoding — the hardware raises a trap.

### 14.1 Causes

| `mcause` | Type |
|----------|------|
| 12 | Instruction page fault |
| 13 | Load page fault |
| 15 | Store page fault |

### 14.2 The trap flow

1. Translation fails during a fetch, load or store.
2. The hardware saves the fault context in the CSRs:
   - `mcause ← cause` (12, 13 or 15)
   - `mtval ← vaddr` that faulted — the handler uses this to learn **which** address caused the fault
   - `mepc ← PC` of the faulting instruction — so `mret` returns here
   - `mstatus.MPP ← current mode` — so `mret` restores the privilege
3. The hardware switches to M-mode and jumps to `mtvec & ~3` (direct mode; vectored mode is not covered).
4. Your handler decides what to do and returns with `mret`.
5. If `mtvec = 0` (you forgot to set it), Raven prints the fault to the console and halts — so you aren't left chasing a bug.

Unless the cause is **delegated** to supervisor mode (`medeleg`): in that case the trap fills the `s*` CSRs and vectors through `stvec`, staying in S-mode. See [§20](#20-trap-delegation-and-demand-paging).

### 14.3 What a real OS would do

A real OS usually handles the fault like this:

- If the page was paged out to swap → bring it back from disk, update the PT, return with `mret`. The program never notices.
- If the region is valid but not yet allocated (demand paging) → allocate a physical page, map it, return.
- If the access is genuinely invalid → kill the process (SIGSEGV on Unix).

Raven has no swap and no page allocator — it gives you the primitives to experiment by writing your own handlers.

---

## 15. The VM modes: standard, Custom and Manual

Virtual memory in Raven isn't a simple on/off switch: you pick **one of four modes**, each meant for a different moment in your learning. The selector is in the **global Settings tab**, or in the **Virtual Memory** tab's overview / settings subtabs (see §17) — any of them cycles through the modes. The choice is persisted in the `.rcfg`.

| Mode | What it does | When to use it |
|------|--------------|----------------|
| **Off** | No translation: VA = PA, the MMU is bypassed and the TLB is left untouched. No extra cycles. | Plain program, flat memory — same behavior as having VM disabled. |
| **Sv32** | The didactic **standard** mode: auto-installs an Sv32 identity map (10+10+12) and forces translation even in M-mode. | See TLB activity on **any** program, with no setup code. |
| **Custom** | Like Sv32, but with a **parametric paging scheme** you design (number of levels, index bits per level, offset). | Test **other ways** of paging — bigger/smaller pages, more/fewer levels. |
| **Manual** | Real RISC-V: M-mode bypasses, the **program** drives `satp` and its own tables. | Study page faults, privilege transitions and hardware-accurate demand paging (§19–§20). |

### 15.1 Standard mode (Sv32) — experiment with no boilerplate

Turning translation on with no configuration would be useless: `satp` would be zero (Bare), the initial privilege is M, and nothing would be translated. You'd have to build a page table just to see any TLB activity. The **Sv32** mode fixes this:

1. On assembly, Raven automatically writes an identity **megapage** map covering the whole space, with `R|W|X|U|V` permissions.
2. `satp` is set to Sv32 pointing at that table.
3. Translation is forced even in M-mode, so any program shows TLB activity.

Result: **any program**, even a simple `addi`/`blt` with no CSRs at all, produces TLB activity. Pick the mode, assemble, run — done. This lets you study translation-cache behavior (hit rate, replacement policies, the effect of TLB size) before learning to build tables, set `mtvec`, write a handler, and drop into U-mode.

### 15.2 Custom mode — design your own paging scheme

The **Custom** mode is the "anything is possible" mode: it installs an auto map like Sv32, but the **shape** of the translation is yours. A paging scheme slices the 32-bit virtual address into fields:

- **offset bits** — set the page size (`page = 2^offset bytes`; `12` → 4 KiB, `22` → 4 MiB).
- **one index per level** — how many bits select an entry at each page-table level (one level per tree depth; 1 to 4 levels).

The **one hard rule**: `offset + Σ (index bits of each level) = 32`. The panel shows the page size, depth, and sum with ✓/✗ live — if it's red, `apply` is refused.

Examples to try:

| offset | levels | sum | result |
|--------|--------|-----|--------|
| 12 | L1=10, L0=10 | 32 | plain Sv32: 4 KiB pages, 2-level walk |
| 22 | L0=10 | 32 | 4 MiB megapages, single-level walk |
| 12 | L2=8, L1=6, L0=6 | 32 | three levels, smaller per-level tables |

Bigger/fewer pages → shorter walks and smaller TLB footprint, but coarser mapping. More levels → smaller tables and a deeper tree. It's exactly the trade-off real ISAs face — and here you change it, hit `apply`, reassemble and compare the hit rate in the stats subtab, all without writing assembly.

### 15.3 Manual mode — the program in charge

When you want hardware-accurate behavior — no didactic override, with the program installing its own tables and handling faults — use **Manual** mode. Here M-mode bypasses the MMU; translation only kicks in after a `csrw satp` (Sv32) and a drop into U/S-mode. It's the mode required by the advanced examples in §19 and §20.

---

## 16. Performance impact

Every translation adds cycles. Where they show up depends on the execution mode.

### 16.1 Pipeline mode

- A **fetch** that misses the TLB → a stall in the **IF** slot (shows as a red stretch in the Gantt view).
- A **load/store** that misses → a stall in the **MEM** slot.
- Hits add `hit_latency` to the same slot. Default `1` cycle — usually overlaps with other latencies.

### 16.2 Interpreter mode

With no pipeline, the extra cycles become part of the total and feed directly into `total_program_cycles` and CPI. You see the impact in **CPI** on the status bar.

### 16.3 Quick heuristic

- Hit rate > 99% → negligible impact.
- Hit rate 90–99% → noticeable; may be worth raising `entry_count` or associativity.
- Hit rate < 90% → the working set doesn't fit; switch to megapages or grow the TLB.

The stats subtab shows the rolling chart and the totals. Capture a snapshot with `s` to compare before/after a config change.

---

## 17. The Virtual Memory tab

The **Virtual Memory** tab is the central panel for everything this document explains. It mirrors the Cache tab: a single flat header with **five** subtabs — **overview · map · tlb · stats · settings** — framed by an **Execution** box (speed / state / reset + live cycles) and a shared controls bar (`results · import cfg · export cfg · flush tlb`). Use `Tab` to cycle subtabs; `r` / `p` / `f` reset, run-pause and change speed without leaving the tab.

### overview
- The landing subtab. Two clickable **quick controls** at the top — `Mode < off | sv32 | custom | manual >` and `TLB [on/off]` — so you can enable the MMU with a single click, no need to open Settings.
- Below them, the live state of `satp` (MODE, ASID, root PPN) and the current privilege level, plus a **Translation active?** line that says why translation is (in)active — the quick diagnostic for "why is the TLB empty?".

### map
- The **live page table**, read straight from RAM starting at `satp.PPN`, walked along the active scheme's N levels: pointer PTEs expand into child tables; leaves at any level are (super)pages; long runs of uniform leaves collapse into one summary line. PTEs cached in the TLB are marked **●TLB**.
- It is **read-only** — to change the map or the scheme, use the settings subtab. It's your window into what the MMU actually sees, invaluable when a mapping isn't taking effect.

### tlb
- Per-entry table: `VPN → PPN | R/W/X/U | ASID | V | G | A | D | mega`.
- Useful to confirm a page was cached, or to watch the A/D bits turn on as the program runs. `↑`/`↓` or the mouse wheel scroll the list.
- When the TLB is disabled (toggle in overview/settings), this subtab shows a notice: every access walks the table (miss + penalty, no hits).

### stats
- Hit-rate gauge and counters: `Hits`, `Misses`, `Evictions`, `Page Faults`, plus a rolling 300-cycle history chart of the hit rate.
- **Session snapshots** (shared with the Cache tab): press `s` to capture the current window; `↑`/`↓` select, `Enter` opens the details popup (now with a TLB block), `D` deletes. Snapshots ride along in the results export (`results` / `Ctrl+r`) — the `.rstats` / `.csv` gain a `tlb.*` section.

### settings (the VM control panel)
Where you reshape virtual memory **without writing a line of code**. Four blocks, top to bottom:

1. **Mode + TLB** — the same `Mode` selector and `TLB` toggle as overview.
2. **Paging scheme** — the shape of the translation (offset + levels). Editable in **Custom** mode; see §15.2.
3. **Page map** — what the auto map installs: `kind` (identity = VA→VA, or offset = shift physical by a fixed delta, handy to watch VPN and PPN diverge in the tlb subtab), `R/W/X/U` permissions, the `G` (global) flag, and the `ASID`.
4. **TLB geometry** — `Entries` (power of two), `Associativity`, `Replacement Policy`, `Hit Latency`, `Miss Penalty`, plus **small / med / large** presets.

Everything is staged: edits only take effect when you hit **apply** (which reconfigures the TLB and, in the auto modes, reinstalls the map and re-points `satp`). **flush tlb** just drops cached translations without touching the map. Click a field to edit or toggle it; `Tab` / `↑` `↓` move between numeric fields while editing. It's the safe sandbox: break the mapping here and nothing crashes — you just see faults you can reason about.
- Save/load config via export/import (`export cfg` / `import cfg`, or `Ctrl+e` / `Ctrl+l`); the `[tlb]` block travels in the `[cache]` section of the unified `.rcfg` (see [Cache config](cache-config.md)).

---

## 18. Minimal example — standard mode

With VM enabled (Sv32 mode), this is already enough:

```asm
.text
    li   t0, 0
    li   t1, 100
loop:
    addi t0, t0, 1
    blt  t0, t1, loop   # every fetch and load/store goes through the TLB
    li   a0, 0
    li   a7, 93
    ecall               # exit
```

1. Pick the **Sv32** mode (in the global Settings or the **Virtual Memory** overview/settings subtab) before assembling.
2. Assemble and run.
3. Go to **Virtual Memory → stats** and watch the hit rate climb as the loop reuses its pages.
4. Visit the **tlb** subtab to confirm the code's `vpn` appears with `X=1` and `A=1`.

---

## 19. Advanced example — custom table

To study page faults, privilege transitions, or your own layouts, pick **Manual** mode (in the **Virtual Memory** overview/settings subtab) and write your own table — with no didactic override, the program is in charge.

```asm
# Maps VA 0x0000 → PA 0x0000 (R|W|X|U, 4 KiB) and drops into U-mode.
#
# Layout (chosen to avoid overlapping the code at 0x0000):
#   0x1000 — root PT  (PPN = 1)
#   0x2000 — leaf PT  (PPN = 2)

.text
boot:
    # ── 1. Write the root PTE ──────────────────────────────────────────
    # Non-leaf pointer: PPN=2 (leaf PT @ 0x2000), V=1
    # Value = (2 << 10) | 0x1 = 0x801
    li   t0, 0x801
    li   t1, 0x1000          # root PT lives at PA 0x1000
    sw   t0, 0(t1)           # root_pt[VPN1=0] = 0x801

    # ── 2. Write the leaf PTE ──────────────────────────────────────────
    # Leaf: PPN=0 (PA 0x0000), R|W|X|U|V = 0x1F
    # Value = (0 << 10) | 0x1F = 0x1F
    li   t2, 0x1F
    li   t3, 0x2000          # leaf PT lives at PA 0x2000
    sw   t2, 0(t3)           # leaf_pt[VPN0=0] = 0x1F

    # ── 3. Install satp: Sv32 (bit 31), ASID=0, root PPN=1 ─────────────
    li   t0, 0x80000001      # bit31=Sv32 | PPN=1
    csrw satp, t0            # writing satp flushes the TLB

    # ── 4. Set up mret to drop into U-mode ─────────────────────────────
    la   t0, user_entry
    csrw mepc, t0
    li   t0, 0               # mstatus.MPP = 0b00 = U
    csrw mstatus, t0
    mret                     # privilege → U, pc → user_entry

user_entry:
    # Translation active (satp=Sv32, priv=U) — hardware-accurate behavior.
    nop
    halt
```

Variations to experiment with:

- Swap `0x1F` for `0x17` (no W) and try a `sw` — store page fault `15`.
- Swap for `0x0F` (no U) — fault `13` in U-mode (no user permission).
- Point the root pointer at `0x2001` instead of `0x801` (wrong PPN) — watch the walker miss.
- Set `mtvec` to a handler that prints `mcause`/`mtval` before calling `mret`.

---

## 20. Trap delegation and demand paging

So far every fault has gone to M-mode through `mtvec`. Real operating systems run their fault handler in **supervisor** mode and reserve machine mode for firmware. Raven models this with **trap delegation**: set bit `c` of `medeleg` and a fault with cause `c`, taken in S- or U-mode, is routed to the supervisor handler at `stvec` instead — filling `sepc` / `scause` / `stval` and recording the previous mode in `sstatus.SPP`. The return is `sret`, mirroring `mret` over `sstatus`.

### 20.1 The CSRs and the instruction

| CSR | Number | Use |
|-----|--------|-----|
| `medeleg` | `0x302` | Exception delegation — bit `c` set ⇒ cause `c` handled in S-mode |
| `mideleg` | `0x303` | Interrupt delegation (stored; no async interrupts yet) |
| `sstatus` | `0x100` | Supervisor status — `SPP` (bit 8), `SPIE` (bit 5), `SIE` (bit 1) |
| `stvec` | `0x105` | Trap-vector base address (S-mode) |
| `sscratch` | `0x140` | Scratch register for the supervisor handler |
| `sepc` | `0x141` | PC saved on a delegated trap |
| `scause` | `0x142` | Delegated trap cause |
| `stval` | `0x143` | Delegated trap value (faulting vaddr) |

> On real RISC-V hardware, `sstatus` is a restricted *view* of `mstatus` — both names read and write the same shared bits. Here it behaves as an independent register instead. This is a deliberate pedagogical simplification: it keeps the delegation path easy to follow, without the shared-bit aliasing real hardware performs.

### 20.2 The demand-paging pattern

1. User code touches a page that isn't mapped yet → **load/store page fault** (cause 13 / 15).
2. Because the cause is delegated (`medeleg`), the CPU vectors to the supervisor handler in S-mode.
3. The handler reads `stval` (the faulting address), installs the missing PTE, and runs `sfence.vma` to drop any stale TLB entry.
4. `sret` returns to `sepc` — the faulting instruction re-executes and now **succeeds**.

> **⚠ Walker / cache coherence.** The page-table walk reads PTEs **directly from RAM**, while a write-back D-cache may hold a freshly stored value without having written it out yet. So a handler that writes a PTE with a normal `sw` can leave that PTE sitting in the cache: the retried walk still reads the old (empty) entry from RAM and faults forever. For demand-paging programs, **disable the cache** (Cache tab) or switch the D-cache to **write-through** so the handler's store reaches RAM before the walk re-runs.

### 20.3 Setup (in the M-mode boot code)

```asm
    # Delegate load (cause 13) and store (cause 15) page faults to S-mode.
    li   t0, (1 << 13) | (1 << 15)
    csrw medeleg, t0

    # Point the supervisor trap vector at the handler.
    la   t0, page_fault_handler
    csrw stvec, t0
    # ... build the initial page tables, csrw satp, then mret into U-mode ...
```

### 20.4 The supervisor handler

```asm
page_fault_handler:
    csrr t0, stval              # faulting virtual address
    # ... derive the leaf-PTE slot, store the new PTE into the (kernel-mapped) leaf table ...
    sfence.vma                  # drop stale TLB entries
    sret                        # return to sepc — the faulting access retries
```

A complete round trip looks like this: boot maps the code/handler/page-table pages, drops into U-mode, faults on an unmapped page, the handler maps it, runs `sfence.vma`, `sret`, and the retried load reads the freshly mapped data. Two layout rules from this pattern are worth repeating:

- The **handler's code and the page-table pages must be mapped non-`U`** (kernel-only), because S-mode cannot touch `U=1` pages (`SUM` is not modeled).
- The handler edits the page table *under translation*, so the leaf-table page needs its own identity (`VA = PA`) mapping — the simulator's stand-in for a kernel direct map.

To watch it live: pick **Manual** mode (in the **Virtual Memory** overview/settings subtab), disable the cache, assemble, and open the **Virtual Memory → map** subtab to see PTEs appear as the handler installs them.

---

## 21. See also

- [Memory map](memory-allocation.md) — the physical address layout used as the backing store.
- [Cache config](cache-config.md) — `.rcfg` `[cache]` fields including the `[tlb]` block.
- [Pipeline simulation](pipeline.md) — where MMU stalls show up in the Gantt view.
- [Cache simulation](cache.md) — the shared terminology (sets, ways, policies) the TLB inherits.
