# Memória Virtual e TLB (Sv32)

> [Read in English](../en/virtual-memory.md)

Este documento é material de estudo sobre **memória virtual no RISC-V** (esquema **Sv32**) e sobre a **TLB** que o simulador coloca na frente dela. A intenção é ensinar do zero: *por que* memória virtual existe, *como* a tradução acontece, *o que* o TLB resolve, e como você pode experimentar tudo isso na aba Virtual Memory do Raven.

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
15. [Os modos de VM: off, auto e program](#15-os-modos-de-vm-off-auto-e-program)
16. [Impacto em performance](#16-impacto-em-performance)
17. [A aba Virtual Memory do simulador](#17-a-aba-virtual-memory-do-simulador)
18. [Exemplo mínimo — modo auto](#18-exemplo-mínimo--modo-auto)
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

O modo **auto** do Raven (§15) usa megapáginas de identidade pra cobrir todo o espaço de endereçamento fora do programa carregado, que fica em páginas de 4 KiB.

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

> **Override didático do Raven:** no modo **auto**, a MMU traduz também em M-mode. Isso é deliberado: a maioria dos programas educacionais roda em M (não há configuração de privilégio), então sem esse override a TLB ficaria silenciosa. Quando você instala suas próprias tabelas e desce pra U via `mret` (modo **program**), o comportamento volta a ser idêntico ao hardware real.

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
- **Miss penalty**: ciclos pra fazer um walk. No Raven, os `20` ciclos do próprio walker por padrão, mais o que cada leitura de PTE custa nas caches (§16).
- **Hit latency**: ciclos pra confirmar um hit. Default `1` ciclo.

A subaba stats da TLB plota o hit rate em janela rolante de 300 ciclos — útil pra ver fases distintas (warmup vs steady state, mudança de working set, etc.).

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

Você não precisa pensar nisso ao escrever programas. O detalhe importa pra entender como, no modo auto, as páginas de 4 KiB do programa e as megapáginas que cobrem o resto do espaço convivem na mesma TLB.

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

Mudar política em runtime: vá em **Virtual Memory → config** e escolha a política. A mudança vale na hora e reconstrói a TLB vazia (todas as entradas viram inválidas), então a tradução recomeça de cold.

Dica de experimento: rode o mesmo programa duas vezes — uma com LRU, outra com MRU. Compare o hit rate na subaba stats. Pra padrões de acesso comuns (loops, arrays), LRU ganha de longe. Pra varreduras sequenciais grandes maiores que a TLB, MRU pode surpreender.

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

O Raven respeita o `rs1`: `sfence.vma` com `rs1 = x0` esvazia a TLB inteira e, com um registrador, descarta só as entradas que traduzem aquele endereço (em qualquer ASID — o `rs2` é ignorado, o que é correto, só menos seletivo). Escrever em `satp` também esvazia tudo, já que trocar de tabela invalida todas as traduções.

Como no hardware, cada core tem a sua própria TLB, e o `sfence.vma` esvazia só a TLB do core que o executa. Um kernel que muda um mapeamento que outros cores podem ter guardado precisa fazer cada um deles esvaziar também (um "TLB shootdown"). O botão `flush tlb` da aba, e um novo formato de TLB, valem para todos os cores.

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

## 15. Os modos de VM: off, auto e program

O modo responde a uma pergunta: **quem monta a tabela de páginas?** Escolha-o na subaba **Virtual Memory → config** (ou na aba Settings global); ele vale no instante em que você escolhe e fica salvo no `.rcfg`.

| Modo | Quem monta a tabela | Quando usar |
|------|---------------------|-------------|
| **off** | Ninguém: VA = PA, a MMU é ignorada e a TLB fica intacta. Nenhum ciclo extra. | Programas comuns, memória plana. |
| **auto** | **O simulador**: escreve um mapa identidade Sv32 quando o programa é carregado e traduz até em M-mode. | Ver a TLB trabalhando em **qualquer** programa, sem código de setup. |
| **program** | **O seu programa**: escreve o `satp` e as próprias tabelas, e o M-mode bypassa a tradução — RISC-V real. | Page faults, transição de privilégio, demand paging (§19–§20). |

Configs gravadas por versões anteriores continuam carregando: `sv32` e `custom` viram **auto**, `manual` vira **program**.

### 15.1 auto — experimentar sem boilerplate

Ligar tradução sem nenhuma configuração seria inútil: o `satp` ficaria em zero (Bare), o privilégio inicial é M, e nada seria traduzido. Você teria que montar uma tabela de páginas só pra ver qualquer atividade na TLB. O **auto** resolve isso:

1. O Raven escreve um mapa identidade (VA = PA): o programa carregado — text, data e bss — em **páginas de 4 KiB** sob uma leaf table, pra que os acessos dele mostrem um walk completo de dois níveis; o resto do espaço de endereçamento em **megapáginas de 4 MiB** direto da raiz.
2. As tabelas ficam **logo abaixo das pilhas dos harts**, então nenhum push consegue cair em cima de uma PTE. A subaba config mostra onde.
3. O `satp` aponta pra raiz, e a tradução é forçada mesmo em M-mode.

Resultado: **qualquer programa**, mesmo um `addi`/`blt` simples sem CSR algum, gera atividade na TLB. Escolher auto no meio da sessão escreve o mapa na hora — não precisa remontar.

As permissões do mapa estão aí pra você quebrar na subaba config: tire o **W** e o próximo store dá page fault, com a subaba walk mostrando exatamente qual PTE o recusou. Mudar uma permissão reescreve o mapa e esvazia a TLB.

### 15.2 program — o programa no comando

Quando você quer o comportamento fiel ao hardware — sem o override didático, com o programa instalando suas próprias tabelas e tratando faults — use o modo **program**. O M-mode bypassa a MMU; só depois de `csrw satp` (Sv32) e de cair em U/S-mode a tradução entra em ação. É o modo de que os exemplos avançados das §19 e §20 precisam. Até lá, o cabeçalho da aba diz por que nada está sendo traduzido.

---

## 16. Impacto em performance

Toda tradução adiciona ciclos. Um hit custa `hit_latency`. Um miss custa `miss_penalty`, o trabalho do próprio walker, **mais cada PTE que ele lê**: essas leituras são loads comuns pela D-cache, então uma PTE que está na L1 custa um hit de L1, e uma que precisa vir da memória custa um miss inteiro e preenche uma linha no caminho. Um walk sobre tabelas que as caches já guardam é barato; o primeiro walk por tabelas frias não é. Com as caches desligadas (o padrão; ligue em Settings), as leituras de PTE não custam nada próprio e o walk custa `miss_penalty`.

Onde os ciclos aparecem depende do modo de execução.

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

A subaba stats mostra o gráfico rolante e os totais. Capture um snapshot com `s` para comparar antes/depois de mudar a config.

---

## 17. A aba Virtual Memory do simulador

A aba **Virtual Memory** é o painel central de tudo que este documento explica. O cabeçalho dela traz cinco subabas — **walk · tlb · map · stats · config** —, os controles de execução (speed / state / step / back / reset), `flush tlb` e `results`, e uma linha de estado: `Sv32 · auto · translating` ou, quando nada é traduzido, **por quê** — com a correção a um clique (`turn on`, `install map`). Com a memória virtual desligada, a aba abre numa introdução curta com o botão `turn on`.

Aperte `?` na aba para abrir o **guia**: um texto com rolagem, em português e inglês (`l` troca o idioma), com as seções listadas à esquerda — como a tradução funciona, o que cada painel mostra, todas as teclas e um glossário. Cada sigla aparece por extenso na primeira vez. Do guia, `t` começa o tour guiado.

As três subabas ao vivo — walk, tlb e map — olham para o mesmo **acesso atual**: o acesso a dados mais recente enquanto o programa roda. Fixe um mais antigo pelo histórico da walk (clique nele, ou ande com `←` `→`) e as três passam a mostrar esse; `g` em qualquer uma delas volta a acompanhar.

Teclas: `Tab` cicla as subabas; `s` avança até a próxima tradução (pulando instruções que terminam sem traduzir nada novo) e `b` volta um passo (em stats, `s` tira um snapshot); `r` / `p` / `f` resetam, rodam-pausam e mudam a velocidade; o rodapé lista as teclas da subaba em que você está.

Cada core guarda a sua própria TLB. Com mais de um core, os títulos dos painéis dizem qual é (`TLB · core 1`), e `[` `]` escolhem de qual core a aba mostra a TLB, as estatísticas e as traduções.

### walk — um acesso, passo a passo
- O acesso: a instrução, o tipo (fetch, load, store), o privilégio e o pc.
- O endereço virtual dividido em **VPN[1] · VPN[0] · offset**, cada um na sua cor, com os bits embaixo. A mesma cor marca a PTE que aquele campo indexa nos passos abaixo, e o offset leva a sua cor até o endereço físico.
- Os passos: qual **set** da TLB foi consultado (`VPN mod sets`) e se algum way bateu; num miss, `satp` → tabela raiz, depois cada **PTE** que o walk leu, com endereço, valor bruto e significado, e onde essa leitura foi atendida (`L1 +1`, `RAM +12`; só `RAM` com as caches desligadas); o **PA** montado como frame + offset, com os ciclos que custou; e o que a TLB **guardou** — o set e o way, e a entrada que ela despejou. Um fault para no passo que falhou e diz por quê (`V=0`, `page is not writable`, …) com o código da causa.
- **Histórico**: as últimas 64 traduções, da mais recente pra mais antiga, filtradas por `all · misses · faults` (`1` `2` `3`). Acompanhe acessos `data`, `fetch` ou `any` com os chips ou com `v`.

### tlb — sets e ways
- A TLB desenhada do jeito que a aba Cache desenha uma cache: uma linha por **set**, uma coluna por **way**. Cada célula mostra o tamanho da página, `VPN→PPN`, as permissões e a sua **recência** (1 = usada mais recentemente).
- Uma página de 4 KiB vai pro set `VPN mod sets`, uma megapágina de 4 MiB pro set `(VPN >> 10) mod sets`; os sets que o acesso atual consultou ficam marcados com `▸`. `●` marca a entrada que ele instalou, `✓` a que deu hit.
- As setas movem um cursor; o painel de baixo detalha a entrada selecionada: os intervalos de VA e PA que ela mapeia, ASID ou global, dirty, e a posição dela na ordem de substituição. Uma matriz larga demais pra tela vira abas de set sobre uma lista de ways.

### map — a própria tabela de páginas
- Uma coluna por nível, lida ao vivo da memória (pelas caches, do jeito que o walker lê): a raiz (**VPN[1]**) à esquerda, a tabela pra qual a entrada selecionada aponta (**VPN[0]**) ao lado, e a **PTE selecionada decodificada campo a campo** — PPN, RSW, D, A, G, U, X, W, R, V — à direita.
- Sequências de entradas iguais se dobram numa linha só (`002–3fe 4M RWXU ×1021`); `→` ou `Enter` as abrem. O caminho do acesso atual fica marcado com `▸`, e as entradas que a TLB guarda com `●`.
- `↑` `↓` andam dentro de uma coluna, `→` segue um ponteiro, `←` volta. Aperte `/` e digite qualquer endereço pra ver o caminho que ele faria.

### stats — quanto custou
- Hits, misses, faults, evictions e o hit rate ao longo do tempo.
- **cost**: ciclos gastos percorrendo tabelas de páginas e a fatia deles na execução. **reach**: as páginas que o programa tocou contra as entradas que a TLB tem.
- **Why it missed** separa cada miss pela causa: o **primeiro toque** numa página (inevitável), uma página que a TLB **tinha e perdeu** (uma TLB maior ou organizada de outro jeito ajuda), um **store numa página limpa** (o walk seta o D) ou **sem mapeamento** (um page fault).
- **Snapshots de sessão** (compartilhados com a aba Cache): `s` captura a janela atual; `↑`/`↓` seleciona, `Enter` abre os detalhes, `d` apaga. Eles vão junto no export de resultados (`results` / `Ctrl+r`) como uma seção `tlb.*`.

### config — tudo vale na hora
Toda opção fica na tela, com a escolhida acesa, e toda mudança vale imediatamente — não há botão de aplicar.

1. **Translation** — o modo (§15).
2. **Page map** (só no auto) — as permissões `R/W/X/U` carimbadas em toda folha, a flag `G` (global) e o `ASID` escrito no `satp`; abaixo, onde as tabelas ficam e qual intervalo é mapeado em páginas de 4 KiB.
3. **TLB** — on/off (off: todo acesso faz walk), `entries`, `ways` (de mapeamento direto a totalmente associativa), a `policy` de substituição e os ciclos de `hit` e de `walk` (`walk` é o do próprio walker; cada leitura de PTE soma o custo dela na cache). Mudar entries, ways ou policy reconstrói vazia a TLB de todos os cores.

Teclas: `m` cicla o modo; `e` edita um número (`Tab` vai pro próximo, `Enter` mantém, `Esc` descarta). Salve/carregue o formato da TLB com `export cfg` / `import cfg` (`Ctrl+e` / `Ctrl+l`); o bloco `[tlb]` viaja na seção `[cache]` do `.rcfg` unificado (ver [Config de cache](cache-config.md)).

---

## 18. Exemplo mínimo — modo auto

Com a VM no modo **auto**, isso já é suficiente:

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

1. Escolha **auto** em **Virtual Memory → config** (ou na aba Settings global).
2. Monte e avance algumas instruções passo a passo.
3. A subaba **walk** mostra o primeiro fetch dando miss na TLB e percorrendo os dois níveis; os seguintes dão hit. (O loop não faz load nem store, então o walk mostra fetches; com acessos a dados ele segue esses, e `v` troca.)
4. Rode até o fim e abra **stats**: o hit rate sobe conforme o loop reutiliza a página dele, e o *why it missed* mostra só primeiros toques.
5. Em **tlb**, a página do código fica no set `VPN mod sets` com `X` setado.

---

## 19. Exemplo avançado — tabela customizada

Pra estudar page faults, transição de privilégio, ou layouts próprios, selecione o modo **program** (em **Virtual Memory → config**) e escreva a sua própria tabela — sem o override didático, o programa fica no comando.

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

> No hardware RISC-V real, `sstatus` é uma *view* restrita de `mstatus` — os dois nomes leem e escrevem os mesmos bits compartilhados. Aqui ele se comporta como um registrador independente. É uma simplificação pedagógica deliberada: deixa o caminho de delegação fácil de seguir, sem o aliasing de bits compartilhados que o hardware faz.

### 20.2 O padrão de demand paging

1. O código de usuário toca uma página ainda não mapeada → **load/store page fault** (causa 13 / 15).
2. Como a causa está delegada (`medeleg`), a CPU vetoriza pro handler supervisor em S-mode.
3. O handler lê `stval` (o endereço que falhou), instala a PTE faltante e roda `sfence.vma` pra descartar entradas obsoletas da TLB.
4. `sret` retorna pra `sepc` — a instrução que falhou re-executa e agora **funciona**.

> **Walker e cache.** O walk da tabela de páginas lê cada PTE como um load veria, então uma PTE que o handler acabou de gravar com um `sw` comum já vale mesmo enquanto ainda está suja num D-cache write-back. Os bits A e D que o walker liga vão para a RAM e para toda cópia em cache, então o write-back dessa linha mais tarde não os apaga. O cache pode ficar ligado.

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

Uma viagem de ida e volta completa fica assim: boot mapeia as páginas de código/handler/tabela, cai em U-mode, falha numa página não-mapeada, o handler a mapeia, roda `sfence.vma`, `sret`, e o load repetido lê o dado recém-mapeado. Duas regras de layout desse padrão valem repetir:

- O **código do handler e as páginas de tabela precisam ser mapeados como não-`U`** (só kernel), porque S-mode não pode tocar páginas `U=1` (`SUM` não é modelado).
- O handler edita a tabela *sob tradução*, então a página da leaf table precisa do próprio mapeamento de identidade (`VA = PA`) — o equivalente do simulador a um direct map de kernel.

Pra ver ao vivo: selecione o modo **program** (em **Virtual Memory → config**), monte e filtre o histórico da **walk** por `faults`: cada fault mostra a PTE que estava inválida, e a subaba **map** mostra as PTEs surgindo conforme o handler as instala.

---

## 21. Veja também

- [Mapa de memória](memory-allocation.md) — layout de endereços físicos que serve de backing store.
- [Config de cache](cache-config.md) — campos `[cache]` do `.rcfg` incluindo o bloco `[tlb]`.
- [Simulação de pipeline](pipeline.md) — onde os stalls da MMU aparecem no Gantt.
- [Simulação de cache](cache.md) — terminologia comum (sets, ways, políticas) que a TLB herda.
