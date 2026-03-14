# RAVEN — RISC-V Simulator & IDE

> [Leia em Português](docs/README.pt-BR.md)

**RAVEN** is a free, open-source RISC-V simulator and terminal IDE for students and anyone learning assembly. It covers **RV32IMAF** — the full base integer set, multiply/divide, atomics, and single-precision float — and makes every part of the machine visible while your program runs.

Write assembly in the built-in editor, assemble with `Ctrl+R`, and step through every instruction watching registers, memory, and the cache update in real time. Nothing is hidden.

![RAVEN in action](docs/assets/raven-example.gif)

---

## Quick Start

Download the latest binary from [Releases](https://github.com/Gaok1/Raven/releases), or build from source:

```bash
git clone https://github.com/Gaok1/Raven
cd Raven
cargo run
```

Requires Rust 1.75+. No other dependencies.

---

## What you get

### Editor (Tab 1)
- Syntax highlighting — instructions, registers, labels, directives, strings
- Ghost operand hints while typing
- `Ctrl+R` to assemble instantly; errors show line number and reason
- Undo/redo (50 levels), word navigation, toggle comment (`Ctrl+/`), duplicate line (`Ctrl+D`)
- Go-to-definition (`F12`), label highlight, address gutter (`F2`)

### Debugger — Run Tab (Tab 2)
- Run free, pause (`Space`/`F5`), or single-step (`n`/`F10`)
- Breakpoints (`b`/`F9`), jump to address (`g`), execution trace (`t`)
- All 32 integer registers with ABI names, change highlighting
- Float registers (`f0–f31` / ABI names), toggled with `Tab`
- Sidebar cycles with `v`: **RAM → Registers → Dyn**
  - **RAM**: scrollable memory view with region selector (Data / Stack / R/W)
  - **Registers**: integer or float register bank with per-register age highlighting
  - **Dyn**: self-narrating mode for single-stepping — shows the memory region written on every STORE, and the register bank on every LOAD, ALU, or branch instruction. The accessed address is always centered in the view.
- Instruction memory panel: type badge `[R][I][S][B][U][J]`, execution heat `×N`, branch outcome
- Instruction decoder: full field breakdown (opcode, funct3/7, rs1/rs2/rd, immediate, sign-extended)

### Cache Simulator (Tab 3)
- Separate I-cache and D-cache, plus unlimited extra levels (L2, L3…)
- Configurable: sets, ways, block size, write policy (write-through/write-back + allocate/no-allocate), inclusion policy (non-inclusive/inclusive/exclusive)
- Six replacement policies: LRU, FIFO, LFU, Clock, MRU, Random
- Live stats: hit rate, MPKI, RAM traffic, top miss PCs
- Academic metrics: AMAT (hierarchical), IPC, CPI per instruction class
- Visual matrix view: every set and way, valid/tag/dirty state, scrollable
- Export results (`Ctrl+R`) to `.fstats`/`.csv`; load baseline for delta comparison (`Ctrl+M`)
- CPI configuration: per-class cycle costs (ALU, MUL, DIV, LOAD, STORE, branch, JUMP, FP…)

### Docs Tab (Tab 4)
- Instruction reference for all supported instructions
- Run tab key guide

---

## ISA Coverage

| Extension | Instructions |
|-----------|-------------|
| RV32I | ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI, LB, LH, LW, LBU, LHU, SB, SH, SW, BEQ, BNE, BLT, BGE, BLTU, BGEU, JAL, JALR, LUI, AUIPC, ECALL, EBREAK |
| RV32M | MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU |
| RV32A | LR.W, SC.W, AMOSWAP.W, AMOADD.W, AMOXOR.W, AMOAND.W, AMOOR.W, AMOMAX.W, AMOMIN.W, AMOMAXU.W, AMOMINU.W |
| RV32F | FADD.S, FSUB.S, FMUL.S, FDIV.S, FSQRT.S, FMIN.S, FMAX.S, FEQ.S, FLT.S, FLE.S, FLW, FSW, FMV.W.X, FMV.X.W, FCVT.W.S, FCVT.WU.S, FCVT.S.W, FCVT.S.WU, FCLASS.S, FMADD.S, FMSUB.S, FNMADD.S, FNMSUB.S, FNEG.S, FABS.S |

**Pseudo-instructions:** `la`, `li`, `mv`, `neg`, `not`, `ret`, `call`, `push`, `pop`, `seqz`, `snez`, `beqz`, `bnez`, `bgt`, `ble`, `fmv.s`, `fneg.s`, `fabs.s`, and more.

**Syscalls (`ecall`):** print integer/string, read input, exit, random bytes — Linux-compatible ABI (`a7` = syscall number).

---

## Loading ELF Binaries

RAVEN can load and execute **ELF32 LE RISC-V** binaries compiled by any standard toolchain. It is officially compatible with:

| Target | Support |
|--------|---------|
| `riscv32im-unknown-none-elf` | ✅ Full |
| `riscv32ima-unknown-none-elf` | ✅ Full |

### Running a Rust no_std program

```bash
# 1. Add the target (once)
rustup target add riscv32im-unknown-none-elf

# 2. Build your project
cargo build --target riscv32im-unknown-none-elf

# 3. Open RAVEN, go to the Editor tab, click [BIN] and select the ELF
#    (found at target/riscv32im-unknown-none-elf/debug/<your-crate>)
```

The ELF is loaded at its linked virtual addresses, the PC is set to the entry point, and the disassembler shows the decoded text segment. Unknown words (data, padding) appear as `.word 0x...`.

A ready-to-use project with `_start`, panic handler, allocator, and wrappers for `write`, `read`, and `exit` is available at [`rust-to-raven/`](rust-to-raven/).

---

## Assembler

- `.text`, `.data`, `.bss` segments
- Directives: `.byte`, `.half`, `.word`, `.dword`, `.float`, `.ascii`, `.asciz`, `.string`, `.space`, `.globl`
- `.word label` — label addresses as data values (jump tables, pointer arrays)
- Block comments (`##!`) and inline annotations (`#!`) visible in the Run tab at runtime
- Clear error messages with line numbers

---

## Key Bindings

### Global
| Key | Action |
|-----|--------|
| `Ctrl+R` | Assemble and load |
| `1`–`4` | Switch tab (Editor / Run / Cache / Docs) |

### Run Tab
| Key | Action |
|-----|--------|
| `F5` / `Space` | Run / Pause |
| `F10` / `n` | Single step |
| `F9` / `b` | Toggle breakpoint at PC |
| `f` | Cycle speed: 1× → 2× → 4× → Instant |
| `v` | Cycle sidebar: RAM → Registers → Dyn |
| `Tab` | Toggle integer / float register bank |
| `t` | Toggle execution trace panel |
| `g` | Jump to address |
| `x` | Toggle raw hex word display |

---

## Included Examples

`Program Examples/` contains ready-to-run programs:

| File | Demonstrates |
|------|-------------|
| `fib.fas` | Recursion, stack frames, calling convention |
| `bubble_sort_20.fas` | Loops, pointer arithmetic, in-place swap |
| `quick_sort_20_push_pop.fas` | Recursive quicksort with `push`/`pop` |
| `binary_search_tree.fas` | Heap allocation, pointer chasing |
| `gcd_euclid.fas` | Iterative algorithm, branch-heavy |
| `cache_locality.fas` | Cache-friendly vs cache-hostile access patterns |

---

## Docs

- [Tutorial (EN)](docs/Tutorial.md) — step-by-step first-program walkthrough
- [Instruction formats (EN)](docs/format.md) — bit layouts, encoding, pseudo-instructions
- [Tutorial (PT-BR)](docs/Tutorial-pt.md) | [Formatos (PT-BR)](docs/format.pt-BR.md)

---

## Contributing

Issues and pull requests are welcome. The CPU core, decoder, and assembler are each under ~500 lines and follow a straightforward structure.
