# From Rust to Raven

A bare-metal Rust project that compiles to a RISC-V ELF binary ready to run in the [Raven](https://github.com/gaok1/Raven) simulator.

The `api` module provides the glue between your Rust code and the simulator: syscall wrappers, `print!`/`println!`/`read_line!` macros, a global allocator, and a panic handler.

## Building

### Option 1 — Make (recommended)

Builds and copies the ELF to the project root so it's easy to find:

```sh
make          # debug build  → rust-to-raven.elf
make release  # release build → rust-to-raven.elf (smaller, optimized)
make clean    # remove build artifacts and the .elf
```

### Option 2 — Cargo

```sh
cargo build          # debug
cargo build --release
```

The binary will be at:
```
target/riscv32im-unknown-none-elf/debug/rust-to-raven
target/riscv32im-unknown-none-elf/release/rust-to-raven
```

Load either file into Raven — it's a standard ELF, no extra steps needed.

## Requirements

- Rust **nightly** (see `rust-toolchain.toml`)
- `riscv32im-unknown-none-elf` target — add it once with:
  ```sh
  rustup target add riscv32im-unknown-none-elf
  ```

## `api` reference

### Raw syscall wrappers (`api::syscall`)

| Function | Syscall | Description |
|----------|---------|-------------|
| `read(fd, buf, len) -> isize` | 63 | Read bytes; `RavenFD::STDIN` only |
| `write(fd, buf, len) -> isize` | 64 | Write bytes; `STDOUT` or `STDERR` |
| `exit(code) -> !` | 93 | Terminate (no return) |
| `exit_group(code) -> !` | 94 | Alias of `exit` |
| `brk(addr) -> usize` | 214 | Advance heap break; `0` to query |
| `munmap(addr, len) -> isize` | 215 | No-op; always returns `0` |
| `mmap(addr, len, prot, flags, fd, offset) -> usize` | 222 | Anonymous heap alloc (`MAP_ANONYMOUS`, `fd=-1`) |
| `getrandom(buf, buflen, flags) -> isize` | 278 | Fill buffer with cryptographic random bytes |
| `clock_gettime(clockid, tp) -> isize` | 403 | Fill `*timespec` with instruction-based time |
| `writev(fd, iov, iovcnt) -> isize` | 66 | Scatter-write from `iovec[]` |
| `getpid() -> u32` | 172 | Always returns `1` |
| `getuid() -> u32` | 174 | Always returns `0` |
| `getgid() -> u32` | 176 | Always returns `0` |
| `pause_sim()` | — | Emit `ebreak`; pauses execution for inspection |
| `writev(fd, iov, iovcnt) -> isize` | 66 | Scatter-write from iovec array |
| `getpid() -> u32` | 172 | Always returns `1` |
| `getuid() -> u32` | 174 | Always returns `0` |
| `getgid() -> u32` | 176 | Always returns `0` |
| `munmap(addr, len) -> isize` | 215 | No-op; always returns `0` |
| `mmap(addr, len, prot, flags, fd, offset) -> usize` | 222 | Anonymous heap alloc (`MAP_ANONYMOUS`, `fd=-1`) |
| `clock_gettime(clockid, tp) -> isize` | 403 | Fill `*timespec` with instruction-based time |

### Macros (`api::io`)

| Macro | Description |
|-------|-------------|
| `print!(...)` | Formatted print to stdout |
| `println!(...)` | Formatted print to stdout with newline |
| `eprint!(...)` | Formatted print to stderr (red in Raven) |
| `eprintln!(...)` | Formatted print to stderr with newline |
| `read_line!(buf)` | Read one console line into `buf: &mut [u8]` |
| `read_int!()` | Read signed integer from stdin |
| `read_uint!()` | Read unsigned integer from stdin |

### Raven teaching extensions (`api::syscall`)

Single-ecall shortcuts — minimal overhead, useful for measuring performance.

| Function | Syscall | Description |
|----------|---------|-------------|
| `print_int(n: i32)` | 1000 | Print signed integer (no newline) |
| `print_str(s: *const u8)` | 1001 | Print NUL-terminated string (no newline) |
| `println_str(s: *const u8)` | 1002 | Print NUL-terminated string + newline |
| `read_line(buf: *mut u8)` | 1003 | Read console line into buffer (NUL-terminated) |
| `print_uint(n: u32)` | 1004 | Print unsigned integer (no newline) |
| `print_hex(n: u32)` | 1005 | Print as hex, e.g. `0xDEADBEEF` |
| `print_char(c: u8)` | 1006 | Print a single ASCII character |
| `print_newline()` | 1008 | Print a newline |
| `read_u8(dst: *mut u8)` | 1010 | Read decimal/hex → store as `u8` |
| `read_u16(dst: *mut u16)` | 1011 | Read decimal/hex → store as `u16` |
| `read_u32(dst: *mut u32)` | 1012 | Read decimal/hex → store as `u32` |
| `read_int(dst: *mut i32)` | 1013 | Read signed integer (accepts `-`) |
| `read_float(dst: *mut f32)` | 1014 | Read float → store as `f32` |
| `print_float(v: f32)` | 1015 | Print `f32` from `fa0` (up to 6 sig. digits) |
| `get_instr_count() -> u32` | 1030 | Instructions executed since start (low 32 bits) |
| `get_cycle_count() -> u32` | 1031 | Alias of `get_instr_count` |
| `memset(dst, byte, len)` | 1050 | Fill memory via simulator |
| `memcpy(dst, src, len)` | 1051 | Copy memory via simulator |
| `strlen(s) -> usize` | 1052 | Length of NUL-terminated string |
| `strcmp(s1, s2) -> i32` | 1053 | Compare strings; `<0` / `0` / `>0` |
