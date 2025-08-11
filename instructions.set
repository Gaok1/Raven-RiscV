# Falcon ASM 🦅 – RISC-V Educational Emulator (RV32I)

Falcon ASM is an educational RISC-V emulator focused on clarity and on visualizing the **fetch–decode–execute** cycle.
The project includes a **decoder**, **encoder**, **two-pass text assembler** (with labels), **registers/memory**, and an **execution engine** — ready to plug into a **Ratatui** UI.

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

## Project layout

```
src/
  main.rs
  falcon/
    mod.rs
    arch.rs           # opcodes/consts
    errors.rs
    registers.rs      # Cpu (x0..x31, pc)
    memory.rs         # Bus + Ram
    instruction.rs    # enum Instruction { ... }
    exec.rs           # step()/run()

    decoder/          # decode(u32) -> Instruction
      mod.rs
      rtype.rs
      itype.rs
      stype.rs
      btype.rs
      jtype.rs

    encoder/          # encode(Instruction) -> u32
      mod.rs

    asm/              # assemble(&str, base_pc) -> Vec<u32> (two-pass)
      mod.rs

    program/
      mod.rs
      loader.rs       # load_words/load_bytes

docs/
  format.md           # Encoding/ISA reference (kept in sync with code)
```

---

## Getting started

Requirements: stable Rust (via [https://rustup.rs](https://rustup.rs)).

```bash
cargo run
```

The sample `main.rs` assembles a small program and runs until `ecall`:

```rust
mod falcon;

use falcon::asm::assemble;
use falcon::program::load_words;

fn main() {
    let asm = r#"
        addi x1, x0, 5
        addi x2, x0, 7
    loop:
        add  x3, x1, x2
        beq  x3, x0, loop
        ecall
    "#;

    let mut mem = falcon::Ram::new(64*1024);
    let mut cpu = falcon::Cpu::default();
    cpu.pc = 0;

    let words = assemble(asm, cpu.pc).expect("assemble");
    load_words(&mut mem, cpu.pc, &words);

    while falcon::exec::step(&mut cpu, &mut mem) {}
    println!("x3 = {}", cpu.x[3]); // expected: 12
}
```

---

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

