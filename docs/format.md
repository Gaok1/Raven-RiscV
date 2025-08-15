
# Falcon ASM – Encoding & ISA Reference (based on RISC-V RV32I)

Falcon ASM is an educational RISC-V emulator. This document describes **what is currently implemented in the project**, including:
- instruction formats,
- opcodes/funct3/funct7 used,
- immediate ranges and alignments,
- and the rules for the text assembler → bytes (with labels and MVP pseudoinstructions).

> **Current state (MVP):** implements **essential RV32I**:
> - R-type: `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
> - I-type (OP-IMM): `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
> - Loads: `LB, LH, LW, LBU, LHU`
> - Stores: `SB, SH, SW`
> - Branches: `BEQ, BNE, BLT, BGE, BLTU, BGEU`
> - U/J: `LUI, AUIPC, JAL`
> - JALR
> - SYSTEM: `ECALL`, `EBREAK` (treated as HALT in MVP)
>
> **Not yet implemented:** FENCE/CSR, FP.

---

## 🧱 Word size, endianness, and PC

- **Word size:** 32 bits.
- **Endianness:** **little-endian** (load/store use `{to,from}_le_bytes`).
- **PC:** advances **+4** per instruction (32-bit aligned). Branches/Jumps use **offset relative to the instruction address** (see below).

---

## 🧠 Registers (aliases accepted by the assembler)

- **x0..x31** (x0 is always zero; writes to x0 are ignored).
- Accepted aliases:  
  `zero=x0, ra=x1, sp=x2, gp=x3, tp=x4, t0=x5..t6=x7/x28..x31, s0/fp=x8, s1=x9, a0=x10..a7=x17, s2=x18..s11=x27`.

---

## 🧾 Instruction formats (32 bits)

### General example (R-type)

| Field    | Bits   | Description                                   |
|----------|--------|-----------------------------------------------|
| opcode   | [6:0]  | primary opcode (7 bits)                       |
| rd       | [11:7] | destination register (5 bits)                 |
| funct3   | [14:12]| subtype (3 bits)                              |
| rs1      | [19:15]| source register 1 (5 bits)                    |
| rs2      | [24:20]| source register 2 (5 bits)                    |
| funct7   | [31:25]| additional subtype (7 bits)                   |

> Other formats (I, S, B, U, J) rearrange fields and/or immediates.

### I-type (OP-IMM and LOADs/JALR)

| Field       | Bits   |
|-------------|--------|
| opcode      | [6:0]  |
| rd          | [11:7] |
| funct3      | [14:12]|
| rs1         | [19:15]|
| imm[11:0]   | [31:20]|

- `ADDI/ANDI/ORI/XORI/Loads/JALR` use **signed imm12** (-2048..2047).
- **Shift immediates (`SLLI/SRLI/SRAI`)**: use I-type bit layout extended with:
  - `shamt` in **[24:20]**,  
  - `funct7` in **[31:25] = 0x00** for `SLLI/SRLI`, **0x20** for `SRAI`,  
  - `funct3` = `0x1` (SLLI) / `0x5` (SRLI/SRAI).  
  > In the encoder, we reuse the “R-like” packing for these I-type shifts because the bit layout in the middle is identical.

### S-type (Stores)

| Field       | Bits   |
|-------------|--------|
| opcode      | [6:0]  |
| imm[4:0]    | [11:7] |
| funct3      | [14:12]|
| rs1         | [19:15]|
| rs2         | [24:20]|
| imm[11:5]   | [31:25]|

- Signed **imm12** (-2048..2047) is split into **[11:5]** and **[4:0]**.

### B-type (Branches)

| Field        | Bits   |
|--------------|--------|
| opcode       | [6:0]  |
| imm[11]      | [7]    |
| imm[4:1]     | [11:8] |
| funct3       | [14:12]|
| rs1          | [19:15]|
| rs2          | [24:20]|
| imm[10:5]    | [30:25]|
| imm[12]      | [31]   |

- **B immediates** are **signed 13 bits** representing **bytes** with **bit 0 = 0** (multiple of 2).  
  Recombine: `imm = sign( imm[12]|imm[10:5]|imm[4:1]|imm[11] ) << 1`.
- The assembler computes `imm = target_pc - instruction_pc`.

### U-type (LUI/AUIPC)

| Field      | Bits   |
|------------|--------|
| opcode     | [6:0]  |
| rd         | [11:7] |
| imm[31:12] | [31:12]|

- Uses the **upper 20 bits** already aligned (bits [31:12] of the desired result).

### J-type (JAL)

| Field      | Bits   |
|------------|--------|
| opcode     | [6:0]  |
| rd         | [11:7] |
| imm[19:12] | [19:12]|
| imm[11]    | [20]   |
| imm[10:1]  | [30:21]|
| imm[20]    | [31]   |

- **J immediates** are **signed 21 bits** in **bytes** with **bit 0 = 0** (multiple of 2).  
  Recombine: `imm = sign( imm[20]|imm[10:1]|imm[11]|imm[19:12] ) << 1`.
- The assembler computes `imm = target_pc - instruction_pc`.

---

## 🔢 Opcodes by type (Falcon values)

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

---

## 🧩 FUNCT3 / FUNCT7 (as implemented)

### R-type (opcode 0x33)
- `funct3`:
  - `0x0`: `ADD` (`funct7=0x00`), `SUB` (`funct7=0x20`)
  - `0x1`: `SLL`  (`funct7=0x00`)
  - `0x4`: `XOR`  (`funct7=0x00`)
  - `0x5`: `SRL`  (`funct7=0x00`), `SRA` (`funct7=0x20`)
  - `0x6`: `OR`   (`funct7=0x00`)
  - `0x7`: `AND`  (`funct7=0x00`)

### I-type OP-IMM (opcode 0x13)
- `funct3`:
  - `0x0`: `ADDI`
  - `0x4`: `XORI`
  - `0x6`: `ORI`
  - `0x7`: `ANDI`
  - `0x1`: `SLLI`  (uses `funct7=0x00`, `shamt` in [24:20])
  - `0x5`: `SRLI`  (`funct7=0x00`) / `SRAI` (`funct7=0x20`), `shamt` in [24:20]

### LOADs (opcode 0x03)
- `funct3`:
  - `0x0`: `LB`
  - `0x1`: `LH`
  - `0x2`: `LW`
  - `0x4`: `LBU`
  - `0x5`: `LHU`

### STOREs (opcode 0x23)
- `funct3`:
  - `0x0`: `SB`
  - `0x1`: `SH`
  - `0x2`: `SW`

### BRANCH (opcode 0x63)
- `funct3`:
  - `0x0`: `BEQ`
  - `0x1`: `BNE`
  - `0x4`: `BLT`
  - `0x5`: `BGE`
  - `0x6`: `BLTU`
  - `0x7`: `BGEU`

### JALR (opcode 0x67)
- `funct3 = 0x0` (always)

### SYSTEM (opcode 0x73)
- MVP treats `ECALL` (`0x00000073`) and `EBREAK` (`0x00100073`) as HALT.
- CSR/FENCE not implemented yet.

---

## 🛠️ Assembler Text Rules (what the project currently accepts)

- **Two passes**: 1st collects labels `label:`, 2nd resolves labels and assembles.
- **Comments**: anything after `;` or `#` is ignored.
- **Separator**: `instruction op1, op2, op3`.
- **Registers**: `xN` or aliases listed above.
- **Loads/Stores**: syntax `imm(rs1)`, e.g. `lw x1, 0(x2)`; `sw x3, 4(x5)`.
- **Branches/Jumps**: operand can be **immediate** or **label**.  
  The assembler computes `imm = target_pc - instruction_pc` (in **bytes**).  
  **B/J require `imm % 2 == 0`** (encoder validates).
- **Pseudoinstructions implemented**:
  - `nop` → `addi x0, x0, 0`
  - `mv rd, rs` → `addi rd, rs, 0`
  - `li rd, imm12` → `addi rd, x0, imm` (**only** if `imm` ∈ [-2048, 2047]; for larger values, use `lui+addi`)
  - `j label` → `jal x0, label`
  - `jr rs1` → `jalr x0, rs1, 0`
  - `ret` → `jalr x0, ra, 0` (ra=x1)
  - `subi rd, rs1, imm` → `addi rd, rs1, -imm`

---

## ✅ Quick examples

### 1) Simple program
```asm
addi x1, x0, 5
addi x2, x0, 7
loop:
  add  x3, x1, x2
  beq  x3, x0, loop
  ecall

Expected encoding (little-endian per word):

addi x1,x0,5 → 0x0050_0093

addi x2,x0,7 → 0x0070_0113

add x3,x1,x2 → 0x0020_81b3

ecall → 0x0000_0073

In the emulator: x3 = 12 at the end.
```