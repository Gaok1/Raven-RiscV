# Hardware threads (harts)

> [Read in English](../en/hart.md)

Um **hart** — *hardware thread* — é um contexto de execução independente dentro de um sistema RISC-V: seu próprio contador de programa, banco de registradores e pilha. No Raven, cada hart é mapeado para um núcleo físico simulado. Múltiplos harts compartilham o mesmo espaço de endereçamento plano e conteúdo de memória; eles diferem apenas no estado dos registradores e na posição de execução.

---

## Modelo de execução multi-hart

Quando um programa cria um hart, o simulador aloca um slot de núcleo livre e escalona ambos os harts em round-robin sincronizado: cada tick global avança cada hart em execução por um passo. Os acessos à memória de todos os harts passam pela mesma hierarquia de cache, então a pressão de cache de um hart é visível para todos os outros.

Propriedades principais:

- Todos os harts compartilham uma única memória plana (sem espaços de endereçamento por hart, sem proteção).
- O estado do cache (I-cache, D-cache, níveis externos) é compartilhado entre todos os harts.
- IDs de hart são atribuídos pelo simulador no momento do spawn e são únicos durante a sessão.
- Um novo hart se torna executável no próximo ciclo global — nunca no meio de um ciclo.
- O núcleo selecionado na UI determina qual estado de registradores e visão de pipeline do hart é exibido.

---

## Ciclo de vida

| Estado  | Significado                                                     |
|---------|-----------------------------------------------------------------|
| `FREE`  | Slot de núcleo ainda não usado (nenhum hart atribuído).         |
| `RUN`   | Hart executando instruções normalmente.                         |
| `BRK`   | Hart atingiu um breakpoint ou `ebreak`; pausado para inspeção.  |
| `EXIT`  | Hart terminou via `hart_exit` ou `exit_group`.                  |
| `FAULT` | Hart encontrou um erro irrecuperável.                           |

Um `FAULT` em qualquer hart para toda a simulação. Um `EXIT` via `exit_group` (ou syscall `exit` 93/94) também termina todos os harts imediatamente. Um `EXIT` via `hart_exit` (syscall 1101) termina apenas o hart chamador; os demais continuam.

---

## Criando harts em assembly

Use a syscall `1100` (`hart_start`) para criar um novo hart.

**Registradores na entrada:**

| Registrador | Valor                                                                  |
|-------------|------------------------------------------------------------------------|
| `a7`        | `1100`                                                                 |
| `a0`        | PC de entrada — endereço da primeira instrução que o novo hart executará |
| `a1`        | Stack pointer — endereço **topo** (alto) da região de pilha do novo hart |
| `a2`        | Argumento passado ao novo hart em seu `a0`                             |

**Valor de retorno:**

| `a0` | Significado                                     |
|------|-------------------------------------------------|
| ≥ 0  | ID de hart atribuído pelo simulador             |
| −1   | Nenhum slot de núcleo livre disponível          |
| −2   | PC de entrada está fora do programa carregado   |
| −3   | Stack pointer inválido (zero, não alinhado a 16 bytes, ou fora da memória) |

O novo hart inicia com um banco de registradores limpo, exceto `sp` (definido como `a1`) e `a0` (definido como `a2`). Todos os outros registradores começam em zero.

**Exemplo (Falcon assembly):**

```asm
.data
hart1_stack: .space 4096
.text

main:
    la   a0, child_entry
    la   a1, hart1_stack
    addi a1, a1, 4096         # topo da pilha = base + tamanho
    li   a2, 42               # argumento passado ao filho
    li   a7, 1100
    ecall                     # a0 = id do hart (ou < 0 em erro)

child_entry:
    # a0 = argumento (42)
    li   a7, 1101
    ecall                     # hart_exit: apenas este hart termina
```

---

## Terminando um hart

| Syscall       | Número | Efeito                                               |
|---------------|--------|------------------------------------------------------|
| `hart_exit`   | `1101` | Termina **somente este hart**. Outros harts continuam. |
| `exit`        | `93`   | Termina **todos os harts**. Saída global do programa.  |
| `exit_group`  | `94`   | Termina **todos os harts**. Idêntico ao `exit` no Raven. |

Use `hart_exit` em harts trabalhadores para que o hart principal continue. Use `exit` ou `exit_group` apenas quando o programa inteiro deve parar.

---

## Criando harts em C (`c-to-raven`)

O cabeçalho `raven.h` fornece `falcon_hart_start` e a macro de conveniência `SPAWN_HART`.

```c
#include "raven.h"

static char worker_stack[4096];

void worker(unsigned int arg) {
    raven_print_uint(arg);
    falcon_hart_exit();
}

int main(void) {
    // Chamada explícita
    falcon_hart_start(
        (unsigned int)worker,
        (unsigned int)(worker_stack + sizeof(worker_stack)),
        /*arg=*/1
    );

    // Ou com a macro de conveniência (array de pilha deve estar no escopo)
    SPAWN_HART(worker, worker_stack, /*arg=*/2);

    raven_print_str("main concluído\n");
    return 0;
}
```

---

## Criando harts em Rust (`rust-to-raven`)

Duas variantes estão disponíveis em `raven_api::hart`:

### `spawn_hart_fn` — ponteiro de função, zero alocação

```rust
use raven_api::{spawn_hart_fn, exit};

static mut STACK: [u8; 4096] = [0; 4096];

fn worker(id: u32) -> ! {
    raven_api::syscall::print_uint(id);
    raven_api::syscall::hart_exit()
}

fn main() {
    spawn_hart_fn(worker, unsafe { &mut STACK }, /*arg=*/1);
    exit(0)
}
```

### `spawn_hart` — closure, alocação no heap

```rust
use raven_api::{spawn_hart, exit};

static mut STACK: [u8; 4096] = [0; 4096];

fn main() {
    let value = 99u32;
    spawn_hart(
        move || {
            raven_api::syscall::print_uint(value);
            raven_api::syscall::hart_exit()
        },
        unsafe { &mut STACK },
    );
    exit(0)
}
```

---

## UI — seletor de núcleos

A aba Pipeline e a aba Run mostram um **seletor de núcleos** na barra de ferramentas. Alternar entre núcleos muda a exibição de registradores, visão de instruções e estado do pipeline para o hart selecionado sem parar a execução.

Emblemas de status do núcleo aparecem ao lado de cada índice de núcleo: `RUN`, `BRK`, `EXIT`, `FAULT`, `FREE`.

A configuração de **escopo de execução** (aba Settings) controla se `Run` (`r`) avança apenas o hart focado (`FOCUS`) ou todos os harts em execução simultaneamente (`ALL`). O escopo `ALL` é o padrão para programas multi-hart.

---

## Requisitos de pilha

Cada hart precisa de sua própria pilha. Os requisitos são:

- `stack_ptr` deve ser o endereço **alto** (topo) da região de pilha — a pilha cresce para baixo.
- `stack_ptr` deve ser **alinhado a 16 bytes** (requisito da ABI RISC-V).
- `stack_ptr` deve estar dentro do intervalo de memória do programa carregado.
- A pilha do hart não deve se sobrepor à pilha de nenhum outro hart ou ao segmento de dados do programa.

O Raven **não** impõe limites de pilha em tempo de execução.

---

## Configurando o número de núcleos

O número máximo de harts simultâneos é definido na **aba Settings** (`max_cores`). O padrão é `1`. Cada núcleo adicional adiciona um slot que pode ser ocupado por um hart criado. Definir `max_cores` como `N` permite até `N − 1` harts filhos concorrentes mais o hart principal.

Alterações em `max_cores` têm efeito após o próximo reset do programa.

---

## Veja também

- [Referência de syscalls](syscalls.md) — tabela completa incluindo `1100` e `1101`
- [Simulação de pipeline](../en/pipeline.md) — estado e visualização do pipeline por hart
