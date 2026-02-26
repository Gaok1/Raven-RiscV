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

## Referência de configuração (.fcache)

O Falcon consegue **importar/exportar** configurações de cache como um arquivo texto simples com pares `chave=valor`.

- Na aba **Cache**: `Ctrl+L` importa um `.fcache` e `Ctrl+E` exporta a configuração pendente.
- Linhas começando com `#` são comentários.
- As chaves/valores de enum são **case-sensitive** e precisam bater exatamente com os nomes abaixo.

O arquivo sempre tem duas configs:

- `icache.*` — cache de instruções (só leitura; políticas de escrita são ignoradas)
- `dcache.*` — cache de dados

Ao importar, o Falcon exige **todas as chaves** nas duas configs; se faltar algo, aparece um erro do tipo `Missing icache.<chave>` / `Missing dcache.<chave>`.

### Campos (o que significa cada termo)

Campos numéricos são inteiros em base 10 (então `1024` é válido; `0x400` não é).

- `size` (bytes)
  - Capacidade total do cache em bytes.
  - Aumentar `size` geralmente reduz **capacity misses** (cabe mais linha dentro do cache).
- `line_size` (bytes)
  - Quantos bytes existem em cada linha (bloco) do cache.
  - Neste simulador, um miss carrega a **linha inteira** da RAM, então `line_size` maior costuma ajudar em **localidade espacial** (acesso sequencial), mas pode desperdiçar tráfego em acesso aleatório.
- `associativity` (ways)
  - Quantas linhas cada set possui (1 = mapeamento direto; maior = por conjuntos; `sets=1` = totalmente associativo).
  - Aumentar associatividade geralmente reduz **conflict misses**, mas deixa o cache “mais caro”/complexo (no mundo real).
- `replacement` (enum)
  - Como escolher a vítima quando um set está cheio e um miss precisa instalar uma nova linha.
  - Valores aceitos: `Lru`, `Fifo`, `Random`, `Lfu`, `Clock`, `Mru`
    - `Lru`: remove a menos recentemente usada (bom default)
    - `Fifo`: remove a mais antiga instalada
    - `Random`: escolhe pseudo-aleatoriamente
    - `Lfu`: remove a menos frequentemente usada
    - `Clock`: algoritmo do relógio / segunda chance (aproxima LRU)
    - `Mru`: remove a mais recentemente usada (pode ser bom em “scans”)
- `write_policy` (apenas D-cache; enum)
  - O que acontece quando a CPU faz stores.
  - Valores aceitos: `WriteBack`, `WriteThrough`
    - `WriteThrough`: todo store escreve na RAM imediatamente (então `RAM W` tende a ser alto).
    - `WriteBack`: store atualiza a linha do cache e marca como **dirty**; a RAM só é atualizada depois, num eviction (então `RAM W` pode ficar bem menor até acontecer writeback).
- `write_alloc` (apenas D-cache; enum)
  - O que acontece em um **miss de store**.
  - Valores aceitos: `WriteAllocate`, `NoWriteAllocate`
    - `WriteAllocate` (write-allocate): no miss de store, o cache aloca/carrega a linha e então faz a escrita na linha.
    - `NoWriteAllocate` (write-around): no miss de store, não aloca linha (a escrita vai direto para a RAM).
- `hit_latency` (ciclos)
  - Custo em ciclos somado nas estatísticas quando há **hit**.
  - Alimenta as métricas `Cycles`, `Avg` e `CPI`.
- `miss_penalty` (ciclos)
  - Custo extra em ciclos somado quando há **miss** (a CPU “espera” a RAM).
  - Um setup didático comum é `hit_latency=1` e `miss_penalty=50` (hit=1 ciclo, miss≈51 ciclos).

### Lógica de mapeamento (tag / index / offset)

Aqui fica o “por que tem conflito?” de verdade.

Dado:

- `sets = size / (line_size * associativity)`
- `offset_bits = log2(line_size)`
- `index_bits = log2(sets)`

Um endereço é dividido assim:

```
[   tag   |   index   |  offset  ]
```

- `offset` escolhe o byte *dentro* da linha
- `index` escolhe o set
- `tag` identifica qual linha está instalada naquele set/way

Dois endereços entram em **conflito** se tiverem o mesmo `index` e tags diferentes (competem pelo mesmo set).

Truque útil:

- `stride_mesmo_set = sets * line_size` bytes

Os endereços `A` e `A + stride_mesmo_set` caem no **mesmo set** (tag diferente). É exatamente isso que `cache_conflict.fas` demonstra com a config padrão.

### Exemplo completo (padrão 1 KB, 16 B, 2-way)

Config padrão do D-cache (o preset embutido é bem próximo disso):

```
dcache.size=1024
dcache.line_size=16
dcache.associativity=2
```

Calculando:

- `bytes_por_set = line_size * associativity = 16 * 2 = 32 B`
- `sets = size / bytes_por_set = 1024 / 32 = 32` (potência de 2: OK)
- `offset_bits = log2(16) = 4`
- `index_bits = log2(32) = 5`
- `stride_mesmo_set = sets * line_size = 32 * 16 = 512 B`

Ou seja: endereços separados por 512 bytes brigam pelo mesmo set (ótimo para provocar conflito em aula).

### Regras de validação (explicadas)

A UI exige algumas regras para o mapeamento acima ficar simples e bem “hardware-like”:

- `line_size` precisa ser potência de 2 e `>= 4`
  - assim `offset_bits = log2(line_size)` é inteiro e as linhas ficam naturalmente alinhadas
- `size` precisa ser múltiplo de `(line_size * associativity)`
  - assim `sets` é inteiro (não existe “meio set”)
- `sets` precisa ser potência de 2
  - assim o índice pode ser calculado por máscara (`index = (addr >> offset_bits) & (sets - 1)`)

Se uma configuração pendente quebrar alguma regra, o botão **Apply** mostra o erro e não aplica.

## Arquivos de exemplo

Programas prontos para explorar o cache ficam em `Program Examples/`:

- `cache_locality.fas` — varredura de array pequeno vs grande (sequencial vs stride), com pausas para você dar Reset nas stats entre fases.
- `cache_conflict.fas` — padrão com 2 vs 3 linhas no mesmo set para mostrar conflict misses em cache por conjuntos.
- `cache_write_policy.fas` — loop de stores para comparar `WriteBack` vs `WriteThrough` e `NoWriteAllocate`.

Configs prontas para importar (`.fcache`):

- `cache_direct_mapped_1kb.fcache`
- `cache_large_4kb_4way.fcache`
- `cache_write_through.fcache`
- `cache_no_write_allocate.fcache`
