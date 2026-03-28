# c-to-raven — API Reference

`raven.h` is a single-header bare-metal runtime for C programs running in the Raven RISC-V simulator.
No libc, no OS. Include it once and you get syscalls, I/O, strings, memory utilities, a heap allocator, random, and multi-hart support — everything implemented as `static inline` functions.

---

## Setup

```c
#include "raven.h"

void _start(void) {
    print_str("Hello, Raven!\n");
    sys_exit(0);
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
| `sys_write(fd, buf, len) -> int` | 64 | Write `len` bytes from `buf` to `fd` |
| `sys_read(fd, buf, len) -> int` | 63 | Read up to `len` bytes from `fd` into `buf` |
| `sys_writev(fd, iov, iovcnt) -> int` | 66 | Scatter write from an array of `raven_iovec` |

```c
// raven_iovec for sys_writev
typedef struct {
    void        *iov_base;
    unsigned int iov_len;
} raven_iovec;
```

### Process

| Function | Syscall | Description |
|---|---|---|
| `sys_exit(code)` | 93 | Terminate the program (noreturn) |
| `sys_exit_group(code)` | 94 | Same as `sys_exit` in Raven |
| `sys_getpid() -> int` | 172 | Always returns `1` |
| `sys_getuid() -> int` | 174 | Always returns `0` |
| `sys_getgid() -> int` | 176 | Always returns `0` |

### Memory

| Function | Syscall | Description |
|---|---|---|
| `sys_brk(addr) -> void*` | 214 | Query or advance program break; pass `NULL` to query |
| `sys_mmap(addr, len, prot, flags, fd, offset) -> void*` | 222 | Anonymous mappings only (`flags=MAP_ANONYMOUS`, `fd=-1`) |
| `sys_munmap(addr, len) -> int` | 215 | No-op; always returns `0` |

### Random

| Function | Syscall | Description |
|---|---|---|
| `sys_getrandom(buf, len, flags) -> int` | 278 | Fill buffer with random bytes |

### Time

| Function | Syscall | Description |
|---|---|---|
| `sys_clock_gettime(clockid, tp) -> int` | 403 | Write `{ tv_sec, tv_nsec }` at `tp`; based on instruction count |

```c
typedef struct {
    unsigned int tv_sec;
    unsigned int tv_nsec;
} raven_timespec;

raven_timespec ts;
sys_clock_gettime(0, &ts);
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

High-level helpers built on top of `sys_write` / `sys_read`.

### Output

| Function | Description |
|---|---|
| `print_char(c)` | Print single character to stdout |
| `print_str(s)` | Print NUL-terminated string to stdout |
| `print_ln()` | Print newline to stdout |
| `print_int(n)` | Print signed `int` (decimal, no newline) |
| `print_uint(n)` | Print unsigned `int` (decimal, no newline) |
| `print_hex(n)` | Print as `0xDEADBEEF` (8 digits, no newline) |
| `print_ptr(p)` | Print pointer address as hex |
| `print_float(v, decimals)` | Print `float` with `decimals` decimal places (0–6) |
| `print_bool(v)` | Print `"true"` or `"false"` |
| `print_bin(n)` | Print 32-bit value as binary, grouped by byte |

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

Backed by `sys_getrandom` (cryptographic quality RNG).

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

## Falcon Teaching Extensions (syscalls 1000–1053)

Raven-specific single-instruction shortcuts. Simpler than the standard wrappers above —
no strlen loop, no fd argument — useful in small programs or in `.fas` assembly.

### Output

| Function | Syscall | Description |
|---|---|---|
| `falcon_print_int(n)` | 1000 | Print `int` (no newline) |
| `falcon_print_uint(n)` | 1004 | Print `unsigned int` (no newline) |
| `falcon_print_hex(n)` | 1005 | Print as `0xDEADBEEF` (no newline) |
| `falcon_print_char(c)` | 1006 | Print single ASCII character |
| `falcon_print_newline()` | 1008 | Print newline |
| `falcon_print_str(s)` | 1001 | Print NUL-terminated string (no newline) |
| `falcon_println_str(s)` | 1002 | Print NUL-terminated string with newline |
| `falcon_print_float(v)` | 1015 | Print `float` (up to 6 significant digits, no newline) |

### Input

| Function | Syscall | Description |
|---|---|---|
| `falcon_read_line(buf)` | 1003 | Read line into NUL-terminated buffer |
| `falcon_read_u8(dst)` | 1010 | Read decimal/hex byte into `*dst` |
| `falcon_read_u16(dst)` | 1011 | Read 16-bit unsigned into `*dst` |
| `falcon_read_u32(dst)` | 1012 | Read 32-bit unsigned into `*dst` |
| `falcon_read_int(dst)` | 1013 | Read signed integer (accepts `-`) into `*dst` |
| `falcon_read_float(dst)` | 1014 | Read IEEE 754 float into `*dst` |

### Performance Counters

| Function | Syscall | Description |
|---|---|---|
| `falcon_get_instr_count() -> unsigned int` | 1030 | Instructions executed so far (low 32 bits) |
| `falcon_get_cycle_count() -> unsigned int` | 1031 | Alias of `falcon_get_instr_count` |

Useful for measuring algorithm cost inside the simulator:

```c
unsigned int before = falcon_get_instr_count();
bubble_sort(arr, 1000);
unsigned int cost = falcon_get_instr_count() - before;
falcon_print_uint(cost);
falcon_println_str(" instructions");
```

### Simulator-accelerated Memory (syscalls 1050–1053)

Execute in the simulator without running a C loop — compare with the standard C implementations
using `falcon_get_instr_count` to see the difference.

| Function | Syscall | Description |
|---|---|---|
| `falcon_memset(dst, byte, len)` | 1050 | Fill region (simulator-side) |
| `falcon_memcpy(dst, src, len)` | 1051 | Copy non-overlapping region (simulator-side) |
| `falcon_strlen(s) -> size_t` | 1052 | Length of NUL-terminated string (simulator-side) |
| `falcon_strcmp(s1, s2) -> int` | 1053 | Compare NUL-terminated strings (simulator-side) |

---

## Hart Management (syscall 1100)

Raven supports multiple hardware threads (harts) running concurrently.

### `falcon_hart_start`

```c
int falcon_hart_start(unsigned int entry_pc,
                      unsigned int stack_ptr,
                      unsigned int arg);
```

Spawns a new hart at `entry_pc`. `arg` arrives in `a0` of the new hart.
`stack_ptr` must point to the **top** (high address) of a valid, aligned stack region.
Returns `0` on success.

The new hart runs from the next simulation cycle alongside the caller.
Use `sys_exit(0)` inside the worker to terminate only that hart without stopping others.

### `RAVEN_SPAWN_HART` macro

Convenience wrapper that computes the stack-top automatically from a stack array:

```c
#define RAVEN_SPAWN_HART(fn_ptr, stack_arr, arg)
```

**Example**

```c
static char worker_stack[4096];

void worker(unsigned int id) {
    falcon_print_str("hart ");
    falcon_print_uint(id);
    falcon_print_newline();
    sys_exit(0);
}

int main(void) {
    RAVEN_SPAWN_HART(worker, worker_stack, /*arg=*/1);
    // main hart continues here
    print_str("main hart running\n");
    sys_exit(0);
}
```

---

## Complete Example

```c
#include "raven.h"

static char input_buf[64];
static char hart_stack[4096];

void counter_hart(unsigned int start) {
    for (unsigned int i = start; i < start + 5; i++) {
        falcon_print_uint(i);
        falcon_print_newline();
    }
    sys_exit(0);
}

void _start(void) {
    print_str("Enter start value: ");
    unsigned int n = read_uint();

    RAVEN_SPAWN_HART(counter_hart, hart_stack, n);

    // While the hart counts, do something in main
    unsigned int before = falcon_get_instr_count();
    int result = ipow(2, 10);
    unsigned int cost = falcon_get_instr_count() - before;

    print_str("2^10 = ");
    print_int(result);
    print_str(" (");
    print_uint(cost);
    print_str(" instructions)\n");

    raven_pause();   // inspect both harts here
    sys_exit(0);
}
```
