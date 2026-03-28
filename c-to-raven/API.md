# c-to-raven — API Reference

`raven.h` is the public bare-metal runtime surface for C programs running in the Raven RISC-V simulator.
No libc, no OS. Include it and you get the intended syscalls, I/O, strings, memory utilities, allocator, random, and multi-hart helpers — everything implemented as `static inline` functions.

Implementation details live under `internal/` in headers such as `internal/raven_internal.h`, `internal/raven_syscall.h`, `internal/raven_libc.h`, `internal/raven_heap.h`, and `internal/raven_teaching.h`. They are included by `raven.h` and are not meant for direct user code.

---

## Setup

```c
#include "raven.h"

void _start(void) {
    print_str("Hello, Raven!\n");
    __sys_exit(0);
}
```

Compile with your RISC-V cross-toolchain:

```bash
riscv32-unknown-elf-gcc -march=rv32im -mabi=ilp32 -nostdlib -O2 \
    -o program.elf crt0.S main.c
```

The repo ships a `Makefile` and `crt0.S` that handle the entry point and stack setup.

---

## Syscall Wrappers

Direct `ecall` wrappers that follow the Linux RISC-V ABI (syscall number in `a7`, arguments in `a0`–`a5`).

### Core I/O

| Function | Syscall | Description |
|---|---|---|
| `__sys_write(fd, buf, len) -> int` | 64 | Low-level write wrapper |
| `__sys_read(fd, buf, len) -> int` | 63 | Low-level read wrapper |
| `__sys_writev(fd, iov, iovcnt) -> int` | 66 | Low-level scatter write wrapper |

```c
// raven_iovec for __sys_writev
typedef struct {
    void        *iov_base;
    unsigned int iov_len;
} raven_iovec;
```

### Process

| Function | Syscall | Description |
|---|---|---|
| `__sys_exit(code)` | 93 | Low-level terminate wrapper (noreturn) |
| `__sys_exit_group(code)` | 94 | Same as `__sys_exit` in Raven |
| `__sys_getpid() -> int` | 172 | Always returns `1` |
| `__sys_getuid() -> int` | 174 | Always returns `0` |
| `__sys_getgid() -> int` | 176 | Always returns `0` |

### Memory

| Function | Syscall | Description |
|---|---|---|
| `__sys_brk(addr) -> void*` | 214 | Query or advance program break; pass `NULL` to query |
| `__sys_mmap(addr, len, prot, flags, fd, offset) -> void*` | 222 | Anonymous mappings only (`flags=MAP_ANONYMOUS`, `fd=-1`) |
| `__sys_munmap(addr, len) -> int` | 215 | No-op; always returns `0` |

### Random

| Function | Syscall | Description |
|---|---|---|
| `__sys_getrandom(buf, len, flags) -> int` | 278 | Fill buffer with random bytes |

### Time

| Function | Syscall | Description |
|---|---|---|
| `__sys_clock_gettime(clockid, tp) -> int` | 403 | Write `{ tv_sec, tv_nsec }` at `tp`; based on instruction count |

```c
typedef struct {
    unsigned int tv_sec;
    unsigned int tv_nsec;
} raven_timespec;

raven_timespec ts;
__sys_clock_gettime(0, &ts);
```

### Constants

```c
// File descriptors
#define STDIN   0
#define STDOUT  1
#define STDERR  2

// mmap prot
#define PROT_NONE   0x00
#define PROT_READ   0x01
#define PROT_WRITE  0x02
#define PROT_EXEC   0x04

// mmap flags
#define MAP_SHARED    0x01
#define MAP_PRIVATE   0x02
#define MAP_ANONYMOUS 0x20
```

---

## I/O Helpers

High-level helpers built on top of `__sys_write` / `__sys_read`.

### Output

| Function | Description |
|---|---|
| `print_char(c)` | Print single character to stdout |
| `print_str(s)` | Print NUL-terminated string to stdout |
| `print_newline()` | Print newline to stdout |
| `print_int(n)` | Print signed `int` (decimal, no newline) |
| `print_uint(n)` | Print unsigned `int` (decimal, no newline) |
| `print_hex(n)` | Print as `0xDEADBEEF` (8 digits, no newline) |
| `print_ptr(p)` | Print pointer address as hex |
| `print_float(v, decimals)` | Print `float` with `decimals` decimal places (0–6) |
| `print_bool(v)` | Print `"true"` or `"false"` |
| `print_bin(n)` | Print 32-bit value as binary, grouped by byte |

> `print_ln()` is a legacy alias for `print_newline()`.

**Stderr variants** (same signatures, write to stderr):

| Function |
|---|
| `eprint_char(c)` |
| `eprint_str(s)` |
| `eprint_ln()` |
| `eprint_int(n)` |
| `eprint_uint(n)` |

### Input

| Function | Description |
|---|---|
| `read_char() -> int` | Read one byte from stdin; returns `-1` on EOF |
| `read_line(buf, max) -> int` | Read a line (stops on `\n`/EOF); always NUL-terminates; returns bytes read |
| `read_int() -> int` | Parse signed decimal from one stdin line |
| `read_uint() -> unsigned int` | Parse unsigned decimal from one stdin line |

---

## String Utilities

Standard C string functions provided without libc:

| Function | Description |
|---|---|
| `strlen(s)` | Length of NUL-terminated string |
| `strcmp(a, b)` | Compare strings; returns negative/0/positive |
| `strncmp(a, b, n)` | Compare up to `n` characters |
| `strcpy(dst, src)` | Copy string |
| `strncpy(dst, src, n)` | Copy up to `n` characters, zero-pads |
| `strcat(dst, src)` | Append string |
| `strchr(s, c)` | First occurrence of `c`; returns `NULL` if not found |
| `strrchr(s, c)` | Last occurrence of `c`; returns `NULL` if not found |

---

## Memory Utilities

| Function | Description |
|---|---|
| `memset(dst, c, n)` | Fill `n` bytes with value `c` |
| `memcpy(dst, src, n)` | Copy `n` bytes (non-overlapping) |
| `memmove(dst, src, n)` | Copy `n` bytes (handles overlap) |
| `memcmp(a, b, n)` | Compare `n` bytes; returns negative/0/positive |

---

## Math Utilities

| Function | Description |
|---|---|
| `abs(n)` | Absolute value of `int` |
| `min(a, b)` | Minimum of two `int` values |
| `max(a, b)` | Maximum of two `int` values |
| `umin(a, b)` | Minimum of two `unsigned int` values |
| `umax(a, b)` | Maximum of two `unsigned int` values |
| `ipow(base, exp)` | Integer power `base^exp` (no overflow check) |

---

## Random Utilities

Backed by `__sys_getrandom` (cryptographic quality RNG).

| Function | Description |
|---|---|
| `rand_u32() -> unsigned int` | Uniformly random 32-bit unsigned integer |
| `rand_u8() -> unsigned char` | Uniformly random byte (0–255) |
| `rand_i32() -> int` | Random signed 32-bit integer |
| `rand_range(lo, hi) -> unsigned int` | Random value in `[lo, hi)` |
| `rand_bool() -> int` | `0` or `1` with equal probability |

> `rand_range` uses modulo reduction — suitable for teaching, not for cryptographic use.

```c
unsigned int die = rand_range(1, 7);  // d6
int flip = rand_bool();
```

---

## Heap Allocator

A first-fit free-list allocator backed by a static `RAVEN_HEAP_SIZE`-byte buffer (default 64 KB).
Every allocation is visible in Raven's **Dyn view** as a `sw` writing the block header.

| Function | Description |
|---|---|
| `malloc(size) -> void*` | Allocate `size` bytes; returns `NULL` on OOM |
| `calloc(nmemb, size) -> void*` | Allocate zeroed `nmemb * size` bytes |
| `realloc(ptr, new_size) -> void*` | Resize a previous allocation |
| `free(ptr)` | Free a previous allocation; coalesces adjacent free blocks |
| `raven_heap_free() -> size_t` | Approximate bytes still available |
| `raven_heap_used() -> size_t` | Bytes currently in use |

**Resize the heap** by defining the macro before including the header:

```c
#define RAVEN_HEAP_SIZE (256 * 1024)  // 256 KB
#include "raven.h"
```

---

## Assert and Panic

```c
// Terminate with "PANIC: <msg>" on stderr and exit(1).
// Also hits ebreak first so you can inspect state in Raven.
raven_panic("something went wrong");

// Assert: if expr is false, calls raven_panic with the expression text.
raven_assert(ptr != NULL);
raven_assert(index < array_size);
```

---

## Simulator Control

```c
raven_pause();  // emits ebreak — freezes execution in Raven for inspection
```

Use `raven_pause()` as a software breakpoint to inspect registers, memory, and the pipeline view
before resuming.

---

## Raven Teaching Extensions (syscalls 1000–1053)

Raven-specific single-instruction shortcuts. Simpler than the standard wrappers above —
no strlen loop, no fd argument — useful in small programs or in `.fas` assembly.

The API uses the `raven_*` prefix throughout. The old `falcon_*` names were removed.

### Output

| Function | Syscall | Description |
|---|---|---|
| `raven_print_int(n)` | 1000 | Print `int` (no newline) |
| `raven_print_uint(n)` | 1004 | Print `unsigned int` (no newline) |
| `raven_print_hex(n)` | 1005 | Print as `0xDEADBEEF` (no newline) |
| `raven_print_char(c)` | 1006 | Print single ASCII character |
| `raven_print_newline()` | 1008 | Print newline |
| `raven_print_str(s)` | 1001 | Print NUL-terminated string (no newline) |
| `raven_println_str(s)` | 1002 | Print NUL-terminated string with newline |
| `raven_print_float(v)` | 1015 | Print `float` (up to 6 significant digits, no newline) |
| `raven_print_bool(v)` | — | Print `"true"` or `"false"` |
| `raven_print_ptr(p)` | — | Print pointer address as hex |
| `raven_print_bin(n)` | — | Print 32-bit value as binary, grouped by byte |

### Input

All read functions **return** their value — no pointer argument needed.

| Function | Syscall | Description |
|---|---|---|
| `raven_read_line(buf)` | 1003 | Read line into NUL-terminated buffer |
| `raven_read_int() -> int` | 1013 | Read signed integer (accepts `-`) |
| `raven_read_uint() -> unsigned int` | 1012 | Read unsigned decimal integer |
| `raven_read_float() -> float` | 1014 | Read IEEE 754 float |
| `raven_read_u8() -> unsigned char` | 1010 | Read decimal/hex byte |
| `raven_read_u16() -> unsigned short` | 1011 | Read 16-bit unsigned |
| `raven_read_u32() -> unsigned int` | 1012 | Read 32-bit unsigned |

### Performance Counters

| Function | Syscall | Description |
|---|---|---|
| `raven_get_instr_count() -> raven_u64` | 1030 | Instructions executed so far (64-bit) |
| `raven_get_cycle_count() -> raven_u64` | 1031 | Simulated cycle count (64-bit) |
| `raven_get_instr_count32() -> unsigned int` | helper | Low 32-bit compatibility wrapper |
| `raven_get_cycle_count32() -> unsigned int` | helper | Low 32-bit compatibility wrapper |

Useful for measuring algorithm cost inside the simulator:

```c
unsigned int before = raven_get_instr_count32();
bubble_sort(arr, 1000);
unsigned int cost = raven_get_instr_count32() - before;
raven_print_uint(cost);
raven_println_str(" instructions");
```

### Simulator-accelerated Memory (syscalls 1050–1053)

Execute in the simulator without running a C loop — compare with the standard C implementations
using `raven_get_instr_count32` to see the difference.

| Function | Syscall | Description |
|---|---|---|
| `raven_memset(dst, byte, len)` | 1050 | Fill region (simulator-side) |
| `raven_memcpy(dst, src, len)` | 1051 | Copy non-overlapping region (simulator-side) |
| `raven_strlen(s) -> size_t` | 1052 | Length of NUL-terminated string (simulator-side) |
| `raven_strcmp(s1, s2) -> int` | 1053 | Compare NUL-terminated strings (simulator-side) |

---

## Hart Management (syscalls 1100–1101)

Raven supports multiple hardware threads (harts) running concurrently.
The user-level API has two layers: `RavenHartTask` for building a task before launching it,
and `RavenHartHandle` for joining or polling after launch.
The raw `__sys_hart_start` / `__sys_hart_exit` syscalls are internal — prefer the helpers below.

### `RavenHartHandle`

Returned by every start/spawn function.  Carries three embedded method pointers
so you can call them directly on the struct, or use the free-function aliases.

```c
struct RavenHartHandle {
    __RavenHartPayload *__p;           // internal — do not touch
    int  (*is_finished)(const RavenHartHandle *h);
    void (*join)(RavenHartHandle *h);
    void (*detach)(RavenHartHandle *h);
};
```

| Method / alias | Description |
|---|---|
| `h.is_finished(&h)` / `raven_hart_handle_is_finished(&h)` | Returns 1 when the hart has exited, 0 while running |
| `h.join(&h)` / `raven_hart_handle_join(&h)` | Spin-wait until done, then free internal resources. Do not use handle after this. |
| `h.detach(&h)` / `raven_hart_handle_detach(&h)` | Abandon: the hart frees itself on exit. Use when you don't need to wait. |

### `RavenHartTask`

Describes a hart before launching it.  A plain value — no hidden function pointers.

```c
typedef struct {
    raven_hart_fn entry;
    void         *stack_base;
    size_t        stack_size;
    unsigned int  arg;
} RavenHartTask;

// Constructor:
RavenHartTask raven_hart_task(raven_hart_fn entry,
                              void *stack_base, size_t stack_size,
                              unsigned int arg);

// Macro that fills stack_size from sizeof(stack_arr) automatically:
#define raven_hart_task_array(fn_ptr, stack_arr, arg_value)

// Launch the task.  Returns a handle for join / poll.
RavenHartHandle raven_hart_task_start(const RavenHartTask *task);
```

### `raven_spawn_hart` — quick one-liner

Skip the task descriptor when you only need a handle back.

```c
RavenHartHandle raven_spawn_hart(raven_hart_fn entry,
                                 void *stack_base, size_t stack_size,
                                 unsigned int arg);

// Macro variant — computes stack_size automatically:
#define raven_spawn_hart_array(fn_ptr, stack_arr, arg)
```

### Examples

**Task + join** — build first, launch later, wait:

```c
static char worker_stack[4096];

void worker(unsigned int n) {
    raven_print_str("sum = ");
    unsigned int s = 0;
    for (unsigned int i = 1; i <= n; i++) s += i;
    raven_print_uint(s);
    raven_print_newline();
}

int main(void) {
    RavenHartTask task = raven_hart_task_array(worker, worker_stack, /*arg=*/100);
    RavenHartHandle h = raven_hart_task_start(&task);

    print_str("main running while worker computes...\n");
    h.join(&h);   // or: raven_hart_handle_join(&h)
    print_str("worker finished.\n");
    return 0;
}
```

**Quick spawn + poll** — one-liner, non-blocking check:

```c
static char stack[4096];

RavenHartHandle h = raven_spawn_hart_array(worker, stack, /*arg=*/50);
while (!h.is_finished(&h)) { /* do other work */ }
```

**Fire and forget (detach)** — hart cleans itself up, no need to join:

```c
RavenHartHandle h = raven_spawn_hart_array(worker, stack, /*arg=*/0);
h.detach(&h);   // main continues; hart frees its own resources on exit
```

---

## Complete Example

```c
#include "raven.h"

static char input_buf[64];
static char hart_stack[4096];

void counter_hart(unsigned int start) {
    for (unsigned int i = start; i < start + 5; i++) {
        raven_print_uint(i);
        raven_print_newline();
    }
}

void _start(void) {
    print_str("Enter start value: ");
    unsigned int n = read_uint();

    raven_spawn_hart_array(counter_hart, hart_stack, n);

    // While the hart counts, do something in main
    unsigned int before = raven_get_instr_count32();
    int result = ipow(2, 10);
    unsigned int cost = raven_get_instr_count32() - before;

    print_str("2^10 = ");
    print_int(result);
    print_str(" (");
    print_uint(cost);
    print_str(" instructions)\n");

    raven_pause();   // inspect both harts here
    __sys_exit(0);
}
```
