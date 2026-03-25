mod falcon;
mod ui;
mod cli;

use ratatui::DefaultTerminal;
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
        n.parse::<usize>().map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024 * 1024 * 1024).ok_or_else(|| format!("'{s}' overflows"))?
    } else if let Some(n) = s.strip_suffix("mb") {
        n.parse::<usize>().map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024 * 1024).ok_or_else(|| format!("'{s}' overflows"))?
    } else if let Some(n) = s.strip_suffix("kb") {
        n.parse::<usize>().map_err(|_| format!("invalid number in '{s}'"))?
            .checked_mul(1024).ok_or_else(|| format!("'{s}' overflows"))?
    } else {
        return Err(format!("unknown unit in '{s}' — use kb, mb or gb (e.g. 256kb, 16mb, 1gb)"));
    };
    if bytes < RAM_MIN { return Err(format!("minimum RAM is 64kb, got '{s}'")); }
    if bytes > RAM_MAX { return Err(format!("maximum RAM is 4gb, got '{s}'")); }
    Ok(bytes)
}

fn parse_max_cycles(s: &str) -> Result<u64, String> {
    s.trim().parse::<u64>().map_err(|_| format!("invalid --max-cycles value '{s}'"))
}

/// Return the value of `--flag <value>` from the arg list, skipping values that look like flags.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).filter(|a| !a.starts_with('-')).cloned()
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
        | Some("export-config")
        | Some("import-config")
        | Some("export-settings")
        | Some("import-settings")
        | Some("help")
        | Some("--help") | Some("-h")
        | Some("--run")         // legacy
        | Some("--export-config") // legacy
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
                    Ok(size) => { ram_override = Some(size); i += 2; }
                    Err(e)   => { eprintln!("error: {e}"); return Ok(()); }
                },
                None => { eprintln!("error: --mem requires a value (e.g. --mem 16mb)"); return Ok(()); }
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

    if let Err(e) = res { eprintln!("error: {e}"); }
    Ok(())
}

// ── Subcommand dispatcher ────────────────────────────────────────────────────

fn dispatch_subcommand(args: &[String]) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("build")         => cmd_build(&args[2..]),
        Some("run")           => cmd_run(&args[2..]),
        Some("export-config")   => cmd_export_config(&args[2..]),
        Some("import-config")   => cmd_import_config(&args[2..]),
        Some("export-settings") => cmd_export_settings(&args[2..]),
        Some("import-settings") => cmd_import_settings(&args[2..]),
        Some("help") | Some("--help") | Some("-h") => { print_help(); Ok(()) }

        // ── Legacy: raven --run <file> [old flags] ────────────────────────
        Some("--run") => {
            let file = flag_value(args, "--run")
                .ok_or("--run requires a file path")?;
            let mut legacy: Vec<String> = vec![file];
            for flag in &["--cache-config", "--output", "--format", "--mem", "--max-cycles"] {
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
            let out = flag_value(args, "--output")
                .or_else(|| { let p = args.iter().position(|a| a == "--export-config").unwrap_or(0); args.get(p + 1).filter(|a| !a.starts_with('-')).cloned() });
            let mut legacy: Vec<String> = vec![];
            if let Some(o) = out { legacy.push("--out".to_string()); legacy.push(o); }
            cmd_export_config(&legacy)
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
    let out = flag_value(args, "--out")
        .or_else(|| positional_nth(args, 1));
    cli::build_program(&file, out.as_deref(), nout)
}

// ── raven run <file> [options] ───────────────────────────────────────────────

fn cmd_run(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("run requires a file argument\n\nUsage: raven run <file> [options]")?;

    let mem_size = match flag_value(args, "--mem") {
        Some(s) => parse_mem_arg(&s)?,
        None    => 16 * 1024 * 1024,
    };
    let max_cycles = match flag_value(args, "--max-cycles") {
        Some(s) => parse_max_cycles(&s)?,
        None    => 1_000_000_000u64,
    };
    let format = match flag_value(args, "--format").as_deref() {
        Some("fstats") => cli::OutputFormat::Fstats,
        Some("csv")    => cli::OutputFormat::Csv,
        Some("json") | None => cli::OutputFormat::Json,
        Some(other) => return Err(format!("unknown --format '{other}' (use json, fstats, or csv)")),
    };

    cli::run_headless(cli::RunArgs {
        file,
        cache_config: flag_value(args, "--cache-config"),
        settings:     flag_value(args, "--settings"),
        output:       flag_value(args, "--out"),
        nout:         has_flag(args, "--nout"),
        format,
        mem_size,
        max_cycles,
    })
}

// ── raven export-config [--out <file>] ───────────────────────────────────────

fn cmd_export_config(args: &[String]) -> Result<(), String> {
    cli::export_default_config(flag_value(args, "--out").as_deref())
}

// ── raven import-config <file> [--out <file>] ────────────────────────────────

fn cmd_import_config(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("import-config requires a file argument\n\nUsage: raven import-config <file.fcache> [--out <file>]")?;
    cli::import_config(&file, flag_value(args, "--out").as_deref())
}

// ── raven export-settings [--out <file>] ─────────────────────────────────────

fn cmd_export_settings(args: &[String]) -> Result<(), String> {
    cli::export_sim_settings(flag_value(args, "--out").as_deref())
}

// ── raven import-settings <file> [--out <file>] ──────────────────────────────

fn cmd_import_settings(args: &[String]) -> Result<(), String> {
    let file = positional(args)
        .ok_or("import-settings requires a file argument\n\nUsage: raven import-settings <file.rcfg> [--out <file>]")?;
    cli::import_sim_settings(&file, flag_value(args, "--out").as_deref())
}

// ── Help ─────────────────────────────────────────────────────────────────────

fn print_help() {
    eprintln!(
r#"raven — RISC-V (RV32IM) simulator and assembler

USAGE:
    raven                                    Open interactive TUI
    raven build  <file> [OPTIONS]            Assemble source file
    raven run    <file> [OPTIONS]            Assemble and simulate
    raven export-config [OPTIONS]            Export default cache config (.fcache)
    raven import-config <file> [OPTIONS]     Validate and inspect a .fcache file
    raven export-settings [OPTIONS]          Export default sim settings (.rcfg)
    raven import-settings <file> [OPTIONS]   Validate and inspect a .rcfg file

OPTIONS  build:
    [output]                    Output .bin file as second positional arg
    --out <path>                Same as above (takes priority over positional)
    --nout                      Check-only: assemble but don't write any file

OPTIONS  run:
    --cache-config <file>       Load cache hierarchy from .fcache
    --settings <file>           Load sim settings (CPI, cache_enabled) from .rcfg
    --mem <size>                RAM size, e.g. 16mb, 256kb, 1gb   (default: 16mb)
    --max-cycles <n>            Instruction limit                  (default: 1000000000)
    --out <file>                Write simulation results to file
    --nout                      Don't write/print results (program output only)
    --format json|fstats|csv    Results format                     (default: json)

OPTIONS  export-config:
    --out <file>                Write to file instead of stdout

OPTIONS  import-config:
    --out <file>                Re-export normalized config to this file

OPTIONS  export-settings:
    --out <file>                Write to file instead of stdout

OPTIONS  import-settings:
    --out <file>                Re-export normalized settings to this file

EXAMPLES:
    raven build program.fas
    raven build program.fas out/prog.bin
    raven build program.fas --nout
    raven run program.fas --nout
    raven run program.fas --out results.json
    raven run program.fas --cache-config l2.fcache --format csv --out stats.csv
    raven run program.fas --settings my.rcfg --nout
    raven export-config --out default.fcache
    raven import-config my.fcache
    raven import-config my.fcache --out normalized.fcache
    raven export-settings --out default.rcfg
    raven import-settings my.rcfg
    raven import-settings my.rcfg --out normalized.rcfg
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
