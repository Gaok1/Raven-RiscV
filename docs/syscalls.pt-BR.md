# RAVEN — Referência de Syscalls

O RAVEN usa a convenção de chamada **Linux RISC-V ABI** para `ecall`.

```
a7      = número da syscall
a0..a5  = argumentos (a0 = arg1, a1 = arg2, ...)
a0      = valor de retorno  (valor negativo = -errno como u32)
```

---

## Layout de Memória

```
0x00000000  ┌──────────────────────────────┐
            │  .text  (código)             │  ← instruções, carregadas em base_pc (padrão 0x0)
            │                              │
0x00001000  ├──────────────────────────────┤
            │  .data  (dados inicializados)│  ← data_base = base_pc + 0x1000
            │  .bss   (inicializado com 0) │  ← zerado ao carregar; cresce para cima após .data
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤
            │                              │
            │  espaço livre  (heap manual) │  ← sem alocador; use sw/lw diretamente
            │                              │    ou implemente um bump pointer manualmente
            │  heap_ptr →                  │    (ex.: guarde heap_ptr em um label .data)
            │                              │
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤
            │  pilha  (cresce ↓)           │  ← sp = 0x00020000 (um além do fim da RAM)
            │                              │    push:  addi sp, sp, -4 / sw rs, 0(sp)
0x0001FFFF  └──────────────────────────────┘    pop:   lw rd, 0(sp)  / addi sp, sp, 4
            sp (0x00020000) — primeiro push → sp = 0x0001FFFC
```

### Endereços importantes

| Símbolo     | Valor        | Descrição                                  |
|------------|-------------|---------------------------------------------|
| `base_pc`  | `0x00000000` | Início do `.text` (configurável na aba Run) |
| `data_base`| `0x00001000` | Início do `.data` / `.bss`                 |
| `bss_end`  | dinâmico     | Primeiro byte após o `.bss`                |
| `sp` inicial | `0x00020000` | Um além do fim da RAM (convenção ABI RISC-V); primeiro `push` escreve em `0x0001FFFC` |

### Heap manual — padrão bump allocator

O RAVEN **não tem `malloc`/`free`**. A região livre entre `bss_end` e o fundo da
pilha é RAM comum. Para alocar memória dinamicamente, guarde um ponteiro em `.data`
e avance-o manualmente:

```asm
.data
heap_ptr: .word 0x00004000   ; base inicial do heap (acima do seu .bss)

.text
; alloc(tamanho) → a0 = ponteiro para o bloco alocado
; a1 = tamanho em bytes
alloc:
    la   t0, heap_ptr
    lw   a0, 0(t0)        ; a0 = heap_ptr atual  (valor de retorno)
    add  t1, a0, a1       ; t1 = heap_ptr + tamanho
    sw   t1, 0(t0)        ; heap_ptr += tamanho
    ret
```

O heap cresce para **cima** (endereços crescentes), enquanto a pilha cresce para
**baixo**. Eles colidirão se a alocação combinada exceder o espaço livre — o RAVEN
não detecta isso; ocorrerá uma falha de memória ou corrupção silenciosa de dados.

```
                    bss_end
                       │
                       ▼
        ┌──────────────────────────────┐
        │  fim do .bss                 │
        ├──────────────────────────────┤ ← heap_ptr (inicial)
        │  bloco heap 0  (alloc #1)    │
        │  bloco heap 1  (alloc #2)    │  heap cresce ↑
        │  ...                         │
        │                              │
        │        ZONA SEGURA           │
        │                              │
        │  ...                         │
        │  frame de pilha N            │  pilha cresce ↓
        │  frame de pilha N-1          │
        ├──────────────────────────────┤
        │  sp (atual)                  │
        └──────────────────────────────┘ 0x0001FFFC
```

---

## Syscalls Linux ABI

### `read` — syscall 63

Lê bytes de um descritor de arquivo para um buffer.

| Registrador | Valor |
|------------|-------|
| `a7`       | `63`  |
| `a0`       | fd (somente 0 = stdin) |
| `a1`       | endereço do buffer |
| `a2`       | máximo de bytes a ler |
| **`a0` (ret)** | bytes lidos, ou `-errno` |

**Restrições:** somente `fd=0` (stdin) é suportado. Qualquer outro fd retorna `-EBADF`.
A chamada bloqueia até que o usuário pressione Enter no console.

```asm
.bss
buf: .space 256

.text
    li   a0, 0          ; fd = stdin
    la   a1, buf        ; buffer
    li   a2, 256        ; máximo de bytes
    li   a7, 63         ; read
    ecall               ; a0 = bytes lidos (inclui '\n')
```

---

### `write` — syscall 64

Escreve bytes de um buffer para um descritor de arquivo.

| Registrador | Valor |
|------------|-------|
| `a7`       | `64`  |
| `a0`       | fd (1=stdout, 2=stderr) |
| `a1`       | endereço do buffer |
| `a2`       | quantidade de bytes |
| **`a0` (ret)** | bytes escritos, ou `-errno` |

**Restrições:** somente `fd=1` e `fd=2` são suportados (ambos vão para o console do RAVEN).

```asm
.data
msg: .asciz "ola\n"

.text
    la   a1, msg
    li   a2, 4          ; comprimento incluindo '\n'
    li   a0, 1          ; stdout
    li   a7, 64         ; write
    ecall
```

---

### `exit` — syscall 93 / `exit_group` — syscall 94

Encerra o programa.

| Registrador | Valor |
|------------|-------|
| `a7`       | `93` ou `94` |
| `a0`       | código de saída |

```asm
    li   a0, 0          ; código de saída 0
    li   a7, 93
    ecall
```

---

### `getrandom` — syscall 278

Preenche um buffer com bytes aleatórios criptograficamente seguros (delegado ao SO).

| Registrador | Valor |
|------------|-------|
| `a7`       | `278` |
| `a0`       | endereço do buffer |
| `a1`       | quantidade de bytes |
| `a2`       | flags (0, `GRND_NONBLOCK`=1, `GRND_RANDOM`=2) |
| **`a0` (ret)** | bytes escritos, ou `-errno` |

```asm
.bss
rng_buf: .space 4

.text
    la   a0, rng_buf
    li   a1, 4          ; 4 bytes aleatórios
    li   a2, 0          ; flags = 0
    li   a7, 278
    ecall               ; rng_buf contém um u32 aleatório
    la   t0, rng_buf
    lw   t1, 0(t0)      ; t1 = palavra aleatória
```

---

## Extensões de ensino Falcon (syscall 1000+)

Estas são syscalls exclusivas do RAVEN, projetadas para uso em sala de aula. São
mais simples que os equivalentes Linux ABI e não precisam de loop strlen nem
argumento fd.

### `1000` — imprimir inteiro

Imprime o inteiro com sinal de 32 bits em `a0` no console (sem newline).

| Registrador | Valor |
|------------|-------|
| `a7`       | `1000` |
| `a0`       | inteiro a imprimir |

```asm
    li   a0, -42
    li   a7, 1000
    ecall               ; imprime "-42"
```

**Pseudo:** `print rd` expande para isso automaticamente.

---

### `1001` — imprimir string NUL-terminada

Imprime a string NUL-terminada a partir de `a0` (sem newline ao final).

| Registrador | Valor |
|------------|-------|
| `a7`       | `1001` |
| `a0`       | endereço da string NUL-terminada |

```asm
.data
s: .asciz "ola"

.text
    la   a0, s
    li   a7, 1001
    ecall
```

---

### `1002` — imprimir string NUL-terminada + newline

Igual à 1001, mas acrescenta `'\n'` após a string.

| Registrador | Valor |
|------------|-------|
| `a7`       | `1002` |
| `a0`       | endereço da string NUL-terminada |

---

### `1003` — ler linha (NUL-terminada)

Lê uma linha da entrada do console para um buffer. Escreve o texto seguido de um
byte NUL (`'\0'`); a quebra de linha **não** é incluída.

| Registrador | Valor |
|------------|-------|
| `a7`       | `1003` |
| `a0`       | endereço do buffer de destino |

A chamada bloqueia até o usuário pressionar Enter. Garanta que o buffer seja grande
o suficiente para a entrada esperada.

---

### `1010` — ler byte

Lê um inteiro da entrada (faixa 0..255) e armazena como `u8` no endereço em `a0`.

| Registrador | Valor |
|------------|-------|
| `a7`       | `1010` |
| `a0`       | endereço de destino |

Aceita decimal ou hexadecimal com prefixo `0x`. Se fora da faixa ou inválido, um
erro é exibido e a execução pausa.

**Pseudo:** `read_byte label`

---

### `1011` — ler meia-palavra

Lê um inteiro da entrada (faixa 0..65535) e armazena como `u16` (little-endian) em `a0`.

| Registrador | Valor |
|------------|-------|
| `a7`       | `1011` |
| `a0`       | endereço de destino |

**Pseudo:** `read_half label`

---

### `1012` — ler palavra

Lê um inteiro da entrada (faixa 0..4294967295) e armazena como `u32` (little-endian) em `a0`.

| Registrador | Valor |
|------------|-------|
| `a7`       | `1012` |
| `a0`       | endereço de destino |

**Pseudo:** `read_word label`

---

## Pseudo-instruções que usam ecall

| Pseudo | Expansão | Syscall(s) | Corrompe |
|--------|---------|------------|---------|
| `print rd` | `li a7,1000; mv a0,rd; ecall` | 1000 | a0, a7 |
| `print_str label` | loop strlen + write | 64 | a0, a1, a2, a7, t0 |
| `print_str_ln label` | strlen + write + write('\n') | 64×2 | a0, a1, a2, a7, t0, sp (temp) |
| `read label` | `li a0,0; la a1,label; li a2,256; li a7,63; ecall` | 63 | a0, a1, a2, a7 |
| `read_byte label` | `li a7,1010; la a0,label; ecall` | 1010 | a0, a7 |
| `read_half label` | `li a7,1011; la a0,label; ecall` | 1011 | a0, a7 |
| `read_word label` | `li a7,1012; la a0,label; ecall` | 1012 | a0, a7 |
| `random rd` | getrandom em temp de pilha, lw rd | 278 | a0, a1, a2, a7, sp (temp) |
| `random_bytes label, n` | `la a0,label; li a1,n; li a2,0; li a7,278; ecall` | 278 | a0, a1, a2, a7 |

> `push rs` / `pop rd` **não** usam ecall — expandem para `addi sp,sp,-4 / sw` e `lw / addi sp,sp,4`.

---

## Códigos de erro

| Código | Nome POSIX | Significado no RAVEN |
|--------|-----------|----------------------|
| `-5`   | `EIO`     | falha do SO em getrandom |
| `-9`   | `EBADF`   | fd não suportado |
| `-14`  | `EFAULT`  | endereço fora dos limites |
| `-22`  | `EINVAL`  | flags não suportadas |

Os valores de retorno são representados como `u32` envolvendo o `i32` negativo
(ex.: `-9` → `0xFFFFFFF7`).

---

## Cartão de referência rápida

```
Num   Nome             a0        a1        a2        retorno
────  ───────────────  ────────  ────────  ────────  ────────────────
 63   read             fd=0      end. buf  máx bytes bytes lidos / -err
 64   write            fd=1/2    end. buf  qtd       bytes escritos / -err
 93   exit             código    —         —         (não retorna)
 94   exit_group       código    —         —         (não retorna)
278   getrandom        end. buf  len       flags     len / -err

1000  print_int        inteiro   —         —         —
1001  print_str        end. str  —         —         —
1002  print_str_ln     end. str  —         —         —
1003  read_line_z      end. buf  —         —         —
1010  read_u8          end. dst  —         —         —
1011  read_u16         end. dst  —         —         —
1012  read_u32         end. dst  —         —         —
```
