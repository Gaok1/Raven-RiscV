# Simulação de cache

O Falcon inclui um simulador simples de **I-cache + D-cache**, com estatísticas ao vivo e uma UI interativa para configuração.

Abra a aba **Cache** para acessar duas subtabs:

- **Stats** — inspecionar hit rate, padrões de miss, tráfego de RAM e custo em ciclos.
- **Config** — ajustar tamanho/linha/associatividade e policies.

## Cache → Stats

### Métricas (por cache)

- Gauge **Hit%** — `hits / (hits + misses)`.
- **H / M / MR / MPKI**
  - `H`: hits (contagem)
  - `M`: misses (contagem)
  - `MR`: miss rate (%)
  - `MPKI`: misses por 1000 instruções (`misses / instruções * 1000`)
- **Acc / Evict / WB / Fills**
  - `Acc`: acessos totais (`hits + misses`)
  - `Evict`: evictions (contagem)
  - `WB`: writebacks (apenas D-cache)
  - `Fills`: line fills (derivado de `bytes_loaded / line_size`)
- **RAM R / RAM W**
  - `RAM R`: bytes lidos da RAM por line fills (`bytes_loaded`)
  - `RAM W`: bytes realmente escritos na RAM (`ram_write_bytes`)
- **CPU Stores** (apenas D-cache) — bytes escritos pela CPU via stores (`bytes_stored`).
- **Cycles / Avg / CPI**
  - `Cycles`: custo acumulado em ciclos de acessos ao cache
  - `Avg`: média de ciclos por acesso (`cycles / acessos`)
  - `CPI`: ciclos por instrução (`cycles / instruções`) — “contribuição de CPI” deste cache

### Top Miss PCs (I-Cache)

A tabela mostra quais **PCs de fetch** causaram misses no I-cache (ordenado por contagem). Use Up/Down (ou a roda do mouse) para rolar.

### Controles

- **Reset** (`r`) — zera as estatísticas do cache (incluindo `miss_pcs` e `ram_write_bytes`).
- **Pause/Resume** (`p`) — pausa/retoma a simulação (as estatísticas do cache param de atualizar enquanto estiver pausado).
- **View scope** — exibir I-cache, D-cache ou ambos.

### O que entra em “RAM W”?

`RAM W` conta **bytes escritos na RAM**, incluindo:

- stores em write-through
- writebacks de linhas dirty (write-back) em evictions
- misses de store em write-back + no-write-allocate que escrevem direto na RAM

Bytes “dirty” ainda dentro de uma linha de write-back **não** contam até serem escritos de volta.

## Cache → Config

A subtab Config mostra um painel para o **I-cache** e outro para o **D-cache**.

### Edição

- Clique num **campo numérico**, digite números, `Backspace` apaga, `Enter` confirma, `Esc` cancela.
- Em **enums**, clique para alternar ou use `◄/►` (Left/Right).
- Use `Tab` / `↑` / `↓` para navegar entre campos enquanto edita.
- Valores em amarelo indicam mudanças **pendentes** (diferentes da config ativa).

### Presets

Use **Small / Medium / Large** para carregar rapidamente presets.

### Apply

- **Apply + Reset Stats** — recria os caches com a config pendente e zera stats/histórico.
- **Apply Keep History** — recria os caches mas mantém o **histórico do gráfico** (contadores zeram).

### Regras de validação (obrigatórias)

- `line_size` é potência de 2 e `>= 4`
- `size` é múltiplo de `(line_size * associativity)`
- `sets = size / (line_size * associativity)` é potência de 2

Obs.: write policies só fazem diferença no **D-cache** (o I-cache é somente leitura).
