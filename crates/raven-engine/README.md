# raven-engine

Trait-driven assembly and simulation engine used by Raven. The crate ships
with `riscv32` (RV32IMAF), `sap`, and `toy16`; applications may register more
architectures without changing the engine or selecting an ISA at compile time.

## Quick start — RV32 only

`Falcon` is the batteries-included runner for graders and tests. It assembles,
runs, and reports in one call:

```rust
use raven_engine::Falcon;

fn main() -> Result<(), String> {
    let r = Falcon::new()
        .asm(".text\n li a0, 42\n li a7, 93\n ecall\n")
        .max_cycles(10_000)
        .run()?;

    assert_eq!(r.exit_code, Some(42));
    assert_eq!(r.reg("a0"), 42);
    Ok(())
}
```

## Quick start — any architecture

Go through the registry when the ISA is chosen at runtime:

```rust
use raven_engine::{ArchitectureRegistry, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::from_registry(ArchitectureRegistry::builtin(), "riscv32")?;
    let image = engine.assemble("li a0, 42\nhalt", 0)?;
    let mut machine = engine.create_machine(engine.architecture().descriptor().default_memory_size)?;
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
  `builtin()` returns the shared registry; `with_builtins()` returns an owned
  copy you can `register` more architectures into.

`ProgramImage::to_falc_v2` writes the architecture-tagged FALC v2 container;
`ProgramImage::from_falc` reads both v2 and legacy RV32 FALC v1 files. Neither
version carries the `SourceMap` — labels and comments stay in the assembler's
output rather than in the shipped binary.

`architectures::riscv32` owns the complete RV32 implementation. Its nested
`falcon` module contains the CPU, cache, MMU, JIT, syscalls, and pipeline;
`raven_engine::falcon` remains as a compatibility reexport. The module
also exposes the RV32 loading primitives every host shares:

- `riscv32::install_image` writes an image's segments and zero-fill into any
  `Bus`, bounds-checking the whole image before the first byte is written.
- `riscv32::heap_break_after` gives the guest heap start for a loaded image.
- `riscv32::image_from_binary` decodes a FALC container (either version) or a
  flat block of machine code into a `ProgramImage`.

The engine's own `Machine`, the CLI loader, and the TUI's Run tab all place a
program through these, so an image lands at the same addresses wherever it is
opened. ELF is the one format that stays outside: it carries a symbol table and
a section list a `ProgramImage` cannot express, so it keeps `falcon::program::load_elf`.

## Capabilities

`Machine` is the floor: load, step, snapshot. A host with only the floor can
draw a list of numbers. Capabilities are how a backend says what else it can do,
so hosts can offer real panes without knowing the ISA.

| Capability | Accessor | What a host can then do |
|---|---|---|
| `RegisterFile` | `Machine::registers` | Draw and edit registers from the backend's own banks — any count, any width, with or without a calling convention |
| `MemoryInspect` | `Machine::memory` | Show memory without disturbing it, and offer the regions the backend names |
| `InstructionCodec` | `Machine::code` | Disassemble a width-correct listing and provide structured fields for the instruction inspector |
| `CacheHierarchy` | `Machine::caches` | Render cache contents, statistics, policies, and address breakdowns without perturbing execution |

Editor help is ISA-owned too: `Assembler::instruction_forms` supplies mnemonic
operand ghosts and `Assembler::is_register` supplies register highlighting.
The host no longer carries a RISC-V mnemonic or register table.

Each accessor defaults to `None`, so adding a capability never breaks an
existing backend and never forces a new one to implement something it has no
concept of. Implement the trait, return `Some(self)`, and every host that
understands the capability lights up for your ISA.

Rules a capability must follow are in the `capability` module docs. The short
version: nothing ISA-shaped in a signature, cheap enough to call while
rendering, and read paths never perturb the machine.

## Adding an architecture

Implement `Assembler`, `Architecture`, and `Machine`, then register an
`Arc<dyn Architecture>` with `ArchitectureRegistry::register`. Add whichever
capabilities your backend can honour. Keep hardware features behind
`ArchitectureDescriptor::capabilities`; callers must not assume 32 registers,
32-bit addresses, fixed-width 32-bit instructions, MMU, cache, pipeline,
floating point, JIT, ELF, or multicore support.

Each ISA lives in its own module directory under `src/architectures/`:

```text
architectures/
  riscv32/
    mod.rs
    falcon/
  sap/mod.rs
  toy16/mod.rs
```

Keep its adapter, assembler, concrete machine, instruction codec, and tests in
that directory. Split those responsibilities into sibling files only when the
module grows; the public path remains `architectures::<isa>`.

SAP and Toy16 use the shared teaching-cache model, so the same Run and Cache
tabs exercise their real instruction fetches and data accesses. The public
Toy16 backend is the smallest complete reference implementation — and
it is deliberately *unlike* RISC-V (one bank of eight 16-bit registers, two-byte
instructions, no calling convention) so the contract tests in
`tests/architecture_contracts.rs` catch any assumption that leaks. Those tests
run over every registered backend, not just RV32; a new architecture is covered
by them the moment it is registered.
