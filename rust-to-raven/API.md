# rust-to-raven — API Reference

`rust-to-raven` is a `no_std` Rust crate that lets you write programs targeting the Raven RISC-V simulator without any OS or libc dependency.
It provides I/O macros, syscall wrappers, a heap allocator, random utilities, and multi-hart support.

---

## Setup

Add to your `Cargo.toml`:

```toml
[dependencies]
# no external deps needed — everything is in raven_api
```

Bring the prelude into scope:

```rust
use raven_api::{exit, pause_sim};
// or for hart spawning:
use raven_api::{spawn_hart, spawn_hart_fn};
```

The crate is `no_std` and targets Raven's RV32IM baseline with `A` and `F` enabled explicitly at build time.
Build with:

```bash
cargo build --target riscv32im-unknown-none-elf --release
```

---

## I/O Macros

These macros work like the standard `print!`/`println!` family but route through `write` syscalls.
They support the full `format_args!` syntax.

| Macro | Description |
|---|---|
| `print!(fmt, ...)` | Print formatted text to stdout (no newline) |
| `println!(fmt, ...)` | Print formatted text to stdout with newline |
| `eprint!(fmt, ...)` | Print formatted text to stderr (no newline) |
| `eprintln!(fmt, ...)` | Print formatted text to stderr with newline |
| `read_line!(buf)` | Read one line from stdin into a `&mut [u8]`; returns bytes read (newline excluded) |
| `read_int!()` | Read a signed decimal integer from stdin; returns `i32` |
| `read_uint!()` | Read an unsigned decimal integer from stdin; returns `u32` |

**Examples**

```rust
println!("Hello, Raven!");
println!("x = {}, y = {}", x, y);

let mut buf = [0u8; 64];
let n = read_line!(buf);
let s = core::str::from_utf8(&buf[..n]).unwrap_or("");

let value = read_int!();
let count = read_uint!();
```

---

## Syscalls

Raw `ecall` wrappers in `raven_api::syscall`. These map directly to the simulator's syscall ABI.

### POSIX-compatible syscalls

| Function | Syscall | Description |
|---|---|---|
| `read(fd, buf, len) -> isize` | 63 | Read bytes from file descriptor |
| `write(fd, buf, len) -> isize` | 64 | Write bytes to file descriptor |
| `writev(fd, iov, iovcnt) -> isize` | 66 | Scatter write from multiple buffers |
| `exit(code: i32) -> !` | 93 | Terminate the program |
| `exit_group(code: i32) -> !` | 94 | Same as `exit` in Raven (single-hart context) |
| `brk(addr: usize) -> usize` | 214 | Query or advance program break; pass `0` to query |
| `munmap(addr, len) -> isize` | 215 | No-op; always returns `0` |
| `mmap(addr, len, prot, flags, fd, offset) -> usize` | 222 | Anonymous mappings only (`flags=0x22`, `fd=-1`) |
| `getrandom(buf, len, flags) -> isize` | 278 | Fill buffer with random bytes |
| `clock_gettime(clockid, tp) -> isize` | 403 | Write `{ tv_sec, tv_nsec }` at `tp`; based on instruction count |
| `getpid() -> u32` | 172 | Always returns `1` |
| `getuid() -> u32` | 174 | Always returns `0` |
| `getgid() -> u32` | 176 | Always returns `0` |

All pointer-taking functions are `unsafe`. `exit` and `exit_group` never return.

### File descriptors

```rust
pub enum RavenFD {
    STDIN  = 0,
    STDOUT = 1,
    STDERR = 2,
}
```

### Raven teaching extensions (syscalls 1000–1053)

Direct single-purpose shortcuts — no strlen loop, no fd argument.

**Output**

| Function | Syscall | Description |
|---|---|---|
| `print_int(n: i32)` | 1000 | Print signed integer (no newline) |
| `print_uint(n: u32)` | 1004 | Print unsigned integer (no newline) |
| `print_hex(n: u32)` | 1005 | Print as `0xDEADBEEF` (no newline) |
| `print_char(c: u8)` | 1006 | Print single ASCII character |
| `print_newline()` | 1008 | Print newline |
| `print_str(s: *const u8)` | 1001 | Print NUL-terminated string (no newline) |
| `println_str(s: *const u8)` | 1002 | Print NUL-terminated string with newline |
| `print_float(v: f32)` | 1015 | Print float (no newline) |

**Input**

| Function | Syscall | Description |
|---|---|---|
| `read_line(buf: *mut u8)` | 1003 | Read line into NUL-terminated buffer |
| `read_u8(dst: *mut u8)` | 1010 | Read decimal/hex byte into `*dst` |
| `read_u16(dst: *mut u16)` | 1011 | Read 16-bit integer into `*dst` |
| `read_u32(dst: *mut u32)` | 1012 | Read 32-bit unsigned into `*dst` |
| `read_int(dst: *mut i32)` | 1013 | Read signed integer (accepts `-`) into `*dst` |
| `read_float(dst: *mut f32)` | 1014 | Read IEEE 754 float into `*dst` |

**Performance counters**

| Function | Syscall | Description |
|---|---|---|
| `get_instr_count() -> u32` | 1030 | Instructions executed so far (low 32 bits) |
| `get_cycle_count() -> u32` | 1031 | Alias of `get_instr_count` |

**Simulator-accelerated memory (syscalls 1050–1053)**

These execute in the simulator without running a loop — useful for benchmarking
(`get_instr_count` before/after to measure the difference).

| Function | Syscall | Description |
|---|---|---|
| `memset(dst, byte, len)` | 1050 | Fill region with byte |
| `memcpy(dst, src, len)` | 1051 | Copy non-overlapping regions |
| `strlen(s) -> usize` | 1052 | Length of NUL-terminated string |
| `strcmp(s1, s2) -> i32` | 1053 | Compare NUL-terminated strings |

All four are `unsafe`.

---

## Simulator Control

```rust
use raven_api::pause_sim;

pause_sim(); // hits ebreak — freezes execution so you can inspect state in Raven
```

`pause_sim()` emits an `ebreak` instruction. Raven pauses at this point so you can
inspect registers, memory, and pipeline state before resuming.

---

## Random

`raven_api::random` — backed by `getrandom` (cryptographic quality RNG).

| Function | Description |
|---|---|
| `rand_u32() -> u32` | Uniformly random 32-bit unsigned integer |
| `rand_u8() -> u8` | Uniformly random byte (0–255) |
| `rand_i32() -> i32` | Random signed 32-bit integer |
| `rand_range(lo, hi) -> u32` | Random `u32` in `[lo, hi)` |
| `rand_bool() -> bool` | `true` or `false` with equal probability |

Each function also has a matching macro (`rand_u32!()`, `rand_range!(lo, hi)`, etc.).

> `rand_range` uses modulo reduction — suitable for teaching, not for cryptographic use.

```rust
use raven_api::{rand_u32, rand_range, rand_bool};

let n = rand_u32();
let die = rand_range(1, 7);   // d6
let flip = rand_bool();
```

---

## Hart Management

Raven supports multiple hardware threads (harts) running concurrently.
The `raven_api` provides two abstraction levels.

### Low-level: `hart_start` / `hart_exit`

```rust
use raven_api::{hart_start, hart_exit};

static mut STACK: [u8; 4096] = [0; 4096];

extern "C" fn worker(id: u32) -> ! {
    println!("hart {} running", id);
    hart_exit()  // terminate only this hart, main keeps running
}

let sp = unsafe { STACK.as_ptr().add(4096) as u32 };
hart_start(worker as u32, sp, /*arg=*/1);
```

- `hart_start(entry_pc, stack_ptr, arg) -> i32` — syscall 1100. Spawns a new hart at `entry_pc`. `arg` arrives in `a0`. `stack_ptr` must point to the **top** (high address) of the stack.
- `hart_exit() -> !` — syscall 1101. Terminates only the calling hart; all other harts continue.

### High-level: `spawn_hart_fn` / `HartTask` / `spawn_hart`

```rust
use raven_api::{spawn_hart_fn, HartTask};

static mut STACK_A: [u8; 4096] = [0; 4096];

// Function pointer — no heap allocation
fn worker(id: u32) -> ! {
    println!("fn hart {}", id);
    exit(0)
}
spawn_hart_fn(worker, unsafe { &mut STACK_A }, /*arg=*/1);

// Preferred builder form
let value = 42u32;
let task = HartTask::new(
    move || {
        println!("closure hart got {}", value);
    },
);
let handle = task.start().expect("failed to start hart");
handle.join();

// Explicit stack size in bytes
let task = HartTask::with_stack_size(
    move || {
        println!("custom stack size hart");
    },
    16 * 1024,
);
let handle = task.start().expect("failed to start hart");
handle.join();
```

| Function | Description |
|---|---|
| `spawn_hart_fn(entry, stack, arg) -> i32` | Spawn from a function pointer; zero allocation |
| `HartTask::new(closure) -> HartTask` | Build a closure-backed hart task with the default stack size |
| `HartTask::with_stack_size(closure, bytes) -> HartTask` | Build a closure-backed hart task with an explicit stack size in bytes |
| `HartTask::start(self) -> Result<HartHandle, i32>` | Start the task and return a join handle |
| `HartHandle::join(self)` | Wait until the hart finishes |
| `spawn_hart(closure, stack) -> Result<HartHandle, i32>` | Convenience wrapper when you already have a stack slice |

`HartTask` allocates and owns its own stack internally.
Closure-backed tasks automatically mark themselves finished when the closure returns;
`join()` spins until that internal completion flag is observed.

---

## Heap Allocator

The crate ships with a `#[global_allocator]` backed by `linked_list_allocator` and the `brk` syscall.
You do not need to initialise it — it bootstraps lazily on the first allocation.

The heap grows automatically in 64 KB steps as needed.
All `alloc` types (`Box`, `Vec`, `String`, etc.) work out of the box once you add
`extern crate alloc;` at the top of your file.

```rust
extern crate alloc;
use alloc::{vec::Vec, string::String, boxed::Box};

let mut v: Vec<u32> = Vec::new();
v.push(1);
v.push(2);
```

If `brk` refuses to grow (RAM limit reached), the OOM handler prints a message to stderr and calls `exit(1)`.

---

## Debug

```rust
use raven_api;

unsafe { raven_api::ENABLED_DEBUG_MESSAGES = true; }

raven_api::print_debug("checkpoint reached");
// prints "[DEBUG]: checkpoint reached" to stderr only when the flag is set
```

---

## Panic

The crate provides a `#[panic_handler]` that prints the panic info to stderr and exits with code `101`.

```
Panic!: panicked at 'index out of bounds', src/main.rs:12:5
```

---

## Complete Example

```rust
#![no_std]
#![no_main]

extern crate raven_to_raven; // the crate root re-exports everything

use raven_api::{exit, pause_sim, rand_range};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("Enter a number:");
    let n = read_int!();
    let r = rand_range(1, n as u32 + 1);
    println!("Random 1..{} = {}", n, r);

    pause_sim();   // inspect state here in Raven before exit

    exit(0)
}
```
