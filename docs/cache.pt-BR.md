# Simulação de cache

O Falcon inclui um simulador de **I-cache + D-cache** integrado. Enquanto seu programa roda, você pode acompanhar o cache se preenchendo, observar hits e misses em tempo real e experimentar configurações diferentes para entender seus trade-offs.

Abra a aba **Cache** para ver três subtabs:

- **Stats** — hit rate, contagem de misses, tráfego de RAM e custo em ciclos para cada cache.
- **View** — mapa ao vivo de todos os slots do cache: quais endereços estão armazenados, quais linhas estão sujas e o que a política de substituição está fazendo.
- **Config** — ajustar tamanho, tamanho de linha, associatividade, política de escrita e parâmetros de latência.

---

## O que é uma cache?

Uma **cache** é uma memória pequena e rápida entre o processador e a RAM. Em vez de ir até a RAM (lenta) a cada acesso, o processador verifica a cache primeiro.

- **Hit** — o dado já está na cache → rápido (poucos ciclos)
- **Miss** — o dado não está → o processador carrega uma **linha inteira** da RAM → lento (dezenas de ciclos)

A cache funciona porque programas tendem a reutilizar dados:

- **Localidade temporal** — se você leu um endereço agora, provavelmente vai lê-lo de novo em breve.
- **Localidade espacial** — se você leu o endereço X, provavelmente vai ler X+4, X+8, … em breve (eles compartilham a mesma linha de cache).

Execute `cache_locality.fas` com a configuração padrão para ver os dois efeitos ao vivo.

---

## Como a cache é organizada

### Sets, ways e linhas

A cache é dividida em **sets**, cada um com **N ways** (slots). Quando o processador acessa um endereço:

1. O endereço seleciona um **set** específico (um pequeno grupo de slots).
2. Todos os N ways daquele set são verificados simultaneamente.
3. **Hit** — encontrado. **Miss** — não encontrado: um slot é liberado (eviction) e a nova linha é carregada da RAM.

```
Endereço → [ tag | índice do set | offset ]
                        ↓
              Set 5 → Way 0 | Way 1 | Way 2 | Way 3
                      verificar os 4 slots ao mesmo tempo
```

- **Offset** — qual byte dentro da linha (depende do tamanho da linha).
- **Índice do set** — em qual grupo de slots procurar (depende do número de sets).
- **Tag** — identifica de qual região da memória a linha veio.

Você pode ver a tag (`T:XXXX`) e os bytes de dados de cada slot ao vivo na subtab **View**.

### Tamanho da linha

Um miss carrega uma **linha inteira** — não só o byte pedido. É aí que entra a localidade espacial: se seu código lê um array sequencialmente, o primeiro elemento causa um miss, mas os próximos vêm de graça da mesma linha.

- **Linha maior** → melhor para acesso sequencial; desperdiça largura de banda para acesso aleatório.

### Número de sets

Mais sets → menos endereços competem pelo mesmo slot → menos **conflict misses**. Alterar o Tamanho total (mantendo Tamanho de Linha e Associatividade constantes) muda o número de sets.

---

## Tipos de associatividade

**Associatividade** é o número de ways por set — quantas linhas podem coexistir no mesmo set.

### Mapeamento direto (1-way)

Cada endereço tem **exatamente um slot**. Se dois endereços caem no mesmo set, eles ficam se expulsando.

```
addr 0x0000 → set 0, way 0  (única opção)
addr 0x0400 → set 0, way 0  (mesmo slot! → conflito)
```

Hardware simples, mas vulnerável a **conflict misses**. Experimente com `cache_conflict.fas` + `cache_direct_mapped_1kb.fcache`.

### Mapeamento por conjuntos (N-way, N > 1)

Cada set tem N slots. Uma nova linha ocupa qualquer slot livre; só quando o set está cheio a política de substituição escolhe uma vítima.

```
addr 0x0000 → set 0, way 0  ✓
addr 0x0400 → set 0, way 1  ✓  (sem conflito!)
addr 0x0800 → set 0, ?      → set cheio → eviction
```

Mais ways → menos conflict misses, um pouco mais de trabalho por acesso. Use `cache_large_4kb_4way.fcache` para experimentar.

### Totalmente associativo

Quando a Associatividade é igual ao número total de linhas, há apenas um set e qualquer linha pode ir para qualquer lugar. Sem conflict misses, mas caro demais para caches grandes — usado principalmente em caches pequenas especiais.

---

## Tipos de miss

| Tipo | Quando ocorre | Como investigar |
|------|---------------|-----------------|
| **Cold miss** | Primeiro acesso a qualquer endereço — a linha nunca foi carregada | Sempre ocorre no início do programa; inevitável |
| **Capacity miss** | O working set é maior que a cache; linhas são evictadas antes de serem reutilizadas | Aumente o **Tamanho** — se o miss rate cair, a capacidade era o gargalo |
| **Conflict miss** | Dois endereços no mesmo set ficam se expulsando | Aumente a **Associatividade** (mesmo Tamanho) — se o miss rate cair, conflitos eram o problema |

---

## Políticas de substituição

Quando um set está cheio e ocorre um miss, o hardware precisa escolher uma vítima para expulsar. Políticas disponíveis:

| Política | Expulsa... | Observações |
|----------|-----------|-------------|
| **LRU** | A menos recentemente usada | Padrão na maioria das CPUs; geralmente a melhor escolha |
| **FIFO** | A linha instalada há mais tempo | Mais simples que LRU; desempenho similar |
| **LFU** | A menos frequentemente acessada | Boa quando a frequência de acesso é estável |
| **Clock** | Uma linha não usada recentemente (aproxima LRU) | Usado em substituição de páginas de SO |
| **MRU** | A mais recentemente usada | Surpreendentemente boa para varreduras sequenciais grandes |
| **Random** | Uma linha aleatória | Hardware simples; desempenho médio razoável |

### Lendo a subtab View

Cada slot mostra um pequeno indicador que revela o que a política de substituição está "pensando." As cores fazem o trabalho principal: **ciano** = slot está seguro, **vermelho** = slot será o próximo a ser expulso.

| Política | Indicador | O que significa |
|----------|-----------|-----------------|
| **LRU** | `r:N` | Rank de recência — 0 = acabou de ser usado **(ciano, seguro)**, maior = mais antigo **(vermelho, evict next)** |
| **FIFO** | `r:N` | Ordem de chegada — 0 = mais novo **(ciano, seguro)**, maior = mais antigo **(vermelho, evict next)** |
| **MRU** | `r:N` | Rank de recência, mas invertido — 0 = acabou de ser usado **(vermelho, será expulso!)**, maior = mais antigo **(ciano, seguro)** |
| **LFU** | `f:N` | Contagem de acessos — o slot com a **menor contagem fica vermelho** (evict next) |
| **Clock** | `>` / `R` | `>` = ponteiro do relógio está aqui; `R` = usado recentemente (protegido); `>` sem `R` = será expulso |
| **Random** | `??` | Sem ordenação — vítima escolhida aleatoriamente |

---

## Políticas de escrita (apenas D-cache)

Essas configurações controlam o que acontece quando a CPU executa uma instrução de **store**.

### Write-Back (padrão)

O store atualiza **apenas a linha de cache**; a linha fica marcada como **dirty** (indicada pelo `D` amarelo na subtab View). A RAM só é atualizada quando essa linha dirty for eventualmente evictada.

- Muito menos tráfego na RAM quando a mesma variável é escrita várias vezes.
- Observe os sinalizadores `D` na View e o contador `WB` em Stats.

### Write-Through

Cada store atualiza **tanto a cache quanto a RAM** imediatamente. Nenhuma linha fica dirty.

- Mais fácil de raciocinar, mas `RAM W` cresce a cada store.
- Use `cache_write_policy.fas` para comparar `RAM W` entre os dois modos.

### Write-Allocate vs No-Write-Allocate

O que acontece quando um **store causa miss** (a linha alvo não está na cache)?

- **Write-Allocate** — carrega a linha na cache primeiro, depois escreve. Melhor quando o mesmo endereço será lido ou escrito novamente.
- **No-Write-Allocate** — escreve diretamente na RAM, sem passar pela cache. Bom para streams de escrita que não serão relidos.

Combinações comuns: **Write-Back + Write-Allocate** (padrão moderno) ou **Write-Through + No-Write-Allocate**.

---

## Subtab Stats

### Métricas por cache

| Display | Significado |
|---------|-------------|
| Gauge Hit% | Fração dos acessos que encontraram o dado na cache |
| `H` / `M` | Contagem de hits / Contagem de misses |
| `MR` | Miss rate (%) |
| `MPKI` | Misses por 1000 instruções — quanto menor, melhor |
| `Acc` | Total de acessos |
| `Evict` | Linhas removidas para abrir espaço |
| `WB` | Writebacks: linhas dirty gravadas na RAM na eviction (apenas D-cache) |
| `Fills` | Linhas carregadas da RAM (uma por miss) |
| `RAM R` | Total de bytes lidos da RAM (fills de linha) |
| `RAM W` | Total de bytes escritos na RAM (stores write-through + evictions dirty) |
| `CPU Stores` | Bytes escritos por instruções store (apenas D-cache) |
| `Cycles` | Total de ciclos gastos em operações de cache |
| `Avg` | Média de ciclos por acesso |
| `CPI` | Contribuição deste cache ao CPI geral |
| `Cost model` | Custo de hit e miss com a configuração atual |

### Barra de resumo do programa

Mostra os totais de **ambas** as caches: ciclos totais, CPI geral, contagem de instruções e as contribuições individuais de I-cache e D-cache.

### Top Miss PCs

Lista quais endereços de instrução causaram mais misses no I-cache. Use ↑↓ para rolar.

### Controles

- `r` — **Reset** todos os contadores e histórico.
- `p` — **Pausar / Retomar** a simulação.
- `i` / `d` / `b` — Alternar a visão: só I-cache, só D-cache ou **A**mbos.

---

## Subtab Config

### Como editar

- **Clique em um número** para começar a editar; digite dígitos, `Backspace` para corrigir, `Enter` para confirmar, `Esc` para cancelar.
- **Setas ◄ ►** (ou teclas Left/Right) alternam as opções em campos de política.
- `Tab` / `↑` / `↓` navegam entre campos durante a edição.
- Valores em **amarelo** indicam mudanças pendentes ainda não aplicadas à cache ativa.

### Campos

| Campo | O que controla |
|-------|----------------|
| Size | Capacidade total em bytes — maior → menos capacity misses |
| Line Size | Bytes por linha de cache — maior → melhor localidade espacial para acesso sequencial; desperdiça largura de banda para acesso aleatório. Deve ser potência de 2 e ≥ 4 |
| Associativity | Ways por set — 1 = mapeamento direto; maior → menos conflict misses |
| Replacement | Qual linha expulsar quando o set está cheio |
| Write Policy | Write-Back ou Write-Through (apenas D-cache) |
| Write Alloc | O que fazer num store miss: alocar na cache ou escrever direto na RAM (apenas D-cache) |
| Hit Latency | Ciclos para um hit de cache — aumente para modelar caches mais lentas |
| Miss Penalty | Ciclos extras esperando pela RAM num miss — faixa típica: 50–200 |
| Assoc Penalty | Ciclos extras por way adicional (custo de verificar mais tags) — padrão: 1 |
| Transfer Width | Largura do barramento de dados em bytes — mais largo = menos ciclos para transferir uma linha — padrão: 8 B |

### Presets

**Small / Medium / Large** carregam configurações pré-construídas — bons pontos de partida para experimentos.

### Apply

- **Apply + Reset Stats** — ativa a configuração e zera todos os contadores. Use para comparações antes/depois limpas.
- **Apply Keep History** — ativa a configuração mas mantém o gráfico de hit rate para sobreposição.

### Validação

O tamanho de linha deve ser potência de 2, e o tamanho total deve ser divisível em um número inteiro de sets. Se uma configuração pendente for inválida, o Apply mostra uma explicação do que precisa mudar.

---

## Salvar e carregar configurações (.fcache)

Use **Ctrl+E** (exportar) e **Ctrl+L** (importar) na aba Cache. O formato é texto simples `chave=valor` — você pode abrir e editar em qualquer editor de texto.

---

## Programas de exemplo

Todos os exemplos estão em `Program Examples/`:

- `cache_locality.fas` — acesso sequencial vs com stride; observe o hit rate mudar conforme o padrão de acesso varia.
- `cache_conflict.fas` — dois endereços no mesmo set que se expulsam mesmo com capacidade livre em outros sets.
- `cache_write_policy.fas` — loop com muitos stores; compare Write-Back vs Write-Through observando o `RAM W`.

Configs prontas para importar:

- `cache_direct_mapped_1kb.fcache`
- `cache_large_4kb_4way.fcache`
- `cache_write_through.fcache`
- `cache_no_write_allocate.fcache`
