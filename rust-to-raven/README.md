# From Rust to Raven

A bare-metal Rust project that compiles to a RISC-V ELF binary ready to run in the [Raven](https://github.com/gaok1/Raven) simulator.

The `raven_api` module provides the glue between your Rust code and the simulator: syscall wrappers, `print!`/`println!`/`read_line!` macros, a global allocator, and a panic handler.

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
