# FALCON ASM — RISC-V Emulator & IDE

<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

**FALCON ASM** is a terminal-based RISC-V emulator, assembler, and IDE written in Rust. It covers **RV32I + M + F** and is designed to make every part of the fetch → decode → execute pipeline visible and interactive — ideal for students, teachers, and anyone learning assembly.

Everything lives in one TUI: write code, assemble, run step-by-step, inspect registers and memory, profile your cache hierarchy, and read the docs — without leaving the terminal.

---

## Features

### ISA Coverage
- **RV32I** — full base integer instruction set
- **RV32M** — integer multiplication and division
- **RV32F** — single-precision floating-point (26 instructions, `f0`–`f31`, `fcsr`)
- Rich pseudo-instruction set: `la`, `li`, `call`, `ret`, `push`, `pop`, `mv`, `neg`, `not`, `seqz`, `snez`, `beqz`, `bnez`, `bgt`, `ble`, `fmv.s`, `fneg.s`, `fabs.s`, and more
- `ecall` syscalls: print integer/string, read input, exit, random bytes

### Assembler
- `.text`, `.data`, `.bss` segments with `.byte`, `.half`, `.word`, `.ascii`, `.asciz`, `.space`
- `.word label` — use label addresses as data values (jump tables, pointer arrays)
- Block comments (`##!`) and inline annotations (`#!`) visible at runtime
- Clear error messages with line numbers

### Editor (Tab 1)
- Syntax highlighting — instructions, registers, directives, labels, strings all styled
- Ghost operand hints while typing
- Go-to-definition (`F12`), label highlight on hover, address gutter (`F2`)
- Undo/redo (50 levels), word navigation, toggle comment (`Ctrl+/`), duplicate line (`Ctrl+D`)
- Auto-indent, bracketed paste, page up/down

### Run Tab (Tab 2)
**Instruction Memory**
- Label headers and block-comment separators rendered inline
- Type badge per instruction (`[R]` `[I]` `[S]` `[B]` `[U]` `[J]`)
- Heat coloring — execution count suffix `×N` colored by frequency
- Branch outcome on current PC: `→ 0xADDR (taken)` / `↛ (not taken)`
- Breakpoints (`b`), jump to address (`g`), execution trace panel (`t`)

**Decoded Details panel**
- Full field breakdown (opcode, funct3/7, rs1/rs2/rd, immediate, sign-extended)
- Effective address for loads/stores; RAW hazard warning (`⚠ RAW`)
- CPI estimate and instruction class per instruction

**Register Sidebar**
- Integer registers: dual-column hex + decimal, age fade highlight, pin (`p`), write trace
- Float registers: ABI names (`ft0`–`ft11`, `fa0`–`fa7`, `fs0`–`fs11`), toggle with `Tab`
- Four sidebar modes: RAM view / integer registers / stack view / breakpoint list (`v`)

### Cache Tab (Tab 3)
- Configurable L1 I-cache + D-cache + unlimited extra levels (L2, L3…)
- Replacement policies: LRU, FIFO, LFU, Clock, MRU, Random
- Write policies: write-through / write-back + write-allocate / no-allocate
- Inclusion policies: Non-inclusive, Inclusive, Exclusive
- Live stats: hit rate, MPKI, RAM traffic, top miss PCs
- Academic metrics: AMAT (hierarchical), IPC, CPI breakdown per level
- Export results (`Ctrl+R`) to `.fstats` / `.csv`; load baseline for delta comparison (`Ctrl+M`)
- Visual cache matrix with horizontal scroll and per-scrollbar drag

### CPI Configuration
- Per-class cycle costs: ALU, MUL, DIV, LOAD, STORE, branch taken/not-taken, JUMP, SYSTEM, FP
- Configurable directly in the Cache → Config tab

### Docs Tab (Tab 4)
- Instruction reference and Run tab guide built into the app

---

## Quick Start

Download the latest binary from [Releases](https://github.com/Gaok1/FALCON-ASM/releases), or build from source:

```bash
git clone https://github.com/Gaok1/FALCON-ASM.git
cd FALCON-ASM
cargo run
```

Requires Rust 1.75+. No external dependencies beyond the Rust toolchain.

---

## Key Bindings (Run Tab)

| Key | Action |
|-----|--------|
| `F5` / `Space` | Run / Pause |
| `F10` / `n` | Single step |
| `F9` / `b` | Toggle breakpoint at PC |
| `f` | Cycle speed: 1× → 2× → 4× → Instant |
| `v` | Cycle sidebar: RAM → Registers → Stack → Breakpoints |
| `Tab` | Toggle integer / float register bank |
| `t` | Toggle execution trace panel |
| `g` | Jump to address |
| `x` | Toggle raw hex word display |
| `e` / `y` | Toggle exec count / type badges |
| `p` / click | Pin / unpin register |

---

## Example Programs

The `Program Examples/` directory includes:
`fib.fas`, `bubble_sort_20.fas`, `quick_sort_20_push_pop.fas`, `binary_search_tree.fas`, `gcd_euclid.fas`, `fatorial.fas`, `cache_locality.fas`, and more.

---

## Docs

- [Tutorial (EN)](Tutorial.md) — step-by-step walkthrough
- [Instruction formats (EN)](format.md) — bit layouts, encoding, pseudo-instructions
- [Cache simulator guide (EN)](cache.md) — configuration, metrics, export
- [Tutorial (PT-BR)](Tutorial-pt.md) | [Formatos (PT-BR)](format.pt-BR.md) | [Cache (PT-BR)](cache.pt-BR.md)

---

## Contributing

Issues and pull requests are welcome. The codebase is intentionally readable — the CPU core, decoder, and assembler are each under ~500 lines and follow a straightforward structure.
