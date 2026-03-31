# RAVEN — Emulador e IDE RISC-V

**RAVEN** é um emulador, montador e IDE RISC-V rodando no terminal, escrito em Rust. Cobre **RV32I + M + A + F** e foi pensado para tornar cada etapa do ciclo buscar → decodificar → executar visível e interativa — ideal para estudantes, professores e qualquer um aprendendo assembly.

Tudo vive em uma única TUI: escreva código, monte, execute passo a passo, inspecione registradores e memória, perfile sua hierarquia de cache e leia a documentação — sem sair do terminal.

![RAVEN em ação](../assets/raven-example.gif)

---

## Funcionalidades

### Cobertura do ISA
- **RV32I** — conjunto base completo de instruções inteiras
- **RV32M** — multiplicação e divisão inteira
- **RV32A** — operações atômicas de memória (LR/SC, AMO)
- **RV32F** — ponto flutuante de precisão simples (26 instruções, `f0`–`f31`, `fcsr`)
- Conjunto rico de pseudoinstruções: `la`, `li`, `call`, `ret`, `push`, `pop`, `mv`, `neg`, `not`, `seqz`, `snez`, `beqz`, `bnez`, `bgt`, `ble`, `fmv.s`, `fneg.s`, `fabs.s`, entre outras
- Syscalls via `ecall`: imprimir inteiro/string, ler entrada, sair, bytes aleatórios

### Montador
- Segmentos `.text`, `.data`, `.bss` com `.byte`, `.half`, `.word`, `.ascii`, `.asciz`, `.space`
- `.word label` — use endereços de labels como valores em dados (tabelas de salto, arrays de ponteiros)
- Comentários de bloco (`##!`) e anotações inline (`#!`) visíveis em tempo de execução
- Mensagens de erro claras com número de linha

### Editor (Aba 1)
- Highlight de sintaxe — instruções, registradores, diretivas, labels e strings com cores distintas
- Hints de operandos enquanto digita
- Ir para definição (`F12`), highlight de label sob o cursor, gutter de endereços (`F2`)
- Desfazer/refazer (50 níveis), navegação por palavra, alternar comentário (`Ctrl+/`), duplicar linha (`Ctrl+D`)
- Auto-indent, colar com formatação, page up/down

### Aba Run (Aba 2)
**Memória de Instruções**
- Headers de label e separadores de bloco renderizados inline
- Badge de tipo por instrução (`[R]` `[I]` `[S]` `[B]` `[U]` `[J]`)
- Heat coloring — sufixo `×N` de contagem de execuções colorido por frequência
- Resultado de branch no PC atual: `→ 0xADDR (taken)` / `↛ (not taken)`
- Breakpoints (`b`), saltar para endereço (`g`), painel de trace de execução (`t`)

**Painel de Detalhes Decodificados**
- Breakdown completo dos campos (opcode, funct3/7, rs1/rs2/rd, imediato com sinal)
- Endereço efetivo para loads/stores; aviso de hazard RAW (`⚠ RAW`)
- Estimativa de CPI e classe da instrução

**Sidebar de Registradores**
- Registradores inteiros: dual-column hex + decimal, fade por idade, pin (`P`), write trace
- Registradores float: nomes ABI (`ft0`–`ft11`, `fa0`–`fa7`, `fs0`–`fs11`), alternar com `Tab`
- Sidebar cicla com `v`: **RAM → Registradores → Dyn**
  - **RAM**: `k` cicla a região: Data / Stack / R/W / **Heap** (ponteiro sbrk, marcador `▶HB`)
  - **R/W**: continua sendo a view de RAM, mas segue automaticamente o endereço do último acesso de memória por `LOAD` e `STORE`
  - **Dyn**: modo auto-narrado para passo a passo — STORE → mostra RAM no endereço escrito; LOAD/ALU/branch → mostra registradores

### Aba Cache (Aba 3)
- L1 I-cache + D-cache configuráveis + níveis extras ilimitados (L2, L3…)
- Políticas de substituição: LRU, FIFO, LFU, Clock, MRU, Random
- Políticas de escrita: write-through / write-back + write-allocate / no-allocate
- Políticas de inclusão: Não-inclusiva, Inclusiva, Exclusiva
- Estatísticas ao vivo: hit rate, MPKI, tráfego de RAM, top miss PCs
- Métricas acadêmicas: AMAT (hierárquico), IPC, breakdown de CPI por nível
- Exportar resultados (`Ctrl+R`) para `.fstats` / `.csv`; carregar baseline para comparação delta (`Ctrl+M`)
- Matriz visual de cache com scroll horizontal e drag por scrollbar

### Configuração de CPI
- Custos de ciclo por classe: ALU, MUL, DIV, LOAD, STORE, branch taken/not-taken, JUMP, SYSTEM, FP
- Configurável diretamente na aba Cache → Config

### Aba Docs (Aba 4)
- Referência de instruções e guia da aba Run embutidos no app

---

## Carregando Binários ELF

O RAVEN carrega e executa diretamente binários **ELF32 LE RISC-V** gerados por qualquer toolchain padrão. Compatibilidade oficial:

| Target | Suporte |
|--------|---------|
| `riscv32im-unknown-none-elf` | ✅ Completo |
| `riscv32ima-unknown-none-elf` | ✅ Completo |

### Executando um programa Rust no_std

```bash
# 1. Adicionar o target (apenas uma vez)
rustup target add riscv32im-unknown-none-elf

# 2. Compilar o projeto
cargo build --target riscv32im-unknown-none-elf

# 3. Abrir o RAVEN, ir para a aba Editor, clicar em [BIN] e selecionar o ELF
#    (está em target/riscv32im-unknown-none-elf/debug/<nome-do-crate>)
```

O ELF é carregado nos endereços virtuais definidos pelo linker, o PC é apontado para o entry point, e o disassembler exibe o segmento de texto decodificado. Palavras não reconhecidas (dados, padding) aparecem como `.word 0x...`.

Um projeto pronto para uso com `_start`, panic handler, alocador e wrappers para `write`, `read` e `exit` está disponível em [`rust-to-raven/`](../../rust-to-raven/).

---

## Início Rápido

Baixe o binário mais recente em [Releases](https://github.com/Gaok1/Raven-RiscV/releases), ou compile da fonte:

```bash
git clone https://github.com/Gaok1/Raven-RiscV.git
cd Raven-RiscV
cargo run
```

Requer Rust 1.75+. Sem dependências externas além da toolchain Rust.

---

## Atalhos de Teclado (Aba Run)

| Tecla | Ação |
|-------|------|
| `F5` / `Space` | Rodar / Pausar |
| `s` / `F10` | Passo único |
| `F9` | Alternar breakpoint no PC |
| `f` | Ciclar velocidade: 1× → 2× → 4× → 8× → GO |
| `v` | Ciclar sidebar: RAM → Registradores → Dyn |
| `k` | Ciclar região de RAM: Data → Stack → R/W → Heap |
| `Tab` | Alternar banco int / float (no modo REGS) |
| `t` | Alternar painel de trace de execução |
| `Ctrl+F` | Saltar visão de RAM para endereço |
| `Ctrl+G` | Saltar instrução para label |
| `e` / `y` | Alternar contador de execuções / badges de tipo |
| `P` / click | Fixar / desafixar registrador |

---

## Programas de Exemplo

O diretório `Program Examples/` inclui:
`fib.fas`, `bubble_sort_20.fas`, `quick_sort_20_push_pop.fas`, `binary_search_tree.fas`, `gcd_euclid.fas`, `fatorial.fas`, `cache_locality.fas` e mais.

---

## CLI

O Raven também pode ser usado pela linha de comando sem interface gráfica — montar, simular, exportar/importar configs e redirecionar saída para arquivos.

```bash
raven build program.fas                             # montar
raven run   program.fas --nout                      # executar, sem stats
raven run   program.fas --out results.json          # executar, salvar stats
raven run   program.fas --cache-config l2.fcache \
                        --settings my.rcfg \
                        --format csv --out stats.csv
raven export-config  --out default.fcache           # exportar config de cache padrão
raven export-settings --out default.rcfg            # exportar configurações padrão
```

Veja a **[Referência da CLI](cli.md)** para todos os subcomandos e flags.

---

## Documentação

- **Tutorial interativo** — pressione `[?]` em qualquer aba no Raven (alterne idioma com `[L]`)
- [Referência da CLI (PT-BR)](cli.md) — subcomandos, flags e formatos de arquivo
- [CLI Reference (EN)](../en/cli.md)
- [Formatos de instrução (PT-BR)](format.md) — layouts de bits, encoding, pseudoinstruções
- [Guia do simulador de cache (PT-BR)](cache.md) — configuração, métricas, exportação
- [Formats (EN)](../en/format.md) | [Cache (EN)](../en/cache.md)
- `threads-plan.md` — plano de design para execução multi-core futura, usando o termo `hart` ("hardware thread") para manter a modelagem em nível de hardware, não de SO
- `Program Examples/hart_spawn_visual_demo.fas` — exemplo multi-hart para forçar atividade simultânea nas abas Run e Pipeline

---

## Contribuições

Issues e pull requests são bem-vindos. O código é intencionalmente legível — o núcleo da CPU, o decoder e o montador têm cada um menos de ~500 linhas e seguem uma estrutura direta.
