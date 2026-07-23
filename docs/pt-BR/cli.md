# Referência da CLI do Raven

> [Read in English](../en/cli.md)

O Raven possui uma CLI completa para uso sem interface gráfica, em paralelo à TUI interativa.
Execute `raven help` a qualquer momento para ver um resumo.

---

## Subcomandos

| Subcomando | Descrição |
|---|---|
| `raven build <arquivo> [opções]` | Montar um arquivo fonte `.s` |
| `raven run <arquivo> [opções]` | Montar e simular |
| `raven export-config [opções]` | Exportar a config unificada padrão (`.rcfg`) |
| `raven check-config <arquivo> [opções]` | Validar e inspecionar um arquivo `.rcfg` |
| `raven debug-run-controls [opções]` | Despejar texto e hitboxes de `Run Controls` para depurar hover |
| `raven debug-help-layout [opções]` | Despejar layout de botão de ajuda / popup de uma aba |
| `raven debug-pipeline-stage [opções]` | Despejar preview de linha de estágio do pipeline |
| `raven help` | Exibir resumo de uso |

> Um único arquivo `.rcfg` agora guarda **toda** a configuração — parâmetros de simulação, hierarquia de cache e comportamento do pipeline — nas seções `[sim]`, `[cache]` e `[pipeline]`. Os antigos arquivos separados `.fcache` / `.pcfg` e seus subcomandos foram removidos.

---

## `raven build`

Monta um arquivo fonte `.s` e gera um binário FALC (`.bin`).

```
raven build <entrada> [saída] [opções]
```

| Argumento / Flag | Descrição |
|---|---|
| `<entrada>` | Caminho para o arquivo `.s` (obrigatório) |
| `[saída]` | Caminho de saída para o `.bin` (segundo argumento posicional) |
| `--out <caminho>` | Equivalente ao acima; tem prioridade sobre o posicional |
| `--nout` | Apenas verifica — monta mas não escreve nenhum arquivo |

**Exemplos**

```bash
# Montar e gerar program.bin
raven build program.s

# Gerar em caminho personalizado
raven build program.s out/prog.bin
raven build program.s --out out/prog.bin

# Verificar sintaxe sem gerar saída
raven build program.s --nout
```

Em caso de sucesso, o Raven imprime a contagem de instruções e o tamanho dos dados no stderr.
Em caso de erro, imprime o número da linha e a mensagem, e sai com código 1.

---

## `raven run`

Monta e simula um programa. Aceita fontes `.s`, binários FALC `.bin` ou binários ELF32 RISC-V.

```
raven run <arquivo> [opções]
```

| Flag | Padrão | Descrição |
|---|---|---|
| `--config <arquivo>` | padrões embutidos | Carrega a config unificada (sim + cache + pipeline) de um `.rcfg` |
| `--pipeline` | desligado | Executa usando o simulador de pipeline em vez do executor sequencial |
| `--pipeline-trace-out <arquivo>` | desligado | Grava um JSON por ciclo do pipeline; requer `--pipeline` |
| `--cores <n>` | settings ou `1` | Máximo de cores físicos disponíveis para `hart_start` durante a execução |
| `--mem <tamanho>` | sim-settings ou `16mb` | Tamanho da RAM — sufixos `kb`, `mb`, `gb` (ex.: `256kb`, `1gb`) |
| `--max-cycles <n>` | `1000000000` | Limite de instruções; um aviso é exibido se atingido |
| `--expect-exit <código>` | desligado | Falha se o código de saída final for diferente |
| `--expect-stdout <texto>` | desligado | Falha se o stdout capturado for diferente |
| `--expect-reg <reg=valor>` | desligado | Verifica valor final de registrador inteiro; repetível |
| `--expect-mem <addr=valor>` | desligado | Verifica palavra de 32 bits na memória; repetível |
| `--out <arquivo>` | stdout | Grava resultados da simulação em arquivo em vez do stdout |
| `--nout` | — | Suprime a saída de resultados (o stdout do programa ainda é exibido) |
| `--format json\|rstats\|csv` | `json` | Formato dos resultados |

> `--mem` tem prioridade sobre o valor `mem_kb` ou o legado `mem_mb` do `.rcfg`. Se nenhum dos dois for informado, o padrão é `16mb`.

**Exemplos**

```bash
# Executar com padrões, imprimir stats JSON no stdout
raven run program.s

# Executar sem imprimir stats
raven run program.s --nout

# Gravar stats em arquivo
raven run program.s --out results.json

# Usar config personalizada (cache + CPI + memória) e gravar CSV
raven run program.s --config my.rcfg --format csv --out stats.csv

# Executar com pipeline e uma config explícita
raven run program.s --pipeline --config my.rcfg --format json

# Validar o estado final
raven run program.s --expect-exit 0 --expect-reg a0=42 --expect-mem 0x1000=0x2a

# Emitir trace por ciclo do pipeline
raven run program.s --pipeline --pipeline-trace-out trace.json --nout

# Permitir até 4 cores para programas multi-hart
raven run program.s --cores 4 --nout

# Executar com 64 MB de RAM (substitui o sim-settings)
raven run program.s --mem 64mb

# Executar um binário pré-montado ou ELF
raven run prog.bin
raven run target/riscv32im-unknown-none-elf/debug/meu_crate
```

**Entrada interativa**

Se o programa lê do stdin (syscalls 3 / 1003), o `raven run` lê do terminal interativamente — qualquer saída pendente é descarregada antes do prompt para que o usuário a veja. Redirecione o stdin normalmente:

```bash
echo "hello" | raven run io_echo.s --nout
printf "42\n" | raven run calculadora.s --nout
```

**Formatos de saída**

| Formato | Descrição |
|---|---|
| `json` | JSON legível por máquina com todas as estatísticas |
| `rstats` | Resultados unificados legíveis (`.rstats`) com seções `[program]`, `[cache]`, `[pipeline]` e `[tlb]` |
| `csv` | CSV compatível com planilhas |

Quando `--pipeline` está ativo, o Raven ainda exporta as estatísticas normais de cache, mas inclui também um resumo do pipeline:

- escopo (`selected` na exportação específica do pipeline, `aggregate` nos resumos de cache/programa)
- instruções committed
- ciclos do pipeline
- contagem de stalls
- contagem de flushes
- CPI do pipeline
- breakdown por tags de stall (`RAW`, `load-use`, `branch`, `FU`, `mem`)

### Asserções

As flags `--expect-*` tornam o `raven run` útil para regressão em CLI. Se alguma falhar, o Raven sai com código `1`.

- `--expect-exit <código>` compara com o código final de saída.
- `--expect-stdout <texto>` compara com todo o stdout capturado.
- `--expect-reg <reg=valor>` compara um registrador inteiro final.
- `--expect-mem <addr=valor>` compara uma palavra final de 32 bits na memória.

Os valores aceitam decimal ou hexadecimal (`0x...`). Registradores aceitam aliases como `a0`, `sp`, `t3` e `x10`.

### Trace JSON do pipeline

`--pipeline-trace-out <arquivo>` grava um trace estruturado por ciclo com:

- ciclo atual
- PC/classe da instrução committed
- fetch PC
- ocupação dos estágios `IF`, `ID`, `EX`, `MEM`, `WB`
- metadados de especulação e stall em cada estágio
- traces de hazard e forwarding daquele ciclo

Esta opção só é válida junto com `--pipeline`.

---

## `raven export-config`

Grava a configuração unificada padrão embutida em um arquivo `.rcfg` para edição. O arquivo contém as seções `[sim]`, `[cache]` e `[pipeline]`.

```
raven export-config [--out <arquivo>]
```

Se `--out` for omitido, a config é impressa no stdout.

```bash
raven export-config                        # imprimir no stdout
raven export-config --out default.rcfg     # gravar em arquivo
```

Veja a seção [Formato do arquivo de config](#formato-do-arquivo-de-config) abaixo para a descrição completa dos campos.

---

## `raven check-config`

Analisa e valida um arquivo `.rcfg`, imprime um resumo de cada seção (configurações de simulação, cada nível de cache, comportamento do pipeline) e, opcionalmente, reexporta a config normalizada.

```
raven check-config <arquivo> [--out <arquivo>]
```

```bash
raven check-config my.rcfg
raven check-config my.rcfg --out normalized.rcfg
```

---

## `raven debug-run-controls`

Despeja a linha textual atual de `Run Controls` e os intervalos de colunas que o mouse reconhece como hover/click.
Isso ajuda a encontrar offsets visuais entre o render e a lógica de hit-test.

```
raven debug-run-controls [opções]
```

| Flag | Padrão | Descrição |
|---|---|---|
| `--width <n>` | `160` | Largura virtual da UI para o dump |
| `--height <n>` | `40` | Altura virtual da UI para o dump |
| `--cores <n>` | `1` | Número simulado de cores |
| `--selected-core <n>` | `0` | Índice do core selecionado |
| `--view ram\|regs\|dyn` | `ram` | Modo do painel lateral Run |
| `--running` | desligado | Renderizar estado como RUN |
| `--out <arquivo>` | stdout | Gravar dump em arquivo |

```bash
raven debug-run-controls
raven debug-run-controls --cores 4 --selected-core 2 --view dyn
raven debug-run-controls --running --out run-controls.txt
```

---

## `raven debug-help-layout`

Despeja o layout do botão de ajuda e do popup para uma aba da TUI. Útil para verificar se as posições das dicas de tecla batem com o que a TUI realmente renderiza em um dado tamanho de terminal.

```
raven debug-help-layout [opções]
```

| Flag | Padrão | Descrição |
|---|---|---|
| `--width <n>` | `160` | Largura virtual da UI para o dump |
| `--height <n>` | `40` | Altura virtual da UI para o dump |
| `--tab editor\|run\|cache\|pipeline\|docs\|config` | `editor` | Aba a inspecionar |
| `--out <arquivo>` | stdout | Gravar dump em arquivo |

```bash
raven debug-help-layout
raven debug-help-layout --tab cache
raven debug-help-layout --tab pipeline --width 120 --height 30
```

---

## `raven debug-pipeline-stage`

Despeja um preview de linha de estágio do pipeline. Útil para verificar o layout de badges e truncamento de disassembly em uma dada largura de estágio.

```
raven debug-pipeline-stage [opções]
```

| Flag | Padrão | Descrição |
|---|---|---|
| `--width <n>` | `24` | Largura interna virtual do estágio |
| `--stage <nome>` | `EX` | Rótulo do estágio |
| `--disasm <texto>` | `addi t4, t4, 1` | Texto de disassembly |
| `--badges <csv>` | `LOAD,RAW,FWD` | Lista de badges |
| `--pred <texto>` | — | Texto do badge especulativo (opcional) |
| `--out <arquivo>` | stdout | Gravar dump em arquivo |

```bash
raven debug-pipeline-stage
raven debug-pipeline-stage --width 24 --disasm "addi t4, t4, 1" --badges LOAD,RAW,FWD
raven debug-pipeline-stage --stage MEM --pred SPEC
```

---

## Formato do arquivo de config

Um único arquivo `.rcfg` (`# Raven Config v3`) guarda toda a configuração em três
seções. Exporte pela TUI com **`Ctrl+e`** e reimporte com **`Ctrl+l`** a partir de
qualquer uma das abas Cache, Config ou Pipeline; na CLI use `raven export-config` /
`raven check-config`.

```ini
# Raven Config v3

[sim]
cache_enabled=true
pipeline_enabled=true
vm_mode=off
trace_syscalls=false
run_scope=focus
max_cores=1
mem_kb=16384
# CPI (ciclos por instrução)
cpi.alu=1
cpi.mul=3
cpi.div=20
cpi.load=0
cpi.store=0
cpi.branch_taken=3
cpi.branch_not_taken=1
cpi.jump=2
cpi.system=10
cpi.fp=5

[cache]
levels=0
icache.size=1024
icache.line_size=16
icache.associativity=2
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=50
# ... dcache.* espelha icache.* ; níveis extras usam l2.* / l3.* ; tlb.* para a TLB

[pipeline]
enabled=true
bypass.ex_to_ex=true
bypass.mem_to_ex=true
bypass.wb_to_id=true
bypass.store_to_load=false
mode=SingleCycle
fu.alu=1
fu.mul=1
fu.div=1
fu.fpu=1
fu.lsu=1
fu.sys=1
branch_resolve=Ex
predict=NotTaken
speed=Normal
```

### `[sim]` — configurações de simulação

- `cache_enabled=false` ignora toda a hierarquia de cache (todos os acessos vão direto para a RAM).
- `pipeline_enabled` alterna o estado global do pipeline usado na aba Config da TUI.
- `vm_mode` — `off`, `sv32`, `custom` ou `manual`.
- `trace_syscalls` controla o log de depuração de syscalls.
- `run_scope` aceita `all` ou `focus`.
- `max_cores` assume `1` quando omitido e deve ficar no intervalo `1..=32`.
- `mem_kb` define o tamanho padrão da RAM em kilobytes e é ajustado para a potência de 2 mais próxima. O campo legado `mem_mb` continua aceito. A flag `--mem` da CLI substitui este valor.
- No headless, `--pipeline` por enquanto suporta apenas `--cores 1`.
- Os valores de CPI são ciclos extras somados à latência de cache para a classe de instrução correspondente.

### `[cache]` — hardware de cache

Descreve a hierarquia de cache: I-cache, D-cache, quaisquer níveis extras (L2, L3…)
e a TLB unificada. Veja a [Referência de Config de Cache](cache-config.md) para a
lista completa de campos.

### `[pipeline]` — comportamento do pipeline

- `enabled` — pipeline habilitado na TUI
- `bypass.ex_to_ex` / `bypass.mem_to_ex` / `bypass.wb_to_id` / `bypass.store_to_load` — caminhos de forwarding
- `mode` — mapeado na UI como `Serialized` ou `Parallel UFs`
- `fu.alu` / `fu.mul` / `fu.div` / `fu.fpu` / `fu.lsu` / `fu.sys` — quantidade de unidades funcionais usada no modo `Parallel UFs`
- `branch_resolve` — `Id`, `Ex` ou `Mem`
- `predict` — `NotTaken`, `Taken`, `Btfnt` ou `TwoBit`
- `speed` — velocidade de reprodução na TUI (`Slow`, `Normal`, `Fast`, `Instant`)

---

## Códigos de saída

| Código | Significado |
|---|---|
| `0` | Sucesso |
| `1` | Erro de montagem, falha na simulação ou argumento inválido |
