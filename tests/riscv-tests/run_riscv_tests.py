#!/usr/bin/env python3
"""
Run the official riscv-tests suite (github.com/riscv-software-src/riscv-tests)
against one or more RAVEN binaries.

Steps
  1. Build each rv32ui/um/ua/uf test with clang (riscv32 target) and the
     linker script env/link.ld, in the environment --env names:
       p      the suite's own env/p/riscv_test.h (default). It sets up the
              trap vector, PMP, satp and delegation, enables F and enters
              the test in U-mode through mret. A test ends in the exit
              syscall; an unexpected trap reports through tohost (HTIF).
       raven  the minimal env/riscv_test.h next to this script, which runs
              the test body straight from _start, with no setup.
     The suite's .S files are not modified.
  2. Run each ELF with `raven run <elf> --format json`, sequentially and with
     `--pipeline`, on every RAVEN binary given. With --modes, also in the
     ParallelUFs, Scoreboard and TomasuloRob pipeline modes, each through a
     --config file made with `raven export-config`.
  3. Classify: PASS (exit 0), FAIL case N (odd exit = (N << 1) | 1),
     NO_EXIT (ended without calling exit, e.g. cycle limit), ERROR (RAVEN
     rejected the ELF or aborted).

Usage
  python run_riscv_tests.py --suite path/to/riscv-tests \
      --raven v1.27.6=path/to/raven.exe --raven HEAD=path/to/raven.exe \
      [--env p|raven] [--modes]
Output: build/<env>/*.elf, results-<env>.csv, summary-<env>.md (next to
this script)

Diagnostic mode (--diag-no-flags) is NOT the official result. It runs rv32uf
only, with a copy of test_macros.h whose exception-flag (fflags) check is
neutralised. It separates "wrong numeric result" from "missing exception
flag". Output: build-diag/<env>/, results-diag-<env>.csv,
summary-diag-<env>.md.
"""
import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SUITES = ["rv32ui", "rv32um", "rv32ua", "rv32uf"]
# amocas_* belong to the Zacas extension, not to A.
OUT_OF_SCOPE = {"rv32ua": {"amocas_d", "amocas_w"}}
MARCH = "rv32imaf_zicsr_zifencei"
MAX_CYCLES = "5000000"
# `--pipeline` alone runs Serialized; the other modes need a --config file.
DEFAULT_MODE = "Serialized"
OTHER_MODES = ["ParallelUFs", "Scoreboard", "TomasuloRob"]


def list_tests(suite_dir, suite):
    text = (suite_dir / "isa" / suite / "Makefrag").read_text()
    m = re.search(rf"{suite}_sc_tests\s*=\s*\\?\s*(.*?)\n\s*\n", text, re.S)
    names = m.group(1).replace("\\", " ").split()
    return [n for n in names if n not in OUT_OF_SCOPE.get(suite, set())]


def macros_without_flags(suite_dir, dest):
    """Copy of test_macros.h with the fflags check neutralised."""
    text = (suite_dir / "isa" / "macros" / "scalar" / "test_macros.h").read_text()
    n1 = text.count("fsflags a1, x0;")
    n2 = text.count("li a2, flags;")
    text = text.replace("fsflags a1, x0;", "li a1, 0;").replace("li a2, flags;", "li a2, 0;")
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "test_macros.h").write_text(text)
    print(f"diagnostic: neutralised {n1} fflags reads and {n2} expected values")
    return dest


def env_include(suite_dir, env):
    """Directory holding the riscv_test.h of environment `env`."""
    return suite_dir / "env" / "p" if env == "p" else HERE / "env"


def build_test(suite_dir, suite, name, build, macros, env):
    src = suite_dir / "isa" / suite / f"{name}.S"
    elf = build / f"{suite}-{env}-{name}.elf"
    cmd = [
        "clang", "--target=riscv32-unknown-elf", f"-march={MARCH}", "-mabi=ilp32",
        "-mno-relax", "-nostdlib", "-static", "-fuse-ld=lld",
        f"-Wl,-T,{HERE / 'env' / 'link.ld'}",
        f"-I{env_include(suite_dir, env)}", f"-I{macros}",
        str(src), "-o", str(elf),
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return elf if r.returncode == 0 else None, r.stderr.strip()


def mode_configs(raven, label, dest, modes):
    """One --config file per pipeline mode in `modes`, for binary `raven`:
    `raven export-config` with the [pipeline] section's mode= line changed.
    The default mode needs none."""
    configs = {DEFAULT_MODE: None}
    if not modes:
        return configs
    r = subprocess.run([raven, "export-config"], capture_output=True, text=True)
    base = r.stdout
    if r.returncode != 0 or not re.search(r"^mode=", base, re.M):
        sys.exit(f"{label}: `export-config` did not print a [pipeline] mode= line")
    dest.mkdir(parents=True, exist_ok=True)
    for mode in modes:
        path = dest / f"{label}-{mode}.rcfg"
        path.write_text(re.sub(r"^mode=.*$", f"mode={mode}", base, count=1, flags=re.M))
        configs[mode] = path
    return configs


def run_test(raven, elf, pipeline, config=None):
    cmd = [raven, "run", str(elf), "--format", "json", "--max-cycles", MAX_CYCLES]
    if pipeline:
        cmd.append("--pipeline")
    if config is not None:
        cmd += ["--config", str(config)]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    except subprocess.TimeoutExpired:
        return "NO_EXIT", "120 s timeout"
    m = re.search(r'"exit_code":\s*(null|\d+)', r.stdout)
    if not m:
        msg = (r.stderr or r.stdout).strip().splitlines()
        return "ERROR", msg[0][:160] if msg else f"return code {r.returncode}"
    if m.group(1) == "null":
        err = r.stderr.strip().splitlines()
        return "NO_EXIT", err[-1][:160] if err else ""
    code = int(m.group(1))
    if code == 0:
        return "PASS", ""
    if "(tohost)" in r.stderr:
        # env/p's trap vector: an exception the test did not expect.
        return "FAIL", f"unexpected trap, tohost exit {code}"
    return "FAIL", f"case {code >> 1}" if code & 1 else f"exit {code}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--suite", required=True, type=Path)
    ap.add_argument("--raven", action="append", required=True,
                    help="label=path to the executable")
    ap.add_argument("--env", choices=["p", "raven"], default="p",
                    help="test environment: the suite's env/p (default) or "
                         "the minimal one in env/")
    ap.add_argument("--modes", action="store_true",
                    help="also run the ParallelUFs, Scoreboard and TomasuloRob "
                         "pipeline modes")
    ap.add_argument("--diag-no-flags", action="store_true")
    a = ap.parse_args()

    diag = "-diag" if a.diag_no_flags else ""
    suffix = f"{diag}-{a.env}"
    suites = ["rv32uf"] if a.diag_no_flags else SUITES
    build = HERE / f"build{diag}" / a.env
    build.mkdir(parents=True, exist_ok=True)
    macros = (macros_without_flags(a.suite, build / "macros") if a.diag_no_flags
              else a.suite / "isa" / "macros" / "scalar")
    ravens = [x.split("=", 1) for x in a.raven]
    commit = subprocess.run(["git", "-C", str(a.suite), "log", "-1", "--format=%h %ad"],
                            capture_output=True, text=True).stdout.strip()
    # Each run is (label, pipeline mode or None for sequential).
    runs = ["seq", DEFAULT_MODE] + (OTHER_MODES if a.modes else [])
    configs = {label: mode_configs(exe, label, build / "config",
                                   OTHER_MODES if a.modes else [])
               for label, exe in ravens}

    rows = []
    for suite in suites:
        for name in list_tests(a.suite, suite):
            elf, err = build_test(a.suite, suite, name, build, macros, a.env)
            base = {"suite": suite, "test": name, "env": a.env}
            if elf is None:
                for label, _ in ravens:
                    for mode in runs:
                        rows.append({**base, "raven": label, "mode": mode,
                                     "status": "BUILD_FAILED", "detail": err[:160]})
                continue
            for label, exe in ravens:
                for mode in runs:
                    if mode == "seq":
                        st, det = run_test(exe, elf, False)
                    else:
                        st, det = run_test(exe, elf, True, configs[label][mode])
                    rows.append({**base, "raven": label, "mode": mode,
                                 "status": st, "detail": det})
                    print(f"{label:<8} {mode:<11} {suite}-{a.env}-{name:<10} {st} {det}")

    with open(HERE / f"results{suffix}.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)

    title = ("# DIAGNOSTIC (not official): rv32uf without the fflags check\n"
             if a.diag_no_flags else "# riscv-tests on RAVEN\n")
    out = [title,
           f"Suite: riscv-software-src/riscv-tests @ {commit}; env: {a.env}; "
           f"-march={MARCH}; "
           f"out of scope: {sorted(set().union(*OUT_OF_SCOPE.values()))}\n",
           "| RAVEN | mode | suite | PASS | total | not passed |",
           "|---|---|---|---|---|---|"]
    for label, _ in ravens:
        for mode in runs:
            tot_p = tot = 0
            for suite in suites:
                sel = [r for r in rows if r["raven"] == label and r["mode"] == mode
                       and r["suite"] == suite]
                p = sum(r["status"] == "PASS" for r in sel)
                failed = ", ".join(f"{r['test']} ({r['status']}"
                                   + (f": {r['detail']}" if r["detail"] else "") + ")"
                                   for r in sel if r["status"] != "PASS")
                out.append(f"| {label} | {mode} | {suite} | {p} | {len(sel)} | {failed} |")
                tot_p += p
                tot += len(sel)
            out.append(f"| {label} | {mode} | **total** | **{tot_p}** | **{tot}** | |")
    (HERE / f"summary{suffix}.md").write_text("\n".join(out) + "\n", encoding="utf-8")
    print("\n".join(out))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
