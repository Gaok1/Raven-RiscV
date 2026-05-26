# c-to-raven — API Reference

Everything you can call from a Raven C program, grouped by module. The umbrella `<raven/raven.h>` pulls in every module below except `<raven/advanced.h>`, which is a separate, opt-in include for low-level escape hatches.

---

## Module index

| Module | Purpose |
|--------|---------|
| [`<raven/types.h>`](#types--rangetypesh) | integer typedefs, `NULL`, `size_t` |
| [`<raven/version.h>`](#version--ravenversionh) | API version macros |
| [`<raven/io.h>`](#io--raveniooh) | print / read / println / eprint |
| [`<raven/fmt.h>`](#fmt--ravenfmth) | printf / scanf / snprintf / sscanf |
| [`<raven/mem.h>`](#mem--ravenmemh) | malloc family, memset / memcpy / memmove / memcmp |
| [`<raven/str.h>`](#strings--ravenstrh) | strlen / strcmp / strcpy / ... |
| [`<raven/math.h>`](#math--ravenmathh) | abs / min / max / clamp / ipow |
| [`<raven/rand.h>`](#random--ravenrandh) | rand_u32 / rand_range / rand_bool |
| [`<raven/hart.h>`](#harts--ravenharth) | multi-hart spawn / join / detach |
| [`<raven/coro.h>`](#coroutines--ravencoroh) | cooperative coroutines: resume / yield |
| [`<raven/perf.h>`](#perf--ravenperfh) | instr_count / cycle_count / RAVEN_MEASURE |
| [`<raven/debug.h>`](#debug--ravendebugh) | assert / panic / exit |
| [`<raven/advanced.h>`](#advanced---ravenadvancedh-opt-in) | ⚠ raw ecalls, ebreak, JIT mark-exec |

---

## types — `<raven/types.h>`

Canonical Raven types:

```c
raven_u8,  raven_u16,  raven_u32,  raven_u64
raven_i8,  raven_i16,  raven_i32,  raven_i64
raven_size_t, raven_ssize_t, raven_uintptr_t, raven_ptrdiff_t
```

Libc-standard aliases also provided for ergonomics: `size_t`, `ptrdiff_t`, `NULL`. The raven-prefixed names are canonical; the aliases are there so generic C idioms compile under `-nostdlib`.

---

## version — `<raven/version.h>`

```c
#define RAVEN_API_VERSION_MAJOR 1
#define RAVEN_API_VERSION_MINOR 0
#define RAVEN_API_VERSION       100      // major*100 + minor
#define RAVEN_API_AT_LEAST(maj, min)     // true if current API ≥ maj.min
```

Use to gate code on the SDK version: `#if RAVEN_API_AT_LEAST(1, 1) ... #endif`.

---

## io — `<raven/io.h>`

Each print/read function is a **single ecall** — the formatting work happens inside the simulator (in Rust) and therefore does **not** count toward `raven_instr_count`. This keeps benchmarks honest: only the *interesting* code shows up in the instruction count.

### Output

| Function | Notes |
|----------|-------|
| `raven_print_int(int n)` | signed decimal |
| `raven_print_uint(raven_u32 n)` | unsigned decimal |
| `raven_print_hex(raven_u32 n)` | `0xDEADBEEF` form |
| `raven_print_str(const char *s)` | NUL-terminated string, no newline |
| `raven_println_str(const char *s)` | + newline |
| `raven_print_char(char c)` | one byte |
| `raven_println(void)` | just a newline |
| `raven_print_float(float v)` | default 6 sig digits, trailing zeros stripped |
| `raven_print_float_n(float v, int decimals)` | custom precision (formats in C) |
| `raven_print_ptr(const void *p)` | hex address |
| `raven_print_bool(int v)` | `"true"` or `"false"` |
| `raven_print_bin(raven_u32 n)` | 32 bits with byte grouping |

### Stderr

`raven_eprint_str / _int / _uint / _char / raven_eprintln` — same as above but to fd=2 (rendered in red by the Raven TUI console).

### Input

| Function | Notes |
|----------|-------|
| `int raven_read_int(void)` | parses decimal/hex (single ecall) |
| `raven_u32 raven_read_uint(void)` | alias `raven_read_u32` |
| `raven_u8 raven_read_u8(void)` | |
| `raven_u16 raven_read_u16(void)` | |
| `float raven_read_float(void)` | |
| `int raven_read_line(char *buf, int max)` | **bounded**: ≤ max-1 bytes, NUL-terminates, returns byte count |
| `int raven_read_char(void)` | -1 on EOF |

---

## fmt — `<raven/fmt.h>`

Formatted I/O. Unlike `<raven/io.h>`, **formatting happens in C**, so the work
counts toward `raven_instr_count`. Reach here when you want a single call to
assemble a mixed-format line; reach for `<raven/io.h>` when you want each
typed value to cost one ecall and stay invisible to benchmarks.

```c
int raven_printf  (const char *fmt, ...);
int raven_snprintf(char *buf, raven_size_t size, const char *fmt, ...);
int raven_scanf   (const char *fmt, ...);
int raven_sscanf  (const char *str, const char *fmt, ...);

/* va_list variants */
int raven_vprintf  (const char *fmt, raven_va_list ap);
int raven_vsnprintf(char *buf, raven_size_t size, const char *fmt, raven_va_list ap);
int raven_vscanf   (const char *fmt, raven_va_list ap);
int raven_vsscanf  (const char *str, const char *fmt, raven_va_list ap);
```

`raven_snprintf` follows C99 semantics: it always NUL-terminates when `size > 0`,
writes at most `size - 1` bytes of content, and returns the number of bytes
that *would* have been written if the buffer was large enough — so truncation
is detectable via `(return_value >= size)`.

`raven_printf` batches output into a 128-byte buffer flushed via one write
ecall per fill — cheaper than calling `raven_print_char` per byte.

### Supported conversions

| Specifier | Meaning |
|-----------|---------|
| `%d` `%i` | signed decimal (`int`) |
| `%u`      | unsigned decimal (`raven_u32`) |
| `%x` `%X` | unsigned hex (lower / upper) |
| `%o`      | unsigned octal |
| `%b`      | unsigned binary (Raven extension) |
| `%c`      | char |
| `%s`      | NUL-terminated string |
| `%p`      | pointer, printed as `0xHHHHHHHH` |
| `%%`      | literal `%` |

Modifiers: flags `-`, `+`, ` `, `0`, `#`; decimal width; `.precision`;
length modifiers `h`, `hh`, `l`, `ll`, `z`, `j`, `t` are accepted and
ignored (`int` and `long` are both 32-bit on rv32 — pass a `raven_u64` as
two `raven_u32` args if you need 64-bit). `scanf` additionally honors `*`
to suppress an assignment.

### Floats

`%f` is **intentionally not supported**. Variadic float promotion to
`double` would pull in soft-float helpers from `compiler-rt` that
`-nostdlib` strips, so the project keeps float printing on the dedicated
path: format integer/string parts with `raven_snprintf`, then call
`raven_print_float_n` from `<raven/io.h>` for each float.

---

## mem — `<raven/mem.h>`

### Heap

| Function | Notes |
|----------|-------|
| `void *raven_malloc(raven_size_t)` | first-fit; NULL on OOM |
| `void *raven_calloc(raven_size_t n, raven_size_t sz)` | zero-initialised |
| `void *raven_realloc(void *, raven_size_t)` | in-place when possible |
| `void  raven_free(void *)` | coalesces free neighbours |
| `raven_size_t raven_heap_used(void)` | bytes in use (including block headers) |
| `raven_size_t raven_heap_free(void)` | bytes currently free |

Default heap size is 64 KB; rebuild `libraven.a` with `-DRAVEN_HEAP_SIZE=<bytes>` to change it.

### memcpy / memset family — two flavours

```c
raven_memset (dst, byte, len);    // simulator-accelerated (1 ecall)
raven_memset_c(dst, byte, len);   // pure C loop (counts toward instr_count)

raven_memcpy (dst, src, len);     // simulator-accelerated
raven_memcpy_c(dst, src, len);    // pure C loop

raven_memmove(dst, src, len);     // overlap-safe; C only
raven_memcmp (a, b, len);
```

The plain `raven_X` form is the default; reach for `raven_X_c` when you specifically want the cost to show up in `raven_instr_count`.

---

## strings — `<raven/str.h>`

```c
raven_size_t raven_strlen (const char *);     // 1 ecall
int          raven_strcmp (const char *, const char *);    // 1 ecall

raven_size_t raven_strlen_c (const char *);   // C loop
int          raven_strcmp_c (const char *, const char *);  // C loop

int   raven_strncmp(const char *, const char *, raven_size_t);
char *raven_strcpy (char *, const char *);
char *raven_strncpy(char *, const char *, raven_size_t);
char *raven_strcat (char *, const char *);
char *raven_strchr (const char *, int);
char *raven_strrchr(const char *, int);
```

---

## math — `<raven/math.h>`

Header-only, all `static inline`:

```c
int          raven_abs  (int);
int          raven_min  (int, int);
int          raven_max  (int, int);
unsigned int raven_umin (unsigned, unsigned);
unsigned int raven_umax (unsigned, unsigned);
int          raven_clamp(int v, int lo, int hi);
int          raven_ipow (int base, unsigned exp);   // base^exp, no overflow check
```

---

## random — `<raven/rand.h>`

Backed by Linux `getrandom` (cryptographic quality — not a seedable PRNG).

```c
raven_u32    raven_rand_u32  (void);
raven_u8     raven_rand_u8   (void);
int          raven_rand_i32  (void);
int          raven_rand_bool (void);
unsigned int raven_rand_range(unsigned lo, unsigned hi);   // [lo, hi)
```

---

## harts — `<raven/hart.h>`

Multi-hart execution. Each hart has its own PC, registers, and stack; harts share one flat memory.

### Spawning

```c
typedef void (*RavenHartEntry)(unsigned int arg);

RAVEN_HART_STACK(worker_stack, 4096);     // 16-byte aligned char buffer

RavenHart h = raven_hart_spawn(my_worker, worker_stack, sizeof(worker_stack), 42);
// or, with the stack-array macro that infers the size and rejects pointers:
RavenHart h = RAVEN_HART_SPAWN(my_worker, worker_stack, 42);
```

If you prefer to bundle launch parameters:

```c
RavenHartTask t = raven_hart_task(my_worker, stack, sizeof(stack), 42);
//             or RAVEN_HART_TASK(my_worker, stack, 42)
RavenHart h = raven_hart_start(&t);
```

### Lifecycle

```c
int  raven_hart_is_done(RavenHart h);     // 0 = running, 1 = exited
void raven_hart_join   (RavenHart *h);    // spin-wait; frees payload; invalidates h
void raven_hart_detach (RavenHart *h);    // trampoline frees on exit; invalidates h
```

If a hart's entry function returns, the trampoline:
1. marks the payload as done,
2. frees it if the hart was detached,
3. calls the `hart_exit` ecall.

You never need to call `raven_unsafe_hart_exit()` manually.

`RavenHart` is opaque — do not read or write `_payload` directly.

---

## coroutines — `<raven/coro.h>`

Stackful, **cooperative** coroutines: a function that runs on its own stack and can suspend itself with `raven_coro_yield`, handing control back to whoever resumed it. Its stack and registers stay live across the suspension, so the next `raven_coro_resume` continues exactly where it left off.

Unlike `<raven/hart.h>` (parallel execution on another hart), coroutines are single-hart — exactly one runs at a time and control only moves on an explicit resume/yield. A switch is a pure user-space register/stack swap: **no ecall is involved**.

### Types

```c
typedef struct RavenCoro RavenCoro;                       // place on the stack or in static storage
typedef void (*RavenCoroFn)(RavenCoro *self, void *arg);  // coroutine body

typedef enum {
    RAVEN_CORO_READY, RAVEN_CORO_SUSPENDED,
    RAVEN_CORO_RUNNING, RAVEN_CORO_DONE
} RavenCoroState;

RAVEN_CORO_STACK(stack, 4096);   // 16-byte-aligned char buffer, owned by the caller
```

### API

| Function | Notes |
|----------|-------|
| `void raven_coro_init(RavenCoro *co, void *stack, raven_size_t size, RavenCoroFn fn, void *arg)` | prepare `co`; does not start it. The stack buffer is caller-owned and must outlive `co` |
| `void *raven_coro_resume(RavenCoro *co, void *send)` | run/continue `co`; `send` becomes the return value of the `yield` that paused it. Returns the value yielded back, or `NULL` once the body has finished |
| `void *raven_coro_yield(RavenCoro *self, void *value)` | suspend `self`, handing `value` to the resumer. Returns the next `send`. Call this from inside the body |
| `int raven_coro_done(const RavenCoro *co)` | 1 once the body has returned |
| `RavenCoroState raven_coro_status(const RavenCoro *co)` | current lifecycle state |

`resume`/`yield` exchange one `void*` in each direction — enough for the generator pattern. Pass `NULL` and ignore the result if you don't need values.

### Example — a generator

```c
static void counter(RavenCoro *self, void *arg) {
    int n = (int)(raven_uintptr_t)arg;
    for (int i = 1; i <= n; i++)
        raven_coro_yield(self, (void *)(raven_uintptr_t)i);
}

int main(void) {
    RAVEN_CORO_STACK(stack, 4096);
    RavenCoro co;
    raven_coro_init(&co, stack, sizeof(stack), counter, (void *)(raven_uintptr_t)5);

    while (!raven_coro_done(&co)) {
        void *v = raven_coro_resume(&co, NULL);
        if (!raven_coro_done(&co)) {
            raven_print_str("yielded ");
            raven_print_int((int)(raven_uintptr_t)v);
            raven_println();
        }
    }
    return 0;
}
```

> Keep stacks modest — the default RAM is 128 KB and there is no stack-overflow guard. Only callee-saved registers are switched (the compiler spills caller-saved ones around the call); on the hardware-float build (`libraven_f.a`) the `fs0–fs11` registers are saved as well.

---

## perf — `<raven/perf.h>`

```c
raven_u64 raven_instr_count(void);       // 64-bit retired-instruction count
raven_u64 raven_cycle_count(void);       // 64-bit cycle count (incl. cache penalties)
raven_u32 raven_instr_count32(void);
raven_u32 raven_cycle_count32(void);
```

### `RAVEN_MEASURE(label, block)`

```c
RAVEN_MEASURE("bubble sort", {
    bubble_sort(arr, N);
});
// → bubble sort: 75025007 instr, 549034132 cycles
```

The `block` is a normal compound statement — declare locals, break, return, anything. The instrumentation itself is two ecalls each side, well below normal benchmark resolution.

---

## debug — `<raven/debug.h>`

```c
__attribute__((noreturn)) void raven_panic(const char *msg);
__attribute__((noreturn)) void raven_exit (int code);
#define raven_assert(expr)   // prints "ASSERT failed: <expr> at <file>:<line>" and exits 1
```

`raven_panic` writes `"PANIC: <msg>\n"` to stderr and exits with code 1. It does **not** ebreak first; if you want the simulator to pause for inspection, call `raven_unsafe_breakpoint()` from `<raven/advanced.h>` before panicking.

---

## advanced - `<raven/advanced.h>` (opt-in)

> ⚠ Everything in this header bypasses Raven's safety and ergonomics. Use only when you have a specific reason.

Two naming conventions inside this header:

- `raven_sys_*` — maps directly to one Linux RISC-V ecall, follows the Linux ABI.
- `raven_unsafe_*` — does something whose correctness depends on Raven internals (memory map, JIT, hart scheduler).

### Constants

```c
RAVEN_FD_STDIN  RAVEN_FD_STDOUT  RAVEN_FD_STDERR
RAVEN_PROT_NONE|READ|WRITE|EXEC
RAVEN_MAP_SHARED|PRIVATE|ANONYMOUS
```

### Linux syscall wrappers

```c
int  raven_sys_write (int fd, const void *buf, int len);
int  raven_sys_read  (int fd, void *buf, int len);
int  raven_sys_writev(int fd, const RavenIovec *iov, int iovcnt);
void raven_sys_exit  (int code) __attribute__((noreturn));

int  raven_sys_getpid (void);  // always 1
int  raven_sys_getuid (void);  // always 0
int  raven_sys_getgid (void);  // always 0

void *raven_sys_brk  (void *addr);
void *raven_sys_mmap (void *addr, raven_size_t len, int prot, int flags,
                      int fd, int offset);
int   raven_sys_munmap(void *addr, raven_size_t len);

int   raven_sys_getrandom    (void *buf, int len, unsigned flags);
int   raven_sys_clock_gettime(int clockid, RavenTimespec *tp);
```

### Unsafe primitives

```c
void raven_unsafe_breakpoint(void);
// Emits ebreak. The simulator pauses; you can inspect state, step, or
// resume from the UI. This is NOT an exit.

int  raven_unsafe_map_exec(void *addr, raven_size_t len);
// Required after writing instruction bytes to memory if you intend to jump
// into them and the simulator runs with --jit=hot or --jit=full. Both addr
// and len must be 4-byte aligned. Returns 0 on success, negative on error.

void raven_unsafe_hart_exit(void) __attribute__((noreturn));
// Exit only the current hart (program continues on other harts). The hart
// trampoline in <raven/hart.h> already calls this for you when an entry
// function returns; calling it directly skips payload cleanup.

int  raven_unsafe_hart_start(unsigned entry_pc, unsigned sp, unsigned arg);
// Raw hart_start ecall. The trampoline-and-payload machinery built on top
// of this is in <raven/hart.h>; use that unless you really need raw access.
// Returns hart_id ≥ 1 on success, -1 if no free core, -2 if entry_pc is
// outside an executable region.
```

### Raw ecall numbers

All ecall numbers are exposed as `RAVEN_ECALL_*` constants in `<raven/_ecall.h>` (transitively included by every module). Use them if you want to invoke ecalls via inline assembly directly.

```c
#define RAVEN_ECALL_WRITE          64
#define RAVEN_ECALL_PRINT_INT    1000
#define RAVEN_ECALL_INSTR_COUNT  1030
// ... see include/raven/_ecall.h for the complete list
```

---

## Example: complete program

```c
#include <raven/raven.h>
#include <raven/advanced.h>      // for raven_unsafe_breakpoint

static void worker(unsigned int arg) {
    raven_print_str("worker says: ");
    raven_print_uint(arg);
    raven_println();
}

int main(void) {
    /* heap */
    int *buf = (int *)raven_malloc(16 * sizeof(int));
    if (!buf) raven_panic("out of memory");
    for (int i = 0; i < 16; i++) buf[i] = i * i;

    /* measured block */
    RAVEN_MEASURE("square-sum", {
        int s = 0;
        for (int i = 0; i < 16; i++) s += buf[i];
        raven_print_str("sum = ");
        raven_print_int(s);
        raven_println();
    });

    /* spawn a hart */
    RAVEN_HART_STACK(stk, 1024);
    RavenHart h = RAVEN_HART_SPAWN(worker, stk, 99);
    raven_hart_join(&h);

    /* pause for inspection */
    raven_unsafe_breakpoint();

    raven_free(buf);
    return 0;
}
```
