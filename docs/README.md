# Falcon ASM — Educational RISC-V (RV32I) Emulator
<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

Falcon ASM is a Rust emulator focused on clarity and learning. It exposes the fetch -> decode -> execute cycle and provides a clear view of how a basic RISC‑V processor works.

The project includes:

- Instruction decoder and encoder
- Two-pass text assembler with labels
- `.text`/`.section .text` and `.data`/`.section .data` segments with data directives
- Little-endian registers and memory
- Execution engine ready for a terminal UI (TUI)

## Project Status

Implements the essential subset of RV32I:

- R-type: `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- I-type (OP-IMM): `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- Loads: `LB, LH, LW, LBU, LHU`
- Stores: `SB, SH, SW`
- Branches: `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- U/J: `LUI, AUIPC, JAL`
- JALR
- SYSTEM: `ECALL`, `HALT`

Not implemented: FENCE/CSR and floating point.

## Assembler and Directives

The assembler accepts code split into segments:

- `.text` or `.section .text` — instruction segment
- `.data` or `.section .data` — data segment, loaded 0x1000 bytes after the program base address

Inside `.data` the following directives are supported:

- `.byte v1, v2, ...` — 8-bit values
- `.half h1, h2, ...` — 16-bit values
- `.word w1, w2, ...` — 32-bit values in little-endian
- `.dword d1, d2, ...` — 64-bit values in little-endian
- `.ascii "text"` — raw bytes
- `.asciz "text"` / `.string "text"` — string with NUL terminator
- `.space n` / `.zero n` — n zero bytes

Labels (`label:`) can be defined in any segment. To load an absolute label address, use `la rd, label` which emits a `lui`/`addi` pair.

### Available Pseudo-instructions

- `nop` -> `addi x0, x0, 0`
- `mv rd, rs` -> `addi rd, rs, 0`
- `li rd, imm12` -> `addi rd, x0, imm` (12-bit only by design)
- `subi rd, rs1, imm` -> `addi rd, rs1, -imm`
- `j label` -> `jal x0, label`
- `call label` -> `jal ra, label`
- `jr rs1` -> `jalr x0, rs1, 0`
- `ret` -> `jalr x0, ra, 0`
- `la rd, label` -> load absolute address of `label`
- `push rs` -> `addi sp, sp, -4` ; `sw rs, 4(sp)`
- `pop rd` -> `lw rd, 4(sp)` ; `addi sp, sp, 4`
- `print rd` -> sets `a7=1`, prints integer in `rd`
- `printStr label` -> sets `a7=2`, loads `a0` with `label`, appends NUL-terminated string (no newline)
- `printStrLn label` -> sets `a7=4`, loads `a0` with `label`, prints string and newline
- `read label` -> sets `a7=3`, loads `a0` with `label`, reads a line into memory

## Registers and Memory

- Registers `x0..x31` with aliases: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`. `x0` is always 0.
- Little-endian memory with `load8/16/32` and `store8/16/32` operations.

## Syscalls

Place the syscall number in `a7`, set arguments in `a0`, then execute `ecall`.

| `a7` | Pseudo-instruction | Description |
|------|--------------------|-------------|
| 1 | `print rd` | Print the decimal value from register `rd` (`a0=rd`). |
| 2 | `printStr label` | Append the NUL-terminated string at `label` (no newline). |
| 3 | `read label` | Read a line into memory at `label` and append a NUL byte. |
| 4 | `printStrLn label` | Append string at `label` and start a new line. |
| 64 | `readByte label` | Read number (dec or `0x`hex) and store 1 byte at `label`. |
| 65 | `readHalf label` | Read number and store 2 bytes (little-endian) at `label`. |
| 66 | `readWord label` | Read number and store 4 bytes (little-endian) at `label`. |

Invalid inputs or values out of range for `readByte/Half/Word` print an error and the emulator waits for a new input without advancing the PC.

Example without pseudo-instructions:

```asm
    li a7, 1      # select syscall
    mv a0, t0     # value to print
    ecall
```

## Instruction Types (how they work)

- R-type (opcode `0x33`): register-register operations. `rd = OP(rs1, rs2)`.
- I-type (opcode `0x13`): register-immediate ALU. `rd = OP(rs1, imm12)`. Shifts use 5-bit shamt (`SRAI` with `funct7=0x20`).
- Loads (opcode `0x03`): `LB/LH/LW/LBU/LHU` read from `rs1 + imm` to `rd`.
- Stores (opcode `0x23`): `SB/SH/SW` write the low 8/16/32 bits of `rs2` to `rs1 + imm`.
- Branches (opcode `0x63`): PC-relative conditional jumps; the assembler computes the offset from labels.
- U-type (`LUI/AUIPC`): `LUI` loads bits [31:12] into `rd`; `AUIPC` adds the immediate to the current `pc`.
- Jumps (`JAL/JALR`): `JAL` writes `pc+4` into `rd` and jumps to `pc + imm21`; `JALR` writes `pc+4` into `rd` and jumps to `(rs1 + imm12) & !1`.

See [`docs/format.md`](format.md) for bit layouts and more details.

## Running

Requirements: stable Rust (via [rustup.rs](https://rustup.rs)).

```bash
cargo run
```


