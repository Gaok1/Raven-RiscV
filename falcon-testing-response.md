# Testing student compiler output with Raven/Falcon

Yes, Raven can be used from Rust without going through the TUI.

The part you want is the **Falcon** engine, the simulator core used by Raven. Raven is mainly shipped as a standalone binary for users, but the crate also exposes its internals as a Rust library. There is now a small, dedicated testing API on top of Falcon so a harness can assemble a generated program, run it headlessly, and inspect the final machine state — stdout, registers, memory, and exit code — not just stdout.

## Recommended: the `Falcon` engine

```rust
use raven::falcon::Falcon;

#[test]
fn program_computes_42() {
    // `generated_asm` is the assembly your student compiler emitted.
    let generated_asm = std::fs::read_to_string("examples/add.s").unwrap();

    let result = Falcon::new()
        .asm(generated_asm)
        .run()
        .expect("program should assemble and run");

    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout(), "42\n");
    assert_eq!(result.reg("a0"), 42);      // ABI name ("a0", "sp", ...) or xN
    assert_eq!(result.read_word(0x1000), 42);
}
```

`Falcon::new()` gives sensible defaults — 16 MB RAM, cache on, VM off, a single hart — so the only thing you *must* provide is a program. Everything else is an optional `set_*`-style builder call, so the test isn't flooded with inputs:

```rust
let result = Falcon::new()
    .asm_file("examples/add.s")?      // load program from a path
    .mem_mb(16)                       // RAM size (default 16 MB)
    .no_cache()                       // bypass the cache model
    .vm(true)                         // enable virtual memory
    .max_cycles(100_000)              // safety cap (default 10M)
    .stdin_line("7")                  // pre-seed stdin for read syscalls
    .run()?;
```

### Inspecting the result

`RunResult` exposes everything a test needs:

| Method | Returns |
| --- | --- |
| `exit_code` | `Option<u32>` — code from an `exit` syscall, `None` if it never exited |
| `stdout()` / `stdout_bytes()` | program output as `Cow<str>` / `&[u8]` |
| `reg("a0")` / `reg_x(10)` | register by ABI name or index |
| `read_word(addr)` / `read_byte(addr)` | memory, cache-coherent (sees writeback values) |
| `pc()` | program counter at exit |
| `cycles` / `timed_out` | progress bound and whether `max_cycles` was hit |
| `cpu()` / `mem()` | escape hatches to the raw `Cpu` / `CacheController` (cache stats, etc.) |

`read_word` reads through the cache, so it sees values still sitting in a writeback cache — the value the program actually wrote, not stale RAM.

## Alternative: the CLI-shaped runner

For the full feature set — multi-hart, pipeline simulation, `.fcache`/`.rcfg` config files — there is still `raven::cli::run_headless`, which the `raven run` command itself uses. It takes a `RunArgs` struct and validates a set of expectations inline (exit code, stdout, registers, memory):

```rust
use raven::cli::{run_headless, OutputFormat, RunArgs};
use raven::falcon::jit::BackendKind;

run_headless(RunArgs {
    file: "examples/add.s".to_string(),
    cache_config: None,
    settings: None,
    pipeline: false,
    pipeline_config: None,
    pipeline_trace_out: None,
    output: None,
    nout: true,
    format: OutputFormat::Json,
    mem_size: Some(16 * 1024 * 1024),
    max_cycles: 100_000,
    max_cores: 1,
    expect_exit: Some(0),
    expect_stdout: None,
    expect_regs: vec![(10, 42)], // a0 == 42
    expect_mems: vec![],
    jit_mode: BackendKind::None,
    screen_window: false,
})
.expect("program should run and match expectations");
```

This works and is exhaustive, but it is shaped like the CLI runner: it takes many fields, prints to stdout/stderr, and validates rather than returning inspectable state. For a test harness, prefer `Falcon`.

## Summary

- Falcon can be used from Rust, separately from the UI.
- `raven::falcon::Falcon` is the small dedicated testing API: build with optional setters, `run()`, then assert on stdout, registers, memory, and exit code.
- `raven::cli::run_headless` remains for multi-hart / pipeline / config-file scenarios.
- `Falcon` currently assembles from `.asm(...)` / `.asm_file(...)` and runs a single hart; pre-assembled binaries (FALC/ELF) and multi-hart go through `run_headless`.
