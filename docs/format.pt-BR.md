# Guia de formato do Falcon ASM — RV32I em linguagem direta

Este é o guia de bolso do Falcon ASM, nosso emulador RISC-V pensado para ensino. Use-o como referência rápida.

## Como aproveitar este documento

- Cada seção mantém as tabelas essenciais, acompanhadas de explicações em português claro.
- Vá direto para [Tabelas de codificação](#tabelas-de-codificacao) quando só precisar rever o arranjo dos bits.
- Quer entender as diretivas do assembler? Pule para [Comportamento do assembler e pseudoinstruções](#comportamento-do-assembler-e-pseudoinstrucoes).
- Prefere aprender praticando? Leia junto com o [Tutorial-pt](Tutorial-pt.md) e teste os trechos de código conforme avança.

## Visão rápida da arquitetura

O Falcon foca no subconjunto RV32I+M para que você entenda cada etapa do ciclo buscar → decodificar → executar sem distrações.

- **Tamanho da palavra:** 32 bits.
- **Endianness:** sempre little-endian (`{to,from}_le_bytes`).
- **Program counter:** avança 4 em cada instrução; desvios e saltos usam deslocamentos relativos ao PC.
- **Registradores:** nomes `x0…x31` com os apelidos tradicionais `zero`, `ra`, `sp`, `gp`, `tp`, `t0…t6`, `s0/fp`, `s1`, `a0…a7`,
  `s2…s11`. Escritas em `x0/zero` são descartadas.

Ainda não implementados: instruções CSR/FENCE e qualquer extensão de ponto flutuante. Manter o escopo pequeno torna o Falcon mais
amigável para aulas e oficinas.

## Conjunto de instruções presente no Falcon

| Categoria | Instruções |
| --- | --- |
| Tipo R | `ADD`, `SUB`, `AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU`, `MUL`, `MULH`, `MULHSU`, `MULHU`, `DIV`, `DIVU`, `REM`, `REMU` |
| Tipo I (OP-IMM) | `ADDI`, `ANDI`, `ORI`, `XORI`, `SLTI`, `SLTIU`, `SLLI`, `SRLI`, `SRAI` |
| Loads | `LB`, `LH`, `LW`, `LBU`, `LHU` |
| Stores | `SB`, `SH`, `SW` |
| Branches | `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU` |
| Superiores / saltos | `LUI`, `AUIPC`, `JAL`, `JALR` |
| Sistema | `ECALL`, `HALT` |

Divisão por zero vira oportunidade de aprendizado: `DIV`, `DIVU`, `REM` e `REMU` encerram o emulador com uma mensagem clara em vez
do resultado “arquitetado”. A interrupção evidencia que algo inesperado ocorreu.

<a id="tabelas-de-codificacao"></a>
## Tabelas de codificação

As tabelas abaixo mostram todos os layouts de 32 bits usados no Falcon. Sempre que aparecer uma observação, ela lembra o alcance
do imediato ou algum detalhe importante.

### Tipo R (aritmética, lógica, multiplicação/divisão)

| Campo  | Bits | Descrição |
| --- | --- | --- |
| opcode | [6:0] | família da instrução |
| rd     | [11:7] | registrador destino |
| funct3 | [14:12] | subtipo |
| rs1    | [19:15] | registrador fonte 1 |
| rs2    | [24:20] | registrador fonte 2 |
| funct7 | [31:25] | subtipo extra / seletor de MUL/DIV |

### Tipo I (OP-IMM, loads, `JALR`)

| Campo | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| imm[11:0] | [31:20] |

- Imediatos com sinal, variando de -2048 a 2047.
- Shifts usam `shamt` nos bits [24:20] com `funct7 = 0x00` (`SLLI`, `SRLI`) ou `0x20` (`SRAI`).

### Tipo S (stores)

| Campo | Bits |
| --- | --- |
| opcode | [6:0] |
| imm[4:0] | [11:7] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| rs2 | [24:20] |
| imm[11:5] | [31:25] |

### Tipo B (branches condicionais)

| Campo | Bits |
| --- | --- |
| opcode | [6:0] |
| imm[11] | [7] |
| imm[4:1] | [11:8] |
| funct3 | [14:12] |
| rs1 | [19:15] |
| rs2 | [24:20] |
| imm[10:5] | [30:25] |
| imm[12] | [31] |

- O assembler preenche um imediato de 13 bits (em bytes). O bit 0 é sempre zero, pois o alvo precisa estar alinhado em palavras.

### Tipo U (`LUI`, `AUIPC`)

| Campo | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| imm[31:12] | [31:12] |

### Tipo J (`JAL`)

| Campo | Bits |
| --- | --- |
| opcode | [6:0] |
| rd | [11:7] |
| imm[19:12] | [19:12] |
| imm[11] | [20] |
| imm[10:1] | [30:21] |
| imm[20] | [31] |

- Imediato de 21 bits armazenado em bytes; o bit 0 fica em zero porque os alvos também são alinhados em palavras.

## Referência de opcode e funct

| Uso | Valor |
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

### `funct3` e `funct7`

**Tipo R (`0x33`):**

- `funct3 = 0x0`: `ADD` (`funct7=0x00`), `SUB` (`funct7=0x20`)
- `funct3 = 0x1`: `SLL`
- `funct3 = 0x2`: `SLT`
- `funct3 = 0x3`: `SLTU`
- `funct3 = 0x4`: `XOR`
- `funct3 = 0x5`: `SRL` (`funct7=0x00`), `SRA` (`funct7=0x20`)
- `funct3 = 0x6`: `OR`
- `funct3 = 0x7`: `AND`
- Multiplicação/divisão usam `funct7 = 0x01` com os mesmos `funct3`: `MUL`, `MULH`, `MULHSU`, `MULHU`, `DIV`, `DIVU`, `REM`, `REMU`.

**Tipo I OP-IMM (`0x13`):**

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

**`JALR` (`0x67`):** usa `funct3 = 0x0`.

**System (`0x73`):** o Falcon implementa dois códigos: `ECALL` (`0x00000073`) e `HALT` (`0x00100073`).

<a id="comportamento-do-assembler-e-pseudoinstrucoes"></a>
## Comportamento do assembler e pseudoinstruções

O assembler do Falcon é propositalmente simples para que você consiga acompanhar cada etapa:

- Comentários começam com `;` ou `#`.
- Operandos são separados por vírgula (`mnemonic op1, op2, ...`).
- Seções/diretivas suportadas incluem `.text`, `.data`, `.bss`, `.section`, `.word`, `.byte`, `.half`, `.ascii`, `.asciz`/`.asciiz`, `.space` e `.align`.

### Referência de pseudoinstruções

A tabela abaixo documenta os formatos aceitos no Falcon e a forma exata (conceitual) de expansão usada pelo assembler.

| Pseudo | Formato aceito | Expansão (conceitual) | Observações |
| --- | --- | --- | --- |
| `nop` | `nop` | `addi x0, x0, 0` | Não aceita operandos. |
| `mv` | `mv rd, rs` | `addi rd, rs, 0` | Cópia entre registradores. |
| `li` | `li rd, imm` | `addi rd, x0, imm` | Imediato deve caber em 12 bits com sinal (`-2048..2047`). |
| `subi` | `subi rd, rs1, imm` | `addi rd, rs1, -imm` | `-imm` deve caber em 12 bits com sinal. |
| `j` | `j label_ou_imm` | `jal x0, label_ou_imm` | Imediato PC-relativo de 21 bits (bit 0 precisa ser par). |
| `call` | `call label_ou_imm` | `jal ra, label_ou_imm` | Salva endereço de retorno em `ra` (`x1`). |
| `jr` | `jr rs1` | `jalr x0, rs1, 0` | Salto indireto sem link. |
| `ret` | `ret` | `jalr x0, ra, 0` | Retorna para o endereço em `ra` (`x1`). |
| `la` | `la rd, label` | `lui rd, hi(label)` + `addi rd, rd, lo(label)` | Usa divisão hi/lo com arredondamento (`+0x800`) para caber em 12 bits com sinal no low. |
| `push` | `push rs` | `addi sp, sp, -4` + `sw rs, 4(sp)` | Guarda o valor de `rs` na pilha em `sp` e subtrai `sp` em 4|
| `pop` | `pop rd` | `lw rd, 4(sp)` + `addi sp, sp, 4` | Aumenta `sp` em 4 e guarda o valor em `rd`|
| `print` | `print rd` | `addi a7, x0, 1` + `addi a0, rd, 0` + `ecall` | Imprime valor de registrador. |
| `printStr` | `printStr label` | `addi a7, x0, 2` + `la a0, label` + `ecall` | Imprime string NUL-terminada (sem quebra de linha). |
| `printString` | `printString label` | igual a `printStr label` | Alias legado aceito pelo assembler. |
| `printStrLn` | `printStrLn label` | `addi a7, x0, 4` + `la a0, label` + `ecall` | Imprime string e adiciona quebra de linha. |
| `read` | `read label` | `addi a7, x0, 3` + `la a0, label` + `ecall` | Lê uma linha inteira para memória em `label` (NUL-terminada). |
| `readByte` | `readByte label` | `addi a7, x0, 64` + `la a0, label` + `ecall` | Grava 1 byte em `label`. |
| `readHalf` | `readHalf label` | `addi a7, x0, 65` + `la a0, label` + `ecall` | Grava 2 bytes (little-endian) em `label`. |
| `readWord` | `readWord label` | `addi a7, x0, 66` + `la a0, label` + `ecall` | Grava 4 bytes (little-endian) em `label`. |

> `jal` e `jalr` são instruções reais da ISA (não pseudo) e também são suportadas diretamente.
> Em `jal`, você pode usar `jal label` (com `rd=ra` implícito) ou `jal rd, label`.

Para observar a expansão final em execução, rode `cargo run`, monte o programa e acompanhe o traço de instruções decodificadas na interface.

## Syscalls disponíveis

Todas as chamadas usam `a7` para selecionar o serviço e os registradores de argumento padrão para dados.

| Valor em `a7` | Comportamento |
| --- | --- |
| `1` | `print rd`: imprime o valor do registrador `rd` (o número do registrador vem em `a0`). |
| `2` | `printStr label`: imprime uma string NUL-terminada sem quebra de linha. |
| `3` | `read label`: lê uma linha da entrada padrão e armazena em `label` (com NUL ao final). |
| `4` | `printStrLn label`: imprime a string NUL-terminada e adiciona uma quebra de linha. |
| `64` | `readByte label`: aceita número decimal ou hexadecimal (`0x`) e grava um byte. |
| `65` | `readHalf label`: mesma leitura, gravando dois bytes (little-endian). |
| `66` | `readWord label`: idem, armazenando quatro bytes (little-endian). |

Nos modos de leitura, o Falcon insiste até receber um valor válido para o tamanho pedido. Entradas inválidas geram uma mensagem
amigável e o PC **não** avança, destacando que a execução está em pausa.

Pronto para ir além? Retorne ao [Tutorial-pt](Tutorial-pt.md) para ver exemplos guiados ou explore `Program Examples/` e enxergue
essas codificações em programas reais.
