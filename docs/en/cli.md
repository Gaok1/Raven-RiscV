# Raven CLI Reference

> [Leia em Português](../pt-BR/cli.md)

Raven has a full headless CLI alongside the interactive TUI.
Run `raven help` at any time to see a summary.

---

## Subcommands

| Subcommand | Description |
|---|---|
| `raven build <file> [options]` | Assemble a `.fas` source file |
| `raven run <file> [options]` | Assemble and simulate |
| `raven export-cache-config [options]` | Export the default cache config (`.fcache`) |
| `raven check-cache-config <file> [options]` | Validate and inspect a `.fcache` file |
| `raven export-sim-settings [options]` | Export the default sim settings (`.rcfg`) |
| `raven check-sim-settings <file> [options]` | Validate and inspect a `.rcfg` file |
| `raven export-pipeline-config [options]` | Export the default pipeline config (`.pcfg`) |
| `raven check-pipeline-config <file> [options]` | Validate and inspect a `.pcfg` file |
| `raven debug-run-controls [options]` | Dump Run Controls text and hitboxes for hover debugging |
| `raven debug-help-layout [options]` | Dump help button / popup layout for a tab |
| `raven debug-pipeline-stage [options]` | Dump a pipeline stage line preview for layout debugging |
| `raven help` | Print usage summary |

> **Legacy aliases** — the old subcommand names (`export-config`, `import-config`, `export-settings`, `import-settings`, `export-pipeline`, `import-pipeline`) still work but are no longer shown in help output.

---

## `raven build`

Assembles a `.fas` source file and writes a FALC binary (`.bin`).

```
raven build <input> [output] [options]
```

| Argument / Flag | Description |
|---|---|
| `<input>` | Path to the `.fas` source file (required) |
| `[output]` | Output path for the `.bin` file (second positional arg) |
| `--out <path>` | Same as above; takes priority over the positional arg |
| `--nout` | Check-only — assemble but write no output file |

**Examples**

```bash
# Assemble and write program.bin
raven build program.fas

# Write to a custom path
raven build program.fas out/prog.bin
raven build program.fas --out out/prog.bin

# Syntax-check only, no output
raven build program.fas --nout
```

On success, Raven prints the instruction count and data size to stderr.
On error, it prints the offending line number and message, then exits with code 1.

---

## `raven run`

Assembles and simulates a program. Accepts `.fas` source, FALC `.bin`, or ELF32 RISC-V binaries.

```
raven run <file> [options]
```

| Flag | Default | Description |
|---|---|---|
| `--cache-config <file>` | built-in defaults | Load cache hierarchy from a `.fcache` file |
| `--sim-settings <file>` | built-in defaults | Load sim settings (CPI, memory, cache_enabled) from a `.rcfg` file |
| `--pipeline` | off | Run with the pipeline simulator instead of the sequential executor |
| `--pipeline-config <file>` | built-in defaults | Load pipeline behavior from a `.pcfg` file |
| `--pipeline-trace-out <file>` | off | Write a per-cycle pipeline trace JSON file; requires `--pipeline` |
| `--cores <n>` | settings or `1` | Maximum physical cores available to `hart_start` during the run |
| `--mem <size>` | sim-settings or `16mb` | RAM size — accepts `kb`, `mb`, `gb` suffix (e.g. `256kb`, `1gb`) |
| `--max-cycles <n>` | `1000000000` | Instruction limit; a warning is printed if reached |
| `--expect-exit <code>` | off | Fail if the final exit code differs |
| `--expect-stdout <text>` | off | Fail if captured stdout differs exactly |
| `--expect-reg <reg=value>` | off | Assert a final integer register value; repeatable |
| `--expect-mem <addr=value>` | off | Assert a final 32-bit memory word; repeatable |
| `--out <file>` | stdout | Write simulation results to a file instead of stdout |
| `--nout` | — | Suppress results output entirely (program stdout still shown) |
| `--format json\|fstats\|csv` | `json` | Results format |

> `--mem` takes priority over the `mem_mb` value in `.rcfg`. If neither is given, the default is `16mb`.

**Examples**

```bash
# Run with defaults, print JSON stats to stdout
raven run program.fas

# Run without printing stats
raven run program.fas --nout

# Write stats to a file
raven run program.fas --out results.json

# Use a custom cache config and write CSV stats
raven run program.fas --cache-config l2.fcache --format csv --out stats.csv

# Apply sim settings (CPI tuning, memory size, cache on/off)
raven run program.fas --sim-settings my.rcfg --nout

# Run through the pipeline simulator with an explicit pipeline config
raven run program.fas --pipeline --pipeline-config mypipe.pcfg --format json

# Assert the final program state
raven run program.fas --expect-exit 0 --expect-reg a0=42 --expect-mem 0x1000=0x2a

# Emit a cycle-by-cycle pipeline trace
raven run program.fas --pipeline --pipeline-trace-out trace.json --nout

# Allow up to 4 cores for multi-hart programs
raven run program.fas --cores 4 --nout

# Run with 64 MB RAM (overrides sim-settings)
raven run program.fas --mem 64mb

# Run a pre-assembled binary or ELF
raven run prog.bin
raven run target/riscv32im-unknown-none-elf/debug/my_crate
```

**Interactive input**

If the program reads from stdin (syscalls 3 / 1003), `raven run` reads from the terminal interactively — any pending output is flushed before the prompt so the user sees it. Pipe or redirect stdin as usual:

```bash
echo "hello" | raven run io_echo.fas --nout
printf "42\n" | raven run calculator.fas --nout
```

**Output formats**

| Format | Description |
|---|---|
| `json` | Machine-readable JSON with all stats |
| `fstats` | Human-readable table (`.fstats`) |
| `csv` | Spreadsheet-friendly CSV |

When `--pipeline` is enabled, Raven still writes the normal cache statistics, but also includes a pipeline summary:

- committed instructions
- pipeline cycles
- stall count
- flush count
- pipeline CPI

### Assertions

The `--expect-*` flags turn `raven run` into a regression-friendly CLI.
If any assertion fails, Raven exits with code `1`.

- `--expect-exit <code>` compares against the final syscall exit code.
- `--expect-stdout <text>` compares against the program's full captured stdout.
- `--expect-reg <reg=value>` compares the final integer register value.
- `--expect-mem <addr=value>` compares a final 32-bit word in memory.

Values accept decimal or hexadecimal (`0x...`) syntax.
Registers use the normal integer aliases, such as `a0`, `sp`, `t3`, or `x10`.

### Pipeline trace JSON

`--pipeline-trace-out <file>` writes a structured per-cycle trace that records:

- current cycle
- committed instruction PC/class
- fetch PC
- stage occupancy (`IF`, `ID`, `EX`, `MEM`, `WB`)
- speculation / stall metadata on each stage
- hazard and forwarding traces for that cycle

This option is only valid together with `--pipeline`.

---

## `raven export-cache-config`

Writes the built-in default cache configuration to a `.fcache` file so you can edit it.

```
raven export-cache-config [--out <file>]
```

If `--out` is omitted, the config is printed to stdout.

```bash
raven export-cache-config                        # print to stdout
raven export-cache-config --out default.fcache   # write to file
```

See [Cache Config Reference](cache-config.md) for a full description of all `.fcache` fields.

---

## `raven check-cache-config`

Parses and validates a `.fcache` file, prints a human-readable summary of every cache level, and optionally re-exports the normalized config.

```
raven check-cache-config <file> [--out <file>]
```

```bash
raven check-cache-config my.fcache
raven check-cache-config my.fcache --out normalized.fcache
```

---

## `raven export-sim-settings`

Writes the built-in default sim settings to a `.rcfg` file.

```
raven export-sim-settings [--out <file>]
```

If `--out` is omitted, the settings are printed to stdout.

```bash
raven export-sim-settings                        # print to stdout
raven export-sim-settings --out default.rcfg     # write to file
```

---

## `raven check-sim-settings`

Parses and validates a `.rcfg` file, prints a summary of all settings, and optionally re-exports the normalized config.

```
raven check-sim-settings <file> [--out <file>]
```

```bash
raven check-sim-settings my.rcfg
raven check-sim-settings my.rcfg --out normalized.rcfg
```

---

## `raven export-pipeline-config`

Writes the built-in default pipeline configuration to a `.pcfg` file.

```
raven export-pipeline-config [--out <file>]
```

If `--out` is omitted, the config is printed to stdout.

```bash
raven export-pipeline-config
raven export-pipeline-config --out default.pcfg
```

---

## `raven check-pipeline-config`

Parses and validates a `.pcfg` file, prints a summary of the pipeline settings, and optionally re-exports the normalized config.

```
raven check-pipeline-config <file> [--out <file>]
```

```bash
raven check-pipeline-config my.pcfg
raven check-pipeline-config my.pcfg --out normalized.pcfg
```

---

## `raven debug-run-controls`

Dumps the current `Run Controls` text line and the hover/click hitbox column ranges that the mouse handler sees.
This is useful when visual offsets appear between the rendered controls and the hover logic.

```
raven debug-run-controls [options]
```

| Flag | Default | Description |
|---|---|---|
| `--width <n>` | `160` | Virtual UI width for the dump |
| `--height <n>` | `40` | Virtual UI height for the dump |
| `--cores <n>` | `1` | Simulated max core count |
| `--selected-core <n>` | `0` | Selected core index |
| `--view ram\|regs\|dyn` | `ram` | Run sidebar mode |
| `--running` | off | Render state as RUN |
| `--out <file>` | stdout | Write dump to file |

```bash
raven debug-run-controls
raven debug-run-controls --cores 4 --selected-core 2 --view dyn
raven debug-run-controls --running --out run-controls.txt
```

---

## `raven debug-help-layout`

Dumps the help button and popup layout for a given UI tab. Useful for verifying that key-hint positions match what the TUI actually renders at a given terminal size.

```
raven debug-help-layout [options]
```

| Flag | Default | Description |
|---|---|---|
| `--width <n>` | `160` | Virtual UI width for the dump |
| `--height <n>` | `40` | Virtual UI height for the dump |
| `--tab editor\|run\|cache\|pipeline\|docs\|config` | `editor` | Tab to inspect |
| `--out <file>` | stdout | Write dump to file |

```bash
raven debug-help-layout
raven debug-help-layout --tab cache
raven debug-help-layout --tab pipeline --width 120 --height 30
```

---

## `raven debug-pipeline-stage`

Dumps a pipeline stage line preview. Useful for verifying that badge layout and disassembly truncation look correct at a given stage width.

```
raven debug-pipeline-stage [options]
```

| Flag | Default | Description |
|---|---|---|
| `--width <n>` | `24` | Virtual stage inner width |
| `--stage <name>` | `EX` | Stage label |
| `--disasm <text>` | `addi t4, t4, 1` | Disassembly preview text |
| `--badges <csv>` | `LOAD,RAW,FWD` | Badge list |
| `--pred <text>` | — | Optional speculative badge text |
| `--out <file>` | stdout | Write dump to file |

```bash
raven debug-pipeline-stage
raven debug-pipeline-stage --width 24 --disasm "addi t4, t4, 1" --badges LOAD,RAW,FWD
raven debug-pipeline-stage --stage MEM --pred SPEC
```

---

## Config file formats

### `.fcache` — cache hardware

Describes the cache hierarchy: I-cache, D-cache, and any extra levels (L2, L3…).
See [Cache Config Reference](cache-config.md) for the full field list.

Export / import from the TUI: **Cache tab → `Ctrl+E` / `Ctrl+L`**

### `.rcfg` — sim settings

Controls global simulation parameters: CPI per instruction class, whether the cache is active, the default RAM size, and the default number of available cores.

```ini
# Raven Sim Config v1
cache_enabled=true
max_cores=1
mem_mb=16

# CPI (cycles per instruction)
cpi.alu=1
cpi.mul=3
cpi.div=20
cpi.load=0
cpi.store=0
cpi.branch_taken=3
cpi.branch_not_taken=1
cpi.jump=2
cpi.system=10
cpi.fp=5
```

- `cache_enabled=false` bypasses the entire cache hierarchy (all accesses go directly to RAM).
- `max_cores` defaults to `1` when omitted and should stay in the range `1..=8`.
- `mem_mb` sets the default RAM size in megabytes (must be a power of 2, e.g. `16`, `64`, `128`). The `--mem` CLI flag overrides this value.
- Headless `--pipeline` currently supports only `--cores 1`.
- CPI values are extra cycles added on top of cache latency for the corresponding instruction class.

Export / import from the TUI: **Config tab → `Ctrl+E` / `Ctrl+L`**

### `.pcfg` — pipeline settings

Controls pipeline-specific behavior used by the TUI pipeline tab and by `raven run --pipeline`.

```ini
# Raven Pipeline Config v1
enabled=true
forwarding=true
mode=SingleCycle
branch_resolve=Ex
predict=NotTaken
speed=Normal
```

Fields:

- `enabled` — pipeline enabled in the TUI
- `forwarding` — enable bypass/forwarding paths
- `mode` — `SingleCycle` or `FunctionalUnits`
- `branch_resolve` — `Id`, `Ex`, or `Mem`
- `predict` — `NotTaken` or `Taken`
- `speed` — TUI playback speed (`Slow`, `Normal`, `Fast`, `Instant`)

Export / import from the TUI: **Pipeline tab → `Ctrl+E` / `Ctrl+L`**

---

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Assembly error, simulation fault, or bad argument |
