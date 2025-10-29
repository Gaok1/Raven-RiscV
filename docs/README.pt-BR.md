# Falcon ASM — um emulador RISC-V (RV32I) para aprender brincando
<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

O Falcon ASM é um projeto pequeno em Rust que revela cada etapa do ciclo buscar → decodificar → executar. Ele foi pensado para
estudantes, entusiastas e professores que querem experimentar a base do ISA RISC-V sem esbarrar em um código cheio de
micro-otimizações.

Além do núcleo do emulador, o Falcon traz uma experiência de IDE integrada: explorador de projetos, editor com destaque de
sintaxe, visualização passo a passo das instruções e painéis dedicados para registradores, memória e syscalls. Você monta,
executa, pausa e volta no tempo sem sair da interface, o que facilita demonstrar como cada instrução altera o estado ou
depurar trabalhos de alunos em tempo real. A proposta é ser um simulador e uma plataforma de ensino acolhedora mesmo para quem
está abrindo o RISC-V pela primeira vez.

## O que vem no Falcon

- **Núcleo legível** – CPU, memória e decodificador foram escritos para você acompanhar linha a linha.
- **Assembler integrado** – monte segmentos `.text`, `.data` e `.bss` com diretivas como `.byte`, `.word`, `.ascii`, `.space`
  e um conjunto de pseudoinstruções (`la`, `call`, `ret`, `push`, `pop`, `printStr`, `printStrLn`, `read`, entre outras).
- **Facilidades de syscall** – basta definir `a7` e chamar `ecall` para imprimir valores, strings, ler entradas do usuário ou
  encerrar o programa.
- **Cobertura RV32I + M** – aritmética, loads/stores, desvios, saltos, multiplicação, divisão e mensagens amigáveis para
  instruções não suportadas.

O emulador usa a convenção padrão de registradores (`zero`, `ra`, `sp`, `a0`…`a7`, `t0`…`t6`, `s0`…`s11`) e memória
little-endian, reproduzindo o comportamento esperado em cursos e materiais introdutórios.

## Primeiros passos

1. Instale o Rust pelo [rustup.rs](https://rustup.rs).
2. Clone este repositório e execute:

   ```bash
   cargo run
   ```

3. Escreva um programa com seções `.text` e `.data`, monte com o Falcon e acompanhe cada passo enquanto avança instrução por
   instrução.

Vai embutir o Falcon em outro projeto? Use os auxiliares para posicionar cada segmento na memória:

```rust
use falcon::program::{load_words, load_bytes, zero_bytes};

let prog = falcon::asm::assemble(source, base_pc)?;
load_words(&mut mem, base_pc, &prog.text)?;
load_bytes(&mut mem, prog.data_base, &prog.data)?;
let bss_base = prog.data_base + prog.data.len() as u32;
zero_bytes(&mut mem, bss_base, prog.bss_size)?;
```

## Continue aprendendo

- Siga o passo a passo do [tutorial em português](Tutorial-pt.md) para montar e executar seus primeiros programas.
- Consulte os layouts de instrução e as pseudoinstruções detalhadas no [`format.pt-BR.md`](format.pt-BR.md).
- Explore o diretório `Program Examples/` para ver programas que exercitam syscalls, aritmética e controle de fluxo.

## Contribuições e próximos passos

O Falcon é propositalmente enxuto, e contribuições são muito bem-vindas! Entre as ideias futuras estão suporte a CSR/fence,
extensões de ponto flutuante e ferramentas extras ao redor do emulador.

Seja preparando uma aula, corrigindo seu primeiro trabalho de assembly ou construindo um material didático, o Falcon ASM quer ser
um espaço acolhedor para explorar o ecossistema RISC-V. Bons voos!
