# Referência da CLI do Raven

> [Read in English](cli.md)

O Raven possui uma CLI completa para uso sem interface gráfica, em paralelo à TUI interativa.
Execute `raven help` a qualquer momento para ver um resumo.

---

## Subcomandos

| Subcomando | Descrição |
|---|---|
| `raven build <arquivo> [opções]` | Montar um arquivo fonte `.fas` |
| `raven run <arquivo> [opções]` | Montar e simular |
| `raven export-config [opções]` | Exportar a config padrão de cache (`.fcache`) |
| `raven import-config <arquivo> [opções]` | Validar e inspecionar um arquivo `.fcache` |
| `raven export-settings [opções]` | Exportar as configurações padrão de simulação (`.rcfg`) |
| `raven import-settings <arquivo> [opções]` | Validar e inspecionar um arquivo `.rcfg` |
| `raven help` | Exibir resumo de uso |

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
| `--settings <arquivo>` | padrões embutidos | Carrega configurações de simulação (CPI, cache_enabled) de um `.rcfg` |
| `--mem <tamanho>` | `16mb` | Tamanho da RAM — sufixos `kb`, `mb`, `gb` (ex.: `256kb`, `1gb`) |
| `--max-cycles <n>` | `1000000000` | Limite de instruções; um aviso é exibido se atingido |
| `--out <arquivo>` | stdout | Grava resultados da simulação em arquivo em vez do stdout |
| `--nout` | — | Suprime a saída de resultados (o stdout do programa ainda é exibido) |
| `--format json\|fstats\|csv` | `json` | Formato dos resultados |

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

# Aplicar configurações de simulação (ajuste de CPI, cache ligado/desligado)
raven run program.fas --settings my.rcfg --nout

# Executar com 64 MB de RAM
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

---

## `raven export-config`

Grava a configuração de cache padrão embutida em um arquivo `.fcache` para edição.

```
raven export-config [--out <arquivo>]
```

Se `--out` for omitido, a config é impressa no stdout.

```bash
raven export-config                        # imprimir no stdout
raven export-config --out default.fcache   # gravar em arquivo
```

---

## `raven import-config`

Analisa e valida um arquivo `.fcache`, imprime um resumo legível de cada nível de cache e, opcionalmente, reexporta a config normalizada.

```
raven import-config <arquivo> [--out <arquivo>]
```

```bash
raven import-config my.fcache
raven import-config my.fcache --out normalized.fcache
```

---

## `raven export-settings`

Grava as configurações de simulação padrão embutidas em um arquivo `.rcfg`.

```
raven export-settings [--out <arquivo>]
```

Se `--out` for omitido, as configurações são impressas no stdout.

```bash
raven export-settings                        # imprimir no stdout
raven export-settings --out default.rcfg     # gravar em arquivo
```

---

## `raven import-settings`

Analisa e valida um arquivo `.rcfg`, imprime um resumo de todas as configurações e, opcionalmente, reexporta a config normalizada.

```
raven import-settings <arquivo> [--out <arquivo>]
```

```bash
raven import-settings my.rcfg
raven import-settings my.rcfg --out normalized.rcfg
```

---

## Formatos de arquivo de configuração

### `.fcache` — hardware de cache

Descreve a hierarquia de cache: I-cache, D-cache e quaisquer níveis extras (L2, L3…).

Exportar / importar pela TUI: **aba Cache → `Ctrl+E` / `Ctrl+L`**

### `.rcfg` — configurações de simulação

Controla parâmetros globais de simulação: CPI por classe de instrução e se a cache está ativa.

```ini
# Raven Sim Config v1
cache_enabled=true

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
- Os valores de CPI são ciclos extras somados à latência de cache para a classe de instrução correspondente.

Exportar / importar pela TUI: **aba Config → `Ctrl+E` / `Ctrl+L`**

---

## Códigos de saída

| Código | Significado |
|---|---|
| `0` | Sucesso |
| `1` | Erro de montagem, falha na simulação ou argumento inválido |
