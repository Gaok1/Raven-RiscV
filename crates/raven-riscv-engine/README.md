# raven-riscv-engine

Headless RISC-V simulation engine extracted from Raven-RiscV.

Use it when you want to embed Raven's RISC-V assembler/simulator in tests,
graders, tools, or other Rust applications without launching the Raven TUI.

## Install

```toml
[dependencies]
raven-riscv-engine = "0.1"
```

## Quick start

```rust
use raven_riscv_engine::Falcon;

fn main() -> Result<(), String> {
    let result = Falcon::new()
        .asm(".text\n li a0, 42\n li a7, 93\n ecall\n")
        .max_cycles(10_000)
        .run()?;

    assert_eq!(result.exit_code, Some(42));
    assert_eq!(result.reg("a0"), 42);

    Ok(())
}
```

## Running a program

The main entry point is [`Falcon`], a small builder-style API:

```rust
use raven_riscv_engine::Falcon;

let run = Falcon::new()
    .asm(".text\n li t0, 123\n li a7, 93\n ecall\n")
    .mem_mb(16)
    .no_cache()
    .max_cycles(100_000)
    .run();
```

`run()` returns:

```rust
Result<RunResult, String>
```

So handle it like any other fallible Rust API:

```rust
match Falcon::new().asm(".text\n ecall\n").run() {
    Ok(result) => println!("exit code: {:?}", result.exit_code),
    Err(err) => eprintln!("Raven failed: {err}"),
}
```

Common `Err(String)` cases include assembly errors, load errors, invalid memory
accesses, unsupported multi-hart usage through this API, and execution faults.

## Inspecting `RunResult`

After a successful run, inspect the final machine state through [`RunResult`]:

```rust
use raven_riscv_engine::Falcon;

let result = Falcon::new()
    .asm(".text\n li t0, 42\n li t1, 0x100\n sw t0, 0(t1)\n li a7, 93\n ecall\n")
    .run()
    .unwrap();

assert_eq!(result.exit_code, Some(0));
assert_eq!(result.reg("t0"), 42);      // ABI name
assert_eq!(result.reg_x(5), 42);       // x5 / t0
assert_eq!(result.read_word(0x100), 42);
```

Useful fields and methods:

- `result.exit_code: Option<u32>` — value passed to Linux `exit`/`exit_group`.
- `result.timed_out: bool` — true when `max_cycles` stopped execution.
- `result.cycles: u64` — scheduler iterations executed.
- `result.stdout()` — stdout as lossy UTF-8.
- `result.stdout_bytes()` — raw stdout bytes.
- `result.reg("a0")` — register by ABI name or `xN` string.
- `result.reg_x(10)` — register by numeric index.
- `result.pc()` — final program counter.
- `result.read_byte(addr)` / `result.read_word(addr)` — final memory reads.
- `result.cpu()` / `result.mem()` — lower-level escape hatches.

## Stdin/stdout example

`stdin_line()` pre-seeds one line of input for read-style syscalls. Program output
can be read from `RunResult`:

```rust
use raven_riscv_engine::Falcon;

let result = Falcon::new()
    .asm(".text\n li a0, 7\n li a7, 1000\n ecall\n li a7, 93\n ecall\n")
    .stdin_line("hello")
    .run()
    .unwrap();

assert_eq!(result.stdout(), "7");
```

## Builder options

```rust
Falcon::new()
    .asm("...")           // assembly source string
    .asm_file("main.s") // load source from file, returns io::Result<Falcon>
    .mem_bytes(1024 * 1024)
    .mem_mb(16)
    .max_cycles(1_000_000)
    .no_cache()
    .vm(false)
    .stdin_line("input line");
```

Notes:

- The simple `Falcon` API currently supports one hart.
- Defaults are 16 MiB RAM, cache enabled, VM off, and `max_cycles = 10_000_000`.
- This crate exposes a small `host` module for engine-side console and screen
  devices; it does **not** include Raven's TUI application.

[`Falcon`]: https://docs.rs/raven-riscv-engine/latest/raven_riscv_engine/struct.Falcon.html
[`RunResult`]: https://docs.rs/raven-riscv-engine/latest/raven_riscv_engine/struct.RunResult.html
