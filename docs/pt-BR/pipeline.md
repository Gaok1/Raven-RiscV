# Simulação de pipeline

O Raven inclui um simulador de pipeline educacional ciclo a ciclo para RV32I/M mais as extensões de ensino do Raven. O objetivo não é apenas executar código, mas tornar visível *por que* uma instrução avançou, parou, foi descartada ou recebeu um valor encaminhado.

Este documento descreve o modelo atual implementado na aba Pipeline da TUI.

---

## Visão geral

O pipeline é um projeto clássico de 5 estágios:

1. `IF` — busca a instrução no `fetch_pc` atual
2. `ID` — decodifica, lê registradores e computa informações de controle adiantadas quando configurado
3. `EX` — ALU, condição de desvio, geração de endereço, execução MUL/DIV/FP, latência de unidade funcional
4. `MEM` — loads, stores, atômicos, latência de cache
5. `WB` — escreve resultados de volta e retira a instrução

Cada passo visível no pipeline corresponde exatamente a um ciclo de clock da CPU. A latência de cache é incorporada em stalls de pipeline, então o usuário nunca precisa "entrar dentro" de um cache separadamente.

---

## Comportamento dos estágios

### IF

- Busca através do I-cache, não diretamente da RAM
- Armazena a latência por busca no slot IF
- Mantém a instrução em `IF` enquanto os ciclos restantes do I-cache são consumidos
- Marca instruções buscadas como especulativas quando uma instrução de controle já está em execução

### ID

- Decodifica a palavra de instrução
- Lê registradores fonte inteiros ou de ponto flutuante
- Aplica encaminhamento nos operandos decodificados quando habilitado
- Relê os operandos quando uma instrução permanece parada em `ID`
- Pode resolver desvios em `ID` quando a resolução de desvios está configurada ali

### EX

- Computa resultados da ALU
- Avalia desvios e saltos
- Computa endereços efetivos para loads/stores/atômicos
- Aplica encaminhamento novamente para consumidores no estágio EX
- Mantém instruções pela latência `CPIConfig` configurada em ambos os modos de pipeline
- Sempre renderiza o painel de unidades funcionais na TUI para que as oportunidades de execução permaneçam visíveis

### MEM

- Executa acesso à memória para loads/stores/atômicos
- Usa o tempo do D-cache para cada acesso
- Converte a latência total de acesso em ciclos de stall visíveis no pipeline
- Aplica encaminhamento de dados de store antes do acesso à memória quando necessário

### WB

- Escreve o resultado final nos registradores inteiros ou FP
- Trata `ecall`, `halt` e `ebreak`
  `exit/exit_group` param o programa inteiro; `halt` para o hart atual permanentemente, enquanto `ebreak`
  é uma pausa de debug resumível
- Executa as instruções de sistema cujo efeito as instruções mais novas precisam ver: Zicsr (`csrrw`,
  `csrrs`, `csrrc` e as formas com imediato), `mret`, `sret`, `fence.i` e `sfence.vma`. A busca fica parada
  desde que uma delas é buscada até ela ser retirada, e retoma no lugar certo — `mepc`/`sepc` depois de um
  retorno, o código recém-escrito depois de `fence.i`, traduções novas depois de `sfence.vma` ou de uma
  escrita em `satp`
- Só levanta uma busca que falhou (palavra inválida, erro de barramento, page fault de instrução) se essa
  instrução chegar aqui: o front end corre à frente dos desvios, e uma busca num caminho que o programa
  nunca toma — depois do último `ret`, por exemplo — é descartada sem deixar rastro. Um page fault entra no
  tratador de trap
- Um `ecall` que precisa esperar — por uma linha de entrada, ou em `screen_sleep_ms` — não é retirado: fica
  estacionado e executa de novo quando a execução continua, exatamente como no modo sequencial
- Conta a instrução como retirada apenas aqui

---

## Hazards modelados pelo Raven

### Hazards RAW

Hazards de leitura-após-escrita são detectados entre instruções produtoras e consumidoras em execução.

- Com encaminhamento habilitado:
  - O Raven emite um trace de encaminhamento e deixa o consumidor prosseguir quando o valor já está disponível
  - Um verdadeiro load-use ainda para até que o valor exista
- Com encaminhamento desabilitado:
  - O Raven para o `ID` até que o produtor tenha escrito de volta com segurança

### Barreira ABI de syscall (`ecall`)

O `ecall` é modelado conservadoramente como uma fronteira ABI privilegiada para os registradores de argumento/resultado inteiros:

- é tratado como lendo todos os `a0..a7`
- instruções mais jovens que consomem `a0..a7` devem esperar até que o `ecall` se retire
- instruções mais velhas que ainda estão produzindo `a0..a7` também podem parar um `ecall` em `ID`

Esta regra é intencionalmente mais forte que o encaminhamento mínimo porque os tratadores de syscall podem:

- consumir argumentos de `a0..a7`
- retornar valores em `a0`
- atualizar estados visíveis do simulador de formas que não devem competir com instruções próximas

Na prática, isso faz o `ecall` se comportar como uma barreira de pipeline conservadora em torno do banco de argumentos ABI, evitando bugs de operando obsoleto em código pesado em syscalls ou gerado em tempo de execução.

### Hazards load-use

Estes são tratados especialmente porque um valor carregado só fica disponível após `MEM`.

Instruções cobertas atualmente incluem:

- `lb`, `lh`, `lw`, `lbu`, `lhu`
- `flw`
- `lr.w`

### Hazards WAW e WAR

O Raven reporta hazards `WAW` e `WAR` como traces informativos para que o usuário possa ver dependências de nomes sobrepostas mesmo quando elas não forçam um stall neste design in-order.

### Hazards de controle

Desvios e saltos podem:

- redirecionar a busca através de predição
- marcar instruções mais jovens como especulativas
- descartar estágios mais jovens em caso de predição errada

A visão Gantt/histórico distingue uma instrução descartada de uma bolha normal.

### Stalls de cache

- Latência do I-cache para o `IF`
- Latência do D-cache para o `MEM`
- Se o `MEM` já está bloqueando o pipeline, o `IF` não queima silenciosamente seus próprios ciclos de stall pendentes em background
- A UI rotula estes separadamente dos stalls de dados: uma instrução válida pode ser mostrada aguardando em `IF`/`MEM`, enquanto `ID` pode mostrar uma espera de upstream/front-end se nenhuma instrução nova chegou

---

## Modelo de encaminhamento

Quando o encaminhamento está habilitado, o Raven pode desviar valores de estágios mais velhos para consumidores mais jovens:

- `EX/MEM/WB -> ID`
- `MEM/WB -> EX`
- `WB -> MEM` para casos de store-data

O encaminhamento é rastreado em dois lugares:

- badges e avisos de estágio dentro do painel de pipeline
- o Mapa de Hazards / Encaminhamento abaixo, que mostra explicitamente os estágios produtor e consumidor

O Raven também distingue um RAW coberto por encaminhamento de um RAW verdadeiro que produz stall.

---

## Predição de desvios e comportamento de flush

A predição estática é configurável nas configurações do pipeline.

Opções disponíveis:

- `NotTaken` — sempre prediz não-tomado (padrão)
- `AlwaysTaken` — sempre prediz tomado
- `BTFNT` — *Backward Taken, Forward Not Taken*
- `2-bit Dynamic` — contador saturante de 2 bits por instrução de desvio

Comportamento atual:

- a predição de desvio/salto é anexada à instrução assim que ela atinge `ID`
- o fluxo de controle predito-tomado redireciona o `fetch_pc`
- instruções mais jovens no caminho errado são marcadas como especulativas
- em caso de discordância, o Raven descarta os estágios mais jovens e redireciona para o alvo real

Marcadores visuais:

- instruções preditas recebem badges de predição
- instruções descartadas recebem marcadores de flush/squash
- bolhas de front-end e esperas de busca são rotuladas separadamente dos stalls de instrução
- o mapa de hazards mostra os caminhos de flush de controle separadamente dos hazards de dados

---

## Modelo de execução

A configuração do pipeline expõe quatro modelos de execução (`mode=` na seção `[pipeline]` de um arquivo de configuração):

| Modo | Tag na configuração | O que modela |
|---|---|---|
| `Serialized` | `Serialized` | Uma instrução em `EX` por vez. Uma instrução de longa latência ocupa o `EX` pela latência configurada e a frente do pipeline espera atrás dela. |
| `Parallel UFs` | `ParallelUFs` | O `ID` despacha para o banco de unidades funcionais (ALU, MUL, DIV, FPU, LSU, SYS, cada uma com um número configurável de instâncias), então instruções independentes se sobrepõem em unidades diferentes. A emissão e a retirada continuam em ordem de programa. Loads e stores — incluindo `flw`/`fsw` — passam pela LSU e pelo `MEM`. |
| `Scoreboard` | `Scoreboard` | Escalonamento dinâmico ao estilo do CDC 6600, sem renomeação: um WAW para na emissão e um WAR é protegido na escrita do resultado. |
| `Tomasulo + ROB` | `TomasuloRob` | Estações de reserva com renomeação de registradores e um buffer de reordenação: a execução corre fora de ordem, o commit continua em ordem e as exceções continuam precisas. |

Os quatro usam a mesma declaração de unidades funcionais e as mesmas latências (`CPIConfig`), então trocar de modelo muda o escalonamento, não a máquina. Em `Scoreboard` e `Tomasulo + ROB` uma instrução altera a máquina de uma vez, no commit, em ordem de programa — é isso que mantém as exceções precisas — e o front end recomeça depois de qualquer instrução que moveu o PC sem ser um desvio (um trap, `mret`, `sret`) e depois de toda instrução de sistema.

Enquanto uma instrução de longa latência ocupa uma unidade, o painel mostra qual recurso está ocupado e por quanto tempo, para que o usuário veja onde existe paralelismo e qual modelo o aproveita.

---

## Interação com o cache

O pipeline e o cache agora compartilham um modelo de clock:

- um acesso retorna uma latência total
- o pipeline converte essa latência em `N` ciclos de stall visíveis
- estatísticas por nível permanecem acumuladas no modelo de cache como custo de serviço local
- o tempo permanece visível no modelo de pipeline

Isso se aplica a:

- stalls de busca do I-cache em `IF`
- stalls do D-cache em `MEM`
- níveis de cache extras quando configurados
- toda latência paga no caminho de acesso, incluindo níveis externos e trabalho de writeback/fill

A barra lateral da RAM também distingue a presença do cache por fonte:

- `I1` = L1 instruction cache
- `D1` = L1 data cache
- `U2`, `U3`, ... = níveis de cache externos unificados

---

## Programas de exemplo recomendados

Os seguintes programas de exemplo são destinados especificamente para inspeção de pipeline:

- `Program Examples/pipeline/pipeline_forwarding_demo.s`
- `Program Examples/pipeline/pipeline_load_use_demo.s`
- `Program Examples/pipeline/pipeline_branch_flush_demo.s`
- `Program Examples/pipeline/pipeline_cache_stall_demo.s`

Fluxo sugerido:

1. Abra o programa no editor
2. Monte e carregue
3. Alterne para a aba Pipeline
4. Avance ciclo a ciclo
5. Observe o Mapa de Hazards / Encaminhamento e o histórico Gantt juntos

---

## Notas e limitações

- O simulador é intencionalmente didático, não uma afirmação de comportamento de silício com precisão de ciclo
- `WAW` e `WAR` são reportados visualmente mesmo quando nenhum stall é necessário
- Níveis de cache externos unificados são compartilhados entre tráfego de instrução e dados, por isso a barra lateral da RAM os rotula como `U2+`
- A execução pela CLI valida o mesmo caminho de assembler/execução, mas os traces de pipeline gráficos são exclusivos da TUI
