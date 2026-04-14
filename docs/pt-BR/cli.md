# Referência da CLI do Raven

> [Read in English](../en/cli.md)

O Raven possui uma CLI completa para uso sem interface gráfica, em paralelo à TUI interativa.
Execute `raven help` a qualquer momento para ver um resumo.

---

## Subcomandos

| Subcomando | Descrição |
|---|---|
| `raven build <arquivo> [opções]` | Montar um arquivo fonte `.fas` |
| `raven run <arquivo> [opções]` | Montar e simular |
| `raven export-cache-config [opções]` | Exportar a config padrão de cache (`.fcache`) |
| `raven check-cache-config <arquivo> [opções]` | Validar e inspecionar um arquivo `.fcache` |
| `raven export-sim-settings [opções]` | Exportar as configurações padrão de simulação (`.rcfg`) |
| `raven check-sim-settings <arquivo> [opções]` | Validar e inspecionar um arquivo `.rcfg` |
| `raven export-pipeline-config [opções]` | Exportar a config padrão de pipeline (`.pcfg`) |
| `raven check-pipeline-config <arquivo> [opções]` | Validar e inspecionar um arquivo `.pcfg` |
| `raven debug-run-controls [opções]` | Despejar texto e hitboxes de `Run Controls` para depurar hover |
| `raven debug-help-layout [opções]` | Despejar layout de botão de ajuda / popup de uma aba |
| `raven debug-pipeline-stage [opções]` | Despejar preview de linha de estágio do pipeline |
| `raven help` | Exibir resumo de uso |

> **Aliases legados** — os nomes antigos (`export-config`, `import-config`, `export-settings`, `import-settings`, `export-pipeline`, `import-pipeline`) ainda funcionam, mas não aparecem mais na saída do `help`.

---

## `raven build`

Monta um arquivo fonte `.fas` e gera um binário FALC (`.bin`).

```
raven build <entrada> [saída] [opções]
```

| Argumento / Flag | Descrição |
|---|---|
| `<entrada>` | Caminho para o arquivo `.fas` (obrigatório) |
| `[saída]` | Caminho de saída para o `.bin` (segundo argumento posicional) |
| `--out <caminho>` | Equivalente ao acima; tem prioridade sobre o posicional |
| `--nout` | Apenas verifica — monta mas não escreve nenhum arquivo |

**Exemplos**

```bash
# Montar e gerar program.bin
raven build program.fas

# Gerar em caminho personalizado
raven build program.fas out/prog.bin
raven build program.fas --out out/prog.bin

# Verificar sintaxe sem gerar saída
raven build program.fas --nout
```

Em caso de sucesso, o Raven imprime a contagem de instruções e o tamanho dos dados no stderr.
Em caso de erro, imprime o número da linha e a mensagem, e sai com código 1.

---

## `raven run`

Monta e simula um programa. Aceita fontes `.fas`, binários FALC `.bin` ou binários ELF32 RISC-V.

```
raven run <arquivo> [opções]
```

| Flag | Padrão | Descrição |
|---|---|---|
| `--cache-config <arquivo>` | padrões embutidos | Carrega hierarquia de cache de um arquivo `.fcache` |
| `--sim-settings <arquivo>` | padrões embutidos | Carrega configurações de simulação (CPI, memória, cache_enabled) de um `.rcfg` |
| `--pipeline` | desligado | Executa usando o simulador de pipeline em vez do executor sequencial |
| `--pipeline-config <arquivo>` | padrões embutidos | Carrega o comportamento do pipeline de um `.pcfg` |
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
| `--format json\|fstats\|csv` | `json` | Formato dos resultados |

> `--mem` tem prioridade sobre o valor `mem_mb` do `.rcfg`. Se nenhum dos dois for informado, o padrão é `16mb`.

**Exemplos**

```bash
# Executar com padrões, imprimir stats JSON no stdout
raven run program.fas

# Executar sem imprimir stats
raven run program.fas --nout

# Gravar stats em arquivo
raven run program.fas --out results.json

# Usar config de cache personalizada e gravar CSV
raven run program.fas --cache-config l2.fcache --format csv --out stats.csv

# Aplicar configurações de simulação (ajuste de CPI, memória, cache ligado/desligado)
raven run program.fas --sim-settings my.rcfg --nout

# Executar com pipeline e uma config explícita
raven run program.fas --pipeline --pipeline-config mypipe.pcfg --format json

# Validar o estado final
raven run program.fas --expect-exit 0 --expect-reg a0=42 --expect-mem 0x1000=0x2a

# Emitir trace por ciclo do pipeline
raven run program.fas --pipeline --pipeline-trace-out trace.json --nout

# Permitir até 4 cores para programas multi-hart
raven run program.fas --cores 4 --nout

# Executar com 64 MB de RAM (substitui o sim-settings)
raven run program.fas --mem 64mb

# Executar um binário pré-montado ou ELF
raven run prog.bin
raven run target/riscv32im-unknown-none-elf/debug/meu_crate
```

**Entrada interativa**

Se o programa lê do stdin (syscalls 3 / 1003), o `raven run` lê do terminal interativamente — qualquer saída pendente é descarregada antes do prompt para que o usuário a veja. Redirecione o stdin normalmente:

```bash
echo "hello" | raven run io_echo.fas --nout
printf "42\n" | raven run calculadora.fas --nout
```

**Formatos de saída**

| Formato | Descrição |
|---|---|
| `json` | JSON legível por máquina com todas as estatísticas |
| `fstats` | Tabela legível por humanos (`.fstats`) |
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

## `raven export-cache-config`

Grava a configuração de cache padrão embutida em um arquivo `.fcache` para edição.

```
raven export-cache-config [--out <arquivo>]
```

Se `--out` for omitido, a config é impressa no stdout.

```bash
raven export-cache-config                        # imprimir no stdout
raven export-cache-config --out default.fcache   # gravar em arquivo
```

---

## `raven check-cache-config`

Analisa e valida um arquivo `.fcache`, imprime um resumo legível de cada nível de cache e, opcionalmente, reexporta a config normalizada.

```
raven check-cache-config <arquivo> [--out <arquivo>]
```

```bash
raven check-cache-config my.fcache
raven check-cache-config my.fcache --out normalized.fcache
```

---

## `raven export-sim-settings`

Grava as configurações de simulação padrão embutidas em um arquivo `.rcfg`.

```
raven export-sim-settings [--out <arquivo>]
```

Se `--out` for omitido, as configurações são impressas no stdout.

```bash
raven export-sim-settings                        # imprimir no stdout
raven export-sim-settings --out default.rcfg     # gravar em arquivo
```

---

## `raven check-sim-settings`

Analisa e valida um arquivo `.rcfg`, imprime um resumo de todas as configurações e, opcionalmente, reexporta a config normalizada.

```
raven check-sim-settings <arquivo> [--out <arquivo>]
```

```bash
raven check-sim-settings my.rcfg
raven check-sim-settings my.rcfg --out normalized.rcfg
```

---

## `raven export-pipeline-config`

Grava a configuração de pipeline padrão embutida em um arquivo `.pcfg`.

```
raven export-pipeline-config [--out <arquivo>]
```

Se `--out` for omitido, a config é impressa no stdout.

```bash
raven export-pipeline-config
raven export-pipeline-config --out default.pcfg
```

---

## `raven check-pipeline-config`

Analisa e valida um arquivo `.pcfg`, imprime um resumo das configurações do pipeline e, opcionalmente, reexporta a config normalizada.

```
raven check-pipeline-config <arquivo> [--out <arquivo>]
```

```bash
raven check-pipeline-config my.pcfg
raven check-pipeline-config my.pcfg --out normalized.pcfg
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

## Formatos de arquivo de configuração

### `.fcache` — hardware de cache

Descreve a hierarquia de cache: I-cache, D-cache e quaisquer níveis extras (L2, L3…).

Exportar / importar pela TUI: **aba Cache → `Ctrl+e` / `Ctrl+l`**

### `.rcfg` — configurações de simulação

Controla parâmetros globais de simulação: CPI por classe de instrução, se a cache está ativa, o tamanho padrão da RAM e o número padrão de cores disponíveis.

```ini
# Raven Sim Config v1
cache_enabled=true
max_cores=1
mem_mb=16

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
```

- `cache_enabled=false` ignora toda a hierarquia de cache (todos os acessos vão direto para a RAM).
- `max_cores` assume `1` quando omitido e deve ficar no intervalo `1..=8`.
- `mem_mb` define o tamanho padrão da RAM em megabytes (deve ser potência de 2, ex.: `16`, `64`, `128`). A flag `--mem` da CLI substitui este valor.
- No headless, `--pipeline` por enquanto suporta apenas `--cores 1`.
- Os valores de CPI são ciclos extras somados à latência de cache para a classe de instrução correspondente.

Exportar / importar pela TUI: **aba Config → `Ctrl+e` / `Ctrl+l`**

### `.pcfg` — configurações de pipeline

Controla o comportamento do pipeline usado pela aba de pipeline da TUI e pelo `raven run --pipeline`.

```ini
# Raven Pipeline Config v1
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

Campos:

- `enabled` — pipeline habilitado na TUI
- `bypass.ex_to_ex` — habilitar bypass EX->EX
- `bypass.mem_to_ex` — habilitar bypass MEM->EX
- `bypass.wb_to_id` — habilitar bypass WB->ID
- `bypass.store_to_load` — habilitar forwarding store-to-load
- `mode` — campo legado hoje mapeado na UI como `Serialized` ou `Parallel UFs`
- `fu.alu` / `fu.mul` / `fu.div` / `fu.fpu` / `fu.lsu` / `fu.sys` — quantidade de unidades funcionais de cada tipo usada no modo `Parallel UFs`
- `branch_resolve` — `Id`, `Ex` ou `Mem`
- `predict` — `NotTaken`, `Taken`, `Btfnt` ou `TwoBit`
- `speed` — velocidade de reprodução na TUI (`Slow`, `Normal`, `Fast`, `Instant`)

Exportar / importar pela TUI: **aba Pipeline → `Ctrl+e` / `Ctrl+l`**

---

## Códigos de saída

| Código | Significado |
|---|---|
| `0` | Sucesso |
| `1` | Erro de montagem, falha na simulação ou argumento inválido |
