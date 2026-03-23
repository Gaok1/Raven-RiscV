# RAVEN — Alocação Dinâmica de Memória

Na maioria dos ambientes você tem `malloc` e `free`. No RAVEN não existe biblioteca padrão — apenas RAM bruta e um punhado de syscalls. Este guia percorre as três formas de alocar memória dinamicamente na plataforma, da mais simples para a mais geral.

---

## Revisão do layout de memória

```
0x00000000  ┌──────────────────────────────┐
            │  .text  (código)             │
            │                              │
0x00001000  ├──────────────────────────────┤
            │  .data  (dados inicializados)│
            │  .bss   (dados zerados)      │
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤ ← bss_end  (heap começa aqui)
            │                              │
            │        espaço livre          │  ← heap cresce ↑
            │                              │
            │        ZONA SEGURA           │
            │                              │
            │        espaço livre          │  ← pilha cresce ↓
            │                              │
0x0001FFFF  └──────────────────────────────┘
            sp (0x00020000) — primeiro push → 0x0001FFFC
```

**RAM total: 128 KiB.** O heap e a pilha compartilham essa zona livre e crescem em direção um ao outro. O Raven não detecta automaticamente uma colisão — planeje o uso de memória com cuidado.

---

## Por que alocação dinâmica?

Dados estáticos (`.data` / `.bss`) funcionam bem quando os tamanhos são conhecidos em tempo de compilação:

```asm
.bss
array: .space 400    ; exatamente 100 palavras — fixo para sempre
```

Alocação dinâmica é necessária quando o tamanho só é conhecido em tempo de execução:

```asm
; o usuário informa N, você precisa de N * 4 bytes
; impossível reservar isso estaticamente
```

---

## Abordagem 1 — Bump pointer manual

**Nenhuma syscall necessária.** Mantenha um ponteiro em `.data` que rastreia o topo atual do heap e avance-o a cada alocação.

```asm
.data
heap_ptr: .word 0x00004000    ; base inicial do heap (escolha acima do seu .bss)

.text
; ─── alloc(a0 = tamanho) → a0 = ponteiro para o bloco alocado ─────────────────
; Arredonda o tamanho para o próximo múltiplo de 4 (alinhamento de palavra).
; NÃO verifica colisão com a pilha — quem chama deve garantir que há espaço.
alloc:
    la   t0, heap_ptr
    lw   a0, 0(t0)            ; a0 = heap_ptr atual  (será retornado)

    ; alinha o tamanho: size = (size + 3) & ~3
    addi t1, a1, 3
    andi t1, t1, -4           ; t1 = tamanho alinhado

    add  t2, a0, t1           ; t2 = novo heap_ptr
    sw   t2, 0(t0)            ; confirma
    ret                       ; a0 = início do bloco alocado
```

### Uso

```asm
    li   a1, 20               ; pede 20 bytes
    call alloc                ; a0 = ponteiro
    ; usa a0 como buffer de 20 bytes
```

### Vantagens / Desvantagens

| | |
|---|---|
| **+** | Zero overhead, sem syscall, trivial de entender |
| **+** | Funciona mesmo no código de inicialização mais básico |
| **−** | Sem `free` — alocações são permanentes |
| **−** | Sem detecção de OOM — corrupção silenciosa se heap encontrar pilha |

---

## Abordagem 2 — Syscall `brk`

`brk` permite que o Raven gerencie o *program break* — a fronteira entre o heap "usado" e o "livre". Esta é a base usada por implementações de `malloc`.

### Referência da syscall

| Registrador | Valor |
|-------------|-------|
| `a7`        | `214` |
| `a0`        | novo endereço do break (passe `0` para consultar sem alterar) |
| **`a0` (ret)** | break real após a chamada |

**Consulta:** `brk(0)` retorna o break atual sem movê-lo.
**Extensão:** `brk(addr)` tenta definir o break para `addr`. Se bem-sucedido, retorna `addr`. Se o Raven ficar sem memória, retorna o break *antigo* (menor que `addr`) — sempre verifique isso.

### Emulando `sbrk(n)` em assembly

`sbrk(n)` é o helper clássico "me dê mais n bytes" construído sobre `brk`. O Raven não tem syscall `sbrk`; implemente você mesmo:

```asm
; ─── sbrk(a0 = bytes) → a0 = ponteiro para o novo bloco, ou -1 em caso de OOM ─
sbrk:
    mv   t0, a0               ; salva o tamanho solicitado

    ; passo 1 — consulta o break atual
    li   a0, 0
    li   a7, 214
    ecall                     ; a0 = break atual  (= início do novo bloco)
    mv   t1, a0               ; t1 = break antigo (valor de retorno em caso de sucesso)

    ; passo 2 — calcula o novo break e solicita
    add  a0, t1, t0           ; a0 = break_antigo + tamanho
    li   a7, 214
    ecall                     ; a0 = break real após a chamada

    ; passo 3 — verifica: o Raven atendeu o pedido?
    add  t2, t1, t0           ; t2 = break_antigo + tamanho  (o que queríamos)
    blt  a0, t2, .sbrk_oom   ; se real < solicitado → OOM
    mv   a0, t1               ; retorna o break antigo (início da região alocada)
    ret

.sbrk_oom:
    li   a0, -1               ; sinaliza falha
    ret
```

### Uso

```asm
    li   a0, 256              ; pede 256 bytes
    call sbrk
    li   t0, -1
    beq  a0, t0, sem_memoria
    ; a0 = ponteiro para bloco de 256 bytes
```

### Visualizando o `brk`

```
Antes de sbrk(256):              Depois de sbrk(256):

    ┌────────────────┐               ┌────────────────┐
    │  .bss / dados  │               │  .bss / dados  │
    ├────────────────┤ ← break ant.  ├────────────────┤
    │                │               │  256 bytes     │ ← ponteiro retornado
    │   livre        │               ├────────────────┤ ← novo break
    │                │               │                │
    │   pilha ↓      │               │   livre        │
    └────────────────┘               │                │
                                     │   pilha ↓      │
                                     └────────────────┘
```

### Vantagens / Desvantagens

| | |
|---|---|
| **+** | OOM é detectável (verifica o valor de retorno) |
| **+** | Sem `heap_ptr` estático — o Raven rastreia o break |
| **+** | Idiomático — espelha como alocadores reais funcionam |
| **−** | Sem `free` — memória só cresce, nunca diminui |
| **−** | Misturar `brk` e `mmap` no mesmo programa corrompe ambos |

---

## Abordagem 3 — `mmap` anônimo

`mmap` aloca um bloco independente de memória sem mover o program break. No Raven apenas mapeamentos **anônimos** são suportados (sem arquivo, sem memória compartilhada).

### Referência da syscall

| Registrador | Valor |
|-------------|-------|
| `a7`        | `222` |
| `a0`        | endereço hint — **ignorado**, sempre passe `0` |
| `a1`        | tamanho em bytes |
| `a2`        | prot — **ignorado**, passe `3` (`PROT_READ\|PROT_WRITE`) |
| `a3`        | flags — deve incluir `MAP_ANONYMOUS` (veja abaixo) |
| `a4`        | fd — **deve ser `-1`** para mapeamentos anônimos |
| `a5`        | offset — **ignorado**, passe `0` |
| **`a0` (ret)** | ponteiro para o bloco alocado, ou `-ENOMEM` / `-EINVAL` |

**Flags necessárias:**

| Flag | Valor | Significado |
|------|-------|-------------|
| `MAP_SHARED`    | `0x01` | (use MAP_PRIVATE no lugar) |
| `MAP_PRIVATE`   | `0x02` | mapeamento privado para este processo |
| `MAP_ANONYMOUS` | `0x20` | sem arquivo de suporte |
| **Combinado**   | **`0x22`** | `MAP_PRIVATE \| MAP_ANONYMOUS` — o valor padrão |

### Exemplo — alocar um buffer de 512 bytes

```asm
    li   a0, 0          ; hint = 0 (ignorado)
    li   a1, 512        ; tamanho = 512 bytes
    li   a2, 3          ; PROT_READ|PROT_WRITE (ignorado pelo Raven)
    li   a3, 0x22       ; MAP_PRIVATE|MAP_ANONYMOUS
    li   a4, -1         ; fd = -1
    li   a5, 0          ; offset = 0
    li   a7, 222
    ecall               ; a0 = ponteiro, ou valor negativo em caso de erro

    ; verifica erro: mmap retorna -ENOMEM (-12) ou -EINVAL (-22) em caso de falha
    li   t0, -1
    bge  a0, t0, .mmap_ok   ; se a0 >= -1 → trata como ponteiro (positivo)
    ; trata erro...
.mmap_ok:
    ; a0 é o ponteiro utilizável
```

> **Verificando erros:** os códigos de erro do `mmap` são retornados como valores
> negativos com sinal (`-12` para OOM, `-22` para flags inválidas). Um ponteiro
> válido é sempre positivo num espaço de endereços de 32 bits onde o heap
> começa bem acima de 0, então verificar `blt a0, zero, erro` é uma
> heurística segura.

### `munmap` — syscall 215

`munmap` é um **no-op** no Raven. A memória alocada com `mmap` (ou `brk`) **nunca é liberada**. Chamar `munmap` retorna `0` mas não tem efeito.

```asm
    ; isso não faz nada no Raven — incluído apenas para compatibilidade de API
    mv   a0, ptr
    li   a1, 512
    li   a7, 215
    ecall               ; sempre retorna 0, memória não é liberada
```

### Vantagens / Desvantagens

| | |
|---|---|
| **+** | API familiar — igual ao `mmap` do Linux |
| **+** | Cada chamada retorna um bloco independente (sem aritmética de ponteiros) |
| **+** | OOM é detectável (valor de retorno negativo) |
| **−** | Sem `free` — `munmap` é no-op |
| **−** | Usa internamente a mesma região de heap que `brk` — **não misture os dois no mesmo programa** |

---

## Limitações específicas do Raven

| Limitação | Detalhe |
|---|---|
| **Sem `free`** | Nem `brk` nem `mmap` liberam memória. Projete programas para alocar uma única vez. |
| **`munmap` é no-op** | Sempre retorna 0; memória não é recuperada. |
| **Sem syscall `sbrk`** | Emule com duas chamadas a `brk` (veja Abordagem 2). |
| **`brk` e `mmap` compartilham o mesmo heap** | Se você chamar os dois, eles alocam da mesma região e vão corromper um ao outro. Escolha um. |
| **128 KiB de RAM total** | Heap + pilha precisam caber juntos. Um heap grande deixa pouco espaço para pilhas de chamada profundas. |
| **OOM = Raven diz não** | Se `brk` retorna menos que o solicitado, ou `mmap` retorna valor negativo, o Raven negou a alocação — você atingiu o limite de memória. |

---

## Comparação

| Característica | Bump pointer | `brk` (estilo sbrk) | `mmap` anônimo |
|---|---|---|---|
| Syscall necessária | Não | Sim (214) | Sim (222) |
| Liberar memória | Não | Não | Não (`munmap` = nop) |
| Detecção de OOM | Manual (sem guarda) | Sim — verifica retorno | Sim — verifica retorno |
| Cresce continuamente | Sim | Sim | Por bloco |
| Pode misturar com outra? | Sim (ela É a outra) | Não — conflita com mmap | Não — conflita com brk |
| Melhor para | Programas pequenos, alocadores didáticos | Crescer um buffer passo a passo | Alocar blocos independentes de tamanho fixo |

---

## Referência rápida

```
Syscall  Nome      a0        a1     a2    a3      a4   a5    ret
───────  ────────  ────────  ─────  ────  ──────  ───  ───   ──────────────────
  214    brk       novo_end  —      —     —       —    —     break real / antigo
  215    munmap    endereço  tam    —     —       —    —     0 (no-op)
  222    mmap      0(hint)   tam    prot  0x22    -1   0     ptr / -errno
```

Veja também: [syscalls.pt-BR.md](syscalls.pt-BR.md) para a referência completa de syscalls.
