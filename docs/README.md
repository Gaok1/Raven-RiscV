# Falcon ASM 🦅 – Emulador Educacional RISC-V (RV32I)
<img width="500" height="400" alt="image" src="https://github.com/user-attachments/assets/ed5354ba-93bc-4717-ab77-8993f1c3abc5" />

Falcon ASM é um emulador escrito em Rust com foco em clareza e aprendizado. O objetivo é expor o ciclo **fetch → decode → execute** e oferecer uma visão completa de como um processador RISC-V básico funciona.

O projeto inclui:

- **Decodificador e encoder** de instruções
- **Montador textual de duas passagens** com suporte a rótulos
- **Segmentos `.text` e `.data`** com diretivas de dados
- **Registradores e memória** little-endian
- **Motor de execução** pronto para integração com interfaces gráficas

## Estado do Projeto

Implementa o subconjunto essencial do **RV32I**:

- **R-type:** `ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU, MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU`
- **I-type (OP-IMM):** `ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI`
- **Loads:** `LB, LH, LW, LBU, LHU`
- **Stores:** `SB, SH, SW`
- **Branches:** `BEQ, BNE, BLT, BGE, BLTU, BGEU`
- **U/J:** `LUI, AUIPC, JAL`
- **JALR**
- **SYSTEM:** `ECALL`, `EBREAK` (tratados como HALT)

*Ainda não implementados:* FENCE/CSR e ponto flutuante.

## Montador e Diretivas

O montador aceita código dividido em segmentos:

- `.text` – segmento padrão de instruções.
- `.data` – segmento de dados, carregado **0x1000 bytes** após o endereço base do programa.

Dentro de `.data` são suportadas as diretivas:

- `.byte v1, v2, ...` – valores de 8 bits
- `.word w1, w2, ...` – valores de 32 bits em little-endian

Rótulos (`label:`) podem ser definidos em qualquer segmento. Para obter o endereço de um rótulo, utilize a pseudoinstrução `la rd, label`, que gera um par `lui`/`addi` automaticamente.

### Pseudoinstruções disponíveis

- `nop` → `addi x0, x0, 0`
- `mv rd, rs` → `addi rd, rs, 0`
- `li rd, imm12` → `addi rd, x0, imm`
- `subi rd, rs1, imm` → `addi rd, rs1, -imm`
- `j label` → `jal x0, label`
- `jr rs1` → `jalr x0, rs1, 0`
- `ret` → `jalr x0, ra, 0`
- `la rd, label` → carrega o endereço de `label`

## Registradores e Memória

- Registradores `x0..x31` com aliases: `zero, ra, sp, gp, tp, t0..t6, s0/fp, s1, a0..a7, s2..s11`. `x0` é sempre 0.
- Memória little-endian com operações `load8/16/32` e `store8/16/32`.

## Resumo de Opcodes

```
RTYPE = 0x33
OPIMM = 0x13
LOAD  = 0x03
STORE = 0x23
BRANCH= 0x63
LUI   = 0x37
AUIPC = 0x17
JAL   = 0x6F
JALR  = 0x67
SYSTEM= 0x73
```

Para detalhes de formato e tabelas `funct3/funct7`, consulte [`docs/format.md`](format.md).

## Execução

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

O emulador executa instruções enquanto `step` retornar `true`.

