use ratatui::DefaultTerminal;
use raven::{cli, ui};
use std::io;

#[cfg(unix)]
fn setup_sigint() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    use std::sync::{Arc, atomic::AtomicBool};
    let flag = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag));
    flag
}

const RAM_MIN: usize = 64 * 1024;
const RAM_MAX: usize = 4 * 1024 * 1024 * 1024;

fn parse_mem_arg(s: &str) -> Result<usize, String> {
    let s = s.trim().to_ascii_lowercase();
    let bytes = if let Some(n) = s.strip_suffix("gb") {
        n.parse::<usize>()
            .map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024 * 1024 * 1024)
            .ok_or_else(|| format!("'{s}' overflows"))?
    } else if let Some(n) = s.strip_suffix("mb") {
        n.parse::<usize>()
            .map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024 * 1024)
            .ok_or_else(|| format!("'{s}' overflows"))?
    } else if let Some(n) = s.strip_suffix("kb") {
        n.parse::<usize>()
            .map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024)
            .ok_or_else(|| format!("'{s}' overflows"))?
    } else {
        return Err(format!(
            "unknown unit in '{s}' — use kb, mb or gb (e.g. 256kb, 16mb, 1gb)"
        ));
    };
    if bytes < RAM_MIN {
        return Err(format!("minimum RAM is 64kb, got '{s}'"));
    }
    if bytes > RAM_MAX {
        return Err(format!("maximum RAM is 4gb, got '{s}'"));
    }
    Ok(bytes)
}

fn parse_max_cycles(s: &str) -> Result<u64, String> {
    s.trim()
        .parse::<u64>()
        .map_err(|_| format!("invalid --max-cycles value '{s}'"))
}

fn parse_cores_arg(s: &str) -> Result<usize, String> {
    let cores = s
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("invalid --cores value '{s}'"))?;
    if !(1..=32).contains(&cores) {
        return Err(format!("invalid --cores value '{s}' (use 1..=32)"));
    }
    Ok(cores)
}

/// Return the value of `--flag <value>` from the arg list, skipping values that look like flags.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).filter(|a| !a.starts_with('-')).cloned()
}

fn flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            if let Some(value) = args.get(i + 1).filter(|a| !a.starts_with('-')) {
                values.push(value.clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    values
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let sub = args.get(1).map(String::as_str);

    // Subcommands and legacy flags → CLI mode
    let is_cli = matches!(
        sub,
        Some("build")
        | Some("run")
        | Some("export-cache-config")
        | Some("check-cache-config")
        | Some("export-sim-settings")
        | Some("check-sim-settings")
        | Some("export-pipeline-config")
        | Some("check-pipeline-config")
        | Some("debug-run-controls")
        | Some("debug-help-layout")
        | Some("debug-pipeline-stage")
        | Some("help")
        | Some("--help") | Some("-h")
        // legacy aliases (still work, not shown in help)
        | Some("export-config") | Some("import-config")
        | Some("export-settings") | Some("import-settings")
        | Some("export-pipeline") | Some("import-pipeline")
        | Some("--run") | Some("--export-config")
    );

    if is_cli {
        let result = dispatch_subcommand(&args);
        if let Err(e) = result {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // ── TUI mode ─────────────────────────────────────────────────────────────
    #[cfg(unix)]
    let quit_flag = setup_sigint();

    let mut ram_override: Option<usize> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--mem" {
            match args.get(i + 1) {
                Some(val) => match parse_mem_arg(val) {
                    Ok(size) => {
                        ram_override = Some(size);
                        i += 2;
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return Ok(());
                    }
                },
                None => {
                    eprintln!("error: --mem requires a value (e.g. --mem 16mb)");
                    return Ok(());
                }
            }
        } else {
            i += 1;
        }
    }

    print!("\x1b[9;1t");
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let mut terminal: DefaultTerminal = ratatui::init();

    #[cfg(unix)]
    let res = ui::run(&mut terminal, ui::App::new(ram_override), quit_flag);
    #[cfg(not(unix))]
    let res = ui::run(&mut terminal, ui::App::new(ram_override));

    ratatui::restore();

    if let Err(e) = res {
        eprintln!("error: {e}");
    }
    Ok(())
}

// ── Subcommand dispatcher ────────────────────────────────────────────────────

fn dispatch_subcommand(args: &[String]) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("build") => cmd_build(&args[2..]),
        Some("run") => cmd_run(&args[2..]),

        // ── Cache config ──────────────────────────────────────────────────
        Some("export-cache-config") | Some("export-config") => cmd_export_cache_config(&args[2..]),
        Some("check-cache-config") | Some("import-config") => cmd_check_cache_config(&args[2..]),

        // ── Sim settings ──────────────────────────────────────────────────
        Some("export-sim-settings") | Some("export-settings") => cmd_export_sim_settings(&args[2..]),
        Some("check-sim-settings") | Some("import-settings") => cmd_check_sim_settings(&args[2..]),

        // ── Pipeline config ───────────────────────────────────────────────
        Some("export-pipeline-config") | Some("export-pipeline") => cmd_export_pipeline_config(&args[2..]),
        Some("check-pipeline-config") | Some("import-pipeline") => cmd_check_pipeline_config(&args[2..]),

        // ── Debug utilities ───────────────────────────────────────────────
        Some("debug-run-controls") => cmd_debug_run_controls(&args[2..]),
        Some("debug-help-layout") => cmd_debug_help_layout(&args[2..]),
        Some("debug-pipeline-stage") => cmd_debug_pipeline_stage(&args[2..]),

        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }

        // ── Legacy: raven --run <file> [old flags] ────────────────────────
        Some("--run") => {
            let file = flag_value(args, "--run").ok_or("--run requires a file path")?;
            let mut legacy: Vec<String> = vec![file];
            for flag in &[
                "--cache-config",
                "--output",
                "--format",
                "--mem",
                "--max-cycles",
            ] {
                if let Some(v) = flag_value(args, flag) {
                    // map --output → --out
                    let mapped = if *flag == "--output" { "--out" } else { flag };
                    legacy.push(mapped.to_string());
                    legacy.push(v);
                }
            }
            cmd_run(&legacy)
        }
        Some("--export-config") => {
            let out = flag_value(args, "--output").or_else(|| {
                let p = args
                    .iter()
                    .position(|a| a == "--export-config")
                    .unwrap_or(0);
                args.get(p + 1).filter(|a| !a.starts_with('-')).cloned()
            });
            let mut legacy: Vec<String> = vec![];
            if let Some(o) = out {
                legacy.push("--out".to_string());
                legacy.push(o);
            }
            cmd_export_cache_config(&legacy)
        }

        Some(other) => Err(format!(
            "unknown subcommand '{other}'\n\nRun 'raven help' for usage."
        )),
        None => unreachable!(),
    }
}

// ── raven build <file> [--out <path>] [--nout] ───────────────────────────────

fn cmd_build(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("build requires a file argument\n\nUsage: raven build <input> [output] [--nout]")?;
    let nout = has_flag(args, "--nout");
    // output: --out flag takes priority, then second positional arg
    let out = flag_value(args, "--out").or_else(|| positional_nth(args, 1));
    cli::build_program(&file, out.as_deref(), nout)
}

// ── raven run <file> [options] ───────────────────────────────────────────────

fn cmd_run(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("run requires a file argument\n\nUsage: raven run <file> [options]")?;

    let mem_size = match flag_value(args, "--mem") {
        Some(s) => Some(parse_mem_arg(&s)?),
        None => None,
    };
    let max_cycles = match flag_value(args, "--max-cycles") {
        Some(s) => parse_max_cycles(&s)?,
        None => 1_000_000_000u64,
    };
    let format = match flag_value(args, "--format").as_deref() {
        Some("fstats") => cli::OutputFormat::Fstats,
        Some("csv") => cli::OutputFormat::Csv,
        Some("json") | None => cli::OutputFormat::Json,
        Some(other) => {
            return Err(format!(
                "unknown --format '{other}' (use json, fstats, or csv)"
            ));
        }
    };
    let expect_regs = flag_values(args, "--expect-reg")
        .into_iter()
        .map(|s| cli::parse_expect_reg_spec(&s))
        .collect::<Result<Vec<_>, _>>()?;
    let expect_mems = flag_values(args, "--expect-mem")
        .into_iter()
        .map(|s| cli::parse_expect_mem_spec(&s))
        .collect::<Result<Vec<_>, _>>()?;
    let expect_exit = match flag_value(args, "--expect-exit") {
        Some(raw) => Some(
            raw.parse::<u32>()
                .map_err(|_| format!("invalid --expect-exit value '{raw}'"))?,
        ),
        None => None,
    };
    let max_cores = match flag_value(args, "--cores") {
        Some(s) => parse_cores_arg(&s)?,
        None => 0,
    };

    cli::run_headless(cli::RunArgs {
        file,
        cache_config: flag_value(args, "--cache-config"),
        settings: flag_value(args, "--sim-settings").or_else(|| flag_value(args, "--settings")),
        pipeline: has_flag(args, "--pipeline"),
        pipeline_config: flag_value(args, "--pipeline-config"),
        pipeline_trace_out: flag_value(args, "--pipeline-trace-out"),
        output: flag_value(args, "--out"),
        nout: has_flag(args, "--nout"),
        format,
        mem_size,
        max_cycles,
        max_cores,
        expect_exit,
        expect_stdout: flag_value(args, "--expect-stdout"),
        expect_regs,
        expect_mems,
    })
}

// ── raven export-cache-config [--out <file>] ─────────────────────────────────

fn cmd_export_cache_config(args: &[String]) -> Result<(), String> {
    cli::export_default_config(flag_value(args, "--out").as_deref())
}

// ── raven check-cache-config <file> [--out <file>] ───────────────────────────

fn cmd_check_cache_config(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("check-cache-config requires a file argument\n\nUsage: raven check-cache-config <file.fcache> [--out <file>]")?;
    cli::import_config(&file, flag_value(args, "--out").as_deref())
}

// ── raven export-sim-settings [--out <file>] ─────────────────────────────────

fn cmd_export_sim_settings(args: &[String]) -> Result<(), String> {
    cli::export_sim_settings(flag_value(args, "--out").as_deref())
}

// ── raven check-sim-settings <file> [--out <file>] ───────────────────────────

fn cmd_check_sim_settings(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("check-sim-settings requires a file argument\n\nUsage: raven check-sim-settings <file.rcfg> [--out <file>]")?;
    cli::import_sim_settings(&file, flag_value(args, "--out").as_deref())
}

// ── raven export-pipeline-config [--out <file>] ──────────────────────────────

fn cmd_export_pipeline_config(args: &[String]) -> Result<(), String> {
    cli::export_pipeline_settings(flag_value(args, "--out").as_deref())
}

// ── raven check-pipeline-config <file> [--out <file>] ────────────────────────

fn cmd_check_pipeline_config(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("check-pipeline-config requires a file argument\n\nUsage: raven check-pipeline-config <file.pcfg> [--out <file>]")?;
    cli::import_pipeline_settings(&file, flag_value(args, "--out").as_deref())
}

fn cmd_debug_run_controls(args: &[String]) -> Result<(), String> {
    let width = flag_value(args, "--width")
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| format!("invalid --width value '{s}'"))
        })
        .transpose()?
        .unwrap_or(160);
    let height = flag_value(args, "--height")
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| format!("invalid --height value '{s}'"))
        })
        .transpose()?
        .unwrap_or(40);
    let max_cores = flag_value(args, "--cores")
        .map(|s| parse_cores_arg(&s))
        .transpose()?
        .unwrap_or(1);
    let selected_core = flag_value(args, "--selected-core")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| format!("invalid --selected-core value '{s}'"))
        })
        .transpose()?
        .unwrap_or(0);
    let view = match flag_value(args, "--view").as_deref() {
        Some("ram") | None => ui::debug_hitboxes::DebugRunView::Ram,
        Some("regs") => ui::debug_hitboxes::DebugRunView::Regs,
        Some("dyn") => ui::debug_hitboxes::DebugRunView::Dyn,
        Some(other) => return Err(format!("unknown --view '{other}' (use ram, regs, or dyn)")),
    };
    let text =
        ui::debug_hitboxes::debug_run_controls_dump(ui::debug_hitboxes::DebugRunControlsOptions {
            width,
            height,
            running: has_flag(args, "--running"),
            selected_core,
            max_cores,
            view,
        });
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, text).map_err(|e| format!("Cannot write '{}': {e}", path))?;
    } else {
        print!("{text}");
    }
    Ok(())
}

fn cmd_debug_pipeline_stage(args: &[String]) -> Result<(), String> {
    let width = flag_value(args, "--width")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| format!("invalid --width value '{s}'"))
        })
        .transpose()?
        .unwrap_or(24);
    let stage = flag_value(args, "--stage").unwrap_or_else(|| "EX".to_string());
    let disasm = flag_value(args, "--disasm").unwrap_or_else(|| "addi t4, t4, 1".to_string());
    let badges = flag_value(args, "--badges")
        .unwrap_or_else(|| "LOAD,RAW,FWD".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let predicted_badge = flag_value(args, "--pred");
    let text = ui::debug_hitboxes::debug_pipeline_stage_dump(
        ui::debug_hitboxes::DebugPipelineStageOptions {
            width,
            stage,
            disasm,
            badges,
            predicted_badge,
        },
    );
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, text).map_err(|e| format!("Cannot write '{}': {e}", path))?;
    } else {
        print!("{text}");
    }
    Ok(())
}

fn cmd_debug_help_layout(args: &[String]) -> Result<(), String> {
    let width = flag_value(args, "--width")
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| format!("invalid --width value '{s}'"))
        })
        .transpose()?
        .unwrap_or(160);
    let height = flag_value(args, "--height")
        .map(|s| {
            s.parse::<u16>()
                .map_err(|_| format!("invalid --height value '{s}'"))
        })
        .transpose()?
        .unwrap_or(40);
    let tab = match flag_value(args, "--tab").as_deref() {
        Some("editor") | None => ui::debug_hitboxes::DebugUiTab::Editor,
        Some("run") => ui::debug_hitboxes::DebugUiTab::Run,
        Some("cache") => ui::debug_hitboxes::DebugUiTab::Cache,
        Some("pipeline") => ui::debug_hitboxes::DebugUiTab::Pipeline,
        Some("docs") => ui::debug_hitboxes::DebugUiTab::Docs,
        Some("config") => ui::debug_hitboxes::DebugUiTab::Config,
        Some(other) => {
            return Err(format!(
                "unknown --tab '{other}' (use editor, run, cache, pipeline, docs, config)"
            ));
        }
    };
    let text =
        ui::debug_hitboxes::debug_help_layout_dump(ui::debug_hitboxes::DebugHelpLayoutOptions {
            width,
            height,
            tab,
        });
    if let Some(path) = flag_value(args, "--out") {
        std::fs::write(&path, text).map_err(|e| format!("Cannot write '{}': {e}", path))?;
    } else {
        print!("{text}");
    }
    Ok(())
}

// ── Help ─────────────────────────────────────────────────────────────────────

fn print_help() {
    eprintln!(
        r#"raven — RISC-V (RV32IM) simulator and assembler

USAGE:
    raven                                          Open interactive TUI
    raven build  <file> [OPTIONS]                  Assemble source file
    raven run    <file> [OPTIONS]                  Assemble and simulate
    raven export-cache-config [OPTIONS]            Export default cache config (.fcache)
    raven check-cache-config  <file> [OPTIONS]     Validate and inspect a .fcache file
    raven export-sim-settings [OPTIONS]            Export default sim settings (.rcfg)
    raven check-sim-settings  <file> [OPTIONS]     Validate and inspect a .rcfg file
    raven export-pipeline-config [OPTIONS]         Export default pipeline config (.pcfg)
    raven check-pipeline-config  <file> [OPTIONS]  Validate and inspect a .pcfg file
    raven debug-run-controls [OPTIONS]             Dump Run Controls hitboxes for debugging
    raven debug-help-layout [OPTIONS]              Dump help button / popup layout for a tab
    raven debug-pipeline-stage [OPTIONS]           Dump a pipeline stage line preview

OPTIONS  build:
    [output]                    Output .bin file as second positional arg
    --out <path>                Same as above (takes priority over positional)
    --nout                      Check-only: assemble but don't write any file

OPTIONS  run:
    --cache-config <file>       Load cache hierarchy from .fcache
    --sim-settings <file>       Load sim settings (CPI, memory, cache_enabled) from .rcfg
    --pipeline                  Run using the pipeline simulator instead of sequential exec
    --pipeline-config <file>    Load pipeline config from a .pcfg file
    --pipeline-trace-out <file> Write per-cycle pipeline trace JSON (requires --pipeline)
    --cores <n>                 Max physical cores / harts for the run (default: settings or 1)
    --mem <size>                RAM size, e.g. 16mb, 256kb, 1gb   (default: sim-settings or 16mb)
    --max-cycles <n>            Instruction limit                  (default: 1000000000)
    --expect-exit <code>        Fail if the final exit code differs
    --expect-stdout <text>      Fail if captured stdout differs exactly
    --expect-reg <reg=value>    Fail if a register differs; repeatable
    --expect-mem <addr=value>   Fail if a 32-bit memory word differs; repeatable
    --out <file>                Write simulation results to file
    --nout                      Don't write/print results (program output only)
    --format json|fstats|csv    Results format                     (default: json)

OPTIONS  export-cache-config / export-sim-settings / export-pipeline-config:
    --out <file>                Write to file instead of stdout

OPTIONS  check-cache-config / check-sim-settings / check-pipeline-config:
    --out <file>                Re-export normalized config to this file

OPTIONS  debug-run-controls:
    --width <n>                 Virtual UI width for the dump          (default: 160)
    --height <n>                Virtual UI height for the dump         (default: 40)
    --cores <n>                 Simulated max core count               (default: 1)
    --selected-core <n>         Selected core index                    (default: 0)
    --view ram|regs|dyn         Run sidebar mode                       (default: ram)
    --running                   Render State as RUN
    --out <file>                Write dump to file instead of stdout

OPTIONS  debug-pipeline-stage:
    --width <n>                 Virtual stage inner width              (default: 24)
    --stage <name>              Stage label                            (default: EX)
    --disasm <text>             Disassembly preview text
    --badges <csv>              Badge list, e.g. LOAD,RAW,FWD
    --pred <text>               Optional speculative badge text
    --out <file>                Write dump to file instead of stdout

OPTIONS  debug-help-layout:
    --width <n>                 Virtual UI width for the dump          (default: 160)
    --height <n>                Virtual UI height for the dump         (default: 40)
    --tab editor|run|cache|pipeline|docs|config
                                Tab to inspect                         (default: editor)
    --out <file>                Write dump to file instead of stdout

EXAMPLES:
    raven build program.fas
    raven build program.fas out/prog.bin
    raven build program.fas --nout
    raven run program.fas --nout
    raven run program.fas --out results.json
    raven run program.fas --cache-config l2.fcache --format csv --out stats.csv
    raven run program.fas --sim-settings my.rcfg --nout
    raven run program.fas --cores 4 --nout
    raven run program.fas --pipeline --pipeline-config mypipe.pcfg --format json
    raven run program.fas --expect-exit 0 --expect-reg a0=42
    raven run program.fas --pipeline --pipeline-trace-out trace.json
    raven export-cache-config --out default.fcache
    raven check-cache-config my.fcache
    raven check-cache-config my.fcache --out normalized.fcache
    raven export-sim-settings --out default.rcfg
    raven check-sim-settings my.rcfg
    raven check-sim-settings my.rcfg --out normalized.rcfg
    raven export-pipeline-config --out default.pcfg
    raven check-pipeline-config my.pcfg
    raven check-pipeline-config my.pcfg --out normalized.pcfg
    raven debug-run-controls --cores 4 --selected-core 2 --view dyn --out run-controls.txt
    raven debug-help-layout --tab cache
    raven debug-pipeline-stage --width 24 --disasm "addi t4, t4, 1" --badges LOAD,RAW,FWD
"#
    );
}

// ── Arg helpers ───────────────────────────────────────────────────────────────

/// First non-flag argument (positional).
fn positional(args: &[String]) -> Option<String> {
    positional_nth(args, 0)
}

/// Nth non-flag argument (0-based).
fn positional_nth(args: &[String], n: usize) -> Option<String> {
    args.iter().filter(|a| !a.starts_with('-')).nth(n).cloned()
}
