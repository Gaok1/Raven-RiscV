# Falcon ASM 🦅 – Educational RISC-V (RV32I) Emulator
<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

Falcon ASM is an emulator written in Rust focused on clarity and learning. It exposes the **fetch → decode → execute** cycle and provides a complete view of how a basic RISC-V processor works.

The project includes:

- Instruction decoder and encoder
- Two-pass text assembler with label support
- `.text` and `.data` segments with data directives
- Little-endian registers and memory
- Execution engine ready for integration with graphical interfaces

## Project Status

Implements the essential subset of **RV32I**:

- **R-type:** `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- **I-type (OP-IMM):** `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- **Loads:** `LB, LH, LW, LBU, LHU`
- **Stores:** `SB, SH, SW`
- **Branches:** `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- **U/J:** `LUI, AUIPC, JAL`
- **JALR`
- **SYSTEM:** `ECALL`, `EBREAK` (treated as HALT)

*Not implemented:* FENCE/CSR and floating point.

## Assembler and Directives

The assembler accepts code split into segments:

- `.text` – instruction segment.
- `.data` – data segment, loaded **0x1000 bytes** after the program base address.

Inside `.data` the following directives are supported:

- `.byte v1, v2, ...` – 8-bit values
- `.word w1, w2, ...` – 32-bit values in little-endian

Labels (`label:`) can be defined in any segment. To load a label address, use the `la rd, label` pseudo-instruction, which emits a `lui`/`addi` pair.

### Available Pseudo-instructions

- `nop` → `addi x0, x0, 0`
- `mv rd, rs` → `addi rd, rs, 0`
- `li rd, imm12` → `addi rd, x0, imm`
- `subi rd, rs1, imm` → `addi rd, rs1, -imm`
- `j label` → `jal x0, label`
- `call label` → `jal ra, label`
- `jr rs1` → `jalr x0, rs1, 0`
- `ret` → `jalr x0, ra, 0`
- `la rd, label` → loads the address of `label`

## Registers and Memory

- Registers `x0..x31` with aliases: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`. `x0` is always 0.
- Little-endian memory with `load8/16/32` and `store8/16/32` operations.

## Opcode Summary

```
RTYPE = 0x33
OPIMM = 0x13
LOAD  = 0x03
STORE = 0x23
BRANCH= 0x63
LUI   = 0x37
AUIPC = 0x17
JAL   = 0x6F
JALR  = 0x67
SYSTEM= 0x73
```

For format details and `funct3/funct7` tables see [`docs/format.md`](format.md).

## Running

Requirements: stable Rust (via [rustup.rs](https://rustup.rs)).

```bash
cargo run
```

Minimal example:

```rust
use falcon::asm::assemble;
use falcon::program::{load_bytes, load_words};

let asm = r#"
    .data
msg: .byte 1, 2, 3
    .text
    la a0, msg
    ecall
"#;

let mut mem = falcon::Ram::new(64 * 1024);
let mut cpu = falcon::Cpu::default();
cpu.pc = 0;

let prog = assemble(asm, cpu.pc).expect("assemble");
load_words(&mut mem, cpu.pc, &prog.text);
load_bytes(&mut mem, prog.data_base, &prog.data);
```

The emulator executes instructions while `step` returns `true`.

# Examples
## Code editor
<img width="1918" height="1009" alt="image" src="https://github.com/user-attachments/assets/4ade62a4-e3e0-4c69-b42b-ae52d5bd8397" />

## Running code (emulator)

### Registers view
<img width="1917" height="997" alt="image" src="https://github.com/user-attachments/assets/6be9a0ec-b64f-4cab-b9b5-ff581a27f692" />

### RAM view
<img width="1920" height="999" alt="image" src="https://github.com/user-attachments/assets/63386101-393f-47d1-a559-9a3b74da95ac" />
