# c-to-raven

Bare-metal C SDK for programs that run on the [Raven](https://github.com/Gaok1/Raven-RiscV) RISC-V 32 simulator. No OS. No libc. One umbrella header, one static library, one `crt0.S`.

```c
#include <raven/raven.h>

int main(void) {
    raven_println_str("Hello, Raven!");
    return 0;
}
```

```sh
make
./target/release/raven run c-to-raven/target/main.elf --nout
# → Hello, Raven!
```

---

## Quickstart

```sh
make            # builds libraven.a, target/main.elf, and the example ELFs
make clean
```

Outputs in `c-to-raven/target/`:

- `main.elf`, `array_bench.elf`, `jit_demo.elf`, `coro_demo.elf`, `coro_hart_demo.elf` — RV32IM, soft-float ABI
- `float-demo.elf` — RV32IMF, hardware-float ABI

Load any of them in the Raven TUI (**Editor tab → [BIN]**) or run headless: `raven run <file>.elf --nout`.

---

## Layout

```
c-to-raven/
├── include/raven/        ← the only directory on the compiler's -I path
│   ├── raven.h           ← umbrella; everything you usually want
│   ├── version.h         RAVEN_API_VERSION
│   ├── types.h           raven_u8/16/32/64, raven_size_t, size_t, NULL
│   ├── io.h              raven_print_*, raven_read_*, raven_println, raven_eprint_*
│   ├── mem.h             raven_malloc/free/calloc/realloc, memset/memcpy/...
│   ├── str.h             raven_strlen/strcmp/strcpy/...
│   ├── math.h            raven_abs/min/max/clamp/ipow
│   ├── rand.h            raven_rand_u32/u8/range/bool/i32
│   ├── hart.h            multi-hart spawn / join / detach
│   ├── coro.h            cooperative coroutines: resume / yield
│   ├── perf.h            raven_instr_count, raven_cycle_count, RAVEN_MEASURE
│   ├── debug.h           raven_assert, raven_panic, raven_exit
│   └── advanced.h        ⚠ opt-in: raw ecalls, ebreak, JIT exec-mapping
│
├── src/
│   └── main.c            ← required user entry point
│
├── lib/                  ← built into libraven.a / libraven_f.a
│   ├── crt0.S            minimal startup (calls main, exits)
│   ├── *.c               formatting, allocator, hart trampoline, panic, ...
│   └── internal/         private headers (unreachable from user code)
│
└── examples/
    ├── array_bench.c     uses RAVEN_MEASURE
    ├── float_demo.c      RV32F hardware floats
    ├── jit_demo.c        uses <raven/advanced.h> for runtime code generation
    ├── coro_demo.c       cooperative coroutine generator (resume / yield)
    └── coro_hart_demo.c  coroutines running on multiple harts
```

Your program must live in `src/main.c`. The compiler still uses `-Iinclude`, so `src/main.c` can include `<raven/raven.h>` directly. `lib/internal/` is not reachable — try `#include "lib/internal/heap_block.h"` and you'll get "file not found". That's by design.

---

## Naming and visibility

Raven reserves these prefixes — don't define your own identifiers in them:

| Prefix | Meaning |
|--------|---------|
| `raven_*` | Public ergonomic API. Stable. |
| `Raven*` | Public struct types (PascalCase). Stable. |
| `RAVEN_*` | Public macros and constants. Stable. |
| `raven_sys_*` | Direct Linux syscall wrappers (in `<raven/advanced.h>`). |
| `raven_unsafe_*` | Footgun ops — read the comment in `advanced.h` (in `<raven/advanced.h>`). |
| `_raven_*` | Private to `libraven.a`. Never call from user code. |

What you get from `<raven/raven.h>` is the recommended surface. If you need raw `ecall`, `ebreak`, or JIT exec-marking, the door is one extra include:

```c
#include <raven/raven.h>
#include <raven/advanced.h>      // <- you're acknowledging the risk

int (*sum)(int,int) = ...;
raven_unsafe_map_exec(sum, 8);   // mark memory executable for JIT
```

---

## Building your own programs

Three things to point your compiler at:

1. The include path: `-Ic-to-raven/include`
2. The startup object: `c-to-raven/lib/crt0.im.o` (or `crt0.imf.o` for hardware float)
3. The library: `-Lc-to-raven/lib -lraven` (or `-lraven_f`)

For RV32IM (the common case):

```sh
clang --target=riscv32-unknown-elf -march=rv32im -mabi=ilp32 \
      -nostdlib -O2 -ffreestanding -fuse-ld=lld \
      -e _start -Wl,--gc-sections \
      -Ic-to-raven/include \
      my_program.c c-to-raven/lib/crt0.im.o \
      -Lc-to-raven/lib -lraven \
      -o my_program.elf
```

For RV32IMF (hardware float): swap `rv32im` → `rv32imf`, `ilp32` → `ilp32f`, `crt0.im.o` → `crt0.imf.o`, `-lraven` → `-lraven_f`.

---

## Toolchain

The `Makefile` auto-detects, in order: `clang`, `clang-18`, `riscv64-unknown-elf-gcc`, `riscv64-none-elf-gcc`.

| Platform | Recommended install |
|----------|---------------------|
| Linux (Debian/Ubuntu) | `sudo apt install clang lld llvm` |
| Linux — GCC route | `sudo apt install gcc-riscv64-unknown-elf binutils-riscv64-unknown-elf` |
| Windows | WSL + the Debian instructions above |
| Windows native | MSYS2 UCRT64: `pacman -S mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-lld` |

Clang is preferred because it ships with LLD and `llvm-ar`, both of which handle RV32 ELF without extra packages.

---

## Heap size

The allocator is a static buffer of 64 KB by default. To change it, rebuild the library with the new size:

```sh
make clean
make CFLAGS_IM='--target=riscv32-unknown-elf -march=rv32im -mabi=ilp32 -nostdlib -O2 -Wall -Wextra -fno-builtin -ffreestanding -Iinclude -DRAVEN_HEAP_SIZE=131072'
```

(You only need to override the variant you'll actually link against.) The constant is read by `lib/mem.c`; user `.c` files never see it.

---

## API reference

For the full per-module reference, see [`API.md`](API.md).
