# raven-riscv-engine

Headless RISC-V simulation engine extracted from Raven-RiscV.

```rust
use raven_riscv_engine::Falcon;

let result = Falcon::new()
    .asm(".text\n li a0, 42\n li a7, 93\n ecall\n")
    .run()
    .unwrap();

assert_eq!(result.exit_code, Some(42));
```

The crate exposes a small `host` module for engine-side console and screen devices;
it does not include Raven's TUI application.
