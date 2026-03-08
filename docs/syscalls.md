# RAVEN — Syscall Reference

RAVEN uses the **Linux RISC-V ABI** calling convention for `ecall`.

```
a7  = syscall number
a0..a5 = arguments (a0 = arg1, a1 = arg2, ...)
a0  = return value  (negative value = -errno as u32)
```

---

## Memory Layout

```
0x00000000  ┌──────────────────────────────┐
            │  .text  (code)               │  ← instructions, loaded at base_pc (default 0x0)
            │                              │
0x00001000  ├──────────────────────────────┤
            │  .data  (initialized data)   │  ← data_base = base_pc + 0x1000
            │  .bss   (zero-initialized)   │  ← zero-filled at load; grows up after .data
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤
            │                              │
            │  free space  (manual heap)   │  ← no allocator; use sw/lw directly
            │                              │    or implement a bump pointer yourself
            │  heap_ptr →                  │    (e.g. store heap_ptr in a .data label)
            │                              │
            ├  ─  ─  ─  ─  ─  ─  ─  ─  ─ ┤
            │  stack  (grows ↓)            │  ← sp starts at 0x0001FFFC
            │                              │    push:  addi sp, sp, -4 / sw rs, 0(sp)
0x0001FFFC  │  sp (initial)                │    pop:   lw rd, 0(sp)  / addi sp, sp, 4
0x0001FFFF  └──────────────────────────────┘
```

### Key addresses

| Symbol       | Value        | Description                              |
|-------------|-------------|------------------------------------------|
| `base_pc`   | `0x00000000` | Start of `.text` (configurable in Run tab) |
| `data_base` | `0x00001000` | Start of `.data` / `.bss`               |
| `bss_end`   | dynamic      | First byte after `.bss` (end of static data) |
| `sp` initial | `0x0001FFFC` | Top of 128 KB RAM, word-aligned         |

### Manual heap — bump allocator pattern

RAVEN has **no `malloc`/`free`**. The free region between `bss_end` and the
stack bottom is plain RAM. To allocate dynamically, keep a pointer in `.data`
and advance it manually:

```asm
.data
heap_ptr: .word 0x00004000   ; initial heap base (above your .bss)

.text
; alloc(size) → a0 = pointer to allocated block
; a1 = size in bytes
alloc:
    la   t0, heap_ptr
    lw   a0, 0(t0)        ; a0 = current heap_ptr  (return value)
    add  t1, a0, a1       ; t1 = heap_ptr + size
    sw   t1, 0(t0)        ; heap_ptr += size
    ret
```

The heap grows **upward** (toward higher addresses), while the stack grows
**downward**. They will collide if the combined allocation exceeds free space —
RAVEN does not detect this; a memory fault or silent data corruption will occur.

```
                    bss_end
                       │
                       ▼
        ┌──────────────────────────────┐
        │  .bss end                    │
        ├──────────────────────────────┤ ← heap_ptr (initial)
        │  heap block 0   (alloc #1)   │
        │  heap block 1   (alloc #2)   │  heap grows ↑
        │  ...                         │
        │                              │
        │        SAFE ZONE             │
        │                              │
        │  ...                         │
        │  stack frame N               │  stack grows ↓
        │  stack frame N-1             │
        ├──────────────────────────────┤
        │  sp (current)                │
        └──────────────────────────────┘ 0x0001FFFC
```

---

## Linux ABI syscalls

### `read` — syscall 63

Read bytes from a file descriptor into a buffer.

| Register | Value        |
|----------|-------------|
| `a7`     | `63`        |
| `a0`     | fd (0 = stdin only) |
| `a1`     | buffer address |
| `a2`     | max bytes to read |
| **`a0` (ret)** | bytes read, or `-errno` |

**Restrictions:** only `fd=0` (stdin) is supported. Any other fd returns `-EBADF`.
The call blocks until the user presses Enter in the console.

```asm
.bss
buf: .space 256

.text
    li   a0, 0          ; fd = stdin
    la   a1, buf        ; buffer
    li   a2, 256        ; max bytes
    li   a7, 63         ; read
    ecall               ; a0 = bytes read (includes '\n')
```

---

### `write` — syscall 64

Write bytes from a buffer to a file descriptor.

| Register | Value        |
|----------|-------------|
| `a7`     | `64`        |
| `a0`     | fd (1=stdout, 2=stderr) |
| `a1`     | buffer address |
| `a2`     | byte count |
| **`a0` (ret)** | bytes written, or `-errno` |

**Restrictions:** only `fd=1` and `fd=2` are supported (both go to the RAVEN console).

```asm
.data
msg: .asciz "hello\n"

.text
    la   a1, msg
    li   a2, 6          ; length including '\n'
    li   a0, 1          ; stdout
    li   a7, 64         ; write
    ecall
```

---

### `exit` — syscall 93 / `exit_group` — syscall 94

Terminate the program.

| Register | Value |
|----------|-------|
| `a7`     | `93` or `94` |
| `a0`     | exit code |

```asm
    li   a0, 0          ; exit code 0
    li   a7, 93
    ecall
```

---

### `getrandom` — syscall 278

Fill a buffer with cryptographically random bytes (delegates to the OS).

| Register | Value |
|----------|-------|
| `a7`     | `278` |
| `a0`     | buffer address |
| `a1`     | byte count |
| `a2`     | flags (0, `GRND_NONBLOCK`=1, `GRND_RANDOM`=2) |
| **`a0` (ret)** | bytes written, or `-errno` |

```asm
.bss
rng_buf: .space 4

.text
    la   a0, rng_buf
    li   a1, 4          ; 4 random bytes
    li   a2, 0          ; flags = 0
    li   a7, 278
    ecall               ; rng_buf contains a random u32
    la   t0, rng_buf
    lw   t1, 0(t0)      ; t1 = random word
```

---

## Falcon teaching extensions (syscall 1000+)

These are RAVEN-specific syscalls designed for classroom use. They are higher
level than the Linux ABI equivalents and need no strlen loop or fd argument.

### `1000` — print integer

Print the signed 32-bit integer in `a0` to the console (no newline).

| Register | Value |
|----------|-------|
| `a7`     | `1000` |
| `a0`     | integer to print |

```asm
    li   a0, -42
    li   a7, 1000
    ecall               ; prints "-42"
```

**Pseudo:** `print rd` expands to this automatically.

---

### `1001` — print null-terminated string

Print a NUL-terminated string starting at `a0` (no newline appended).

| Register | Value |
|----------|-------|
| `a7`     | `1001` |
| `a0`     | address of NUL-terminated string |

```asm
.data
s: .asciz "hello"

.text
    la   a0, s
    li   a7, 1001
    ecall
```

---

### `1002` — print null-terminated string + newline

Same as 1001 but appends `'\n'` after the string.

| Register | Value |
|----------|-------|
| `a7`     | `1002` |
| `a0`     | address of NUL-terminated string |

---

### `1003` — read line (NUL-terminated)

Read one line of user input from the console into a buffer. Writes the text
followed by a NUL byte (`'\0'`); the newline is **not** included.

| Register | Value |
|----------|-------|
| `a7`     | `1003` |
| `a0`     | destination buffer address |

The call blocks until the user presses Enter. Ensure the buffer is large enough.

---

### `1010` — read byte

Parse one integer from stdin (range 0..255) and store it as a `u8` at the address in `a0`.

| Register | Value |
|----------|-------|
| `a7`     | `1010` |
| `a0`     | destination address |

Accepts decimal or `0x`-prefixed hex. If out of range or invalid, an error is shown and execution pauses.

**Pseudo:** `read_byte label`

---

### `1011` — read half

Parse one integer from stdin (range 0..65535) and store it as a `u16` (little-endian) at `a0`.

| Register | Value |
|----------|-------|
| `a7`     | `1011` |
| `a0`     | destination address |

**Pseudo:** `read_half label`

---

### `1012` — read word

Parse one integer from stdin (range 0..4294967295) and store it as a `u32` (little-endian) at `a0`.

| Register | Value |
|----------|-------|
| `a7`     | `1012` |
| `a0`     | destination address |

**Pseudo:** `read_word label`

---

## Pseudo-instructions that use ecall

| Pseudo | Expands to | Syscall(s) | Clobbers |
|--------|-----------|------------|---------|
| `print rd` | `li a7,1000; mv a0,rd; ecall` | 1000 | a0, a7 |
| `print_str label` | strlen loop + write | 64 | a0, a1, a2, a7, t0 |
| `print_str_ln label` | strlen + write + write('\n') | 64×2 | a0, a1, a2, a7, t0, sp (temp) |
| `read label` | `li a0,0; la a1,label; li a2,256; li a7,63; ecall` | 63 | a0, a1, a2, a7 |
| `read_byte label` | `li a7,1010; la a0,label; ecall` | 1010 | a0, a7 |
| `read_half label` | `li a7,1011; la a0,label; ecall` | 1011 | a0, a7 |
| `read_word label` | `li a7,1012; la a0,label; ecall` | 1012 | a0, a7 |
| `random rd` | getrandom into stack temp, lw rd | 278 | a0, a1, a2, a7, sp (temp) |
| `random_bytes label, n` | `la a0,label; li a1,n; li a2,0; li a7,278; ecall` | 278 | a0, a1, a2, a7 |

> `push rs` / `pop rd` do **not** use ecall — they expand to `addi sp,sp,-4 / sw` and `lw / addi sp,sp,4`.

---

## Error codes

| Code | POSIX name | Meaning in RAVEN |
|------|-----------|-------------------|
| `-5`  | `EIO`    | getrandom OS failure |
| `-9`  | `EBADF`  | unsupported fd |
| `-14` | `EFAULT` | address out of bounds |
| `-22` | `EINVAL` | unsupported flags |

Return values are returned as `u32` wrapping of the negative `i32` (e.g. `-9` → `0xFFFFFFF7`).

---

## Quick reference card

```
Num   Name             a0        a1        a2        ret
────  ───────────────  ────────  ────────  ────────  ───────────────
 63   read             fd=0      buf addr  max bytes bytes read / -err
 64   write            fd=1/2    buf addr  count     bytes written / -err
 93   exit             code      —         —         (no return)
 94   exit_group       code      —         —         (no return)
278   getrandom        buf addr  len       flags     len / -err

1000  print_int        int       —         —         —
1001  print_str        str addr  —         —         —
1002  print_str_ln     str addr  —         —         —
1003  read_line_z      buf addr  —         —         —
1010  read_u8          dst addr  —         —         —
1011  read_u16         dst addr  —         —         —
1012  read_u32         dst addr  —         —         —
```
