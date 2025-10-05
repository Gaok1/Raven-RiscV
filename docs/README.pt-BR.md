# Falcon ASM - Emulador Educacional RISC-V (RV32I)
<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

Idioma: [Português (BR)](README.pt-BR.md) | [English](README.md)

Falcon ASM é um emulador escrito em Rust focado em clareza e aprendizado. Ele expõe o ciclo buscar -> decodificar -> executar e oferece uma visão clara de como um processador básico RISC-V funciona.

O projeto inclui:

- Decoder e encoder de instruções
- Assembler de texto em duas passagens com suporte a rótulos
- Segmentos `.text`/`.section .text` e `.data`/`.section .data` com diretivas de dados
- Registradores e memória little-endian
- Motor de execução pronto para integração com interfaces de terminal (TUI)

## Status do Projeto

Implementa o subconjunto essencial de RV32I:

- Tipo R: `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- Tipo I (OP-IMM): `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- Loads: `LB, LH, LW, LBU, LHU`
- Stores: `SB, SH, SW`
- Branches: `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- U/J: `LUI, AUIPC, JAL`
- JALR
- SYSTEM: `ECALL`, `HALT`

Não implementado: FENCE/CSR e ponto flutuante.

## Assembler e Diretivas

O assembler aceita código dividido em segmentos:

- `.text` ou `.section .text` – segmento de instruções.
- `.data` ou `.section .data` – segmento de dados, carregado 0x1000 bytes após o endereço base do programa.

No `.data` as seguintes diretivas são suportadas:

- `.byte v1, v2, ...` – valores de 8 bits
- `.half h1, h2, ...` – valores de 16 bits
- `.word w1, w2, ...` – valores de 32 bits em little-endian
- `.dword d1, d2, ...` – valores de 64 bits em little-endian
- `.ascii "texto"` – bytes brutos
- `.asciz "texto"` / `.string "texto"` – string com terminador NUL
- `.space n` / `.zero n` – n bytes zero

Rótulos (`label:`) podem ser definidos em qualquer segmento. Para carregar o endereço absoluto de um rótulo, use a pseudoinstrução `la rd, label`, que emite um par `lui`/`addi`.

### Pseudoinstruções Disponíveis

- `nop` -> `addi x0, x0, 0`
- `mv rd, rs` -> `addi rd, rs, 0`
- `li rd, imm12` -> `addi rd, x0, imm` (somente 12 bits, por escolha didática)
- `subi rd, rs1, imm` -> `addi rd, rs1, -imm`
- `j label` -> `jal x0, label`
- `call label` -> `jal ra, label`
- `jr rs1` -> `jalr x0, rs1, 0`
- `ret` -> `jalr x0, ra, 0`
- `la rd, label` -> carrega o endereço de `label`
- `push rs` -> `addi sp, sp, -4` ; `sw rs, 4(sp)`
- `pop rd` -> `lw rd, 4(sp)` ; `addi sp, sp, 4`
- `print rd` -> define `a7=1`, imprime o valor em `rd`
- `printString label` -> define `a7=2`, carrega `a0` e imprime string no rótulo
- `read label` -> define `a7=3`, carrega `a0` e lê uma linha para a memória no rótulo

## Registradores e Memória

- Registradores `x0..x31` com apelidos: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`. `x0` é sempre 0.
- Memória little‑endian com operações `load8/16/32` e `store8/16/32`.

## Syscalls

Falcon ASM implementa algumas chamadas de sistema básicas. Coloque o número da syscall em `a7`,
defina argumentos em `a0` e execute `ecall`.

| `a7` | Pseudoinstrução | Descrição |
|------|------------------|-----------|
| 1 | `print rd` | Imprime o valor decimal do registrador `rd` (`a0=rd`). |
| 2 | `printString label` | Imprime a string NUL‑terminada em `label` (`a0=addr`). |
| 3 | `read label` | Lê uma linha de entrada para a memória em `label` e adiciona NUL. |

Exemplo sem pseudoinstruções:

```asm
    li a7, 1      # seleciona syscall
    mv a0, t0     # valor a imprimir
    ecall
```

## Tipos de Instrução (como funcionam)

- Tipo R (opcode `0x33`): operações registrador‑registrador. `rd = OP(rs1, rs2)`.
- Tipo I (opcode `0x13`): ALU registrador‑imediato. `rd = OP(rs1, imm12)`. Shifts usam `shamt` de 5 bits (`SRAI` com `funct7=0x20`).
- Loads (opcode `0x03`): `LB/LH/LW/LBU/LHU` leem de `rs1 + imm` e escrevem em `rd`.
- Stores (opcode `0x23`): `SB/SH/SW` escrevem os 8/16/32 bits menos significativos de `rs2` em `rs1 + imm`.
- Branches (opcode `0x63`): desvios condicionais relativos ao PC (13 bits em bytes). O assembler calcula a partir de rótulos.
- U‑type (`LUI/AUIPC`): `LUI` carrega os bits [31:12] em `rd`; `AUIPC` soma o imediato ao `pc` atual.
- Jumps (`JAL/JALR`): `JAL` escreve `pc+4` em `rd` e salta para `pc + imm21`; `JALR` escreve `pc+4` em `rd` e salta para `(rs1 + imm12) & !1`.

Veja [`docs/format.pt-BR.md`](format.pt-BR.md) para layouts de bits e mais detalhes.

## Executando

Requisitos: Rust estável (via [rustup.rs](https://rustup.rs)).

```bash
cargo run
```

Exemplo mínimo:

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

O emulador executa instruções enquanto `step` retorna `true`; `halt` ou syscall desconhecida encerram a execução.

# Exemplos
## Editor de código
<img width="1918" height="1009" alt="image" src="https://github.com/user-attachments/assets/4ade62a4-e3e0-4c69-b42b-ae52d5bd8397" />

## Executando código (emulador)

### Visão de registradores
<img width="1917" height="997" alt="image" src="https://github.com/user-attachments/assets/6be9a0ec-b64f-4cab-b9b5-ff581a27f692" />

### Visão da RAM
<img width="1920" height="999" alt="image" src="https://github.com/user-attachments/assets/63386101-393f-47d1-a559-9a3b74da95ac" />

### Console

A aba Run possui um console inferior onde as syscalls `print`, `printString` e `read` fazem E/S. `print rd` mostra o valor decimal de um registrador, `printString label` imprime uma string terminada em NUL no rótulo, e `read label` armazena uma linha no endereço do rótulo. Role com `Ctrl+Up/Down` para revisar linhas anteriores.

