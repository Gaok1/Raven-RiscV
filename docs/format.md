# Falcon ASM – Encoding and ISA Reference (RV32I)

This document describes what is implemented in **Falcon ASM**, an educational RISC-V emulator. It covers:

- instruction formats and bit fields;
- opcodes, `funct3` and `funct7` used;
- immediate ranges and alignment requirements;
- rules of the text assembler, including labels, segments and pseudo-instructions.

## Current State

Supports the essential subset of **RV32I**:

- **R-type:** `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- **I-type (OP-IMM):** `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- **Loads:** `LB, LH, LW, LBU, LHU`
- **Stores:** `SB, SH, SW`
- **Branches:** `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- **U/J:** `LUI, AUIPC, JAL`
- **JALR**
- **SYSTEM:** `ECALL`, `EBREAK` (interpreted as HALT)

*Not implemented:* FENCE/CSR instructions and floating point.

## Word Size, Endianness and PC

- **Word:** 32 bits
- **Endianness:** little-endian (`{to,from}_le_bytes`)
- **PC:** advances **+4** per instruction. Branches and jumps use offsets relative to the instruction address.

## Registers

- Registers `x0..x31`; writes to `x0` are ignored.
- Assembler aliases: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`.

## 🧾 Instruction Formats (32 bits)

### General example (R-type)

| Field   | Bits  | Description                     |
|--------|-------|---------------------------------|
| opcode | [6:0] | main opcode                     |
| rd     | [11:7]| destination register            |
| funct3 |[14:12]| subtype                         |
| rs1    |[19:15]| source register 1               |
| rs2    |[24:20]| source register 2               |
| funct7 |[31:25]| additional subtype              |

Other formats (I, S, B, U, J) rearrange fields and immediates.

### I-type (OP-IMM, LOADs and JALR)

| Field     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        | [11:7]|
| funct3    |[14:12]|
| rs1       |[19:15]|
| imm[11:0] |[31:20]|

- 12-bit signed immediates (-2048..2047)
- Shifts (`SLLI/SRLI/SRAI`) use `shamt` in [24:20] and `funct7` = `0x00` (`SLLI/SRLI`) or `0x20` (`SRAI`).

### S-type (Stores)

| Field     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| imm[4:0]  | [11:7]|
| funct3    |[14:12]|
| rs1       |[19:15]|
| rs2       |[24:20]|
| imm[11:5] |[31:25]|

### B-type (Branches)

| Field     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| imm[11]   | [7]   |
| imm[4:1]  |[11:8] |
| funct3    |[14:12]|
| rs1       |[19:15]|
| rs2       |[24:20]|
| imm[10:5] |[30:25]|
| imm[12]   |[31]  |

- 13-bit immediates (bytes) with **bit0 = 0**. The assembler computes `target_pc - instruction_pc`.

### U-type (LUI/AUIPC)

| Field     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        |[11:7]|
| imm[31:12]|[31:12]|

### J-type (JAL)

| Field     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        |[11:7]|
| imm[19:12]|[19:12]|
| imm[11]   | [20] |
| imm[10:1] |[30:21]|
| imm[20]   | [31] |

- 21-bit immediates (bytes) with **bit0 = 0**. The assembler computes the relative offset.

## 🔢 Opcodes by type

- `OPC_RTYPE = 0x33`
- `OPC_OPIMM = 0x13`
- `OPC_LOAD  = 0x03`
- `OPC_STORE = 0x23`
- `OPC_BRANCH= 0x63`
- `OPC_LUI   = 0x37`
- `OPC_AUIPC = 0x17`
- `OPC_JAL   = 0x6F`
- `OPC_JALR  = 0x67`
- `OPC_SYSTEM= 0x73`

## FUNCT3/FUNCT7

### R-type (opcode 0x33)

- `0x0`: `ADD` (`funct7=0x00`), `SUB` (`funct7=0x20`)
- `0x1`: `SLL`
- `0x4`: `XOR`
- `0x5`: `SRL` (`0x00`), `SRA` (`0x20`)
- `0x6`: `OR`
- `0x7`: `AND`

### I-type OP-IMM (opcode 0x13)

- `0x0`: `ADDI`
- `0x4`: `XORI`
- `0x6`: `ORI`
- `0x7`: `ANDI`
- `0x1`: `SLLI`
- `0x5`: `SRLI` (`0x00`) / `SRAI` (`0x20`)

### LOADs (opcode 0x03)

- `0x0`: `LB`
- `0x1`: `LH`
- `0x2`: `LW`
- `0x4`: `LBU`
- `0x5`: `LHU`

### STOREs (opcode 0x23)

- `0x0`: `SB`
- `0x1`: `SH`
- `0x2`: `SW`

### BRANCH (opcode 0x63)

- `0x0`: `BEQ`
- `0x1`: `BNE`
- `0x4`: `BLT`
- `0x5`: `BGE`
- `0x6`: `BLTU`
- `0x7`: `BGEU`

### JALR (opcode 0x67)

- `funct3 = 0x0`

### SYSTEM (opcode 0x73)

- `ECALL` (`0x00000073`) and `EBREAK` (`0x00100073`) halt execution.

## Assembler Rules

- **Two passes**: the first collects labels (`label:`); the second resolves and encodes.
- **Comments**: anything after `;` or `#` is ignored.
- **Separator**: `instr op1, op2, op3`.
- **Segment directives**:
  - `.text` starts the code section.
  - `.data` starts the data section (allocated from `base_pc + 0x1000`).
  - Inside `.data`:
    - `.byte` inserts 8-bit values.
    - `.word` inserts 32-bit words.
- **Loads/Stores**: syntax `imm(rs1)`.
- **Branches/Jumps**: operand may be immediate or label. Offsets are byte-based; `B`/`J` require multiples of 2.
- **Pseudo-instructions**:
  - `nop` → `addi x0, x0, 0`
  - `mv rd, rs` → `addi rd, rs, 0`
  - `li rd, imm12` → `addi rd, x0, imm`
  - `subi rd, rs1, imm` → `addi rd, rs1, -imm`
  - `j label` → `jal x0, label`
  - `jr rs1` → `jalr x0, rs1, 0`
  - `ret` → `jalr x0, ra, 0`
  - `la rd, label` → emits `lui`/`addi` to load a data address

## Example Code

```asm
.data
val: .word 0
.text
  la t0, val
  addi t1, x0, 5
  sw t1, 0(t0)
  ecall
```

This program loads the address of `val`, stores the number 5 in memory and calls `ecall`.
