# Raven v1.20.0

## `riscv32im-unknown-none-elf` Compatibility

Raven runs ELF binaries compiled for the **`riscv32im-unknown-none-elf`** target — the standard bare-metal Rust target for 32-bit RISC-V without an operating system.

To compile a project for Raven:

```toml
# .cargo/config.toml
[build]
target = "riscv32im-unknown-none-elf"
```

```bash
cargo build --release
# load the generated .elf in Raven via Ctrl+O
```

A working, commented example is available at [`docs/RustCompatibleABI.rs`](docs/RustCompatibleABI.rs), showing the minimal structure of a `#![no_std]` program with `_start`, a panic handler, and communication with the emulator.

> **Note:** communication with the emulator uses the Linux RISC-V syscall ABI (`ecall`). No external crates are needed — just `core::arch::asm!`. The example in `docs/RustCompatibleABI.rs` wraps this into simple helper functions to keep application code clean.

### Supported Extensions

| Extension | Support |
|---|---|
| RV32I — base integer | ✅ |
| RV32M — multiply/divide | ✅ |
| RV32A — atomics | ✅ |
| RV32F — single-precision float | ✅ |
| `fence` | ✅ (no-op on single-core) |

### Available Syscalls

Raven implements a subset of the Linux RISC-V syscall ABI. You don't need to call `ecall` directly — the example in `docs/RustCompatibleABI.rs` provides ready-to-use wrappers.

| Syscall | Purpose |
|---|---|
| `write` (64) | Write text to the Raven console |
| `read` (63) | Read a line typed by the user |
| `exit` (93) | Terminate execution |
| `getrandom` (278) | Fill a buffer with random bytes |

---

## What's new in v1.20.0 — Visible exit code

When a program calls `exit`, Raven now prints the exit code in **red** in the console:

```
Exit 0
```

Makes it easy to tell at a glance whether the program finished successfully or with an error, without having to inspect registers manually.

---

## Download

| Platform | File |
|---|---|
| Windows x64 | `raven-x86_64-pc-windows-msvc.zip` |
| Linux x64 | `raven-x86_64-unknown-linux-gnu.tar.gz` |
| macOS x64 | `raven-x86_64-apple-darwin.tar.gz` |
| macOS ARM64 (Apple Silicon) | `raven-aarch64-apple-darwin.tar.gz` |

SHA256 checksums for all files in `SHA256SUMS.txt`.
