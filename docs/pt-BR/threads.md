# Escalonamento de threads

> [Read in English](../en/threads.md)

Sem escalonador, cada hart que um programa inicia ocupa um core livre e fica com ele até terminar, e `hart_start` falha quando todos os cores estão ocupados. Esse é o modelo descrito em [Hardware threads](hart.md), e continua sendo o padrão (`sched=off`).

Ligando um escalonador, as harts viram **threads** que dividem os cores. O programa pode iniciar mais threads do que há cores: a thread que não encontra core livre espera na **fila de prontos**, e o escalonador decide qual thread cada core executa. Com um core e três threads dá para ver FIFO, round robin e prioridade acontecendo instrução por instrução, e ler os tempos de espera, de resposta e de turnaround que uma disciplina de escalonamento pede.

---

## 1. Políticas

| `sched` | Política | Uma thread em execução deixa o core quando… |
|---|---|---|
| `off` | nenhuma (padrão) | termina. Uma hart por core. |
| `fifo` | primeiro a chegar, primeiro a ser servido | termina, bloqueia em `thread_join` ou chama `thread_yield`. Nunca é preemptada. |
| `rr` | round robin | como no FIFO, ou já executou um **quantum** inteiro enquanto outra thread espera. Vai para o fim da fila. |
| `priority` | prioridade preemptiva | como no round robin entre threads de mesma prioridade, ou uma thread **mais urgente** fica pronta: essa toma o core na hora. Número menor é mais urgente. |

Toda thread começa com prioridade `10`, ou com a do pai se o pai mudou a própria. Em `priority`, quando mais de um core pode ser tomado, perde o core primeiro a thread em execução menos urgente.

## 2. Ticks, quantum e custo da troca

O escalonador conta o tempo em **ticks**. Um tick é uma rodada do simulador: cada core com thread executa uma instrução (modo sequencial) ou um ciclo de clock (pipeline). Ao fim de cada tick o escalonador:

1. contabiliza onde cada thread passou o tick: executando, esperando na fila de prontos ou bloqueada;
2. tira do core toda thread que terminou ou bloqueou;
3. dá a cada core ocioso a thread que espera há mais tempo (em `priority`: a mais urgente);
4. passa o core de uma thread que cedeu a vez, ou esgotou o quantum, para a próxima thread da fila, se houver;
5. em `priority`, deixa uma thread pronta mais urgente preemptar uma em execução menos urgente.

| Configuração | Padrão | Significado |
|---|---|---|
| `sched_quantum` | `50` | Ticks que uma thread executa antes de o round robin passar o core adiante. Só `rr` e `priority` usam. |
| `sched_switch_cost` | `0` | Ticks que um core gasta em cada troca de contexto sem executar thread nenhuma: salvando os registradores de uma thread e carregando os da próxima. |

Uma thread iniciada durante um tick consegue um core, no mínimo, ao fim desse tick. Uma thread sozinha no core nunca é preemptada à toa: se ninguém espera, o quantum dela simplesmente recomeça.

Com o pipeline, a troca de contexto esvazia o pipeline do core e volta a buscar instruções a partir do PC da nova thread. Nada se perde: uma instrução só altera a máquina no commit, então a thread antiga busca de novo o que estava em voo quando voltar a executar.

## 3. Syscalls

| `a7` | Nome | Argumentos | Retorno |
|---|---|---|---|
| `1100` | `hart_start` | `a0` PC de entrada, `a1` topo da pilha, `a2` argumento | id da thread, ou `-2` (PC inválido) / `-3` (pilha inválida). Com escalonador nunca falha por falta de core. |
| `1101` | `hart_exit` | — | não retorna; as threads que fizeram join nesta acordam |
| `1103` | `thread_yield` | — | `0`. Passa o core para a próxima thread da fila; continua executando se ninguém espera. |
| `1104` | `thread_join` | `a0` id da thread | `0` quando essa thread terminar; `-1` para id desconhecido ou o da própria thread |
| `1105` | `thread_set_priority` | `a0` id da thread, `a1` prioridade | `0`, ou `-1` para id desconhecido |

O `sched_yield` do Linux (124) é o mesmo pedido que `thread_yield`. O id de uma thread é o id de hart que ela lê em `mhartid`; a primeira thread do programa é a `0`.

Sem escalonador as três chamadas novas continuam funcionando: `thread_yield` não tem para quem passar o core, `thread_set_priority` só confere o argumento, e `thread_join` deixa a hart parada no próprio core até o alvo terminar.

Se todas as threads que restam estão bloqueadas em `thread_join` (A faz join em B enquanto B faz join em A), nenhuma volta a executar: a execução para com um erro de **deadlock** dizendo quem espera por quem.

## 4. Exemplo

[`Program Examples/harts/thread_scheduling.s`](../../Program%20Examples/harts/thread_scheduling.s) inicia três workers, A, B e C, e faz join em cada um. Cada worker faz quatro rodadas de trabalho e imprime sua letra depois de cada rodada:

```
raven run thread_scheduling.s --cores 1 --sched fifo             # AAAABBBBCCCC
raven run thread_scheduling.s --cores 1 --sched rr --quantum 20  # ABCABCABCABC
raven run thread_scheduling.s --cores 1 --sched rr --quantum 200 # AABBCCAABBCC
raven run thread_scheduling.s --cores 2 --sched rr --quantum 20
raven run thread_scheduling.s --cores 1 --sched rr --quantum 20 --switch-cost 10 --format csv
```

Para experimentar:

- Diminua o quantum e veja as trocas de contexto e o tempo de espera subirem enquanto o tempo de resposta cai.
- Aumente `--switch-cost`: a utilização cai, e com um quantum minúsculo os cores passam mais tempo trocando do que executando.
- Em `priority`, dê prioridade `1` a um worker com `thread_set_priority`: ele termina primeiro, e depois os outros dividem o core por round robin.
- Troque um `thread_join` por uma espera ativa numa flag: com um core, a thread que gira gasta o quantum inteiro a cada vez. Um `thread_yield` dentro do laço resolve.

## 5. Na TUI

**Settings → THREAD SCHEDULING** tem três linhas: `scheduler` (off / FIFO / round robin / priority; mudar reinicia a simulação), `quantum` e `switch cost` (valem na hora). Com escalonador, um passo avança todos os cores em um tick, seja qual for o run scope.

A aba **Threads** mostra:

- a política, a contagem de ticks, as trocas de contexto, as preempções, a utilização dos cores e as médias de espera, resposta e turnaround;
- uma linha por thread: estado (`READY`, `RUN`, `SWITCH`, `JOIN Tn`, `EXIT`), core, prioridade, PC, chegada, primeira execução, fim, ticks executando / esperando / bloqueada, resposta, turnaround, despachos, preempções e yields;
- uma linha do tempo por core: todos os ticks até agora comprimidos na largura, cada coluna com a thread que executou (`0`-`9`, `a`-`z`), `·` durante a troca e `─` ocioso;
- a fila de prontos, da frente para trás, o que cada core está fazendo e quanto do quantum já usou;
- os eventos de escalonamento mais recentes: inícios, despachos, preempções, yields, joins e términos.

O seletor de core das abas Run e Pipeline continua escolhendo um **core**. Os registradores e o pipeline que ele mostra são da thread que esse core executa no momento.

## 6. Na CLI e no `.rcfg`

```
raven run program.s --sched rr --quantum 20 --switch-cost 2 --cores 1
```

`--sched`, `--quantum` e `--switch-cost` sobrescrevem as chaves do `[sim]`:

```ini
[sim]
max_cores=1
sched=rr
sched_quantum=20
sched_switch_cost=2
```

Sem TUI o escalonamento roda em modo sequencial; `--pipeline` junto com escalonador é recusado. `--max-cycles` conta ticks.

Todo formato de resultado traz a contabilidade do escalonador: um objeto `"scheduler"` no JSON (política, médias, ticks ocupados/trocando/ociosos por core e uma entrada por thread), uma seção `[sched]` no rstats (chaves `sched.*`, `coreN.*_ticks` e `threadN.*`) e as tabelas `SCHEDULER`, `CORES` e `THREADS` no CSV.

## 7. Métricas

| Métrica | Definição |
|---|---|
| chegada | tick em que a thread foi iniciada |
| primeira execução | tick em que executou pela primeira vez num core |
| fim | tick em que terminou |
| resposta | primeira execução − chegada |
| turnaround | fim − chegada |
| espera | ticks na fila de prontos |
| bloqueada | ticks em `thread_join` |
| despachos | vezes em que foi colocada num core |
| preempções | vezes em que o escalonador tirou o core dela (quantum, ou thread mais urgente) |
| utilização | fração dos core-ticks gastos executando uma thread, em vez de ociosos ou trocando |

As médias consideram todas as threads; o turnaround médio considera as que terminaram. Quando `exit` encerra o programa, as threads ainda vivas ficam com o estado que tinham.

---

## Veja também

- [Hardware threads (harts)](hart.md): iniciar harts, pilhas, `max_cores`
- [Referência de syscalls](syscalls.md): a tabela completa
- [Coerência de cache](coherence.md): o que threads em cores diferentes veem das escritas umas das outras
