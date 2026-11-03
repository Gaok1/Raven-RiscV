# Coerência de cache

> [Read in English](../en/coherence.md)

Quando vários cores têm cada um a sua própria cache, a mesma linha de memória pode estar em mais de uma delas. No instante em que um core escreve na sua cópia, os outros passam a ter dado obsoleto. Um **protocolo de coerência** é o conjunto de regras que as caches seguem para que todo core continue lendo o valor mais recente.

O RAVEN deixa você ligar e desligar caches privadas e um protocolo, e mostra cada passo: qual core pediu uma linha, o que cada cache fez com a sua cópia e quanto isso custou.

---

## Conteúdo

1. [O modelo de cache](#1-o-modelo-de-cache)
2. [Configurando a coerência](#2-configurando-a-coerência)
3. [Estados de linha](#3-estados-de-linha)
4. [Pedidos no barramento](#4-pedidos-no-barramento)
5. [Os três protocolos](#5-os-três-protocolos)
6. [Casos especiais](#6-casos-especiais)
7. [Quanto custa](#7-quanto-custa)
8. [Acompanhando na TUI](#8-acompanhando-na-tui)
9. [Contadores na saída da CLI](#9-contadores-na-saída-da-cli)
10. [Experimentos](#10-experimentos)
11. [Limites do modelo](#11-limites-do-modelo)

---

## 1. O modelo de cache

Há duas formas:

```
 coherence = shared (padrão)           coherence = none | msi | mesi | moesi

  core 0   core 1   core 2              core 0      core 1      core 2
     \       |       /                  ┌──────┐    ┌──────┐    ┌──────┐
      ┌─────────────┐                   │L1 I/D│    │L1 I/D│    │L1 I/D│
      │   L1 I / D  │                   └──┬───┘    └──┬───┘    └──┬───┘
      └──────┬──────┘                   ═══╪═══ barramento (snoop) ╪═══
          L2, L3…                            L2, L3… (compartilhados)
            RAM                                       RAM
```

- **`shared`** — todos os cores usam uma L1 I-cache e uma L1 D-cache. Cada linha tem uma única cópia, então não há o que manter coerente. É o que o RAVEN sempre fez, e continua sendo o padrão.
- **Qualquer outro valor** — cada core ganha a sua própria L1 I-cache e D-cache, todas com a forma definida na aba Cache. O L2 para baixo e a RAM continuam compartilhados. As D-caches ficam num **barramento com snooping**: todo miss, e toda escrita numa linha que outra cache pode ter, é um pedido que todas as outras D-caches veem e respondem.

Os harts se revezam: cada rodada executa uma instrução em cada core ativo (veja [hart.md](hart.md)). Um pedido é respondido antes da próxima instrução, então a máquina é sempre sequencialmente consistente: aqui a coerência trata de *qual cópia* um core lê e de *quanto isso custa*, não de reordenação.

---

## 2. Configurando a coerência

| Onde | Como |
|---|---|
| TUI | **Settings → MULTI-CORE CACHES**: `coherence` (←/→ ou Enter troca), `snoop latency`, `cache-to-cache`. Aumente `max cores` para 2 ou mais e ponha `run scope` em `ALL`. |
| `.rcfg` | Na seção `[cache]`: `coherence=`, `coherence.snoop_latency=`, `coherence.c2c_latency=` (veja [cache-config.md](cache-config.md#chaves-de-coerência)). |
| CLI | `raven run prog.s --cores 2 --coherence mesi` — sobrescreve o `coherence=` da config (veja [cli.md](cli.md)). |

| `coherence=` | L1 privadas | Protocolo | O que mostra |
|---|---|---|---|
| `shared` | não | — | uma cópia por linha; a referência |
| `none` | sim | nenhum | dado obsoleto: um core continua lendo a sua cópia antiga |
| `msi` | sim | MSI | o protocolo de invalidação mínimo |
| `mesi` | sim | MESI | + Exclusive: a primeira escrita numa linha não compartilhada é de graça |
| `moesi` | sim | MOESI | + Owned: uma linha suja é compartilhada sem write-back |

Trocar o protocolo reinicia a simulação, porque as caches privadas são montadas junto com a máquina. As duas latências só definem o custo dos pedidos seguintes e valem na hora.

---

## 3. Estados de linha

Cada linha de uma D-cache privada está num estado:

| Estado | Letra | Válida | Suja | Outras cópias | Significado |
|---|---|---|---|---|---|
| Modified | `M` | sim | sim | nenhuma | A única cópia, mais nova que a memória. |
| Owned | `O` | sim | sim | talvez, todas `S` | Mais nova que a memória e compartilhada. Esta cache responde pela linha e faz o write-back quando ela sai. Só MOESI. |
| Exclusive | `E` | sim | não | nenhuma | A única cópia, igual à memória. Uma escrita não precisa de pedido no barramento. MESI e MOESI. |
| Shared | `S` | sim | não | talvez | Uma de possivelmente várias cópias só de leitura. |
| Invalid | `I` | não | — | — | Não está na cache. |

O MSI usa `M S I`, o MESI `M E S I`, o MOESI os cinco. Na subaba **View** da aba Cache, a coluna da D-cache que mostrava o bit dirty mostra a letra no lugar, na cor do estado.

As I-caches guardam cópias só de leitura que o protocolo não acompanha; as linhas delas mostram `-` como antes.

---

## 4. Pedidos no barramento

| Pedido | Enviado quando | O que as outras caches fazem |
|---|---|---|
| `BusRd` | miss de leitura | Uma cópia suja (`M`/`O`) faz o write-back (MSI, MESI) ou manda o dado direto (MOESI). Cópias `M`/`E` viram `S` (MOESI: `M` → `O`). |
| `BusRdX` | miss de escrita (leitura para posse); também a leitura de um AMO | Toda cópia vira `I`; uma suja faz o write-back antes (MSI, MESI) ou manda o dado (MOESI). |
| `BusUpgr` | hit de escrita numa linha `S` ou `O` | Toda outra cópia vira `I`. Nenhum dado se move: quem escreve já o tem. |
| `BusWr` | uma escrita que vai ao barramento como está: todo store write-through, e um miss write-back sem write-allocate | Toda cópia vira `I`; uma suja faz o write-back antes, para que esse write-back depois não desfaça esta escrita. |
| `IFetch` | um miss da I-cache | Uma cópia suja da linha numa D-cache faz o write-back e vira `S`, então a busca lê os bytes atuais. |
| `WriteBack` | uma linha suja sai da cache (despejo ou `fence.i`) | Nada; fica registrado para você ver a linha sair. |

---

## 5. Os três protocolos

Eventos do processador (`PrRd`, `PrWr`) vêm do core dono da cache; eventos do barramento vêm dos outros.

### MSI

| Estado | PrRd | PrWr | BusRd observado | BusRdX / BusUpgr / BusWr observado |
|---|---|---|---|---|
| `I` | `BusRd` → `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | fica `S` | → `I` |
| `M` | hit | hit | write-back → `S` | write-back → `I` |

Uma linha lida sem outra cópia por perto continua `S`, então a primeira escrita nela custa um `BusUpgr`.

### MESI

| Estado | PrRd | PrWr | BusRd observado | BusRdX / BusUpgr / BusWr observado |
|---|---|---|---|---|
| `I` | `BusRd` → `E` se não há outra cópia, senão `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | fica `S` | → `I` |
| `E` | hit | **silenciosa** → `M` | → `S` | → `I` |
| `M` | hit | hit | write-back → `S` | write-back → `I` |

### MOESI

| Estado | PrRd | PrWr | BusRd observado | BusRdX / BusUpgr / BusWr observado |
|---|---|---|---|---|
| `I` | `BusRd` → `E` ou `S` | `BusRdX` → `M` | — | — |
| `S` | hit | `BusUpgr` → `M` | fica `S` | → `I` |
| `E` | hit | silenciosa → `M` | → `S` | → `I` |
| `O` | hit | `BusUpgr` → `M` | **manda o dado**, fica `O` | manda o dado → `I` (`BusWr`: write-back → `I`) |
| `M` | hit | hit | **manda o dado** → `O` | manda o dado → `I` (`BusWr`: write-back → `I`) |

A dona manda a linha de cache para cache em vez de fazer o write-back e deixar quem pediu ler do L2 ou da RAM. A memória fica desatualizada até a linha `O` ser despejada.

### `none`

Nenhum pedido. Cada L1 privada preenche do L2/RAM num miss e faz write-back no despejo, e nunca fica sabendo o que os outros cores fizeram. É o caso que os protocolos existem para corrigir.

---

## 6. Casos especiais

- **Write-through.** Todo store já vai para a memória, então vai ao barramento como `BusWr` e invalida as outras cópias. As linhas nunca ficam sujas, então os estados se reduzem a `S` e `I`; não há `E`, porque uma escrita precisaria do barramento de qualquer jeito.
- **Sem write-allocate.** Um store write-back que dá miss é um `BusWr`: uma cópia suja em outro lugar faz o write-back antes, e os bytes vão para o nível compartilhado (o L2, se houver, senão a RAM).
- **Atômicas.** Um AMO (`amoadd.w`, …) lê para posse: um `BusRdX` no miss, e a escrita dá hit na linha `M`. `lr.w`/`sc.w` continuam usando a tabela de reservas do RAVEN, que todo store de qualquer core limpa; isso independe do protocolo. Com `coherence=none` um AMO opera na cópia privada do core, então as atômicas entre cores também quebram — faz parte do que essa opção mostra.
- **Busca de instrução e `fence.i`.** Um miss da I-cache observa as D-caches dos outros cores, então código que outro core escreveu é buscado atualizado. Os próprios stores de um core ainda precisam de `fence.i` para chegar à sua I-cache, como a especificação RISC-V exige: `fence.i` grava as linhas sujas da D-cache do core no nível compartilhado (`M` → `E`, ou `S` no MSI; `O` → `S`) e descarta a sua I-cache.
- **Memória que uma syscall ou o depurador lê.** É lida como o core em execução a veria: primeiro a própria cópia, depois (com protocolo) a única cópia suja que outro core tem, depois L2/RAM. Com `none` um core não vê os dados sujos privados dos outros.

---

## 7. Quanto custa

| Evento | Ciclos, cobrados do core que fez o pedido |
|---|---|
| qualquer pedido no barramento (e o snoop por trás de um miss da I-cache) | `coherence.snoop_latency` (padrão 4) |
| uma cópia suja que fez write-back por causa dele | a miss penalty da L1 + a transferência da linha, como num despejo |
| uma linha mandada de cache para cache (MOESI) | `coherence.c2c_latency` (padrão 10), *no lugar* da ida ao L2/RAM |

Esses ciclos entram nos totais da L1 D-cache (ou I-cache) do core, então aparecem no AMAT, no CPI e no total do programa como qualquer outro custo de cache.

---

## 8. Acompanhando na TUI

1. Settings: `max cores` = 2, `run scope` = `ALL`, `coherence` = `MESI`.
2. Abra `Program Examples/harts/coherence_flag_handoff.s`, monte e execute passo a passo ou direto.
3. **Cache → view** mostra a L1 do core selecionado (`[` / `]` troca de core). O título do painel diz `Core N D-Cache`, e a letra do estado de cada linha substitui o bit dirty.
4. **Cache → coherence** mostra:
   - o protocolo e as suas latências;
   - **Bus traffic per core**: `BusRd`, `BusRdX`, `BusUpgr`, `BusWr`, invalidações recebidas, flushes (write-backs que outro core causou), transferências cache a cache, misses de coerência e ciclos de barramento;
   - **Recent lines**: o estado de cada linha pedida recentemente na D-cache de cada core;
   - **Bus requests**, do mais novo ao mais antigo, por exemplo
     `c1  BusRd  0x00008010  c1 I→S, c0 M→S  written back first`.

---

## 9. Contadores na saída da CLI

Com L1 privadas, todo formato de resultado acrescenta um par de L1 e um conjunto de contadores de barramento por core:

- **JSON** — um objeto `"coherence"`: `protocol`, `snoop_latency`, `c2c_latency` e `cores[]`, cada um com os contadores e os seus próprios objetos `icache`/`dcache`.
- **rstats** — chaves `coherence.*`, depois `coreN.<contador>` e `coreN.icache.*` / `coreN.dcache.*`.
- **CSV** — linhas `Core N I-Cache L1` / `Core N D-Cache L1` na tabela de caches, e uma tabela `COHERENCE (<protocolo>)`.

As entradas `icache`/`dcache` do topo são as do core 0.

| Contador | Significado |
|---|---|
| `bus_reads`, `bus_read_exclusive`, `bus_upgrades`, `bus_writes` | pedidos que este core enviou |
| `bus_transactions` | a soma deles |
| `invalidations_received` | linhas deste core que o pedido de outro core invalidou |
| `flushes` | linhas sujas que este core gravou porque outro core pediu |
| `cache_to_cache` | linhas que este core mandou direto para outra cache (MOESI) |
| `coherence_misses` | misses numa linha que a escrita de outro core tinha invalidado aqui |
| `bus_cycles` | ciclos que este core esperou no barramento |

---

## 10. Experimentos

### Dado obsoleto sem protocolo

```bash
raven run "Program Examples/harts/coherence_flag_handoff.s" --cores 2 --coherence none --max-cycles 200000
raven run "Program Examples/harts/coherence_flag_handoff.s" --cores 2 --coherence mesi
```

Com `none` a execução nunca termina: o core 1 leu `flag` para a sua L1 antes de o core 0 a ligar e fica girando nessa cópia. Com qualquer protocolo ela sai com 42.

| `coherence_flag_handoff.s` | ciclos | pedidos | write-backs | cache a cache | ciclos de barramento |
|---|---|---|---|---|---|
| `shared` | 772 | — | — | — | — |
| `msi` | 1376 | 14 | 4 | 0 | 292 |
| `mesi` | 1376 | 14 | 4 | 0 | 292 |
| `moesi` | 1000 | 14 | 0 | 4 | 124 |

O MOESI manda as linhas sujas de cache para cache em vez de fazer o write-back, e é aí que ele economiza ciclos.

### False sharing

`coherence_false_sharing.s` faz dois cores incrementarem cada um o seu contador 200 vezes. Com os contadores a 4 bytes um do outro, eles dividem uma linha de 16 bytes; a 16 bytes, não (mude `STRIDE`).

| `mesi` | ciclos | pedidos | invalidações | misses de coerência |
|---|---|---|---|---|
| mesma linha (`STRIDE` 4) | 50444 | 607 | 400 | 400 |
| linhas separadas (`STRIDE` 16) | 6558 | 8 | 0 | 0 |

Nenhum core lê o contador do outro, e mesmo assim a linha pula entre as duas caches a cada escrita.

### MSI contra MESI

Com os contadores em linhas separadas, o MSI precisa de um `BusUpgr` na primeira escrita de cada core (2 no total, 10 pedidos) e o MESI de nenhum (8 pedidos): a linha foi lida quando ninguém mais a tinha, então estava em `E` e a escrita foi silenciosa.

---

## 11. Limites do modelo

- Só barramento com snooping; não há diretório.
- Um pedido por vez e consistência sequencial: sem estados transitórios, buffers de escrita ou reordenação de memória.
- As I-caches não são invalidadas pelas escritas de outros cores; programas RISC-V usam `fence.i` para isso.
- O page-table walker lê a RAM diretamente (veja [virtual-memory.md](virtual-memory.md)), como já fazia com uma L1 só.
- O JIT executa blocos inteiros por vez, então os cores se intercalam de forma mais grossa que no interpretador. Use o interpretador (`--jit none`, o padrão) para estudar intercalações.
- O `--pipeline` headless roda um core só; pipelines com vários cores rodam na TUI.
