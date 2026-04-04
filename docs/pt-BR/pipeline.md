# Simulação de pipeline

O Raven inclui um simulador de pipeline educacional ciclo a ciclo para RV32I/M mais as extensões Falcon já suportadas pelo projeto. O objetivo não é apenas executar código, mas tornar visível *por que* uma instrução avançou, parou, foi descartada ou recebeu um valor encaminhado.

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

A configuração do pipeline atualmente expõe dois modelos de execução:

- `Serialized`
- `Parallel UFs`

Ambos usam o mesmo painel de unidades funcionais na UI. A diferença é semântica, não cosmética.

Na implementação atual, a execução ainda se comporta como um único caminho EX in-order, e `EX` pode permanecer ocupado por múltiplos ciclos dependendo da classe:

- `ALU`
- `MUL`
- `DIV`
- `LOAD`
- `STORE`
- `BRANCH`
- `JUMP`
- `SYSTEM`
- `FP`

Enquanto uma instrução de longa latência mantém o `EX` ocupado, a frente do pipeline permanece bloqueada e o Raven mantém esse estado visível sem deixar a latência de IF não relacionada progredir incorretamente. O painel de unidades funcionais decompõe essa latência por UF para que o usuário possa ver qual recurso está ativo e onde o paralelismo poderia existir quando o modelo de execução permitir.

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

- `Program Examples/pipeline_forwarding_demo.fas`
- `Program Examples/pipeline_load_use_demo.fas`
- `Program Examples/pipeline_branch_flush_demo.fas`
- `Program Examples/pipeline_cache_stall_demo.fas`

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
