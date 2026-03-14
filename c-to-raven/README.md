# From C to Raven

A bare-metal C project that compiles to a RISC-V ELF binary ready to run in the [Raven](https://github.com/Gaok1/Raven) simulator.

No OS, no libc. `raven.h` is the only runtime you need.

---

## Files

| File | Purpose |
|------|---------|
| `raven.h` | Syscall wrappers, I/O helpers, `raven_pause()` |
| `crt0.S` | Minimal startup: calls `main()`, then `exit(return_value)` |
| `main.c` | Example: fill array with random numbers, bubble-sort, print result |
| `float_demo.c` | Example: sum, dot product and basic arithmetic using RV32F hardware floats |
| `Makefile` | Builds both demos with Clang/LLD targeting `rv32im` and `rv32imf` |

---

## Building

```sh
make              # → c-to-raven.elf   (RV32IM, integer only)
make float-demo   # → float-demo.elf   (RV32IMF, hardware float)
make clean        # remove .elf files
```

Load the resulting ELF into Raven: Editor tab → **[BIN]** button → select the file.

---

## Requirements

### Linux (Ubuntu / Debian)

The Makefile uses **Clang 18** + **LLD**. Both are available from the standard LLVM packages:

```sh
sudo apt install clang-18 lld
```

If you prefer GCC instead, change the first two lines of the Makefile:

```makefile
CC     = riscv64-unknown-elf-gcc
CFLAGS = -march=rv32im -mabi=ilp32 -nostdlib -O2 -Wall -Wextra
```

Install GCC with:

```sh
sudo apt install gcc-riscv64-unknown-elf
```

### Windows — Option 1: WSL (recommended)

1. Install WSL with Ubuntu:
   ```sh
   wsl --install
   ```
2. Inside WSL, follow the Linux instructions above.

### Windows — Option 2: MSYS2

1. Install [MSYS2](https://www.msys2.org/).
2. Open the **UCRT64** terminal and run:
   ```sh
   pacman -S mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-lld
   ```
3. Build from the UCRT64 terminal:
   ```sh
   make
   ```

---

## `raven.h` API reference

### Syscall wrappers

| Function | Signature | Description |
|----------|-----------|-------------|
| `sys_write` | `int sys_write(int fd, const void *buf, int len)` | Write `len` bytes to file descriptor (use `STDOUT` = 1) |
| `sys_read` | `int sys_read(int fd, void *buf, int len)` | Read up to `len` bytes from file descriptor (use `STDIN` = 0) |
| `sys_exit` | `void sys_exit(int code)` | Terminate with exit code (no return) |
| `sys_getrandom` | `int sys_getrandom(void *buf, int len, unsigned flags)` | Fill buffer with random bytes (flags = 0) |

All wrappers use the Linux RISC-V ABI (`a7` = syscall number, `a0`–`a2` = args, `a0` = return value).

### I/O helpers

| Function | Description |
|----------|-------------|
| `print_char(char c)` | Print a single character to stdout |
| `print_str(const char *s)` | Print a null-terminated string to stdout |
| `print_int(int n)` | Print a signed integer (decimal) to stdout |
| `print_uint(unsigned int n)` | Print an unsigned integer (decimal) to stdout |
| `print_ln()` | Print a newline |
| `read_line(char *buf, int max)` | Read a line from stdin; null-terminates; returns bytes read |

### Simulator control

| Function | Description |
|----------|-------------|
| `raven_pause()` | Emit `ebreak` — Raven pauses execution so you can inspect registers and memory |

---

## Adding more source files

Edit the `SRCS` variable in the Makefile:

```makefile
SRCS = crt0.S main.c mylib.c another.c
```

---

## Float support

`float_demo.c` is compiled with `-march=rv32imf -mabi=ilp32f`, enabling hardware `f`-registers.
Open the Run tab in Raven and press `Tab` on the register sidebar to switch from integer to float registers and watch `f0`–`f31` update in real time as the program runs.

To add float support to your own program, change the Makefile target:

```makefile
CC     = clang-18
CFLAGS = --target=riscv32-unknown-elf -march=rv32imf -mabi=ilp32f \
         -nostdlib -fuse-ld=lld -O2
```

---

## How it works

`crt0.S` sets up the bare minimum before `main()`:

```asm
_start:
    call  main     # sp is set by the ELF loader (Raven initialises it)
    li    a7, 93   # exit(return value of main)
    ecall
```

Raven zeroes BSS automatically when loading the ELF, so no explicit BSS-clear loop is needed in the startup code.
