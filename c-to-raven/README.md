# From C to Raven

A bare-metal C project that compiles to a RISC-V ELF binary ready to run in the [Raven](https://github.com/Gaok1/Raven-RiscV) simulator.

No OS. No libc. `raven.h` is the only runtime you need — it gives you syscalls, I/O, strings, memory utilities, a heap allocator, and simulator control, all as `static inline` functions.

---

## Files

| File | Purpose |
|------|---------|
| `raven.h` | The entire runtime: syscalls, I/O, strings, memory, malloc, assert |
| `crt0.S` | Minimal startup: calls `main()`, forwards return value to `exit` |
| `main.c` | Example: malloc/free, string ops, sorting, raven_pause |
| `float_demo.c` | Example: RV32F hardware float — sum, dot product, basic arithmetic |
| `Makefile` | Builds both demos; auto-detects clang or riscv64-unknown-elf-gcc |

---

## Building

```sh
make              # → c-to-raven.elf   (RV32IM, integer only)
make float-demo   # → float-demo.elf   (RV32IMF, hardware float)
make clean
```

Load the ELF into Raven: Editor tab → **[BIN]** → select the file.

---

## Requirements

The Makefile auto-detects the compiler. Install one of:

### Linux (Ubuntu / Debian) — recommended: Clang + LLD

```sh
sudo apt install clang lld
```

### Linux — alternative: GCC cross-compiler

```sh
sudo apt install gcc-riscv64-unknown-elf
```

### Windows — WSL (recommended)

```sh
wsl --install   # then follow Linux instructions inside WSL
```

### Windows — MSYS2

Open the **UCRT64** terminal:

```sh
pacman -S mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-lld
```

---

## `raven.h` API reference

### Syscall wrappers — Linux ABI

Raw `ecall` wrappers matching the Linux RISC-V ABI (`a7` = syscall number, `a0`..`a5` = args, `a0` = return).

| Function | Syscall | Description |
|----------|---------|-------------|
| `sys_read(fd, buf, len)` | 63 | Read up to `len` bytes; `fd=STDIN` only |
| `sys_write(fd, buf, len)` | 64 | Write `len` bytes; `fd=STDOUT` or `STDERR` |
| `sys_exit(code)` | 93 | Terminate (no return) |
| `sys_exit_group(code)` | 94 | Alias of `sys_exit` (single-threaded) |
| `sys_getpid()` | 172 | Always returns `1` |
| `sys_getuid()` | 174 | Always returns `0` |
| `sys_getgid()` | 176 | Always returns `0` |
| `sys_brk(addr)` | 214 | Advance heap break; pass `NULL` to query |
| `sys_munmap(addr, len)` | 215 | No-op; always returns `0` |
| `sys_mmap(addr, len, prot, flags, fd, offset)` | 222 | Anonymous heap alloc (`MAP_ANONYMOUS\|MAP_PRIVATE`, `fd=-1`) |
| `sys_getrandom(buf, len, flags)` | 278 | Fill buffer with cryptographic random bytes |
| `sys_clock_gettime(clockid, tp)` | 403 | Fill `raven_timespec*` with instruction-based time |
| `sys_writev(fd, iov, iovcnt)` | 66 | Scatter-write from `raven_iovec[]` array |

**Structs:**
```c
typedef struct { void *iov_base; unsigned int iov_len; } raven_iovec;
typedef struct { unsigned int tv_sec; unsigned int tv_nsec; } raven_timespec;
```

**`mmap` flags/prot:** `PROT_NONE/READ/WRITE/EXEC`, `MAP_SHARED/PRIVATE/ANONYMOUS`.
Note: only anonymous mappings (`MAP_ANONYMOUS`, `fd=-1`) are supported. `munmap` is a no-op.

### Simulator control

| Function | Description |
|----------|-------------|
| `raven_pause()` | Emit `ebreak` — Raven pauses so you can inspect registers and memory |

### I/O helpers (C implementation via `sys_write`)

| Function | Description |
|----------|-------------|
| `print_char(c)` | Print a single character |
| `print_str(s)` | Print a null-terminated string |
| `print_ln()` | Print a newline |
| `print_int(n)` | Print a signed decimal integer |
| `print_uint(n)` | Print an unsigned decimal integer |
| `print_hex(n)` | Print unsigned int as `0x00000000` |
| `print_ptr(p)` | Print a pointer as hex address |
| `print_float(v, decimals)` | Print a float with the given number of decimal places |
| `print_bool(v)` | Print `"true"` or `"false"` |
| `print_bin(n)` | Print 32-bit value as binary, grouped by byte |
| `read_char()` | Read one character from stdin; returns `-1` on EOF |
| `read_line(buf, max)` | Read a line from stdin; null-terminates; returns byte count |
| `read_int()` | Read a signed decimal integer from stdin |
| `read_uint()` | Read an unsigned decimal integer from stdin |
| `eprint_char(c)` / `eprint_str(s)` / `eprint_ln()` | Print to stderr (shown in red in Raven) |
| `eprint_int(n)` / `eprint_uint(n)` | Print integer to stderr |

### Memory utilities

| Function | Description |
|----------|-------------|
| `memset(dst, c, n)` | Fill `n` bytes with value `c`; returns `dst` |
| `memcpy(dst, src, n)` | Copy `n` bytes from `src` to `dst`; returns `dst` |
| `memmove(dst, src, n)` | Copy `n` bytes, safe for overlapping regions; returns `dst` |
| `memcmp(a, b, n)` | Compare `n` bytes; returns 0 if equal |

### String utilities

| Function | Description |
|----------|-------------|
| `strlen(s)` | Length of string (not counting `'\0'`) |
| `strcmp(a, b)` | Lexicographic comparison; returns 0 if equal |
| `strncmp(a, b, n)` | Comparison limited to `n` characters |
| `strcpy(dst, src)` | Copy string; returns `dst` |
| `strncpy(dst, src, n)` | Copy at most `n` characters, zero-pad; returns `dst` |
| `strcat(dst, src)` | Append `src` to `dst`; returns `dst` |
| `strchr(s, c)` | First occurrence of `c` in `s`, or `NULL` |
| `strrchr(s, c)` | Last occurrence of `c` in `s`, or `NULL` |

### Random utilities

Backed by `sys_getrandom` — cryptographic quality, not a PRNG.

| Function | Description |
|----------|-------------|
| `rand_u32()` | Uniformly random 32-bit unsigned integer |
| `rand_u8()` | Uniformly random byte (0–255) |
| `rand_range(lo, hi)` | Random unsigned int in `[lo, hi)` |
| `rand_i32()` | Random signed 32-bit integer |
| `rand_bool()` | `0` or `1` with equal probability |

### Math utilities

| Function | Description |
|----------|-------------|
| `abs(n)` | Absolute value of signed int |
| `min(a, b)` / `max(a, b)` | Integer min/max |
| `umin(a, b)` / `umax(a, b)` | Unsigned int min/max |
| `ipow(base, exp)` | Integer power: `base^exp` |

### Assert / panic

| Function / Macro | Description |
|------------------|-------------|
| `raven_assert(expr)` | If `expr` is false: print message, pause, exit(1) |
| `raven_panic(msg)` | Print `msg` to stderr, pause for inspection, exit(1) |

### Heap allocator

A first-fit free-list allocator backed by a static `64 KB` heap.

| Function | Description |
|----------|-------------|
| `malloc(size)` | Allocate `size` bytes; returns `NULL` on out-of-memory |
| `calloc(nmemb, size)` | Allocate `nmemb * size` bytes, zero-initialised |
| `realloc(ptr, new_size)` | Resize allocation; copies existing data |
| `free(ptr)` | Release allocation; coalesces adjacent free blocks |
| `raven_heap_free()` | Approximate bytes remaining in the heap |
| `raven_heap_used()` | Bytes currently in use on the heap |

Change the heap size before including the header:

```c
#define RAVEN_HEAP_SIZE (128 * 1024)  // 128 KB
#include "raven.h"
```

> **Tip:** single-step with Raven's **[Dyn]** view active (`v` until `DYN` shows in the status bar). Every `sw` that writes a malloc header will flip the sidebar to show exactly what was written in RAM and where.

### Falcon teaching extensions (syscall-based shortcuts)

Single-ecall wrappers — faster than the C I/O helpers above for simple programs.
Useful when you want minimal instruction count overhead.

| Function | Syscall | Description |
|----------|---------|-------------|
| `falcon_print_int(n)` | 1000 | Print signed 32-bit integer (no newline) |
| `falcon_print_str(s)` | 1001 | Print NUL-terminated string (no newline) |
| `falcon_println_str(s)` | 1002 | Print NUL-terminated string + newline |
| `falcon_read_line(buf)` | 1003 | Read console line into `buf` (NUL-terminated, no newline) |
| `falcon_print_uint(n)` | 1004 | Print unsigned 32-bit integer (no newline) |
| `falcon_print_hex(n)` | 1005 | Print as hex, e.g. `0xDEADBEEF` (no newline) |
| `falcon_print_char(c)` | 1006 | Print a single ASCII character |
| `falcon_print_newline()` | 1008 | Print a newline |
| `falcon_read_u8(dst)` | 1010 | Read decimal/hex from stdin → store as `u8` at `*dst` |
| `falcon_read_u16(dst)` | 1011 | Read decimal/hex from stdin → store as `u16` at `*dst` |
| `falcon_read_u32(dst)` | 1012 | Read decimal/hex from stdin → store as `u32` at `*dst` |
| `falcon_read_int(dst)` | 1013 | Read signed integer (accepts `-`) → store as `int` at `*dst` |
| `falcon_read_float(dst)` | 1014 | Read float from stdin → store as `float` at `*dst` |
| `falcon_print_float(v)` | 1015 | Print `float` value (up to 6 significant digits, no newline) |
| `falcon_get_instr_count()` | 1030 | Return instructions executed since start (low 32 bits) |
| `falcon_get_cycle_count()` | 1031 | Alias of `falcon_get_instr_count` |
| `falcon_memset(dst, byte, len)` | 1050 | Fill `len` bytes at `dst` with `byte` (via simulator) |
| `falcon_memcpy(dst, src, len)` | 1051 | Copy `len` bytes from `src` to `dst` (via simulator) |
| `falcon_strlen(s)` | 1052 | Return length of NUL-terminated string (via simulator) |
| `falcon_strcmp(s1, s2)` | 1053 | Compare strings; returns `<0` / `0` / `>0` |

**Measuring algorithm cost:**
```c
unsigned int t0 = falcon_get_instr_count();
bubble_sort(arr, n);
unsigned int cost = falcon_get_instr_count() - t0;
falcon_print_uint(cost);
falcon_println_str(" instructions");
```

---

## Adding more source files

```makefile
SRCS = crt0.S main.c mylib.c another.c
```

---

## Float support

`float_demo.c` is compiled with `-march=rv32imf -mabi=ilp32f` for hardware `f`-registers. In Raven's Run tab, press `Tab` on the register sidebar to switch between integer and float register banks.

To enable float in your own program, change `make float-demo` or add the float target to the Makefile pointing at your files.

---

## How it works

`crt0.S` is the only startup code:

```asm
_start:
    call  main     # Raven initialises sp; main's return value lands in a0
    li    a7, 93   # exit(a0)
    ecall
```

Raven zeroes BSS automatically when loading the ELF, so no explicit BSS-clear loop is needed.
