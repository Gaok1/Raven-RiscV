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

### Syscall wrappers

| Function | Description |
|----------|-------------|
| `sys_write(fd, buf, len)` | Write `len` bytes to file descriptor (`STDOUT` = 1, `STDERR` = 2) |
| `sys_read(fd, buf, len)` | Read up to `len` bytes from file descriptor (`STDIN` = 0) |
| `sys_exit(code)` | Terminate (no return) |
| `sys_getrandom(buf, len, flags)` | Fill buffer with random bytes; flags = 0 |

### Simulator control

| Function | Description |
|----------|-------------|
| `raven_pause()` | Emit `ebreak` — Raven pauses so you can inspect registers and memory |

### I/O helpers

| Function | Description |
|----------|-------------|
| `print_char(c)` | Print a single character |
| `print_str(s)` | Print a null-terminated string |
| `print_int(n)` | Print a signed decimal integer |
| `print_uint(n)` | Print an unsigned decimal integer |
| `print_hex(n)` | Print unsigned int as `0x00000000` |
| `print_ptr(p)` | Print a pointer as hex address |
| `print_float(v, decimals)` | Print a float with the given number of decimal places |
| `print_bool(v)` | Print `"true"` or `"false"` |
| `print_ln()` | Print a newline |
| `read_line(buf, max)` | Read a line from stdin; null-terminates; returns byte count |
| `read_int()` | Read a decimal integer from stdin |

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

### Math utilities

| Function | Description |
|----------|-------------|
| `abs(n)` | Absolute value of signed int |
| `min(a, b)` / `max(a, b)` | Integer min/max |
| `umin(a, b)` / `umax(a, b)` | Unsigned int min/max |
| `ipow(base, exp)` | Integer power: `base^exp` |

### Assert / panic

| Function / Macro | Description |
|----------|-------------|
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
| `raven_heap_free()` | Returns approximate bytes remaining in the heap |

Change the heap size before including the header:

```c
#define RAVEN_HEAP_SIZE (128 * 1024)  // 128 KB
#include "raven.h"
```

> **Tip:** single-step with Raven's **[Dyn]** view active (`v` until `DYN` shows in the status bar). Every `sw` that writes a malloc header will flip the sidebar to show exactly what was written in RAM and where.

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
