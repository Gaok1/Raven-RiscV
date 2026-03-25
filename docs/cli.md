# Raven CLI Reference

> [Leia em Português](cli.pt-BR.md)

Raven has a full headless CLI alongside the interactive TUI.
Run `raven help` at any time to see a summary.

---

## Subcommands

| Subcommand | Description |
|---|---|
| `raven build <file> [options]` | Assemble a `.fas` source file |
| `raven run <file> [options]` | Assemble and simulate |
| `raven export-config [options]` | Export the default cache config (`.fcache`) |
| `raven import-config <file> [options]` | Validate and inspect a `.fcache` file |
| `raven export-settings [options]` | Export the default sim settings (`.rcfg`) |
| `raven import-settings <file> [options]` | Validate and inspect a `.rcfg` file |
| `raven help` | Print usage summary |

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
| `--settings <file>` | built-in defaults | Load sim settings (CPI, cache_enabled) from a `.rcfg` file |
| `--mem <size>` | `16mb` | RAM size — accepts `kb`, `mb`, `gb` suffix (e.g. `256kb`, `1gb`) |
| `--max-cycles <n>` | `1000000000` | Instruction limit; a warning is printed if reached |
| `--out <file>` | stdout | Write simulation results to a file instead of stdout |
| `--nout` | — | Suppress results output entirely (program stdout still shown) |
| `--format json\|fstats\|csv` | `json` | Results format |

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

# Apply sim settings (CPI tuning, cache on/off)
raven run program.fas --settings my.rcfg --nout

# Run with 64 MB RAM
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

---

## `raven export-config`

Writes the built-in default cache configuration to a `.fcache` file so you can edit it.

```
raven export-config [--out <file>]
```

If `--out` is omitted, the config is printed to stdout.

```bash
raven export-config                        # print to stdout
raven export-config --out default.fcache   # write to file
```

See [Cache Config Reference](cache-config.md) for a full description of all `.fcache` fields.

---

## `raven import-config`

Parses and validates a `.fcache` file, prints a human-readable summary of every cache level, and optionally re-exports the normalized config.

```
raven import-config <file> [--out <file>]
```

```bash
raven import-config my.fcache
raven import-config my.fcache --out normalized.fcache
```

---

## `raven export-settings`

Writes the built-in default sim settings to a `.rcfg` file.

```
raven export-settings [--out <file>]
```

If `--out` is omitted, the settings are printed to stdout.

```bash
raven export-settings                        # print to stdout
raven export-settings --out default.rcfg     # write to file
```

---

## `raven import-settings`

Parses and validates a `.rcfg` file, prints a summary of all settings, and optionally re-exports the normalized config.

```
raven import-settings <file> [--out <file>]
```

```bash
raven import-settings my.rcfg
raven import-settings my.rcfg --out normalized.rcfg
```

---

## Config file formats

### `.fcache` — cache hardware

Describes the cache hierarchy: I-cache, D-cache, and any extra levels (L2, L3…).
See [Cache Config Reference](cache-config.md) for the full field list.

Export / import from the TUI: **Cache tab → `Ctrl+E` / `Ctrl+L`**

### `.rcfg` — sim settings

Controls global simulation parameters: CPI per instruction class and whether the cache is active.

```ini
# Raven Sim Config v1
cache_enabled=true

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
- CPI values are extra cycles added on top of cache latency for the corresponding instruction class.

Export / import from the TUI: **Config tab → `Ctrl+E` / `Ctrl+L`**

---

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Assembly error, simulation fault, or bad argument |
