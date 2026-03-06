# FALCON ASM — Emulador e IDE RISC-V

<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

**FALCON ASM** é um emulador, montador e IDE RISC-V rodando no terminal, escrito em Rust. Cobre **RV32I + M + F** e foi pensado para tornar cada etapa do ciclo buscar → decodificar → executar visível e interativa — ideal para estudantes, professores e qualquer um aprendendo assembly.

Tudo vive em uma única TUI: escreva código, monte, execute passo a passo, inspecione registradores e memória, perfile sua hierarquia de cache e leia a documentação — sem sair do terminal.

---

## Funcionalidades

### Cobertura do ISA
- **RV32I** — conjunto base completo de instruções inteiras
- **RV32M** — multiplicação e divisão inteira
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
- Registradores inteiros: dual-column hex + decimal, fade por idade, pin (`p`), write trace
- Registradores float: nomes ABI (`ft0`–`ft11`, `fa0`–`fa7`, `fs0`–`fs11`), alternar com `Tab`
- Quatro modos de sidebar: visão de RAM / registradores inteiros / stack view / lista de breakpoints (`v`)

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

## Início Rápido

Baixe o binário mais recente em [Releases](https://github.com/Gaok1/FALCON-ASM/releases), ou compile da fonte:

```bash
git clone https://github.com/Gaok1/FALCON-ASM.git
cd FALCON-ASM
cargo run
```

Requer Rust 1.75+. Sem dependências externas além da toolchain Rust.

---

## Atalhos de Teclado (Aba Run)

| Tecla | Ação |
|-------|------|
| `F5` / `Space` | Rodar / Pausar |
| `F10` / `n` | Passo único |
| `F9` / `b` | Alternar breakpoint no PC |
| `f` | Ciclar velocidade: 1× → 2× → 4× → Instant |
| `v` | Ciclar sidebar: RAM → Registradores → Stack → Breakpoints |
| `Tab` | Alternar banco de registradores inteiros / float |
| `t` | Alternar painel de trace de execução |
| `g` | Saltar para endereço |
| `x` | Alternar exibição de word hex bruto |
| `e` / `y` | Alternar contador de execuções / badges de tipo |
| `p` / click | Fixar / desafixar registrador |

---

## Programas de Exemplo

O diretório `Program Examples/` inclui:
`fib.fas`, `bubble_sort_20.fas`, `quick_sort_20_push_pop.fas`, `binary_search_tree.fas`, `gcd_euclid.fas`, `fatorial.fas`, `cache_locality.fas` e mais.

---

## Documentação

- [Tutorial (PT-BR)](Tutorial-pt.md) — passo a passo
- [Formatos de instrução (PT-BR)](format.pt-BR.md) — layouts de bits, encoding, pseudoinstruções
- [Guia do simulador de cache (PT-BR)](cache.pt-BR.md) — configuração, métricas, exportação
- [Tutorial (EN)](Tutorial.md) | [Formats (EN)](format.md) | [Cache (EN)](cache.md)

---

## Contribuições

Issues e pull requests são bem-vindos. O código é intencionalmente legível — o núcleo da CPU, o decoder e o montador têm cada um menos de ~500 linhas e seguem uma estrutura direta.
