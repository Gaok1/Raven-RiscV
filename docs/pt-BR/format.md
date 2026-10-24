# Guia de formato do RAVEN — RV32I em linguagem direta

Este é o guia de bolso do RAVEN, nosso emulador RISC-V pensado para ensino. Use-o como referência rápida.

## Como aproveitar este documento

- Cada seção mantém as tabelas essenciais, acompanhadas de explicações em português claro.
- Vá direto para [Tabelas de codificação](#tabelas-de-codificacao) quando só precisar rever o arranjo dos bits.
- Quer entender as diretivas do assembler? Pule para [Comportamento do assembler e pseudoinstruções](#comportamento-do-assembler-e-pseudoinstrucoes).
- Prefere aprender praticando? Use o tutorial interativo `[?]` dentro do Raven e teste os trechos de código conforme avança.

## Visão rápida da arquitetura

O RAVEN foca no subconjunto RV32IMAF para que você entenda cada etapa do ciclo buscar → decodificar → executar sem distrações.

- **Tamanho da palavra:** 32 bits.
- **Endianness:** sempre little-endian (`{to,from}_le_bytes`).
- **Program counter:** avança 4 em cada instrução; desvios e saltos usam deslocamentos relativos ao PC.
- **Registradores:** nomes `x0…x31` com os apelidos tradicionais `zero`, `ra`, `sp`, `gp`, `tp`, `t0…t6`, `s0/fp`, `s1`, `a0…a7`,
  `s2…s11`. Escritas em `x0/zero` são descartadas.

O RAVEN cobre RV32IMAF — inteiros base, multiplicação/divisão, atômicos (LR/SC + AMO) e ponto flutuante de precisão simples — e as instruções Zicsr (`csrrw`, `csrrs`, `csrrc`, as formas com imediato e as pseudos `csrr`/`csrw`). As instruções de ponto flutuante levantam as flags IEEE em `fflags`; `frm` e `fcsr` também têm nome. Toda instrução de ponto flutuante que arredonda respeita o seu modo de arredondamento, escrito como último operando opcional ou tirado do `frm`; veja [Modos de arredondamento de ponto flutuante](#modos-de-arredondamento-de-ponto-flutuante).

## Conjunto de instruções presente no RAVEN

| Categoria | Instruções |
| --- | --- |
| Tipo R | `ADD`, `SUB`, `AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`, `SLT`, `SLTU`, `MUL`, `MULH`, `MULHSU`, `MULHU`, `DIV`, `DIVU`, `REM`, `REMU` |
| Tipo I (OP-IMM) | `ADDI`, `ANDI`, `ORI`, `XORI`, `SLTI`, `SLTIU`, `SLLI`, `SRLI`, `SRAI` |
| Loads | `LB`, `LH`, `LW`, `LBU`, `LHU` |
| Stores | `SB`, `SH`, `SW` |
| Branches | `BEQ`, `BNE`, `BLT`, `BGE`, `BLTU`, `BGEU` |
| Superiores / saltos | `LUI`, `AUIPC`, `JAL`, `JALR` |
| Sistema | `ECALL`, `EBREAK` (alias: `HALT`) |

Divisão por zero vira oportunidade de aprendizado: `DIV`, `DIVU`, `REM` e `REMU` encerram o emulador com uma mensagem clara em vez
do resultado “arquitetado”. A interrupção evidencia que algo inesperado ocorreu.

<a id="tabelas-de-codificacao"></a>
## Tabelas de codificação

As tabelas abaixo mostram todos os layouts de 32 bits usados no RAVEN. Sempre que aparecer uma observação, ela lembra o alcance
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

**System (`0x73`):** o RAVEN implementa três códigos: `ECALL` (`0x00000073`), `EBREAK`
(`0x00100073`) e `HALT` (`0x00200073`). `ebreak` é um breakpoint de debug retomável; `halt`
interrompe permanentemente apenas o hart atual e deve ser usado como marca semântica de "este hart
terminou aqui" nos exemplos.

<a id="comportamento-do-assembler-e-pseudoinstrucoes"></a>
## Comportamento do assembler e pseudoinstruções

O assembler do RAVEN é propositalmente simples para que você consiga acompanhar cada etapa:

- Comentários começam com `;` ou `#`.
- Operandos são separados por vírgula (`mnemonic op1, op2, ...`).
- Seções/diretivas suportadas incluem `.text`, `.data`, `.bss`, `.section`, `.word`, `.byte`, `.half`, `.ascii`, `.asciz`/`.asciiz`, `.space` e `.align`.

### Referência de pseudoinstruções

A tabela abaixo documenta os formatos aceitos no RAVEN e a forma exata (conceitual) de expansão usada pelo assembler.

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
| `push` | `push rs` | `addi sp, sp, -4` + `sw rs, 0(sp)` | Decrementa `sp` em 4 e armazena `rs` no novo `sp` (convenção RISC-V full-descending padrão) |
| `pop` | `pop rd` | `lw rd, 0(sp)` + `addi sp, sp, 4` | Lê `rd` do `sp` atual e incrementa `sp` em 4 |
| `print` | `print rd` | `addi a7, x0, 1000` + `addi a0, rd, 0` + `ecall` | Imprime valor de registrador. |
| `printStr` | `printStr label` | `addi a7, x0, 1001` + `la a0, label` + `ecall` | Imprime string NUL-terminada (sem quebra de linha). |
| `printString` | `printString label` | igual a `printStr label` | Alias legado aceito pelo assembler. |
| `printStrLn` | `printStrLn label` | `addi a7, x0, 1002` + `la a0, label` + `ecall` | Imprime string e adiciona quebra de linha. |
| `read` | `read label` | `addi a7, x0, 1003` + `la a0, label` + `ecall` | Lê uma linha inteira para memória em `label` (NUL-terminada). |
| `readByte` | `readByte label` | `addi a7, x0, 1010` + `la a0, label` + `ecall` | Grava 1 byte em `label`. |
| `readHalf` | `readHalf label` | `addi a7, x0, 1011` + `la a0, label` + `ecall` | Grava 2 bytes (little-endian) em `label`. |
| `readWord` | `readWord label` | `addi a7, x0, 1012` + `la a0, label` + `ecall` | Grava 4 bytes (little-endian) em `label`. |

> `jal` e `jalr` são instruções reais da ISA (não pseudo) e também são suportadas diretamente.
> Em `jal`, você pode usar `jal label` (com `rd=ra` implícito) ou `jal rd, label`.

Para observar a expansão final em execução, rode `cargo run`, monte o programa e acompanhe o traço de instruções decodificadas na interface.

<a id="modos-de-arredondamento-de-ponto-flutuante"></a>
### Modos de arredondamento de ponto flutuante

As instruções de ponto flutuante que arredondam aceitam um último operando opcional com o nome do modo de arredondamento, como no GNU as:

```asm
fadd.s  frd, frs1, frs2[, rm]          # também fsub.s, fmul.s, fdiv.s
fsqrt.s frd, frs1[, rm]
fmadd.s frd, frs1, frs2, frs3[, rm]    # também fmsub.s, fnmsub.s, fnmadd.s
fcvt.w.s  rd, frs1[, rm]               # também fcvt.wu.s
fcvt.s.w  frd, rs1[, rm]               # também fcvt.s.wu
```

O modo vai para o `funct3` (o campo `rm`):

| `rm` | Nome | Arredonda |
| --- | --- | --- |
| `0` | `rne` | para o mais próximo, empate para o par |
| `1` | `rtz` | em direção a zero |
| `2` | `rdn` | para baixo, em direção a −∞ |
| `3` | `rup` | para cima, em direção a +∞ |
| `4` | `rmm` | para o mais próximo, empate para longe de zero |
| `7` | `dyn` | como manda o `frm` (`fcsr[7:5]`) |

- Sem modo, o assembler escreve `rne`, como o RAVEN sempre fez, e os programas existentes continuam gerando as mesmas palavras. O GNU as escreve `dyn` nesse caso: acrescente `, dyn` para seguir o `frm`.
- O desmontador mostra todo modo, exceto `rne`, então uma listagem volta a gerar as mesmas palavras.
- `fmin.s`, `fmax.s`, as injeções de sinal, as comparações, `fmv.x.w`, `fmv.w.x` e `fclass.s` usam o `funct3` para escolher a operação. Elas não aceitam operando de arredondamento.
- `rm` = 5 ou 6, ou `dyn` com o `frm` valendo 5, 6 ou 7, é reservado. A instrução levanta então uma exceção de instrução ilegal: `mcause` = 2, `mtval` = a palavra da instrução, `mepc` = o seu endereço. Nem `rd` nem `fflags` mudam. Sem tratador (`mtvec` = 0), a execução para e informa o trap. O assembler não tem nome para 5 nem 6; o desmontador os mostra como `rm5` e `rm6`.
- No pipeline a exceção é tomada quando a instrução faz commit, então uma instrução num caminho que um branch descarta nunca gera trap. O modelo Scoreboard não tem reorder buffer e as suas exceções são imprecisas: uma instrução mais nova e independente pode já ter terminado.

```asm
    fdiv.s  ft0, ft1, ft2, rtz   # em direção a zero, seja qual for o frm
    csrrwi  zero, frm, 3         # frm = rup
    fdiv.s  ft3, ft1, ft2, dyn   # arredonda para cima
    fdiv.s  ft4, ft1, ft2        # rne: nenhum modo escrito
```

## Syscalls disponíveis

O Raven suporta um ABI estilo Linux (mínimo) e algumas extensões didáticas do próprio Raven.

### ABI (estilo Linux)

- `a7` (`x17`): número do syscall
- `a0..a5` (`x10..x15`): argumentos
- retorno em `a0` (`x10`) (valores negativos significam `-errno`, representados como `i32 as u32`)

### Como usar syscalls (mini tutorial)

1) Coloque o número do syscall em `a7`.
2) Coloque os argumentos em `a0..a5`.
3) Execute `ecall`.
4) Leia o retorno em `a0`.

O Raven implementa só um subconjunto pequeno. Quando um syscall não existe, o Raven para a execução e mostra uma mensagem no
console (isso ajuda no ensino, porque o erro fica explícito).

#### Retornos e erros

- Em sucesso, o syscall retorna um valor **não-negativo** em `a0` (por exemplo, quantos bytes foram lidos/escritos).
- Em erro, o Raven usa o padrão Linux de `-errno` em `a0`. Internamente isso fica em `u32` (porque os registradores são `u32`):
  `a0 = (-(errno as i32)) as u32`.

### Syscalls Linux (subset suportado)

| `a7` | Nome | Notas |
| --- | --- | --- |
| `63` | `read` | `a0=fd`, `a1=buf`, `a2=count` (somente fd=0). Lê bytes (por linha, adiciona `\n`). |
| `64` | `write` | `a0=fd`, `a1=buf`, `a2=count` (fd=1/2). Escreve `count` bytes da memória. |
| `93` | `exit` | `a0=status`. Encerra execução. |
| `94` | `exit_group` | Igual a `exit` por enquanto. |

#### Linux `write(64)`

Argumentos:

- `a0 = fd` (Raven suporta `1` (stdout) e `2` (stderr); por enquanto ambos aparecem no console)
- `a1 = buf` (ponteiro para bytes na memória)
- `a2 = count` (quantidade de bytes para escrever)

Retorno:

- `a0 = bytes_written` ou `-errno`

Exemplo curto (imprime "Hello!\\n" e sai):

```asm
.data
msg: .ascii "Hello!"
.byte 10          # '\n' (o Raven não interpreta escapes dentro de .ascii/.asciz)

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

Argumentos:

- `a0 = fd` (Raven suporta apenas `0`: stdin)
- `a1 = buf` (ponteiro onde os bytes serão gravados)
- `a2 = count` (máximo de bytes para ler)

Retorno:

- `a0 = bytes_read` ou `-errno`

Notas importantes (simplificações didáticas):

- A entrada vem do console da UI e é por linha. Quando há uma linha disponível, o Raven adiciona `\n` ao final e entrega como bytes.
- Se não há entrada, o Raven pausa a execução (PC não avança) e fica aguardando o usuário digitar algo na UI.

Exemplo curto (lê e faz echo):

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

Argumentos:

- `a0 = status` (código de saída)

Efeito:

- Encerra a VM “normalmente” (isso não é fault na UI).

### Extensões Raven (usadas pelas pseudos)

| `a7` | Nome | Usado por |
| --- | --- | --- |
| `1000` | `raven_print_int` | `print rd` |
| `1001` | `raven_print_zstr` | `printStr` / `printString` |
| `1002` | `raven_print_zstr_ln` | `printStrLn` |
| `1003` | `raven_read_line_z` | `read label` |
| `1010` | `raven_read_u8` | `readByte label` |
| `1011` | `raven_read_u16` | `readHalf label` |
| `1012` | `raven_read_u32` | `readWord label` |

Nas extensões `read*` do Raven, ele insiste até receber um valor válido para o tamanho pedido. Entradas inválidas geram uma mensagem
amigável e o PC **não** avança, destacando que a execução está em pausa.

Pronto para ir além? Abra o tutorial interativo `[?]` no Raven para ver exemplos guiados ou explore `Program Examples/` e enxergue
essas codificações em programas reais.
