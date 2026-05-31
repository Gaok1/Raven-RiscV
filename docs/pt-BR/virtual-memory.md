# Memória Virtual e TLB (Sv32)

> [Read in English](../en/virtual-memory.md)

Este documento é material de estudo sobre **memória virtual no RISC-V** (esquema **Sv32**) e sobre a **TLB** que o simulador coloca na frente dela. A intenção é ensinar do zero: *por que* memória virtual existe, *como* a tradução acontece, *o que* o TLB resolve, e como você pode experimentar tudo isso na aba TLB do Raven.

Você não precisa abrir nenhum código pra ler este texto. Tudo é apresentado em termos de conceitos, diagramas e programas de exemplo em assembly.

---

## Sumário

1. [Por que memória virtual existe](#1-por-que-memória-virtual-existe)
2. [O modelo conceitual: VA → PA](#2-o-modelo-conceitual-va--pa)
3. [Tabelas de página: do array gigante à árvore multinível](#3-tabelas-de-página-do-array-gigante-à-árvore-multinível)
4. [Sv32 em detalhes](#4-sv32-em-detalhes)
5. [O formato do PTE](#5-o-formato-do-pte)
6. [Megapáginas (superpages)](#6-megapáginas-superpages)
7. [Bits A e D — accessed e dirty](#7-bits-a-e-d--accessed-e-dirty)
8. [Permissões e níveis de privilégio](#8-permissões-e-níveis-de-privilégio)
9. [Por que existe o TLB](#9-por-que-existe-o-tlb)
10. [Organização do TLB: sets, ways, indexação](#10-organização-do-tlb-sets-ways-indexação)
11. [Políticas de substituição](#11-políticas-de-substituição)
12. [ASIDs e a flag global](#12-asids-e-a-flag-global)
13. [`sfence.vma` e coerência da TLB](#13-sfencevma-e-coerência-da-tlb)
14. [Page faults: causas e fluxo do trap](#14-page-faults-causas-e-fluxo-do-trap)
15. [Modo padrão: experimentar sem boilerplate](#15-modo-padrão-experimentar-sem-boilerplate)
16. [Impacto em performance](#16-impacto-em-performance)
17. [A aba TLB do simulador](#17-a-aba-tlb-do-simulador)
18. [Exemplo mínimo — modo padrão](#18-exemplo-mínimo--modo-padrão)
19. [Exemplo avançado — tabela customizada](#19-exemplo-avançado--tabela-customizada)
20. [Delegação de traps e demand paging](#20-delegação-de-traps-e-demand-paging)
21. [Veja também](#21-veja-também)

---

## 1. Por que memória virtual existe

Imagine um computador sem memória virtual. Cada programa vê diretamente a RAM física: o endereço `0x1000` no seu programa é o byte `0x1000` na placa de memória. Tudo funciona até você tentar rodar **dois programas ao mesmo tempo**.

Surgem quatro problemas práticos:

1. **Isolamento.** O processo A pode ler ou escrever o endereço `0x1000` do processo B simplesmente acessando `0x1000`. Não há barreira. Um bug em A pode corromper B.
2. **Realocação.** Cada programa é linkado para começar em algum endereço fixo (digamos `0x10000`). Se dois programas escolheram o mesmo endereço, eles colidem — e mesmo se não colidirem, é o sistema operacional que decide *onde* na RAM cada programa cabe, não o linker.
3. **Fragmentação.** Conforme processos sobem e descem, a memória livre vira um quebra-cabeça de buracos. Pode haver 200 MiB livres no total, mas nenhum bloco contíguo de 100 MiB para o próximo processo.
4. **Compartilhamento controlado.** Duas instâncias de `bash` deveriam compartilhar a mesma cópia do código em RAM (economia), mas precisam de pilhas separadas (isolamento). Sem uma camada de indireção, é tudo ou nada.

**Memória virtual** resolve os quatro inserindo uma indireção entre o endereço que o programa usa (**endereço virtual**, ou VA) e o endereço real na RAM (**endereço físico**, ou PA). O hardware traduz VA → PA em cada acesso. A tabela de tradução é controlada pelo sistema operacional. Daí:

- Cada processo recebe o seu **espaço de endereçamento virtual** próprio. O `0x1000` de A traduz pra uma página física diferente do `0x1000` de B.
- O linker pode assumir um endereço fixo; o SO coloca as páginas físicas onde quiser.
- Os "buracos" só existem no mundo físico. Virtualmente, cada processo vê um espaço contíguo.
- Mapear a *mesma* página física em VAs diferentes implementa compartilhamento. Marcar uma cópia como somente-leitura implementa copy-on-write.

A indireção custa: cada acesso à memória vira potencialmente vários acessos (ler a tabela de tradução). A próxima seção mostra como o hardware estrutura essa tabela; depois veremos como o **TLB** corta o custo a quase zero no caso comum.

---

## 2. O modelo conceitual: VA → PA

A unidade de tradução não é um byte; é uma **página** (tipicamente 4 KiB). Endereços dentro de uma mesma página são traduzidos juntos.

```
endereço virtual (VA)         endereço físico (PA)
┌──────────────┬──────┐       ┌──────────────┬──────┐
│ Virtual Page │offset│  ─→   │ Physical Page│offset│
│   Number     │      │       │   Number     │      │
│  (VPN)       │      │       │  (PPN)       │      │
└──────────────┴──────┘       └──────────────┴──────┘
```

- O **offset** (bits baixos) passa direto: byte 7 da página virtual `X` é byte 7 da página física correspondente.
- O hardware só precisa traduzir o VPN → PPN. Essa tradução vive em uma estrutura chamada **tabela de páginas**.

Com páginas de 4 KiB, o offset tem 12 bits (`2^12 = 4096`). Os bits restantes formam o VPN. Em RV32, isso deixa 20 bits de VPN, então existem `2^20 ≈ 1 milhão` de páginas virtuais possíveis por processo.

---

## 3. Tabelas de página: do array gigante à árvore multinível

### 3.1 A abordagem ingênua

A forma mais simples seria um array indexado pelo VPN: dado o VPN, leia a entrada e descubra o PPN.

```
tabela_de_paginas[VPN] = PPN
```

Para RV32 (20 bits de VPN), isso é `2^20` entradas × 4 bytes = **4 MiB por processo** — e quase tudo zero, porque um processo típico usa só uma fração do espaço de endereçamento. Em sistemas 64-bit a coisa explode: bilhões de entradas, terabytes de tabela.

### 3.2 A solução: hierarquia

Em vez de um array gigante, usamos uma **árvore**: uma tabela aponta pra subtabelas, que apontam pras páginas finais. Só os ramos efetivamente usados ocupam espaço.

Em Sv32, a hierarquia tem 2 níveis: uma **tabela raiz** com 1024 entradas, cada entrada apontando ou pra uma subtabela de 1024 entradas (que aponta pras páginas finais) ou diretamente pra uma **megapágina** de 4 MiB.

```
        satp aponta pra raiz
        ┌─────────────────┐
        │  root PT (1024) │   ← 1 página de 4 KiB
        └─┬─────┬─────────┘
          │     │
          ▼     ▼
       ┌────┐ ┌────┐         ← leaf PTs (uma por região usada)
       │1024│ │1024│
       └─┬──┘ └─┬──┘
         ▼     ▼
       páginas físicas de 4 KiB
```

- Processos pequenos: 1 root + 1-2 leaves = 8–12 KiB de overhead, não 4 MiB.
- Processos enormes: paga só pelas regiões realmente populadas.

A travessia dessa árvore se chama **page-table walk**, e o hardware faz esse walk a cada acesso que não esteja na TLB.

---

## 4. Sv32 em detalhes

**Sv32** é o esquema de paginação de 32 bits do RISC-V. O nome significa "Supervisor virtual addressing, 32 bits".

### 4.1 Divisão do endereço virtual

O VA de 32 bits é cortado em três campos:

```
 31         22 21         12 11           0
┌─────────────┬─────────────┬─────────────┐
│   VPN[1]    │   VPN[0]    │   offset    │
│  (10 bits)  │  (10 bits)  │  (12 bits)  │
└─────────────┴─────────────┴─────────────┘
```

- `offset` (12 bits) → byte dentro da página (0..4095).
- `VPN[0]` (10 bits) → índice na tabela de **nível 0** (leaf PT).
- `VPN[1]` (10 bits) → índice na tabela de **nível 1** (root PT).

`2^10 = 1024` entradas por tabela × 4 bytes/entrada = **4 KiB por tabela** — exatamente o tamanho de uma página. Conveniente: cada tabela cabe numa página e é endereçada por um único PPN.

### 4.2 O algoritmo do walk

Dado um endereço virtual `vaddr` e o CSR `satp` apontando pra raiz:

```
1. pte1_addr = (satp.PPN << 12) + VPN[1] * 4
   pte1 = mem[pte1_addr]
   se !pte1.V          → page fault
   se pte1 é leaf      → MEGAPÁGINA (4 MiB), pula direto pra checagem
   senão               → continua pro nível 0

2. pte0_addr = (pte1.PPN << 12) + VPN[0] * 4
   pte0 = mem[pte0_addr]
   se !pte0.V          → page fault
   se !pte0 é leaf     → page fault (PTE no último nível tem que ser leaf)

3. checa permissões (R/W/X conforme o tipo de acesso) e privilégio (U)
4. paddr = (pte0.PPN << 12) | offset
```

Note: **dois acessos à RAM por tradução**, mais o acesso original. Sem TLB, cada `lw` se torna potencialmente 3 acessos à memória.

### 4.3 O CSR `satp`

O **S**upervisor **A**ddress **T**ranslation **P**rotection é o registrador que controla a tradução:

```
 31  30          22 21                          0
┌────┬─────────────┬────────────────────────────┐
│MODE│    ASID     │           PPN              │
│ 1b │   9 bits    │         22 bits            │
└────┴─────────────┴────────────────────────────┘
```

- **MODE**: `0` = Bare (sem tradução, VA = PA), `1` = Sv32 (tradução ativa).
- **ASID** (Address Space IDentifier): identifica o processo. A TLB usa o ASID pra evitar misturar traduções de processos diferentes (ver §12).
- **PPN**: número da página física onde fica a tabela raiz. Endereço byte = `PPN << 12`.

Escrever em `satp` **flusha a TLB**, porque trocar de tabela invalidaria todas as traduções cacheadas.

---

## 5. O formato do PTE

Cada **P**age **T**able **E**ntry tem 32 bits:

```
 31                  10  9  8  7  6  5  4  3  2  1  0
┌───────────────────────┬────┬──┬──┬──┬──┬──┬──┬──┬──┐
│        PPN (22 b)     │RSW │ D│ A│ G│ U│ X│ W│ R│ V│
└───────────────────────┴────┴──┴──┴──┴──┴──┴──┴──┴──┘
```

| Bit | Nome | Função |
|-----|------|--------|
| 0 | V | **Valid**. Se 0, a entrada não conta — qualquer travessia para aqui faz page fault. |
| 1 | R | **Read**. Página pode ser lida (loads). |
| 2 | W | **Write**. Página pode ser escrita (stores). |
| 3 | X | **eXecute**. Página pode ser buscada como instrução (fetches). |
| 4 | U | **User**. Página é acessível em U-mode. Sem este bit, só S e M podem tocar. |
| 5 | G | **Global**. A entrada não tem ASID — match em qualquer espaço de endereçamento. |
| 6 | A | **Accessed**. Algum acesso já tocou esta página desde o último clear. |
| 7 | D | **Dirty**. Algum store já tocou esta página. |
| 8-9 | RSW | Reservado pra software (SO pode usar livremente). |
| 10-31 | PPN | Número da página física (ou ponteiro pra subtabela, se não-leaf). |

### 5.1 Codificação do valor

A fórmula é sempre:

```
PTE = (ppn << 10) | flags
```

onde `ppn = endereço_fisico >> 12`. Erro clássico: **não confunda o endereço físico de uma tabela com o valor do PTE que aponta pra ela.** Uma leaf PT no endereço `0x2000` tem `ppn = 0x2000 >> 12 = 2`, então o PTE não-leaf que aponta pra ela vale `(2 << 10) | 0x1 = 0x801` — não `0x2001`.

### 5.2 Leaf vs não-leaf

A distinção fica nos bits R, W, X:

- **Não-leaf (ponteiro)**: R=W=X=0, V=1. PPN aponta pra subtabela.
- **Leaf**: pelo menos um de R/W/X setado. PPN aponta pra página de dados/código.

Encodings comuns:

| Hex | Bits | Significado |
|-----|------|-------------|
| `0x01` | V | Ponteiro pra subtabela |
| `0x0F` | V\|R\|W\|X | Página kernel (código+dados, sem U) |
| `0x1F` | V\|R\|W\|X\|U | Página user (código+dados) |
| `0x17` | V\|R\|X\|U | Página user de código somente leitura |
| `0x0B` | V\|R\|X | Página kernel de código somente leitura |

### 5.3 Encoding reservado: W=1, R=0

A combinação `W=1, R=0` é **reservada** e gera page fault no walk. Intuição: faz pouco sentido permitir escrita sem leitura — código que escreve normalmente quer reler. A arquitetura aproveita a codificação pra futuras extensões.

---

## 6. Megapáginas (superpages)

Um PTE de nível 1 pode ser **leaf**. Nesse caso, ele mapeia uma região contígua de **4 MiB** (1024 × 4 KiB) com um único PTE. Chamamos isso de **megapágina** (ou superpage).

```
VA com megapágina:
 31         22 21                       0
┌─────────────┬─────────────────────────┐
│   VPN[1]    │     offset (22 bits)    │
└─────────────┴─────────────────────────┘
                ↑
                vpn[0] + page_offset viram só offset
```

Por que isso importa?

- **Walk mais curto**: 1 acesso à RAM em vez de 2.
- **Pegada na TLB menor**: 1 entrada cobre 4 MiB. Sem megapáginas, mapear 4 MiB exigiria 1024 entradas — mais do que o TLB inteiro tem.
- **Bom pra regiões grandes e contíguas**: kernel text, frame buffers, identity maps.

Restrição: o PPN da megapágina precisa ser **alinhado em 4 MiB** (os 10 bits baixos zero). Caso contrário a arquitetura considera "superpage misaligned" e gera page fault.

O modo padrão do Raven (§15) usa megapáginas pra cobrir os 4 GiB inteiros com 1024 entradas únicas, todas com identity mapping.

---

## 7. Bits A e D — accessed e dirty

Toda vez que uma página é tocada, o hardware deveria sinalizar isso de algum jeito — caso contrário o SO não tem como saber:

- *Quais páginas posso despejar com segurança pro swap?* (precisa do bit A para LRU aproximado)
- *Esta página foi modificada desde a última escrita no disco?* (precisa do bit D)

A spec RISC-V deixa **duas opções de implementação**:

1. **Hardware seta os bits direto na RAM** quando o walker termina com sucesso (`A` sempre, `D` em store). Simples pro SO.
2. **Hardware levanta um trap** quando A ou D estão zerados e o SO atualiza. Mais complicado, mas dispensa o hardware de escrever na PT.

O Raven escolhe a opção 1, por dois motivos:

- É didaticamente mais simples: você roda um programa e vê os bits A/D aparecendo nos PTEs sem precisar escrever um handler.
- Evita uma segunda volta no trap só pra atualizar uma flag.

Consequência observável: se você inspeciona a tabela de páginas em RAM depois de rodar um programa, vai ver os bits A e D ligados nas páginas que foram realmente usadas e escritas. Isso é informação que um SO real usaria pra decidir o que vai pro swap primeiro.

---

## 8. Permissões e níveis de privilégio

### 8.1 Os três bits R/W/X + U

Cada PTE leaf carrega quatro flags de proteção:

| Bit | Acesso permitido se setado |
|-----|----------------------------|
| R | Loads (`lw`, `lh`, `lb`, ...) |
| W | Stores (`sw`, `sh`, `sb`, ...) |
| X | Fetches de instrução |
| U | Modo usuário pode tocar a página |

A combinação faz controle de acesso fino: páginas de código rodando em userland geralmente são `R + X + U`; pilha é `R + W + U`; código do kernel é `R + X` (sem U).

### 8.2 Privilégio: M, S, U

| Modo | Notação | O que pode fazer |
|------|---------|------------------|
| Machine | M | Acesso total, ignora tradução em hardware real |
| Supervisor | S | Kernel; vê páginas sem `U`. Com SUM=0 não vê páginas `U`. |
| User | U | Aplicação; só vê páginas com `U=1` |

Regras com `satp.MODE = Sv32`:

- **U**: PTE precisa ter `U=1`, senão page fault.
- **S**: PTE precisa ter `U=0`.
- **M**: o hardware real **bypassa** a MMU completamente em M-mode.

> **Override didático do Raven:** quando o "modo padrão" está ativo, a MMU traduz também em M-mode. Isso é deliberado: a maioria dos programas educacionais roda em M (não há configuração de privilégio), então sem esse override a TLB ficaria silenciosa. Quando você instala suas próprias tabelas e desce pra U via `mret`, o comportamento volta a ser idêntico ao hardware real.

### 8.3 Causa do page fault

A causa do trap depende do tipo de acesso:

| Causa | Tipo |
|-------|------|
| 12 | Instruction page fault (fetch falhou) |
| 13 | Load page fault (load falhou) |
| 15 | Store page fault (store falhou) |

---

## 9. Por que existe o TLB

Vamos contar acessos à memória sem TLB.

Um simples `lw t0, 0(t1)`:

1. **Fetch** da instrução: 1 leitura de RAM → mas precisa traduzir o PC → 2 acessos à PT + 1 leitura = **3 acessos**.
2. **Load** do operando: 1 leitura de RAM → traduz `t1` → 2 acessos à PT + 1 leitura = **3 acessos**.

Total: **6 acessos à RAM** pra executar uma instrução que originalmente custava 2. **Slowdown de 3×.**

E não é só lentidão: cada acesso à PT também é candidato a cache miss, e a árvore de PT compete com os dados do programa pelo D-cache.

### 9.1 A observação que salva tudo

Programas têm **localidade**: o mesmo VPN se repete inúmeras vezes em curtas janelas de tempo (loop sobre array, fetch sequencial de instruções, etc.). Se cachearmos a tradução `VPN → PPN`, o walk acontece **uma vez** e os próximos N acessos àquela página são essencialmente gratuitos.

Esse cache se chama **Translation Lookaside Buffer (TLB)**. É a estrutura mais importante pra performance de qualquer sistema com paginação.

### 9.2 Métricas a observar

- **Hit rate**: fração de traduções que vieram do TLB. Programas bem-comportados ficam acima de 99%.
- **Miss penalty**: ciclos pra fazer um walk. No Raven, default `20` ciclos.
- **Hit latency**: ciclos pra confirmar um hit. Default `1` ciclo.

A subtab Stats da TLB plota o hit rate em janela rolante de 300 ciclos — útil pra ver fases distintas (warmup vs steady state, mudança de working set, etc.).

---

## 10. Organização do TLB: sets, ways, indexação

O TLB é uma cache pequena, então herda a mesma terminologia de caches comuns: **sets**, **ways**, **associatividade**, **políticas de substituição**.

### 10.1 Set-associative

Cada VPN é mapeado pra **um set específico** (via hash). Dentro do set, qualquer um dos **N ways** (slots) pode ter a entrada.

```
                       ┌── way 0 ──┬── way 1 ──┬── way 2 ──┬── way 3 ──┐
VPN → hash → set k →   │   entry   │   entry   │   entry   │   entry   │
                       └───────────┴───────────┴───────────┴───────────┘
                       todos N ways comparados em paralelo
```

- **Hit**: algum way no set tem `vpn` igual e `asid` compatível.
- **Miss**: nenhum way bate → walk → instala em algum way (despejando alguém se o set está cheio).

### 10.2 Megapáginas e indexação

Uma megapágina cobre 1024 VPNs consecutivos. Se ela fosse colocada apenas no set do VPN do *início*, qualquer probe pra um VPN no meio da megapágina seria miss. Pra evitar isso, megapáginas usam um esquema de indexação diferente das páginas de 4 KiB — efetivamente vivendo em "seus próprios" sets — e o TLB consulta ambos os esquemas a cada lookup, garantindo que entradas grandes sejam encontradas pelos VPNs que cobrem.

Você não precisa pensar nisso ao escrever programas. O detalhe importa pra entender por que uma única megapágina no modo padrão consegue servir um programa inteiro com hit rate próximo de 100%.

### 10.3 Trade-offs de associatividade

| Associatividade | Vantagem | Custo |
|-----------------|----------|-------|
| 1-way (mapeamento direto) | Hardware mais simples | Vulnerável a conflict misses |
| N-way (típico: 4-8) | Tolera colisões | Mais comparadores em paralelo |
| Totalmente associativo | Sem conflict miss | Caro pra escalar |

O default do Raven é **32 entradas, 4-way** → 8 sets.

---

## 11. Políticas de substituição

Quando um set está cheio, qual entrada é despejada? O Raven oferece as mesmas seis políticas que o D-cache:

| Política | Despeja | Ideal pra... |
|----------|---------|--------------|
| **LRU** (Least Recently Used) | A menos recentemente acessada | Padrão; bom em quase tudo |
| **FIFO** | A entrada instalada há mais tempo | Hardware simples, pouco overhead |
| **LFU** (Least Frequently Used) | A menos frequentemente acessada | Working sets estáveis |
| **Clock** (second-chance) | Aproxima LRU com 1 bit por entrada | Compromisso clássico LRU vs FIFO |
| **MRU** (Most Recently Used) | A mais recentemente acessada | Streams sequenciais grandes |
| **Random** | Aleatória | Baseline; surpreendentemente OK |

Mudar política em runtime: vá em **Cache → TLB → Settings**, escolha a política, Apply. O Apply reinicia a TLB (todas as entradas viram inválidas), então a próxima execução começa de cold.

Dica de experimento: rode o mesmo programa duas vezes — uma com LRU, outra com MRU. Compare o hit rate na subtab Stats. Pra padrões de acesso comuns (loops, arrays), LRU ganha de longe. Pra varreduras sequenciais grandes maiores que a TLB, MRU pode surpreender.

---

## 12. ASIDs e a flag global

### 12.1 O problema

Imagine dois processos: A e B. Ambos têm tradução pro VPN `0x1000`, mas pra páginas físicas diferentes. Se a TLB cachear a tradução do A e o SO trocar pro B, a próxima vez que B acessar `0x1000` ele vai *bater na entrada do A* — e ler/escrever na página física errada. Catástrofe.

### 12.2 A solução tradicional: flush total

Toda troca de processo, flush a TLB inteira. Funciona, mas joga fora trabalho útil — entradas de páginas compartilhadas (kernel, libc) seriam revalidadas.

### 12.3 A solução melhor: ASIDs

Cada processo recebe um número (**Address Space ID**) único — 9 bits em Sv32. O ASID atual vive no `satp`. Cada entrada da TLB carrega o ASID com que foi instalada, e um match só é válido se `vpn` E `asid` baterem.

Resultado: troca de processo *não precisa* flushar a TLB. Entradas antigas ficam dormindo até que sejam despejadas naturalmente.

### 12.4 A flag G (global)

Páginas usadas por todos os processos (mapeamentos de kernel, por exemplo) podem ter `G=1`. Entradas globais **ignoram o ASID** no match — bater no VPN basta. Economiza espaço no TLB porque você não precisa de uma entrada por ASID.

---

## 13. `sfence.vma` e coerência da TLB

A TLB é um cache de algo que vive em RAM (a tabela de páginas). Se o SO modificar a PT — instalar uma nova página, mudar permissões, despaginar — entradas antigas na TLB ficam **estale**.

A instrução **`sfence.vma`** sinaliza pro hardware: "invalide as traduções cacheadas; eu mexi na PT". Variantes:

- `sfence.vma rs1=x0, rs2=x0` → flush tudo.
- `sfence.vma rs1=vaddr, rs2=x0` → flush só a entrada do `vaddr`.
- `sfence.vma rs1=x0, rs2=asid` → flush só o ASID dado.
- `sfence.vma rs1=vaddr, rs2=asid` → flush a entrada específica.

O Raven implementa `sfence.vma` como **flush total** nessa fase (`rs1`/`rs2` ignorados), o que é correto mas não otimizado. Escrever em `satp` também flusha, já que troca de tabela invalida tudo.

---

## 14. Page faults: causas e fluxo do trap

Quando a tradução falha — PTE inválido, permissão errada, privilégio insuficiente, megapágina misaligned, encoding reservado — o hardware levanta um trap.

### 14.1 Causas

| `mcause` | Tipo |
|----------|------|
| 12 | Instruction page fault |
| 13 | Load page fault |
| 15 | Store page fault |

### 14.2 O fluxo do trap

1. A tradução falha durante um fetch, load ou store.
2. O hardware salva o contexto da falha nos CSRs:
   - `mcause ← causa` (12, 13 ou 15)
   - `mtval ← vaddr` que falhou — o handler usa isso pra saber **qual** endereço causou a falha
   - `mepc ← PC` da instrução que falhou — pra `mret` voltar pra cá
   - `mstatus.MPP ← modo atual` — pra `mret` restaurar o privilégio
3. O hardware troca pra M-mode e pula pra `mtvec & ~3` (modo direto; modo vetorizado não é coberto).
4. Seu handler decide o que fazer e retorna com `mret`.
5. Se `mtvec = 0` (você esqueceu de configurar), o Raven imprime a falha no console e para — pra você não ficar caçando bug.

A menos que a causa esteja **delegada** ao modo supervisor (`medeleg`): nesse caso o trap preenche os CSRs `s*` e vetoriza por `stvec`, permanecendo em S-mode. Veja a [§20](#20-delegação-de-traps-e-demand-paging).

### 14.3 O que um SO real faria

Um SO real normalmente trata o fault assim:

- Se a página foi paginada pro swap → traz de volta do disco, atualiza a PT, retorna com `mret`. O programa nem percebe.
- Se a região é válida mas ainda não foi alocada (demand paging) → aloca uma página física, mapeia, retorna.
- Se o acesso é genuinamente inválido → mata o processo (SIGSEGV no Unix).

O Raven não tem swap nem alocador de páginas — ele te dá os primitivos pra você experimentar montando seus próprios handlers.

---

## 15. Modo padrão: experimentar sem boilerplate

Ligar VM sem nenhuma configuração extra seria inútil: o `satp` ficaria em zero (Bare), o privilégio inicial é M, e nenhuma tradução aconteceria. Você precisaria escrever uma tabela de páginas manualmente só pra ver qualquer atividade na TLB.

O Raven resolve isso com o **modo padrão**:

1. Ao montar um programa com VM ligada, o Raven escreve automaticamente **1024 PTEs de megapágina** no último bloco de 4 KiB da RAM. Cada entrada `i` mapeia o i-ésimo bloco de 4 MiB pra si mesmo — um **identity mapping** completo do espaço de endereçamento — com permissões `R|W|X|U|V`.
2. O CSR `satp` é configurado pra Sv32 apontando pra essa tabela.
3. A tradução é forçada mesmo em M-mode, pra que qualquer programa veja atividade na TLB.

Resultado: **qualquer programa**, mesmo um `addi`/`blt` simples sem CSR algum, gera atividade na TLB. Você liga o switch em **Settings → Virtual Memory**, monta, roda — pronto.

Por que isso é didaticamente valioso? Porque você consegue estudar comportamento de cache de tradução (hit rate, políticas de substituição, efeito do tamanho do TLB) sem primeiro ter que aprender a montar tabelas de página, configurar `mtvec`, escrever um handler de fault e cair em U-mode. Tudo isso é importante e está na §19 — mas o modo padrão te deixa começar pelo TLB e ir pros detalhes depois.

Quando você está pronto pra estudar layouts customizados, basta escrever sua própria tabela e fazer `csrw satp` apontando pra ela. A TLB é flushada automaticamente e seu mapeamento substitui o automático.

---

## 16. Impacto em performance

Toda tradução adiciona ciclos. Onde eles aparecem depende do modo de execução.

### 16.1 Pipeline mode

- **Fetch** que sofre miss na TLB → stall no slot **IF** (aparece como faixa vermelha no Gantt).
- **Load/store** que sofre miss → stall no slot **MEM**.
- Hits adicionam `hit_latency` ao mesmo slot. Default `1` ciclo — costuma se sobrepor a outras latências.

### 16.2 Interpreter mode

Sem pipeline, os ciclos extras viram parte do total e contribuem direto pra `total_program_cycles` e CPI. Você vê o impacto em **CPI** na barra de status.

### 16.3 Heurística rápida

- Hit rate > 99% → impacto desprezível.
- Hit rate 90-99% → notável; pode valer aumentar `entry_count` ou associatividade.
- Hit rate < 90% → working set não cabe; mude pra megapáginas ou aumente o TLB.

A subtab Stats mostra o gráfico rolante e os totais. Compare antes/depois de mudar a config usando **Apply + Reset Stats**.

---

## 17. A aba TLB do simulador

A aba **TLB / Virtual Memory** tem 4 subtabs, na ordem **stats · entries · vm · settings**:

### Stats
- Gauge de hit rate.
- Contadores: `Hits`, `Misses`, `Evictions`, `Page Faults`.
- Histórico rolante de 300 ciclos com o hit rate.
- Atalho `r` reseta os contadores; `p` pausa.

### Entries
- Tabela por entrada: `VPN | PPN | RWXU | ASID | V | G | A | D | mega`.
- Útil pra confirmar que uma página específica foi cacheada, ou pra ver bits A/D ligando conforme o programa roda.
- `↑` / `↓` rolam a lista.

### VM (Status)
- Mostra o estado vivo do `satp` (MODE, ASID, root PPN) e o `priv_mode`.
- O chip no header diz: `vm=off`, `vm=on · translating`, ou `vm=on · inactive (satp=Bare ou priv=M)`. Diagnóstico rápido pra "por que a TLB tá vazia?".

### Settings
- `Entries` (potência de 2), `Associativity`, `Replacement Policy`, `Hit Latency`, `Miss Penalty`.
- **Apply + Reset Stats** ou **Apply Keep History**.
- Salvar/carregar config via export/import do Cache (`.fcache` / `.rcfg`); o bloco `[tlb]` carrega junto.

`Tab` cicla entre as subtabs na mesma ordem do header.

---

## 18. Exemplo mínimo — modo padrão

Com VM ligada, isso já é suficiente:

```asm
.text
    li   t0, 0
    li   t1, 100
loop:
    addi t0, t0, 1
    blt  t0, t1, loop   # cada fetch e load/store passa pela TLB
    li   a0, 0
    li   a7, 93
    ecall               # exit
```

1. Ligue **Settings → Virtual Memory** antes de montar.
2. Monte e rode.
3. Vá em **Cache → TLB → Stats** e veja a hit rate subir conforme o loop reutiliza as páginas.
4. Visite **Entries** pra confirmar que o `vpn` do código aparece com `X=1` e `A=1`.

---

## 19. Exemplo avançado — tabela customizada

Pra estudar page faults, transição de privilégio, ou layouts próprios, escreva a sua própria tabela.

```asm
# Mapeia VA 0x0000 → PA 0x0000 (R|W|X|U, 4 KiB) e cai em U-mode.
#
# Layout (escolhido pra não sobrepor o código em 0x0000):
#   0x1000 — root PT  (PPN = 1)
#   0x2000 — leaf PT  (PPN = 2)

.text
boot:
    # ── 1. Escrever PTE raiz ────────────────────────────────────────────
    # Ponteiro não-leaf: PPN=2 (leaf PT @ 0x2000), V=1
    # Valor = (2 << 10) | 0x1 = 0x801
    li   t0, 0x801
    li   t1, 0x1000          # root PT fica em PA 0x1000
    sw   t0, 0(t1)           # root_pt[VPN1=0] = 0x801

    # ── 2. Escrever PTE leaf ────────────────────────────────────────────
    # Leaf: PPN=0 (PA 0x0000), R|W|X|U|V = 0x1F
    # Valor = (0 << 10) | 0x1F = 0x1F
    li   t2, 0x1F
    li   t3, 0x2000          # leaf PT fica em PA 0x2000
    sw   t2, 0(t3)           # leaf_pt[VPN0=0] = 0x1F

    # ── 3. Instalar satp: Sv32 (bit 31), ASID=0, PPN raiz=1 ────────────
    li   t0, 0x80000001      # bit31=Sv32 | PPN=1
    csrw satp, t0            # escrita em satp flusha a TLB

    # ── 4. Configurar mret pra cair em U-mode ──────────────────────────
    la   t0, user_entry
    csrw mepc, t0
    li   t0, 0               # mstatus.MPP = 0b00 = U
    csrw mstatus, t0
    mret                     # priv → U, pc → user_entry

user_entry:
    # Tradução ativa (satp=Sv32, priv=U) — comportamento idêntico ao hardware.
    nop
    halt
```

Variações pra experimentar:

- Troque `0x1F` por `0x17` (sem W) e tente um `sw` — page fault `15`.
- Troque por `0x0F` (sem U) — fault `13` em U-mode (sem permissão de usuário).
- Aponte o ponteiro raiz pra `0x2001` em vez de `0x801` (PPN errado) — veja o walker faltar.
- Configure `mtvec` pra um handler que printa `mcause`/`mtval` antes de chamar `mret`.

---

## 20. Delegação de traps e demand paging

Até aqui todo fault foi parar em M-mode via `mtvec`. Sistemas operacionais reais rodam o handler de fault em modo **supervisor** e reservam o modo machine pro firmware. O Raven modela isso com **delegação de traps**: setando o bit `c` de `medeleg`, um fault com causa `c` ocorrido em S- ou U-mode é roteado pro handler supervisor em `stvec` — preenchendo `sepc` / `scause` / `stval` e gravando o modo anterior em `sstatus.SPP`. O retorno é feito com `sret`, espelhando `mret` sobre o `sstatus`.

### 20.1 Os CSRs e a instrução

| CSR | Número | Uso |
|-----|--------|-----|
| `medeleg` | `0x302` | Delegação de exceções — bit `c` setado ⇒ causa `c` tratada em S-mode |
| `mideleg` | `0x303` | Delegação de interrupções (armazenado; ainda não há interrupções assíncronas) |
| `sstatus` | `0x100` | Status supervisor — `SPP` (bit 8), `SPIE` (bit 5), `SIE` (bit 1) |
| `stvec` | `0x105` | Endereço-base do vetor de trap (S-mode) |
| `sscratch` | `0x140` | Registrador de scratch do handler supervisor |
| `sepc` | `0x141` | PC salvo no trap delegado |
| `scause` | `0x142` | Causa do trap delegado |
| `stval` | `0x143` | Valor do trap delegado (vaddr que falhou) |

> O Raven modela `sstatus` como um registrador próprio, e não como a view mascarada de `mstatus` que o hardware real implementa. É uma simplificação pedagógica deliberada: deixa o caminho de delegação fácil de ler, ao custo do aliasing de bits compartilhados que o hardware faz.

### 20.2 O padrão de demand paging

1. O código de usuário toca uma página ainda não mapeada → **load/store page fault** (causa 13 / 15).
2. Como a causa está delegada (`medeleg`), a CPU vetoriza pro handler supervisor em S-mode.
3. O handler lê `stval` (o endereço que falhou), instala a PTE faltante e roda `sfence.vma` pra descartar entradas obsoletas da TLB.
4. `sret` retorna pra `sepc` — a instrução que falhou re-executa e agora **funciona**.

> **⚠ Coerência walker / cache.** O walker de tabela de páginas do Raven lê PTEs **direto da RAM** e *não* é coerente com o D-cache write-back. Um handler que escreve uma PTE com um `sw` normal deixa ela parada no cache, então o walk repetido continua vendo a entrada antiga (vazia) e falha pra sempre. Pra programas de demand paging, **desligue o cache** (aba Cache) ou troque o D-cache pra **write-through**, de modo que o store do handler chegue à RAM antes do walk re-rodar. (O teste de demand paging em `tests/mmu_traps.rs` usa um D-cache write-through exatamente por isso.)

### 20.3 Configuração (no boot em M-mode)

```asm
    # Delega page faults de load (causa 13) e store (causa 15) pra S-mode.
    li   t0, (1 << 13) | (1 << 15)
    csrw medeleg, t0

    # Aponta o vetor de trap supervisor pro handler.
    la   t0, page_fault_handler
    csrw stvec, t0
    # ... monta as tabelas iniciais, csrw satp, e mret pra U-mode ...
```

### 20.4 O handler supervisor

```asm
page_fault_handler:
    csrr t0, stval              # endereço virtual que falhou
    # ... deriva o slot da PTE leaf, escreve a nova PTE na leaf table (mapeada no kernel) ...
    sfence.vma                  # descarta entradas obsoletas da TLB
    sret                        # retorna pra sepc — o acesso que falhou repete
```

Uma viagem de ida e volta completa e executável — boot mapeia as páginas de código/handler/tabela, cai em U-mode, falha numa página não-mapeada, o handler a mapeia, `sfence.vma`, `sret`, e o load repetido lê o dado recém-mapeado — está em `tests/mmu_traps.rs::demand_paging_end_to_end`. Duas regras de layout daquele teste valem repetir:

- O **código do handler e as páginas de tabela precisam ser mapeados como não-`U`** (só kernel), porque S-mode não pode tocar páginas `U=1` (`SUM` não é modelado).
- O handler edita a tabela *sob tradução*, então a página da leaf table precisa do próprio mapeamento de identidade (`VA = PA`) — o equivalente do simulador a um direct map de kernel.

Pra ver ao vivo: selecione **Settings → Virtual Memory → Manual**, desligue o cache, monte e abra a subaba **TLB → tree** pra ver as PTEs surgindo conforme o handler as instala.

---

## 21. Veja também

- [Mapa de memória](memory-allocation.md) — layout de endereços físicos que serve de backing store.
- [Config de cache](cache-config.md) — campos do `.fcache` / `.rcfg` incluindo o bloco `[tlb]`.
- [Simulação de pipeline](pipeline.md) — onde os stalls da MMU aparecem no Gantt.
- [Simulação de cache](cache.md) — terminologia comum (sets, ways, políticas) que a TLB herda.
