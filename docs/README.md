# Falcon ASM 🦅 – RISC-V Educational Emulator (RV32I)

<img width="500" height="400" alt="image" src="https://github.com/user-attachments/assets/ed5354ba-93bc-4717-ab77-8993f1c3abc5" />

Falcon ASM is an educational RISC-V emulator focused on clarity and on visualizing the **fetch–decode–execute** cycle.
The project includes a **decoder**, **encoder**, **two-pass text assembler** (with labels), **registers/memory**, and an **execution engine**

> **Current state (MVP): RV32I essentials**
>
> * **R-type:** `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA`
> * **I-type (OP-IMM):** `ADDI, ANDI, ORI, XORI, SLLI, SRLI, SRAI`
> * **Loads:** `LB, LH, LW, LBU, LHU`
> * **Stores:** `SB, SH, SW`
> * **Branches:** `BEQ, BNE, BLT, BGE, BLTU, BGEU`
> * **U/J:** `LUI, AUIPC, JAL`
> * **JALR**
> * **SYSTEM:** `ECALL`, `EBREAK` (treated as HALT)

*Not yet implemented:* `SLT/SLTU`, M extension (`MUL*`), FENCE/CSR, floating point.

---
## Getting started

Requirements: stable Rust (via [https://rustup.rs](https://rustup.rs)).

```bash
cargo run
```

## Registers & Memory

* **Registers:** `x0..x31` (x0 is immutable/always 0). Assembler also accepts aliases:
  `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`.
* **Memory:** little-endian; simple `Ram` implementing `load8/16/32` and `store8/16/32`.

---

## Text Assembler (source → bytes)

* **Two passes:** pass 1 collects `label:` symbols; pass 2 resolves and encodes.
* **Comments:** anything after `;` or `#` is ignored.
* **Operands:** `instr op1, op2, op3`.
* **Loads/Stores:** `imm(rs1)`, e.g. `lw x1, 0(x2)`, `sw x3, 4(x5)`.
* **Branches/Jumps:** operand can be an **immediate** or a **label**.
  The assembler computes `imm = target_pc - instruction_pc` (in **bytes**).
  For **B/J** formats the offset must be even (multiple of 2) — the encoder validates this.

**Pseudoinstructions (MVP):**

* `nop` → `addi x0, x0, 0`
* `mv rd, rs` → `addi rd, rs, 0`
* `li rd, imm12` → `addi rd, x0, imm` (only if it fits 12-bit signed; otherwise use `lui+addi`)
* `j label` → `jal x0, label`
* `jr rs1` → `jalr x0, rs1, 0`
* `ret` → `jalr x0, ra, 0` (ra = x1)

---

## Encoding summary (used opcodes)

* `RTYPE = 0x33`
* `OPIMM  = 0x13`
* `LOAD   = 0x03`
* `STORE  = 0x23`
* `BRANCH = 0x63`
* `LUI    = 0x37`
* `AUIPC  = 0x17`
* `JAL    = 0x6F`
* `JALR   = 0x67`
* `SYSTEM = 0x73`

See `docs/format.md` for full bit layouts and funct3/funct7 tables exactly as implemented.

---

