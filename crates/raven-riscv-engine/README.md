# raven-riscv-engine

Trait-driven assembly and simulation engine used by Raven. The crate ships
with `riscv32` (RV32IMAF) and `toy16`; applications may register more
architectures without changing the engine or selecting an ISA at compile time.

## Quick start

```rust
use raven_riscv_engine::{ArchitectureRegistry, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ArchitectureRegistry::with_builtins();
    let engine = Engine::from_registry(&registry, "riscv32")?;
    let image = engine.assemble("li a0, 42\nhalt", 0)?;
    let mut machine = engine.create_machine(64 * 1024)?;
    machine.load(&image)?;
    machine.run(100)?;

    assert_eq!(machine.snapshot().registers[10].value, 42);
    Ok(())
}
```

## Public contracts

- `Assembler` converts source into an ISA-neutral `ProgramImage` made from byte
  segments, zero-fill regions, an entry point, and source metadata.
- `Architecture` describes capabilities and creates an object-safe `Machine`.
- `Machine` loads, steps, runs, inspects, and edits a CPU without exposing its
  concrete register file or instruction width.
- `ArchitectureRegistry` selects implementations by stable runtime ID.

`ProgramImage::to_falc_v2` writes the architecture-tagged FALC v2 container;
`ProgramImage::from_falc` reads both v2 and legacy RV32 FALC v1 files.

## Adding an architecture

Implement `Assembler`, `Architecture`, and `Machine`, then register an
`Arc<dyn Architecture>` with `ArchitectureRegistry::register`. Keep CPU-specific
features behind `ArchitectureDescriptor::capabilities`; callers must not assume
32 registers, 32-bit addresses, fixed-width 32-bit instructions, MMU, cache,
pipeline, floating point, JIT, ELF, or multicore support.

The public Toy16 backend is the smallest complete reference implementation.
