# RAVEN format guide — RV32I in plain language

This is the companion reference for RAVEN, our teaching-friendly RISC-V emulator. Use it as a cheat sheet.

## How to read this document

- Every section keeps the tables you need, followed by a short explanation in everyday language.
- Head straight to [Encoding cheat sheets](#encoding-cheat-sheets) when you only need bit layouts.
- New to the assembler directives? Jump to [Assembler behaviour and pseudo-instructions](#assembler-behaviour-and-pseudo-instructions).
- Prefer learning by doing? Use the in-app `[?]` tutorial for hands-on walkthroughs and try the snippets as you read.

## Architecture snapshot

RAVEN focuses on a approachable RV32I+M subset so you can reason about each pipeline stage without being buried in extras.

- **Word size:** 32 bits.
- **Endianness:** little-endian throughout (`{to,from}_le_bytes`).
- **Program counter:** advances by 4 each instruction; branches and jumps are PC-relative.
- **Registers:** hardware names `x0…x31` with the usual aliases `zero`, `ra`, `sp`, `gp`, `tp`, `t0…t6`, `s0/fp`, `s1`, `a0…a7`,
  `s2…s11`. Writes to `x0/zero` are ignored.

Not yet implemented: CSR/FENCE instructions. RAVEN covers RV32IMF — base integer, multiply/divide, and single-precision float.

## Instruction set inside RAVEN

| Category | Instructions |
| --- | --- |
| R-type | `ADD`, `SUB`, `AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU`, `MUL`, `MULH`, `MULHSU`, `MULHU`, `DIV`, `DIVU`, `REM`, `REMU` |
| I-type (OP-IMM) | `ADDI`, `ANDI`, `ORI`, `XORI`, `SLTI`, `SLTIU`, `SLLI`, `SRLI`, `SRAI` |
| Loads | `LB`, `LH`, `LW`, `LBU`, `LHU` |
| Stores | `SB`, `SH`, `SW` |
| Branches | `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU` |
| Upper / jumps | `LUI`, `AUIPC`, `JAL`, `JALR` |
| System | `ECALL`, `EBREAK` (alias: `HALT`) |

Division by zero is treated as a teaching moment: `DIV`, `DIVU`, `REM`, and `REMU` halt the emulator with a descriptive error instead
of following the architected “divide-by-zero” results. The interruption makes it obvious something unexpected happened.

## Encoding cheat sheets

The tables below show all 32-bit layouts RAVEN uses. When an instruction name appears in bold, read the note beneath the table for
a reminder about immediate ranges or special cases.

### R-type (arithmetic, logic, multiply/divide)

| Field  | Bits  | Description |
| --- | --- | --- |
| opcode | [6:0] | instruction family |
| rd     | [11:7] | destination register |
| funct3 | [14:12] | operation subtype |
| rs1    | [19:15] | source register 1 |
| rs2    | [24:20] | source register 2 |
| funct7 | [31:25] | extra subtype / MUL/DIV selector |

### I-type (OP-IMM, loads, `JALR`)

| Field | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| imm[11:0] | [31:20] |

- Immediates are signed and range from -2048 to 2047.
- Shift instructions use `shamt` in bits [24:20] with `funct7 = 0x00` (`SLLI`, `SRLI`) or `0x20` (`SRAI`).

### S-type (stores)

| Field | Bits |
| --- | --- |
| opcode | [6:0] |
| imm[4:0] | [11:7] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| rs2 | [24:20] |
| imm[11:5] | [31:25] |

### B-type (conditional branches)

| Field | Bits |
| --- | --- |
| opcode | [6:0] |
| imm[11] | [7] |
| imm[4:1] | [11:8] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| rs2 | [24:20] |
| imm[10:5] | [30:25] |
| imm[12] | [31] |

- The assembler fills a 13-bit immediate (in bytes). Bit 0 is always zero because branch targets are word-aligned.

### U-type (`LUI`, `AUIPC`)

| Field | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| imm[31:12] | [31:12] |

### J-type (`JAL`)

| Field | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| imm[19:12] | [19:12] |
| imm[11] | [20] |
| imm[10:1] | [30:21] |
| imm[20] | [31] |

- The 21-bit immediate is stored in bytes; bit 0 is zero because jump targets are word-aligned.

## Opcode and funct reference

| Purpose | Value |
| --- | --- |
| `OPC_RTYPE` | `0x33` |
| `OPC_OPIMM` | `0x13` |
| `OPC_LOAD` | `0x03` |
| `OPC_STORE` | `0x23` |
| `OPC_BRANCH` | `0x63` |
| `OPC_LUI` | `0x37` |
| `OPC_AUIPC` | `0x17` |
| `OPC_JAL` | `0x6F` |
| `OPC_JALR` | `0x67` |
| `OPC_SYSTEM` | `0x73` |

### `funct3` and `funct7`

**R-type (`0x33`):**

- `funct3 = 0x0`: `ADD` (`funct7=0x00`), `SUB` (`funct7=0x20`)
- `funct3 = 0x1`: `SLL`
- `funct3 = 0x2`: `SLT`
- `funct3 = 0x3`: `SLTU`
- `funct3 = 0x4`: `XOR`
- `funct3 = 0x5`: `SRL` (`funct7=0x00`), `SRA` (`funct7=0x20`)
- `funct3 = 0x6`: `OR`
- `funct3 = 0x7`: `AND`
- Multiply/divide share the `0x33` opcode with `funct7 = 0x01` and use the same `funct3` positions (`MUL`, `MULH`, `MULHSU`, `MULHU`,
  `DIV`, `DIVU`, `REM`, `REMU`).

**I-type OP-IMM (`0x13`):**

- `funct3 = 0x0`: `ADDI`
- `funct3 = 0x1`: `SLLI`
- `funct3 = 0x2`: `SLTI`
- `funct3 = 0x3`: `SLTIU`
- `funct3 = 0x4`: `XORI`
- `funct3 = 0x5`: `SRLI` (`funct7=0x00`), `SRAI` (`funct7=0x20`)
- `funct3 = 0x6`: `ORI`
- `funct3 = 0x7`: `ANDI`

**Loads (`0x03`):** `LB` (`0x0`), `LH` (`0x1`), `LW` (`0x2`), `LBU` (`0x4`), `LHU` (`0x5`).

**Stores (`0x23`):** `SB` (`0x0`), `SH` (`0x1`), `SW` (`0x2`).

**Branches (`0x63`):** `BEQ` (`0x0`), `BNE` (`0x1`), `BLT` (`0x4`), `BGE` (`0x5`), `BLTU` (`0x6`), `BGEU` (`0x7`).

**`JALR` (`0x67`):** uses `funct3 = 0x0`.

**System (`0x73`):** RAVEN implements three encodings: `ECALL` (`0x00000073`), `EBREAK`
(`0x00100073`), and `HALT` (`0x00200073`). `ebreak` is a resumable debug breakpoint; `halt`
permanently stops only the current hart and should be used as the semantic "hart finished here"
marker in examples.

## Assembler behaviour and pseudo-instructions

RAVEN’s assembler is intentionally lightweight so you can follow every step:

- Comments begin with `;` or `#`.
- Operands are comma-separated (`mnemonic op1, op2, ...`).
- Supported sections/directives include `.text`, `.data`, `.bss`, `.section`, `.word`, `.byte`, `.half`, `.ascii`, `.asciz`/`.asciiz`, `.space`, and `.align`.

### Pseudo-instructions reference

The table below documents the pseudo forms implemented by RAVEN and the exact expansion shape used by the assembler.

| Pseudo | Accepted format | Expansion (conceptual) | Notes |
| --- | --- | --- | --- |
| `nop` | `nop` | `addi x0, x0, 0` | No operands allowed. |
| `mv` | `mv rd, rs` | `addi rd, rs, 0` | Register copy. |
| `li` | `li rd, imm` | `addi rd, x0, imm` | Immediate must fit signed 12-bit (`-2048..2047`). |
| `subi` | `subi rd, rs1, imm` | `addi rd, rs1, -imm` | `-imm` must fit signed 12-bit. |
| `j` | `j label_or_imm` | `jal x0, label_or_imm` | 21-bit PC-relative immediate (bit 0 must be even). |
| `call` | `call label_or_imm` | `jal ra, label_or_imm` | Saves return address in `ra` (`x1`). |
| `jr` | `jr rs1` | `jalr x0, rs1, 0` | Indirect jump, no link. |
| `ret` | `ret` | `jalr x0, ra, 0` | Return to address in `ra` (`x1`). |
| `la` | `la rd, label` | `lui rd, hi(label)` + `addi rd, rd, lo(label)` | Uses hi/lo split with rounding (`+0x800`) so low part fits signed 12-bit. |
| `push` | `push rs` | `addi sp, sp, -4` + `sw rs, 0(sp)` | Decrements `sp` by 4, then stores `rs` at the new `sp` (standard RISC-V full-descending convention) |
| `pop` | `pop rd` | `lw rd, 0(sp)` + `addi sp, sp, 4` | Loads `rd` from the current `sp`, then increments `sp` by 4 |
| `print` | `print rd` | `addi a7, x0, 1000` + `addi a0, rd, 0` + `ecall` | Prints register value. |
| `printStr` | `printStr label` | `addi a7, x0, 1001` + `la a0, label` + `ecall` | Prints NUL-terminated string (no newline). |
| `printString` | `printString label` | same as `printStr label` | Legacy alias accepted by the assembler. |
| `printStrLn` | `printStrLn label` | `addi a7, x0, 1002` + `la a0, label` + `ecall` | Prints string and appends newline. |
| `read` | `read label` | `addi a7, x0, 1003` + `la a0, label` + `ecall` | Reads a full line into memory at `label` (NUL-terminated). |
| `readByte` | `readByte label` | `addi a7, x0, 1010` + `la a0, label` + `ecall` | Stores 1 byte at `label`. |
| `readHalf` | `readHalf label` | `addi a7, x0, 1011` + `la a0, label` + `ecall` | Stores 2 bytes (little-endian) at `label`. |
| `readWord` | `readWord label` | `addi a7, x0, 1012` + `la a0, label` + `ecall` | Stores 4 bytes (little-endian) at `label`. |

> `jal` and `jalr` are real ISA instructions (not pseudos) and are also supported directly.
> `jal` accepts both `jal label` (implicit `rd=ra`) and `jal rd, label`.

If you want to inspect the final expansion in practice, assemble with `cargo run` and check the decoded instruction trace in the RAVEN run view.

## Syscalls you can try

RAVEN supports a tiny Linux-style ABI plus a few RAVEN-only teaching extensions.

### ABI (Linux-style)

- `a7` (`x17`): syscall number
- `a0..a5` (`x10..x15`): arguments
- return value in `a0` (`x10`) (negative values mean `-errno`, represented as `i32 as u32`)

### How to use syscalls (mini tutorial)

1) Put the syscall number in `a7`.
2) Put the arguments in `a0..a5`.
3) Execute `ecall`.
4) Read the return value from `a0`.

RAVEN currently supports a very small set. When a syscall is not implemented, RAVEN stops execution and prints a message in the
console so the failure is obvious during teaching.

#### Return values and errors

- On success, the syscall returns a non-negative value in `a0` (for example, bytes read/written).
- On error, RAVEN uses Linux-style `-errno` in `a0`. Internally that is stored as `u32` (because registers are `u32`):
  `a0 = (-(errno as i32)) as u32`.

### Linux syscalls (supported subset)

| `a7` | Name | Notes |
| --- | --- | --- |
| `63` | `read` | `a0=fd`, `a1=buf`, `a2=count` (fd=0 only). Reads bytes (line-based, appends `\n`). |
| `64` | `write` | `a0=fd`, `a1=buf`, `a2=count` (fd=1/2). Writes `count` bytes from memory. |
| `278` | `getrandom` | `a0=buf`, `a1=buflen`, `a2=flags` (flags bits `0x1/0x2` accepted). Fills memory with random bytes. |
| `93` | `exit` | `a0=status`. Stops execution. |
| `94` | `exit_group` | Same as `exit` for now. |

#### Linux `write(64)`

Arguments:

- `a0 = fd` (RAVEN supports `1` (stdout) and `2` (stderr), both show up in the console for now)
- `a1 = buf` (pointer to bytes in memory)
- `a2 = count` (how many bytes to write)

Return:

- `a0 = bytes_written` or `-errno`

Short example (prints "Hello!\n" and exits):

```asm
.data
msg: .ascii "Hello!"
.byte 10          # '\n' (RAVEN does not parse escape sequences inside .ascii/.asciz)

.text
    li a0, 1       # fd=stdout
    la a1, msg     # buf
    li a2, 7       # count
    li a7, 64      # write
    ecall

    li a0, 0
    li a7, 93      # exit
    ecall
```

#### Linux `read(63)`

Arguments:

- `a0 = fd` (RAVEN supports `0` only: stdin)
- `a1 = buf` (pointer where bytes will be written)
- `a2 = count` (maximum bytes to read)

Return:

- `a0 = bytes_read` or `-errno`

Important notes (teaching simplifications):

- Input is line-based via the UI console. When a line is available, RAVEN appends a final `\n` and serves it as bytes.
- If there is no input yet, RAVEN pauses execution (PC does not advance) and waits for the user to provide input in the UI.

Short example (read and echo back):

```asm
.data
buf: .space 64

.text
    li a0, 0       # fd=stdin
    la a1, buf
    li a2, 64
    li a7, 63      # read
    ecall

    mv t0, a0      # n = bytes_read
    li a0, 1       # fd=stdout
    la a1, buf
    mv a2, t0
    li a7, 64      # write
    ecall

    li a0, 0
    li a7, 93
    ecall
```

#### Linux `exit(93)` / `exit_group(94)`

Arguments:

- `a0 = status` (exit code)

Effect:

- Stops the VM “normally” (this is not a fault in the UI).

### RAVEN extensions (used by pseudos)

| `a7` | Name | Used by |
| --- | --- | --- |
| `1000` | `raven_print_int` | `print rd` |
| `1001` | `raven_print_zstr` | `printStr` / `printString` |
| `1002` | `raven_print_zstr_ln` | `printStrLn` |
| `1003` | `raven_read_line_z` | `read label` |
| `1010` | `raven_read_u8` | `readByte label` |
| `1011` | `raven_read_u16` | `readHalf label` |
| `1012` | `raven_read_u32` | `readWord label` |

For the RAVEN `read*` extensions, RAVEN keeps asking until a valid value fits in the requested size. Invalid input results in a friendly
error message and the program counter does **not** advance, making it clear that execution is paused.

Ready for a deeper dive? Open the in-app `[?]` tutorial for hands-on assembly walkthroughs, or explore the sample programs in
`Program Examples/` to see how these encodings look in real projects.
