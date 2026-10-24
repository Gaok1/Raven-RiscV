# ELF loader fixture

`fixture.elf` is a small RV32 executable for the ELF loader tests. It is
committed so that `cargo test` works on a clean checkout, without building
`rust-to-raven` or `c-to-raven` first.

| File | What it is |
|---|---|
| `fixture.S` | The program: it prints a line, adds one to a word in `.data` and exits with 42. |
| `fixture.ld` | Linker script. It puts `.rodata` at `0x10000`, `.text` at `0x11000` and `.data`/`.bss` at `0x12000`, each in its own `PT_LOAD`. |
| `fixture.elf` | The linked image (about 1 KB). |

Used by:

- `crates/raven-engine/tests/support/falcon_program_elf.rs` (the loader tests);
- `loading_elf_resets_pipeline_to_entry_pc` and
  `elf_fixture_runs_to_its_exit_code_in_every_mode` in
  `tests/support/ui_app_internal.rs`.

The tests check the layout: an executable and a non-executable `PT_LOAD`, the
read-only data below the code (as lld lays out `rust-to-raven`), an entry point
past the start of `.text`, a `.bss` tail to zero, and the `greeting`,
`counter`, `scratch`, `bump` and `_start` symbols. If you change the program,
keep those properties, or change the tests with it.

## Rebuilding

Run this in this folder. You need clang with the RISC-V target and lld:

```sh
clang --target=riscv32-unknown-elf -march=rv32imaf -mabi=ilp32 -nostdlib -static \
      -fuse-ld=lld -Wl,-T,fixture.ld -Wl,--nmagic fixture.S -o fixture.elf
```

`--nmagic` turns off page alignment in the file. The segments stay on separate
pages in memory, but the file does not carry 4 KiB of padding before each one.

On NixOS, run it inside `nix-shell -p llvmPackages_21.clang-unwrapped lld`. The
default clang wrapper is set up for the host, not for a bare-metal target.

The committed file was built with clang and lld 21.1.8. `.gitattributes` marks
`*.elf` as binary, so git does not change its line endings.
