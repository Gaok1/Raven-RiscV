# Falcon ASM — an approachable RISC-V (RV32I) emulator
<img src="https://github.com/user-attachments/assets/b0a9c716-3750-4aba-85f0-6957d2b510fc" height="400"/>

Falcon ASM is a small Rust project that shows every moving part of a classic fetch → decode → execute loop. It is meant for
students, hobbyists, and teachers who want to tinker with the RISC-V base ISA without reading thousands of lines of
optimization-heavy code.

Beyond the core emulator, Falcon ships with a built-in IDE experience: a project browser, source editor with syntax
highlighting, an instruction-by-instruction visualizer, and panels for registers, memory, and syscalls. You can assemble,
run, pause, and rewind without leaving the interface, making it easy to demonstrate how each instruction manipulates state or
to debug student assignments in real time. The goal is to be a simulator and teaching platform that feels welcoming even if
you are opening RISC-V for the very first time.

## What Falcon includes

- **Readable core** – the CPU, memory, and instruction decoder are written to be followed line by line.
- **Integrated assembler** – assemble `.text`, `.data`, and `.bss` segments (with directives like `.byte`, `.word`, `.ascii`,
  `.space`) and common pseudo-instructions (`la`, `call`, `ret`, `push`, `pop`, `printStr`, `printStrLn`, `read`, and more).
- **Syscall helpers** – set `a7` and call `ecall` to print values or strings, read user input, or halt the program.
- **RV32I + M coverage** – arithmetic, loads/stores, branches, jumps, multiplication, division, and a friendly error model for
  unsupported instructions.

The emulator follows the usual RISC-V register naming (`zero`, `ra`, `sp`, `a0`…`a7`, `t0`…`t6`, `s0`…`s11`) and uses a
little-endian memory layout so you can mirror the behavior of popular textbooks and course material.

## Quick start
0. Download and run the latest release or
1. Install the Rust toolchain from [rustup.rs](https://rustup.rs).
2. Clone this repository and run:

   ```bash
   cargo run
   ```

3. Write a program with `.text` and `.data` sections, assemble it with Falcon, and watch each step as you single-step through
   execution.

Embedding Falcon elsewhere? Use the helper functions to place each segment in memory:

```rust
use falcon::program::{load_words, load_bytes, zero_bytes};

let prog = falcon::asm::assemble(source, base_pc)?;
load_words(&mut mem, base_pc, &prog.text)?;
load_bytes(&mut mem, prog.data_base, &prog.data)?;
let bss_base = prog.data_base + prog.data.len() as u32;
zero_bytes(&mut mem, bss_base, prog.bss_size)?;
```

## Learn more

- Follow the step-by-step walkthrough in the English [tutorial](Tutorial.md) to assemble and run your first programs.
- Dive into instruction formats, bit layouts, and pseudo-instructions in the English [`format.md`](format.md).
- Browse the `Program Examples/` directory for sample projects that exercise syscalls, arithmetic, and control flow.

## Contributing and roadmap

Falcon is intentionally small, and contributions are welcome! Ideas on the horizon include CSR/fence support, floating-point
extensions, and higher-level tooling around the emulator.

Whether you are preparing a lecture, debugging your first assembly homework, or building a teaching tool, Falcon ASM aims to be
a friendly place to explore the RISC-V ecosystem. Enjoy the flight!
