# riscv-tests on RAVEN

This folder runs the official [riscv-tests](https://github.com/riscv-software-src/riscv-tests)
suite on RAVEN. It covers `rv32ui`, `rv32um`, `rv32ua` and `rv32uf`: 71 tests.
Each test runs twice, in sequential mode and with `--pipeline`.

## Files

| File | What it is |
|---|---|
| `run_riscv_tests.py` | Builds each test with clang and runs each ELF on one or more RAVEN binaries. |
| `env/riscv_test.h` | Minimal test environment. It replaces the official `env/p`. |
| `env/link.ld` | Links at `0x10000`. RAVEN's RAM starts at 0 and is 16 MiB by default. |

## How the environment differs from `env/p`

Among other steps, the official `p` environment sets up the trap vector, PMP,
`satp` and delegation, enables F, clears `fcsr` and enters the test through
`mret`. It reports the result through `tohost` (HTIF). RAVEN has no HTIF host,
so `env/riscv_test.h`:

- starts straight in the test body, with no initialisation and no privileged
  transition (`fcsr` is not cleared either);
- keeps `RVTEST_PASS` and `RVTEST_FAIL` identical to the official macros. They
  end in the exit syscall (`ecall`, a7 = 93): a pass is `exit(0)`, a failure is
  `exit((case << 1) | 1)`.

The suite's `.S` files and `test_macros.h` are not modified. The results cover
the instructions in the test bodies, not privileged behaviour.

`amocas_w` and `amocas_d` sit in `rv32ua` but belong to Zacas, not to A. They
are left out.

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
```

On Windows the binary is `target/release/raven.exe`. Repeat
`--raven label=path` to compare several binaries in one run. The script
writes `build/` (the ELFs), `results.csv` and `summary.md` next to itself.

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
from missing exception flags. Its output goes to `build-diag/`,
`results-diag.csv` and `summary-diag.md`.

## Results

Suite at commit `bcffa2b`. Sequential and pipeline modes give the same status
on every test, in every version below.

| Suite | v1.27.6 | v1.27.7 | `6bdb561` | `253d4ed` |
|---|---|---|---|---|
| `rv32ui` | 41/42 | 41/42 | 41/42 | 41/42 |
| `rv32um` | 8/8 | 8/8 | 8/8 | 8/8 |
| `rv32ua` | 10/10 | 10/10 | 10/10 | 10/10 |
| `rv32uf` | 2/11 | 2/11 | 3/11 | 11/11 |

- `fence_i` fails at case 2 in every version. The test writes instructions to
  memory and runs them after `fence.i`.
- Before `253d4ed`, F failed on the exception flags, on `fcsr` and on NaN
  handling. `253d4ed` fixed them, together with two pipeline bugs the F tests
  exposed: CSR instructions retired as no-ops, and results in `f0` were not
  forwarded.
- The v1.27.x counts also depend on this environment skipping `csrwi fcsr, 0`.
  With it, `ldst` and `recoding` stop with a fault on those versions.
