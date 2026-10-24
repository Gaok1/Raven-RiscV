# riscv-tests on RAVEN

This folder runs the official [riscv-tests](https://github.com/riscv-software-src/riscv-tests)
suite on RAVEN. It covers `rv32ui`, `rv32um`, `rv32ua` and `rv32uf`: 71 tests.
Each test runs in sequential mode and with `--pipeline`; `--modes` adds the
other three pipeline modes.

## Files

| File | What it is |
|---|---|
| `run_riscv_tests.py` | Builds each test with clang and runs each ELF on one or more RAVEN binaries. |
| `env/link.ld` | Links at `0x10000`, for either environment. RAVEN's RAM starts at 0 and is 16 MiB by default. |
| `env/riscv_test.h` | Minimal test environment, used with `--env raven`. |

## Environments

### `--env p` (default): the official environment

The tests are built with the suite's own `env/p/riscv_test.h`. Before the
test body, it parks every hart whose `mhartid` is not 0, sets up the trap
vector, `satp`, PMP and delegation, enables F, clears `fcsr` and enters the
test in U-mode through `mret`. RAVEN runs all of it, in every mode:

- `mhartid` reads the hart's id, so hart 0 goes on;
- the CSRs RAVEN does not implement (`mnstatus`, `mie`, `pmpaddr0`,
  `pmpcfg0`) ignore writes and read as 0, which is all this setup needs;
- `mret` drops to U-mode in the pipeline as well (#84).

A test ends with `RVTEST_PASS` or `RVTEST_FAIL`. Both end in the exit syscall
(`ecall`, a7 = 93): a pass is `exit(0)`, a failure is `exit((case << 1) | 1)`.

The trap vector reports an exception the test did not expect through `tohost`,
the word through which a program asks an HTIF host for something. RAVEN takes
its address from the ELF's `tohost` symbol (`env/link.ld` gives it a section
of its own, as the official linker script does). A 32-bit store of an odd
value `v` to it ends the run with exit code `v >> 1`, the way the exit syscall
does. An even value asks the host to run a system call; RAVEN does not support
that, reports it once on the console and ignores it. The harness reports an
exit through `tohost` as `FAIL` with "unexpected trap".

### `--env raven`: the minimal environment

`env/riscv_test.h` starts straight in the test body, with no initialisation
and no privileged transition (`fcsr` is not cleared either). It keeps
`RVTEST_PASS` and `RVTEST_FAIL` identical to the official macros. It checks the
instructions in the test bodies without the setup around them.

### Scope

The suite's `.S` files and `test_macros.h` are not modified in either
environment.

`amocas_w` and `amocas_d` belong to Zacas, not to A. They are left out.

`rv32mi` and `rv32si` are out of scope. They test the trap path itself:
`scall`, for one, expects `ecall` to raise an environment-call exception. In
RAVEN, `ecall` is a call to the host (a system call, `a7` = number), not an
exception, and RAVEN raises no illegal-instruction, misaligned-access or
breakpoint exceptions either (`ebreak` pauses the run).

## Requirements

- `clang` and `ld.lld` from LLVM 18 or later, with the `riscv32` target
- Python 3
- a clone of riscv-tests
- one or more RAVEN binaries (`cargo build --release`)

## Usage

```bash
git clone https://github.com/riscv-software-src/riscv-tests.git ../riscv-tests
cargo build --release
python tests/riscv-tests/run_riscv_tests.py --suite ../riscv-tests --raven current=target/release/raven
python tests/riscv-tests/run_riscv_tests.py --suite ../riscv-tests --raven current=target/release/raven --env raven --modes
```

On Windows the binary is `target/release/raven.exe`. Repeat
`--raven label=path` to compare several binaries in one run. The script
writes `build/<env>/` (the ELFs), `results-<env>.csv` and `summary-<env>.md`
next to itself.

`--pipeline` alone runs the `Serialized` mode. `--modes` also runs
`ParallelUFs`, `Scoreboard` and `TomasuloRob`, each through a `--config` file:
the script runs `raven export-config` and changes the `mode=` line of its
`[pipeline]` section. The files go in `build/<env>/config/`.

Each run gets one status:

| Status | Meaning |
|---|---|
| `PASS` | exit code 0 |
| `FAIL` | odd exit code; the detail gives the failing case, `code >> 1` |
| `NO_EXIT` | the program never called exit (for example, the cycle limit) |
| `ERROR` | RAVEN rejected the ELF or aborted |
| `BUILD_FAILED` | clang could not build the test |

`--diag-no-flags` is a diagnostic, not a result. It runs `rv32uf` only, with a
copy of `test_macros.h` that skips the `fflags` check. It separates wrong values
from missing exception flags. Its output goes to `build-diag/<env>/`,
`results-diag-<env>.csv` and `summary-diag-<env>.md`.

## Results

Suite at commit `3df703c`, RAVEN with the fixes for #84, #85, #86 and #88.
Each cell is the same in both environments, `p` and `raven`:

| Suite | seq | Serialized | ParallelUFs | Scoreboard | TomasuloRob |
|---|---|---|---|---|---|
| `rv32ui` | 42/42 | 42/42 | 42/42 | 42/42 | 42/42 |
| `rv32um` | 8/8 | 8/8 | 8/8 | 8/8 | 8/8 |
| `rv32ua` | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 |
| `rv32uf` | 11/11 | 11/11 | 11/11 | 11/11 | 11/11 |

- Until `e8cd3c2`, `mode=Scoreboard` and `mode=TomasuloRob` in a `--config`
  file only renamed the mode: `raven run` built no engine for them and ran
  the `Serialized` datapath, so those two models had never run the suite.
  Run for real, `Scoreboard` failed `rv32uf` `fcvt` and `ldst` at case 2 in
  both environments. The out-of-order models named the operands of F
  instructions in the wrong register file (#86, `fcvt`), and scheduled `flw`
  and `fsw` as float work, outside the program order the load/store unit
  keeps (`ldst`). The scoreboard lets an instruction finish as soon as its
  operands look ready, so it ran instructions before the ones they depend
  on. `TomasuloRob` passed anyway: it executes at commit, in program order.
- `fence_i` passes since #85. The test writes instructions to memory and runs
  them after `fence.i`.

### Earlier results

Suite at commit `bcffa2b`, minimal environment (the only one then), sequential
and `--pipeline` (`Serialized`). The two gave the same status on every test,
in every version below.

| Suite | v1.27.6 | v1.27.7 | `6bdb561` | `253d4ed` |
|---|---|---|---|---|
| `rv32ui` | 41/42 | 41/42 | 41/42 | 41/42 |
| `rv32um` | 8/8 | 8/8 | 8/8 | 8/8 |
| `rv32ua` | 10/10 | 10/10 | 10/10 | 10/10 |
| `rv32uf` | 2/11 | 2/11 | 3/11 | 11/11 |

- `fence_i` failed at case 2 in every version.
- Before `253d4ed`, F failed on the exception flags, on `fcsr` and on NaN
  handling. `253d4ed` fixed them, together with two pipeline bugs the F tests
  exposed: CSR instructions retired as no-ops, and results in `f0` were not
  forwarded.
- The v1.27.x counts also depend on this environment skipping `csrwi fcsr, 0`.
  With it, `ldst` and `recoding` stop with a fault on those versions.
- At `253d4ed`, `rv32uf` passed only 4/11 in `ParallelUFs`, because `flw` and
  `fsw` skipped memory there. `222e26c` fixed that.
