# Raven — Arquivo de Configuração de Cache (`.fcache`)

Este guia explica cada campo de um arquivo de configuração de cache do Raven para que você (ou um LLM) possa escrever configs válidas do zero. Importe qualquer `.fcache` no Raven via **Cache → ↓ Import cfg**.

---

## Formato do arquivo

Texto simples, um par `chave=valor` por linha.

```
# Linhas começando com # são comentários — ignorados pelo Raven
chave=valor
```

- Espaços em branco ao redor do `=` são removidos.
- Chaves desconhecidas são silenciosamente ignoradas (compatibilidade futura).
- Valores de enum são **PascalCase sensível a maiúsculas** (ex: `Lru`, não `lru`).
- A extensão do arquivo deve ser `.fcache`.

---

## Cabeçalho

Todo arquivo deve declarar quantos níveis extras (além do L1) existem.

```
# Raven Cache Config v2
levels=N
```

| Chave | Tipo | Significado |
|-------|------|-------------|
| `levels` | inteiro ≥ 0 | Número de níveis de cache unificados adicionados além do L1 (`0` = apenas L1, `1` = L1+L2, `2` = L1+L2+L3, …) |

> Se `levels` estiver ausente, o parser assume `0` (apenas L1 — compatibilidade com v1).

---

## Prefixos de nível

Cada nível é identificado por um prefixo que é adicionado antes de cada nome de campo com um ponto.

| Prefixo | Nível | Tipo |
|---------|-------|------|
| `icache` | L1 Instruction Cache | Separado (sempre presente) |
| `dcache` | L1 Data Cache | Separado (sempre presente) |
| `l2` | L2 Unified Cache | Extra (requer `levels≥1`) |
| `l3` | L3 Unified Cache | Extra (requer `levels≥2`) |
| `l4` | L4 Unified Cache | Extra (requer `levels≥3`) |
| `lN` | LN Unified Cache | Extra (requer `levels≥N-1`) |

Então `icache.size=4096` define o tamanho do L1-I para 4 KB, e `l2.line_size=64` define o tamanho de linha do L2 para 64 bytes.

---

## Campos

Todo nível (icache, dcache, l2, l3, …) aceita o mesmo conjunto de campos.

### Geometria

| Chave | Tipo | Faixa válida | Notas |
|-------|------|-------------|-------|
| `size` | inteiro (bytes) | 64 – 1 048 576, **potência de 2** | Capacidade total do cache. Deve ser igual a `line_size × associativity × num_sets` onde `num_sets` também é potência de 2. |
| `line_size` | inteiro (bytes) | 4 – 512, **potência de 2** | Tamanho de um bloco/linha de cache. Linhas maiores reduzem a taxa de miss em acesso sequencial, mas aumentam o tráfego de penalidade de miss. |
| `associativity` | inteiro | 1 – 16 | Número de vias por conjunto. `1` = mapeamento direto, `N` = N-way set-associative. Deve satisfazer `associativity × line_size ≤ size`. |

**Derivado (somente leitura, não no arquivo):**
`num_sets = size / (line_size × associativity)` — deve ser potência de 2 ou o Raven rejeitará a config.

**Fórmula de verificação rápida:**
```
sets = size / (line_size * associativity)   → deve ser potência de 2
```

Exemplo: `size=4096, line_size=32, associativity=4` → `sets = 4096/128 = 32` ✓

---

### Temporização

| Chave | Tipo | Faixa válida | Padrão | Notas |
|-------|------|-------------|--------|-------|
| `hit_latency` | inteiro (ciclos) | 1 – 999 | — | Ciclos consumidos em cada hit do cache. |
| `miss_penalty` | inteiro (ciclos) | 0 – 9999 | — | Ciclos de stall **extras** adicionados em um miss do cache (em cima do `hit_latency`). Modela o tempo para buscar do próximo nível ou RAM. |
| `assoc_penalty` | inteiro (ciclos) | 0 – 99 | `1` | Ciclos extras por via adicional durante a busca de tag. `(associativity - 1) × assoc_penalty` é adicionado ao `hit_latency`. Defina como `0` para modelar busca de tag totalmente paralela. |
| `transfer_width` | inteiro (bytes) | 1 – 512 | `8` | Largura do barramento entre este nível e o de baixo. O custo de transferência = `ceil(line_size / transfer_width)` ciclos, adicionado automaticamente à penalidade de miss. |

> **Fórmula AMAT usada pelo Raven:**
> `AMAT = hit_latency + assoc_penalty*(associativity-1) + miss_rate * (miss_penalty + ceil(line_size/transfer_width))`

---

### Política de substituição

Chave: `replacement`
Tipo: enum (uma string exata da tabela abaixo)

| Valor | Regra de despejo | Melhor para |
|-------|-----------------|-------------|
| `Lru` | Least Recently Used — despeja a via não acessada há mais tempo. | Cargas de trabalho de uso geral |
| `Mru` | Most Recently Used — despeja a linha acessada mais recentemente. | Padrões de scan/streaming que não devem poluir o cache |
| `Fifo` | First In First Out — despeja a linha instalada há mais tempo. | Previsível, simples em hardware |
| `Lfu` | Least Frequently Used — despeja a via com menos acessos (empates resolvidos por LRU). | Padrões de acesso com distribuição de frequência assimétrica |
| `Clock` | Clock / Second-Chance — ponteiro circular com bit de referência por linha. | Aproximação de LRU com menor custo em hardware |
| `Random` | Pseudo-aleatório via LCG. | Análise de pior caso; evita thrashing patológico de LRU |

---

### Política de escrita

Chave: `write_policy`
Tipo: enum

| Valor | Significado |
|-------|-------------|
| `WriteBack` | As escritas ficam no cache e são propagadas para o próximo nível somente na evicção. Reduz tráfego de escrita; requer bits dirty. |
| `WriteThrough` | Cada escrita é imediatamente encaminhada para o próximo nível. Mais simples; sem bits dirty; maior tráfego de escrita. |

---

### Política de alocação em escrita

Chave: `write_alloc`
Tipo: enum

| Valor | Significado |
|-------|-------------|
| `WriteAllocate` | Em um miss de escrita, uma nova linha é buscada no cache antes de escrever. Funciona naturalmente com `WriteBack`. |
| `NoWriteAllocate` | Em um miss de escrita, a escrita é enviada diretamente ao próximo nível sem alocar uma linha. Comum com `WriteThrough`. |

**Combinações convencionais:**

| `write_policy` | `write_alloc` | Notas |
|----------------|---------------|-------|
| `WriteBack` | `WriteAllocate` | Padrão para L1/L2 em CPUs modernas |
| `WriteThrough` | `NoWriteAllocate` | Comum para caches L1 simples ou pequenos |
| `WriteBack` | `NoWriteAllocate` | Incomum, mas válido |
| `WriteThrough` | `WriteAllocate` | Incomum; alto tráfego |

---

### Política de inclusão (somente L2 e acima)

Chave: `inclusion`
Tipo: enum
Padrão: `NonInclusive`

| Valor | Significado |
|-------|-------------|
| `NonInclusive` | Sem restrição — uma linha pode ou não existir em ambos os níveis simultaneamente. Padrão para a maioria das configs. |
| `Inclusive` | Toda linha neste nível está **garantidamente** também no nível abaixo. Simplifica a coerência; desperdiça capacidade. |
| `Exclusive` | Uma linha vive em **exatamente um** nível. Quando buscada para o L1, é despejada do L2 (modelo de cache vítima). |

> `inclusion` só é significativo para L2 e acima. Em `icache`/`dcache` é parseado mas não tem efeito.

---

## Configuração de CPI (opcional)

Controla o modelo de latência por classe de instrução. Se omitido, o Raven usa valores padrão.

- Modo sequencial: esses valores contribuem para o modelo serial de CPI/ciclos-totais.
- Modo pipeline: esses valores tornam-se latência de estágio e comportamento de stall dentro do clock-wall do pipeline.

```
# --- CPI Config ---
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

| Chave | Padrão | Significado |
|-------|--------|-------------|
| `cpi.alu` | `1` | Instruções ALU inteiras (add, sub, and, or, …) |
| `cpi.mul` | `3` | Multiplicação inteira (mul, mulh, …) |
| `cpi.div` | `20` | Divisão inteira (div, rem, …) |
| `cpi.load` | `0` | Overhead extra por load além do AMAT do cache |
| `cpi.store` | `0` | Overhead extra por store além do custo do cache |
| `cpi.branch_taken` | `3` | Custo de flush de pipeline quando o desvio é tomado |
| `cpi.branch_not_taken` | `1` | Custo quando o desvio não é tomado |
| `cpi.jump` | `2` | Custo de jal / jalr |
| `cpi.system` | `10` | ecall / ebreak |
| `cpi.fp` | `5` | Instruções de ponto flutuante (se emuladas) |

Todos os valores são inteiros sem sinal ≥ 0.

---

## Regras de validação

O Raven rejeitará a config e mostrará um erro se qualquer uma delas falhar:

1. `line_size` deve ser potência de 2 e ≥ 4.
2. `size` deve ser potência de 2.
3. `associativity ≥ 1`.
4. `associativity × line_size ≤ size` (pelo menos um conjunto deve existir).
5. `num_sets = size / (line_size × associativity)` deve ser potência de 2.

Potências de 2 para lembrar: 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576.

---

## Exemplos completos

### Apenas L1 (mínimo)

```
# Raven Cache Config v2
levels=0

icache.size=1024
icache.line_size=16
icache.associativity=2
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=50
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=1024
dcache.line_size=16
dcache.associativity=2
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=50
dcache.assoc_penalty=1
dcache.transfer_width=8
```

---

### L1 + L2

```
# Raven Cache Config v2
levels=1

icache.size=4096
icache.line_size=32
icache.associativity=4
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=10
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=4096
dcache.line_size=32
dcache.associativity=4
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=10
dcache.assoc_penalty=1
dcache.transfer_width=8

l2.size=131072
l2.line_size=64
l2.associativity=8
l2.replacement=Lru
l2.write_policy=WriteBack
l2.write_alloc=WriteAllocate
l2.inclusion=NonInclusive
l2.hit_latency=10
l2.miss_penalty=200
l2.assoc_penalty=2
l2.transfer_width=16
```

---

### L1 + L2 + L3

```
# Raven Cache Config v2
levels=2

icache.size=4096
icache.line_size=32
icache.associativity=4
icache.replacement=Lru
icache.write_policy=WriteBack
icache.write_alloc=WriteAllocate
icache.hit_latency=1
icache.miss_penalty=10
icache.assoc_penalty=1
icache.transfer_width=8

dcache.size=4096
dcache.line_size=32
dcache.associativity=4
dcache.replacement=Lru
dcache.write_policy=WriteBack
dcache.write_alloc=WriteAllocate
dcache.hit_latency=1
dcache.miss_penalty=10
dcache.assoc_penalty=1
dcache.transfer_width=8

l2.size=131072
l2.line_size=64
l2.associativity=8
l2.replacement=Lru
l2.write_policy=WriteBack
l2.write_alloc=WriteAllocate
l2.inclusion=NonInclusive
l2.hit_latency=10
l2.miss_penalty=30
l2.assoc_penalty=2
l2.transfer_width=16

l3.size=4194304
l3.line_size=64
l3.associativity=16
l3.replacement=Lru
l3.write_policy=WriteBack
l3.write_alloc=WriteAllocate
l3.inclusion=Inclusive
l3.hit_latency=30
l3.miss_penalty=300
l3.assoc_penalty=3
l3.transfer_width=32
```

---

## Cartão de referência rápida

```
Campo            Tipo        Valores válidos
───────────────  ──────────  ──────────────────────────────────────────
size             inteiro     potência de 2, 64–1048576
line_size        inteiro     potência de 2, 4–512
associativity    inteiro     1–16
replacement      enum        Lru | Mru | Fifo | Lfu | Clock | Random
write_policy     enum        WriteBack | WriteThrough
write_alloc      enum        WriteAllocate | NoWriteAllocate
inclusion        enum        NonInclusive | Inclusive | Exclusive
hit_latency      inteiro     1–999
miss_penalty     inteiro     0–9999
assoc_penalty    inteiro     0–99  (padrão 1)
transfer_width   inteiro     1–512 (padrão 8)
```
