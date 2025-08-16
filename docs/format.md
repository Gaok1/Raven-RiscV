# Falcon ASM – Referência de Codificação e ISA (RV32I)

Este documento descreve o que está implementado no **Falcon ASM**, um emulador
educacional de RISC-V. Aqui estão documentados:

- formatos de instrução e campos de bits;
- opcodes, `funct3` e `funct7` usados;
- faixas de imediatos e requisitos de alinhamento;
- regras do montador textual, incluindo rótulos, segmentos e pseudoinstruções.

## Estado atual

Suporte ao subconjunto essencial do **RV32I**:

- **R-type:** `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- **I-type (OP-IMM):** `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- **Loads:** `LB, LH, LW, LBU, LHU`
- **Stores:** `SB, SH, SW`
- **Branches:** `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- **U/J:** `LUI, AUIPC, JAL`
- **JALR`
- **SYSTEM:** `ECALL`, `EBREAK` (interpretados como HALT)

*Não implementados:* instruções FENCE/CSR e ponto flutuante.

## 🧱 Tamanho de palavra, endianness e PC

- **Palavra:** 32 bits
- **Endianness:** little-endian (`{to,from}_le_bytes`)
- **PC:** avança **+4** por instrução. Branches e jumps usam deslocamento relativo ao endereço da instrução.

## 🧠 Registradores

- Registradores `x0..x31`; gravações em `x0` são ignoradas.
- Aliases aceitos pelo montador: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`.

## 🧾 Formatos de instrução (32 bits)

### Exemplo geral (R-type)

| Campo   | Bits  | Descrição                            |
|---------|-------|--------------------------------------|
| opcode  | [6:0] | opcode principal                     |
| rd      | [11:7]| registrador destino                  |
| funct3  | [14:12]| subtipo                             |
| rs1     | [19:15]| registrador fonte 1                 |
| rs2     | [24:20]| registrador fonte 2                 |
| funct7  | [31:25]| subtipo adicional                   |

Outros formatos (I, S, B, U, J) reorganizam campos e imediatos.

### I-type (OP-IMM, LOADs e JALR)

| Campo      | Bits  |
|------------|-------|
| opcode     | [6:0] |
| rd         | [11:7]|
| funct3     | [14:12]|
| rs1        | [19:15]|
| imm[11:0]  | [31:20]|

- Imediatos de 12 bits com sinal (-2048..2047)
- Shifts (`SLLI/SRLI/SRAI`) usam `shamt` em [24:20] e `funct7` = `0x00` (`SLLI/SRLI`) ou `0x20` (`SRAI`).

### S-type (Stores)

| Campo      | Bits  |
|------------|-------|
| opcode     | [6:0] |
| imm[4:0]   | [11:7]|
| funct3     | [14:12]|
| rs1        | [19:15]|
| rs2        | [24:20]|
| imm[11:5]  | [31:25]|

### B-type (Branches)

| Campo      | Bits  |
|------------|-------|
| opcode     | [6:0] |
| imm[11]    | [7]   |
| imm[4:1]   | [11:8]|
| funct3     | [14:12]|
| rs1        | [19:15]|
| rs2        | [24:20]|
| imm[10:5]  | [30:25]|
| imm[12]    | [31]  |

- Imediatos de 13 bits (em bytes) com **bit0 = 0**. O montador calcula `target_pc - instruction_pc`.

### U-type (LUI/AUIPC)

| Campo      | Bits  |
|------------|-------|
| opcode     | [6:0] |
| rd         | [11:7]|
| imm[31:12] | [31:12]|

### J-type (JAL)

| Campo      | Bits  |
|------------|-------|
| opcode     | [6:0] |
| rd         | [11:7]|
| imm[19:12] | [19:12]|
| imm[11]    | [20]  |
| imm[10:1]  | [30:21]|
| imm[20]    | [31]  |

- Imediatos de 21 bits (bytes) com **bit0 = 0**. Montador calcula deslocamento relativo.

## 🔢 Opcodes por tipo

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

## 🧩 FUNCT3/FUNCT7

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

- `ECALL` (`0x00000073`) e `EBREAK` (`0x00100073`) terminam a execução.

## 🛠️ Regras do Montador

- **Duas passagens**: a primeira coleta rótulos (`label:`); a segunda resolve e codifica.
- **Comentários**: qualquer coisa após `;` ou `#` é ignorada.
- **Separador**: `instr op1, op2, op3`.
- **Diretivas de segmento**:
  - `.text` inicia a seção de código.
  - `.data` inicia a seção de dados (alocada a partir de `base_pc + 0x1000`).
  - Dentro de `.data`:
    - `.byte` insere valores de 8 bits.
    - `.word` insere palavras de 32 bits.
- **Loads/Stores**: sintaxe `imm(rs1)`.
- **Branches/Jumps**: operando pode ser imediato ou rótulo. Deslocamento calculado em bytes; `B`/`J` exigem múltiplos de 2.
- **Pseudoinstruções**:
  - `nop` → `addi x0, x0, 0`
  - `mv rd, rs` → `addi rd, rs, 0`
  - `li rd, imm12` → `addi rd, x0, imm`
  - `subi rd, rs1, imm` → `addi rd, rs1, -imm`
  - `j label` → `jal x0, label`
  - `jr rs1` → `jalr x0, rs1, 0`
  - `ret` → `jalr x0, ra, 0`
  - `la rd, label` → gera `lui`/`addi` para carregar o endereço de dados

## ✅ Exemplo rápido

```asm
.data
val: .word 0
.text
  la t0, val
  addi t1, x0, 5
  sw t1, 0(t0)
  ecall
```

Este programa carrega o endereço de `val`, grava o número 5 na memória e chama `ecall`.

