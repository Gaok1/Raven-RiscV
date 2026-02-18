# Falcon ASM format guide — RV32I in plain language

This is the companion reference for Falcon ASM, our teaching-friendly RISC-V emulator. Use it as a cheat sheet.

## How to read this document

- Every section keeps the tables you need, followed by a short explanation in everyday language.
- Head straight to [Encoding cheat sheets](#encoding-cheat-sheets) when you only need bit layouts.
- New to the assembler directives? Jump to [Assembler behaviour and pseudo-instructions](#assembler-behaviour-and-pseudo-instructions).
- Prefer learning by doing? Pair this guide with the [Tutorial](Tutorial.md) and try the snippets as you read.

## Architecture snapshot

Falcon focuses on a approachable RV32I+M subset so you can reason about each pipeline stage without being buried in extras.

- **Word size:** 32 bits.
- **Endianness:** little-endian throughout (`{to,from}_le_bytes`).
- **Program counter:** advances by 4 each instruction; branches and jumps are PC-relative.
- **Registers:** hardware names `x0…x31` with the usual aliases `zero`, `ra`, `sp`, `gp`, `tp`, `t0…t6`, `s0/fp`, `s1`, `a0…a7`,
  `s2…s11`. Writes to `x0/zero` are ignored.

Not yet implemented: CSR/FENCE instructions and any floating-point extension. Keeping the surface area small makes Falcon easier to
use in class or during workshops.

## Instruction set inside Falcon

| Category | Instructions |
| --- | --- |
| R-type | `ADD`, `SUB`, `AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU`, `MUL`, `MULH`, `MULHSU`, `MULHU`, `DIV`, `DIVU`, `REM`, `REMU` |
| I-type (OP-IMM) | `ADDI`, `ANDI`, `ORI`, `XORI`, `SLTI`, `SLTIU`, `SLLI`, `SRLI`, `SRAI` |
| Loads | `LB`, `LH`, `LW`, `LBU`, `LHU` |
| Stores | `SB`, `SH`, `SW` |
| Branches | `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU` |
| Upper / jumps | `LUI`, `AUIPC`, `JAL`, `JALR` |
| System | `ECALL`, `HALT` |

Division by zero is treated as a teaching moment: `DIV`, `DIVU`, `REM`, and `REMU` halt the emulator with a descriptive error instead
of following the architected “divide-by-zero” results. The interruption makes it obvious something unexpected happened.

## Encoding cheat sheets

The tables below show all 32-bit layouts Falcon uses. When an instruction name appears in bold, read the note beneath the table for
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

**System (`0x73`):** Falcon implements two encodings: `ECALL` (`0x00000073`) and `HALT` (`0x00100073`).

## Assembler behaviour and pseudo-instructions

Falcon’s assembler is intentionally lightweight so you can follow every step:

- Comments begin with `;` or `#`.
- Instructions follow the `mnemonic rd, rs1, rs2` style with commas as separators.
- Text, data, and BSS segments are all supported (`.text`, `.data`, `.bss`) along with directives such as `.word`, `.byte`, `.ascii`,
  `.asciiz`, and `.space`.
- Common pseudo-instructions expand to real RV32I encodings: `nop`, `mv`, `li` (12-bit immediates), `subi`, `j`/`jal`/`call`,
  `jr`/`ret`, `la`, `push`, `pop`, `print`, `printStr`, `printStrLn`, `read`, `readByte`, `readHalf`, `readWord`.

If you ever wonder how a pseudo expands, assemble with `cargo run` and peek at the instruction trace; Falcon prints every decoded
instruction so you can see the real opcodes.

## Syscalls you can try

All syscalls use `a7` to choose the service and read/write data through the usual argument registers.

| `a7` value | Behaviour |
| --- | --- |
| `1` | `print rd`: prints the value in register `rd` (`a0` carries the register number). |
| `2` | `printStr label`: prints a NUL-terminated string without a trailing newline. |
| `3` | `read label`: reads a full line from stdin and stores it at `label` (NUL-terminated). |
| `4` | `printStrLn label`: prints a NUL-terminated string followed by a newline. |
| `64` | `readByte label`: reads an unsigned decimal or `0x`-prefixed hex number and stores one byte. |
| `65` | `readHalf label`: same as above, storing two bytes (little-endian). |
| `66` | `readWord label`: same parsing, storing four bytes (little-endian). |

For the read variants, Falcon keeps asking until a valid value fits in the requested size. Invalid input results in a friendly error
message and the program counter does **not** advance, making it clear that execution is paused.

Ready for a deeper dive? Revisit the [Tutorial](Tutorial.md) for hands-on assembly walkthroughs, or explore the sample programs in
`Program Examples/` to see how these encodings look in real projects.
