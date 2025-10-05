# Falcon ASM - Referência de Codificação e ISA (RV32I)

Este documento descreve o que está implementado no Falcon ASM, um emulador educacional RISC‑V. Cobre:

- formatos de instrução e campos de bits;
- opcodes, `funct3` e `funct7` usados;
- faixas de imediatos e requisitos de alinhamento;
- regras do assembler de texto, incluindo rótulos, segmentos e pseudoinstruções.

## Estado Atual

Suporta o subconjunto essencial de RV32I:

- Tipo R: `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- Tipo I (OP-IMM): `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- Loads: `LB, LH, LW, LBU, LHU`
- Stores: `SB, SH, SW`
- Branches: `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- U/J: `LUI, AUIPC, JAL`
- JALR
- SYSTEM: `ECALL`, `HALT`

Não implementado: instruções FENCE/CSR e ponto flutuante.

## Tamanho de Palavra, Endianness e PC

- Palavra: 32 bits
- Endianness: little‑endian (`{to,from}_le_bytes`)
- PC: avança +4 por instrução. Branches e jumps usam deslocamentos relativos ao endereço da instrução.

## Registradores

- Registradores `x0..x31`; escritas em `x0` são ignoradas.
- Apelidos do assembler: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`.

## Formatos de Instrução (32 bits)

Ex.: Tipo R

| Campo   | Bits  | Descrição                     |
|---------|-------|--------------------------------|
| opcode  | [6:0] | opcode principal               |
| rd      | [11:7]| registrador destino            |
| funct3  |[14:12]| subtipo                        |
| rs1     |[19:15]| registrador fonte 1            |
| rs2     |[24:20]| registrador fonte 2            |
| funct7  |[31:25]| subtipo adicional              |

Outros formatos (I, S, B, U, J) reorganizam campos e imediatos.

### Tipo I (OP-IMM, LOADs e JALR)

| Campo     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        | [11:7]|
| funct3    |[14:12]|
| rs1       |[19:15]|
| imm[11:0] |[31:20]|

- Imediatos de 12 bits com sinal (-2048..2047)
- Shifts (`SLLI/SRLI/SRAI`) usam `shamt` em [24:20] e `funct7` = `0x00` (`SLLI/SRLI`) ou `0x20` (`SRAI`).

### Tipo S (Stores)

| Campo     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| imm[4:0]  | [11:7]|
| funct3    |[14:12]|
| rs1       |[19:15]|
| rs2       |[24:20]|
| imm[11:5] |[31:25]|

### Tipo B (Branches)

| Campo     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| imm[11]   | [7]   |
| imm[4:1]  |[11:8] |
| funct3    |[14:12]|
| rs1       |[19:15]|
| rs2       |[24:20]|
| imm[10:5] |[30:25]|
| imm[12]   |[31]   |

- Imediatos de 13 bits (em bytes) com bit0 = 0. O assembler calcula `target_pc - instruction_pc`.

### Tipo U (LUI/AUIPC)

| Campo     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        |[11:7] |
| imm[31:12]|[31:12]|

### Tipo J (JAL)

| Campo     | Bits  |
|-----------|-------|
| opcode    | [6:0] |
| rd        |[11:7] |
| imm[19:12]|[19:12]|
| imm[11]   | [20]  |
| imm[10:1] |[30:21]|
| imm[20]   | [31]  |

- Imediatos de 21 bits (em bytes) com bit0 = 0. O assembler calcula o deslocamento relativo.

## Opcodes por tipo

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

Tipo R (opcode 0x33)

- `0x0`: `ADD` (`funct7=0x00`), `SUB` (`funct7=0x20`)
- `0x1`: `SLL`
- `0x4`: `XOR`
- `0x5`: `SRL` (`0x00`), `SRA` (`0x20`)
- `0x6`: `OR`
- `0x7`: `AND`

Tipo I OP-IMM (opcode 0x13)

- `0x0`: `ADDI`
- `0x4`: `XORI`
- `0x6`: `ORI`
- `0x7`: `ANDI`
- `0x1`: `SLLI`
- `0x5`: `SRLI` (`0x00`) / `SRAI` (`0x20`)

LOADs (opcode 0x03)

- `0x0`: `LB`
- `0x1`: `LH`
- `0x2`: `LW`
- `0x4`: `LBU`
- `0x5`: `LHU`

STOREs (opcode 0x23)

- `0x0`: `SB`
- `0x1`: `SH`
- `0x2`: `SW`

BRANCH (opcode 0x63)

- `0x0`: `BEQ`
- `0x1`: `BNE`
- `0x4`: `BLT`
- `0x5`: `BGE`
- `0x6`: `BLTU`
- `0x7`: `BGEU`

JALR (opcode 0x67)

- `funct3 = 0x0`

SYSTEM (opcode 0x73)

- `ECALL` (`0x00000073`) e `HALT` (`0x00100073`) encerram a execução.

## Syscalls e Pseudos

`ecall` usa `a7` para selecionar a chamada de sistema e `a0` para argumentos.

- `a7=1` — `print rd`: imprime o valor de `rd` (`a0=rd`).
- `a7=2` — `printString label`: imprime a string NUL‑terminada em `label` (`a0=addr`).
- `a7=3` — `read label`: lê uma linha para a memória em `label` e adiciona NUL.

Observação: por escolha pedagógica, `DIV/DIVU/REM/REMU` com divisor zero interrompem a execução com erro, em vez do comportamento padrão do RISC‑V. Isso é intencional para evidenciar condições de erro.

## Regras do Assembler

- Duas passagens: a primeira coleta rótulos (`label:`); a segunda resolve e codifica.
- Comentários: tudo após `;` ou `#` é ignorado.
- Separador: `instr op1, op2, op3`.
- Pseudos suportadas: `nop`, `mv`, `li` (12 bits), `subi`, `j/call`, `jr/ret`, `la`, `push/pop` (usa `4(sp)`), `print`, `printString label`, `read label`.

