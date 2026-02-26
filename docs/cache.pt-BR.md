# Simulação de cache

O Falcon inclui um simulador simples de **I-cache + D-cache**, com estatísticas ao vivo e uma UI interativa para configuração.

Abra a aba **Cache** para acessar três subtabs:

- **Stats** — inspecionar hit rate, padrões de miss, tráfego de RAM e custo em ciclos.
- **View** — visualizar o conteúdo interno das linhas (sets × ways) em tempo real. Use ↑↓ para rolar verticalmente e ←→ (ou scroll horizontal do touchpad) para rolar horizontalmente quando houver muitas ways.
- **Config** — ajustar tamanho/linha/associatividade e policies.

---

## O que é uma cache? (conceito geral)

Uma **cache** é uma memória pequena e rápida que guarda cópias de dados recentemente acessados da RAM (lenta). Quando o processador acessa um endereço:

- **Hit** — o dado está na cache → acesso rápido (custo = `hit_latency` ciclos)
- **Miss** — o dado não está na cache → a linha inteira é buscada da RAM → acesso lento (custo = `hit_latency + miss_penalty` ciclos)

A eficiência da cache depende do **princípio de localidade**:
- **Temporal**: se acessei o endereço X agora, provavelmente vou acessar X de novo em breve.
- **Espacial**: se acessei X, provavelmente vou acessar X+4, X+8, … em breve.

---

## Lógica de mapeamento (tag / index / offset)

Esta é a parte fundamental de qualquer cache de hardware.

### Decomposição do endereço

Todo endereço de 32 bits é dividido em três campos:

```
 31                 ...        offset+index   offset    0
┌──────────────────────────────┬─────────────┬──────────┐
│             TAG              │    INDEX    │  OFFSET  │
└──────────────────────────────┴─────────────┴──────────┘
```

Dados os parâmetros da cache:

```
sets        = size / (line_size × associativity)
offset_bits = log₂(line_size)
index_bits  = log₂(sets)
tag_bits    = 32 - offset_bits - index_bits
```

- **OFFSET** (bits `[offset_bits-1 : 0]`) — escolhe o **byte dentro da linha**. Se `line_size=8` → offset_bits=3 → bits [2:0].
- **INDEX** (bits `[offset_bits+index_bits-1 : offset_bits]`) — seleciona **qual set** da cache. Se `sets=4` → index_bits=2 → bits [4:3].
- **TAG** (bits `[31 : offset_bits+index_bits]`) — identifica **qual bloco de memória** está naquele set/way.

### Exemplo com números reais

Config: `size=32, line_size=8, associativity=1`

```
sets        = 32 / (8 × 1) = 4
offset_bits = log₂(8) = 3  →  bits [2:0]
index_bits  = log₂(4) = 2  →  bits [4:3]
tag_bits    = 32 - 3 - 2   = 27 → bits [31:5]
```

Endereço `0x1000` = `0001 0000 0000 0000` em binário:

```
TAG    = 0x1000 >> 5 = 0x80   (bits 31..5)
INDEX  = (0x1000 >> 3) & 0x3  = 0  → Set 0
OFFSET = 0x1000 & 0x7         = 0  → byte 0 dentro da linha
```

A **linha carregada** cobre os endereços `0x1000–0x1007` (todos os bytes com o mesmo TAG+INDEX).

### Dois endereços entram em conflito quando...

...têm o mesmo INDEX mas tags diferentes. Isso significa que ambos querem o **mesmo set**, mas carregam dados de regiões distintas da memória.

```
stride_mesmo_set = sets × line_size
```

Os endereços `A` e `A + stride_mesmo_set` sempre caem no mesmo set. Com `size=32, line_size=8, assoc=1`: stride = 4 × 8 = **32 bytes**. Ou seja, `0x0000` e `0x0020` são rivais no mesmo set.

---

## Tipos de associatividade

A **associatividade** define quantas linhas (ways) existem em cada set, e portanto como o hardware resolve conflitos.

### 1. Mapeamento direto (`associativity = 1`)

Cada set tem **exatamente 1 way**. Cada endereço mapeia para exatamente um slot na cache. Dois endereços com o mesmo INDEX sempre se expulsam.

```
Endereço A   → [ Set 2 | Way 0 ]  ← única opção
Endereço B   → [ Set 2 | Way 0 ]  ← mesma opção! → miss garantido se A e B são acessados alternadamente
```

**Prós:** hardware simples, barato, acesso de 1 ciclo (sem comparar tags de múltiplas ways).
**Contras:** sofredor de **conflict misses** — dois endereços rivais se expulsam infinitamente, mesmo que a cache tenha capacidade livre em outros sets.

Experimente com `cache_conflict.fas` e `cache_direct_mapped_1kb.fcache`.

### 2. Mapeamento por conjuntos (`associativity = N`, com N > 1)

Cada set tem **N ways**. O hardware testa todos os N tags em paralelo. Um novo endereço pode entrar em qualquer das N ways vagas — a política de substituição escolhe a vítima quando o set está cheio.

```
Endereço A   → [ Set 2 | Way 0 ✓ ]   ← A entra na way 0
Endereço B   → [ Set 2 | Way 1 ✓ ]   ← B entra na way 1 → sem conflito!
Endereço C   → [ Set 2 | ? ]         ← set cheio → eviction por política (LRU, FIFO…)
```

**Prós:** elimina a maioria dos conflict misses.
**Contras:** hardware mais complexo (compara N tags, precisa de política de substituição).

Valores típicos: 2-way, 4-way, 8-way. Use `cache_large_4kb_4way.fcache` para experimentar.

### 3. Totalmente associativo (`associativity = sets` → `sets = 1`)

Há **um único set** com todas as ways. Qualquer linha pode ir para qualquer lugar. Sem INDEX — o hardware compara o endereço com **todos** os tags em paralelo.

```
sets = size / (line_size × associativity) = 1   →   index_bits = 0
```

**Prós:** sem conflict misses.
**Contras:** circuito de comparação muito grande para caches grandes; usado principalmente em TLBs e caches pequenas especiais.

Para simular no Falcon: configure `associativity` igual ao número de linhas (`size / line_size`), e o simulador terá `sets=1`.

---

## Tipos de miss

| Tipo | Também chamado | Quando ocorre |
|------|---------------|---------------|
| **Cold miss** | Compulsory miss | Primeira vez que um endereço é acessado (a linha nunca foi carregada) |
| **Capacity miss** | — | A cache é pequena demais para o working set; linhas são evictadas antes de serem reutilizadas |
| **Conflict miss** | Interference miss | Dois endereços rivais (mesmo INDEX) se expulsam repetidamente, mesmo com capacidade livre em outros sets |

**Como identificar no simulador:**
- Cold misses: execute o programa uma vez; os primeiros acessos a cada linha sempre geram miss.
- Capacity misses: aumente `size` e veja se o miss rate cai significativamente.
- Conflict misses: aumente `associativity` (mantendo `size`) e veja se o miss rate cai.

---

## Políticas de substituição (`replacement`)

Quando um set está cheio e ocorre um miss, o hardware precisa **escolher uma vítima** (uma linha para expulsar). As políticas disponíveis são:

| Política | Estratégia | Uso típico |
|----------|-----------|-----------|
| `Lru` | Remove a **menos recentemente usada** (Least Recently Used) | Default em CPUs modernas |
| `Fifo` | Remove a **mais antiga instalada** (First In, First Out) | Mais simples que LRU |
| `Random` | Escolhe **pseudo-aleatoriamente** | Fácil de implementar em hardware |
| `Lfu` | Remove a **menos frequentemente usada** (Least Frequently Used) | Bom quando frequência é previsível |
| `Clock` | Aproxima LRU com um "ref bit" por linha; varre em círculo | Usado em OSes para TLB/page eviction |
| `Mru` | Remove a **mais recentemente usada** | Útil em varreduras sequenciais (scan) |

**LRU** é geralmente o mais eficiente para padrões de acesso típicos. **MRU** pode surpreender: numa varredura sequencial grande, a linha mais recente é a que menos vai ser reutilizada.

Na **View** do Falcon, cada linha exibe metadados da política:
- `r:0` = posição mais recente (LRU/FIFO) ou a ser evictada (MRU)
- `f:N` = frequência de acesso (LFU)
- `>R` = ponteiro do relógio + ref bit setado (Clock)

---

## Políticas de escrita (`write_policy` e `write_alloc`)

Essas configurações só afetam o **D-cache** (o I-cache é somente leitura).

### Write-Back vs Write-Through

O que acontece quando a CPU executa um **store** (escrita)?

#### Write-Through

A escrita vai **simultaneamente** para a cache E para a RAM.

```
CPU: sw t0, 0(t1)
  → cache[set][way].data atualizado
  → RAM[endereço] atualizado  ← imediatamente
```

**RAM W** sobe a cada store. Simples de implementar, mas gera tráfego intenso na RAM.

#### Write-Back

A escrita vai **apenas para a cache**; a linha fica marcada como **dirty (D)**. A RAM só é atualizada quando a linha dirty for evictada.

```
CPU: sw t0, 0(t1)
  → cache[set][way].data atualizado
  → linha marcada dirty (D = amarelo na View)
  → RAM intacta por enquanto

  ... mais tarde, em eviction:
  → linha dirty escrita na RAM (writeback)
  → RAM W sobe apenas agora
```

**Prós:** muito menos tráfego na RAM enquanto há localidade temporal.
**Contras:** complexidade de hardware (precisa rastrear dirty bits).

### Write-Allocate vs No-Write-Allocate

O que acontece quando um store causa **miss**?

#### Write-Allocate (write-allocate)

No miss de store: aloca a linha na cache (faz um line fill da RAM), depois atualiza a linha.

```
CPU: sw t0, 0(t1)  ← miss no endereço X
  → carrega linha contendo X da RAM para a cache  (line fill)
  → atualiza o byte em cache
  → (write-back: marca dirty; write-through: escreve na RAM também)
```

**Vantagem:** se a CPU escrever naquele mesmo endereço mais tarde → hit.
**Combinação típica:** `WriteBack + WriteAllocate`.

#### No-Write-Allocate (write-around)

No miss de store: **não** aloca linha na cache; escreve direto na RAM.

```
CPU: sw t0, 0(t1)  ← miss no endereço X
  → RAM[X] atualizado diretamente
  → cache não muda
```

**Vantagem:** não polui a cache com dados que provavelmente não serão lidos.
**Combinação típica:** `WriteThrough + NoWriteAllocate`.

### Combinações comuns

| `write_policy`  | `write_alloc`     | Comportamento                                              |
|-----------------|-------------------|------------------------------------------------------------|
| WriteBack       | WriteAllocate     | Padrão moderno; minimiza RAM W; ideal para loops de leitura+escrita |
| WriteThrough    | NoWriteAllocate   | Simples; alto RAM W; cache limpa (sem dirty lines)         |
| WriteThrough    | WriteAllocate     | Incomum; fill na cache + escrita na RAM em cada store      |
| WriteBack       | NoWriteAllocate   | Incomum; útil em streams de escrita sem releitura          |

Experimente com `cache_write_policy.fas` e os arquivos `.fcache` correspondentes para ver o impacto no `RAM W` e nos contadores de writeback.

---

## Cache → Stats

### Métricas (por cache)

- Gauge **Hit%** — `hits / (hits + misses)`.
- **H / M / MR / MPKI**
  - `H`: hits (contagem)
  - `M`: misses (contagem)
  - `MR`: miss rate (%)
  - `MPKI`: misses por 1000 instruções (`misses / instruções × 1000`)
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
  - `CPI`: ciclos por instrução (`cycles / instruções`) — "contribuição de CPI" deste cache

### Top Miss PCs (I-Cache)

A tabela mostra quais **PCs de fetch** causaram misses no I-cache (ordenado por contagem). Use Up/Down (ou a roda do mouse) para rolar.

### Controles

- **Reset** (`r`) — zera as estatísticas do cache (incluindo `miss_pcs` e `ram_write_bytes`).
- **Pause/Resume** (`p`) — pausa/retoma a simulação (as estatísticas do cache param de atualizar enquanto estiver pausado).
- **View scope** (`i`/`d`/`b`) — exibir I-cache, D-cache ou ambos.

### O que entra em "RAM W"?

`RAM W` conta **bytes escritos na RAM**, incluindo:

- stores em write-through
- writebacks de linhas dirty (write-back) em evictions
- misses de store em write-back + no-write-allocate que escrevem direto na RAM

Bytes "dirty" ainda dentro de uma linha de write-back **não** contam até serem escritos de volta.

---

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

### Regras de validação (explicadas)

A UI exige essas regras para que o mapeamento tag/index/offset fique simples e "hardware-like":

- `line_size` potência de 2 → `offset_bits = log₂(line_size)` é inteiro; linhas naturalmente alinhadas
- `size` múltiplo de `(line_size × associativity)` → `sets` é inteiro (sem "meio set")
- `sets` potência de 2 → o índice pode ser calculado por máscara: `index = (addr >> offset_bits) & (sets - 1)`

Se uma configuração pendente quebrar alguma regra, o botão **Apply** mostra o erro e não aplica.

---

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

- `size` (bytes) — capacidade total do cache em bytes. Aumentar `size` geralmente reduz **capacity misses**.
- `line_size` (bytes) — quantos bytes em cada linha (bloco). Linha maior ajuda em **localidade espacial** (acesso sequencial), mas pode desperdiçar tráfego em acesso aleatório.
- `associativity` (ways) — número de ways por set. 1 = mapeamento direto; `sets=1` = totalmente associativo. Aumentar associatividade reduz **conflict misses**.
- `replacement` (enum) — política de substituição. Valores: `Lru`, `Fifo`, `Random`, `Lfu`, `Clock`, `Mru`.
- `write_policy` (apenas D-cache) — `WriteBack` ou `WriteThrough`. Veja seção acima.
- `write_alloc` (apenas D-cache) — `WriteAllocate` ou `NoWriteAllocate`. Veja seção acima.
- `hit_latency` (ciclos) — custo em ciclos num hit. Alimenta `Cycles`, `Avg` e `CPI`.
- `miss_penalty` (ciclos) — custo extra em ciclos num miss. Um setup didático comum: `hit_latency=1`, `miss_penalty=50`.

### Exemplo completo (padrão 1 KB, 16 B, 2-way)

```
dcache.size=1024
dcache.line_size=16
dcache.associativity=2
```

Calculando:

```
bytes_por_set = line_size × associativity = 16 × 2 = 32 B
sets          = size / bytes_por_set = 1024 / 32 = 32
offset_bits   = log₂(16) = 4
index_bits    = log₂(32) = 5
tag_bits      = 32 - 4 - 5 = 23
stride_mesmo_set = sets × line_size = 32 × 16 = 512 B
```

Endereços separados por 512 bytes brigam pelo mesmo set — ótimo para provocar conflict misses em aula.

---

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
