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
            │  stack  (grows ↓)            │  ← sp = 0x00020000 (one past end of RAM)
            │                              │    push:  addi sp, sp, -4 / sw rs, 0(sp)
0x0001FFFF  └──────────────────────────────┘    pop:   lw rd, 0(sp)  / addi sp, sp, 4
            sp (0x00020000) — first push → sp = 0x0001FFFC
```

### Key addresses

| Symbol       | Value        | Description                              |
|-------------|-------------|------------------------------------------|
| `base_pc`   | `0x00000000` | Start of `.text` (configurable in Run tab) |
| `data_base` | `0x00001000` | Start of `.data` / `.bss`               |
| `bss_end`   | dynamic      | First byte after `.bss` (end of static data) |
| `sp` initial | `0x00020000` | One past end of RAM (RISC-V ABI convention); first `push` writes to `0x0001FFFC` |

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
Output to `fd=2` (stderr) is displayed in **red** in the console.

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

In multi-hart mode, `exit` and `exit_group` stop the whole program. The final exit code comes from the hart that executed the syscall.

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

### `writev` — syscall 66

Write data from multiple buffers (scatter write).

| Register | Value |
|----------|-------|
| `a7`     | `66` |
| `a0`     | fd (1=stdout, 2=stderr) |
| `a1`     | pointer to `iovec[]` array |
| `a2`     | number of entries |
| **`a0` (ret)** | total bytes written, or `-errno` |

Each `iovec` entry is `{ u32 base, u32 len }` (8 bytes, little-endian).

**Restrictions:** same fd restrictions as `write` (fd=1 or fd=2 only).

---

### `getpid` — syscall 172

Returns the simulated process ID (always `1`).

| Register | Value |
|----------|-------|
| `a7`     | `172` |
| **`a0` (ret)** | `1` |

---

### `getuid` — syscall 174 / `getgid` — syscall 176

Returns simulated user/group ID (always `0`).

| Register | Value |
|----------|-------|
| `a7`     | `174` or `176` |
| **`a0` (ret)** | `0` |

---

### `mmap` — syscall 222

Allocate a block of anonymous memory from the heap.

| Register | Value |
|----------|-------|
| `a7`     | `222` |
| `a0`     | hint address (ignored; pass 0) |
| `a1`     | length in bytes |
| `a2`     | prot (ignored) |
| `a3`     | flags — must include `MAP_ANONYMOUS` (0x20) |
| `a4`     | fd — must be `-1` for anonymous mappings |
| `a5`     | offset (ignored) |
| **`a0` (ret)** | allocated pointer, or `-EINVAL` / `-ENOMEM` |

**Restrictions:** only anonymous mappings (`MAP_ANONYMOUS=0x20`, `fd=-1`) are supported.
Memory is allocated from the heap (same region as `brk`). `munmap` is a no-op.

```asm
    li   a0, 0          ; hint = 0
    li   a1, 256        ; allocate 256 bytes
    li   a2, 3          ; PROT_READ|PROT_WRITE (ignored)
    li   a3, 0x22       ; MAP_ANONYMOUS|MAP_PRIVATE
    li   a4, -1         ; fd = -1
    li   a5, 0          ; offset = 0
    li   a7, 222
    ecall               ; a0 = pointer to allocated block
```

---

### `munmap` — syscall 215

No-op in RAVEN (always returns 0). Memory is not freed.

| Register | Value |
|----------|-------|
| `a7`     | `215` |
| **`a0` (ret)** | `0` |

---

### `clock_gettime` — syscall 403

Fill a `timespec` with the simulated time (derived from instruction count).

| Register | Value |
|----------|-------|
| `a7`     | `403` |
| `a0`     | clock ID (ignored; all clocks return instruction-based time) |
| `a1`     | pointer to `timespec { u32 tv_sec, u32 tv_nsec }` |
| **`a0` (ret)** | `0`, or `-EFAULT` |

Time is approximated at 10 ns per instruction (100 MHz equivalent).

---

## Falcon teaching extensions (syscall 1000+)

These are RAVEN-specific syscalls designed for classroom use. They are higher
level than the Linux ABI equivalents and need no strlen loop or fd argument.

### `1100` — start hart

Start a new hart on a free physical core.

`hart` means `hardware thread` in RISC-V terminology. RAVEN uses this name intentionally to describe a hardware execution context, not an OS-managed software thread.

| Register | Value |
|----------|-------|
| `a7`     | `1100` |
| `a0`     | child entry PC |
| `a1`     | child initial stack pointer |
| `a2`     | child initial argument |
| **`a0` (ret)** | hart id on success, negative error code on failure |

Current v1 semantics:

- the child hart starts with a clean register file
- `pc = child entry`
- `sp = child stack pointer`
- `a0 = child initial argument`
- the new hart becomes runnable on the next global cycle, never mid-cycle
- if no free core exists, the syscall fails immediately

This syscall is only meaningful when the machine is configured with more than one core.

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

### `1004` — print unsigned integer

Print the unsigned 32-bit integer in `a0` (no newline).

| Register | Value |
|----------|-------|
| `a7`     | `1004` |
| `a0`     | u32 to print |

```asm
    li   a0, 4294967295
    li   a7, 1004
    ecall               ; prints "4294967295"
```

---

### `1005` — print hex

Print the value in `a0` as an 8-digit hex string with `0x` prefix (no newline).

| Register | Value |
|----------|-------|
| `a7`     | `1005` |
| `a0`     | value to print |

```asm
    li   a0, 0xDEADBEEF
    li   a7, 1005
    ecall               ; prints "0xDEADBEEF"
```

---

### `1006` — print character

Print the ASCII character whose code is in `a0` (no newline).

| Register | Value |
|----------|-------|
| `a7`     | `1006` |
| `a0`     | ASCII code (0..127) |

```asm
    li   a0, 65         ; 'A'
    li   a7, 1006
    ecall               ; prints "A"
```

---

### `1008` — print newline

Print a `'\n'` character (no arguments).

| Register | Value |
|----------|-------|
| `a7`     | `1008` |

```asm
    li   a7, 1008
    ecall
```

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

### `1013` — read signed integer

Parse one signed integer from stdin (range -2147483648..2147483647) and store it as an `i32` at `a0`.
Accepts decimal (optionally negative) or `0x`-prefixed hex.

| Register | Value |
|----------|-------|
| `a7`     | `1013` |
| `a0`     | destination address |

```asm
.bss
n: .space 4

.text
    la   a0, n
    li   a7, 1013
    ecall               ; reads e.g. "-100", stores as i32
    lw   t0, n
```

---

### `1014` — read float

Parse one floating-point number from stdin and store it as an IEEE 754 `f32` at `a0`.

| Register | Value |
|----------|-------|
| `a7`     | `1014` |
| `a0`     | destination address |

---

### `1015` — print float

Print the `f32` value in `fa0` to the console (no newline). Up to 6 significant digits.

| Register | Value |
|----------|-------|
| `a7`     | `1015` |
| `fa0`    | f32 value to print |

```asm
    fli.s  fa0, 3.14    ; or flw fa0, addr
    li     a7, 1015
    ecall               ; prints "3.14"
```

---

### `1030` — get instruction count

Return the number of instructions executed since program start (low 32 bits).

| Register | Value |
|----------|-------|
| `a7`     | `1030` |
| **`a0` (ret)** | instruction count |

Useful for measuring performance of algorithms directly inside a program.

```asm
    li   a7, 1030
    ecall
    mv   s0, a0         ; s0 = baseline count

    ; ... algorithm A ...

    li   a7, 1030
    ecall
    sub  a0, a0, s0     ; a0 = instructions used by algorithm A
    li   a7, 1000
    ecall               ; print it
```

---

### `1031` — get cycle count

Returns the total elapsed cycle count for the current execution mode.

- Sequential mode: returns the sequential/cache model total.
- Pipeline mode: returns the pipeline wall-clock.

| Register | Value |
|----------|-------|
| `a7`     | `1031` |
| **`a0` (ret)** | cycle count |

---

### `1050` — memset

Fill `a2` bytes starting at `a0` with the byte value in `a1`.

| Register | Value |
|----------|-------|
| `a7`     | `1050` |
| `a0`     | destination address |
| `a1`     | byte value (0..255) |
| `a2`     | length in bytes |

```asm
.bss
buf: .space 64

.text
    la   a0, buf
    li   a1, 0          ; fill with 0
    li   a2, 64
    li   a7, 1050
    ecall
```

---

### `1051` — memcpy

Copy `a2` bytes from `a1` to `a0`. Regions must not overlap.

| Register | Value |
|----------|-------|
| `a7`     | `1051` |
| `a0`     | destination address |
| `a1`     | source address |
| `a2`     | length in bytes |

---

### `1052` — strlen

Return the length of the NUL-terminated string at `a0` (NUL not counted).

| Register | Value |
|----------|-------|
| `a7`     | `1052` |
| `a0`     | string address |
| **`a0` (ret)** | length |

```asm
.data
s: .asciz "hello"

.text
    la   a0, s
    li   a7, 1052
    ecall               ; a0 = 5
```

---

### `1053` — strcmp

Compare NUL-terminated strings at `a0` and `a1`.

| Register | Value |
|----------|-------|
| `a7`     | `1053` |
| `a0`     | address of string 1 |
| `a1`     | address of string 2 |
| **`a0` (ret)** | negative if s1 < s2, 0 if equal, positive if s1 > s2 |

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
| `-12` | `ENOMEM` | heap exhausted (mmap) |
| `-14` | `EFAULT` | address out of bounds |
| `-22` | `EINVAL` | unsupported flags / bad arguments |

Return values are returned as `u32` wrapping of the negative `i32` (e.g. `-9` → `0xFFFFFFF7`).

---

## Quick reference card

```
Num   Name             a0          a1          a2          ret
────  ───────────────  ──────────  ──────────  ──────────  ─────────────────
 63   read             fd=0        buf addr    max bytes   bytes read / -err
 64   write            fd=1/2      buf addr    count       bytes written / -err
 66   writev           fd=1/2      iov[]       iovcnt      bytes written / -err
 93   exit             code        —           —           (no return)
 94   exit_group       code        —           —           (no return)
172   getpid           —           —           —           1
174   getuid           —           —           —           0
176   getgid           —           —           —           0
215   munmap           addr        len         —           0 (nop)
222   mmap             hint=0      len         prot        ptr / -err
278   getrandom        buf addr    len         flags       len / -err
403   clock_gettime    clockid     *timespec   —           0 / -err

1000  print_int        int         —           —           —
1001  print_str        str addr    —           —           —
1002  print_str_ln     str addr    —           —           —
1003  read_line_z      buf addr    —           —           —
1004  print_uint       u32         —           —           —
1005  print_hex        u32         —           —           —
1006  print_char       ascii code  —           —           —
1008  print_newline    —           —           —           —
1010  read_u8          dst addr    —           —           —
1011  read_u16         dst addr    —           —           —
1012  read_u32         dst addr    —           —           —
1013  read_int         dst addr    —           —           —
1014  read_float       dst addr    —           —           —
1015  print_float      (fa0=f32)   —           —           —
1030  get_instr_count  —           —           —           count (u32)
1031  get_cycle_count  —           —           —           count (u32)
1050  memset           dst addr    byte val    len         —
1051  memcpy           dst addr    src addr    len         —
1052  strlen           str addr    —           —           len
1053  strcmp           s1 addr     s2 addr     —           <0 / 0 / >0
```
