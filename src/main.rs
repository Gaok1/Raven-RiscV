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

const RAM_MIN: usize = 64 * 1024;           // 64 KB
const RAM_MAX: usize = 4 * 1024 * 1024 * 1024; // 4 GB (full 32-bit address space)

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
        return Err(format!("unknown unit in '{s}' — use kb, mb or gb (e.g. 256kb, 16mb, 1gb)"));
    };

    if bytes < RAM_MIN {
        return Err(format!("minimum RAM is 64kb, got '{s}'"));
    }
    if bytes > RAM_MAX {
        return Err(format!("maximum RAM is 4gb (full 32-bit address space), got '{s}'"));
    }
    Ok(bytes)
}

fn parse_max_cycles(s: &str) -> Result<u64, String> {
    s.trim().parse::<u64>().map_err(|_| format!("invalid --max-cycles value '{s}'"))
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // ── CLI mode: raven --run <file> [options] ───────────────────────────────
    if args.iter().any(|a| a == "--run") || args.iter().any(|a| a == "--export-config") {
        let result = parse_and_run_cli(&args);
        if let Err(e) = result {
            eprintln!("raven: {e}");
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
                    Err(e) => { eprintln!("error: {e}"); return Ok(()); }
                },
                None => { eprintln!("error: --mem requires a value (e.g. --mem 16mb)"); return Ok(()); }
            }
        } else {
            i += 1;
        }
    }

    // Send xterm-compatible maximize hint before entering raw/alternate mode.
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

fn parse_and_run_cli(args: &[String]) -> Result<(), String> {
    // --export-config [file]
    if args.iter().any(|a| a == "--export-config") {
        let output = flag_value(args, "--output")
            .or_else(|| {
                // positional: first non-flag arg after --export-config
                let pos = args.iter().position(|a| a == "--export-config").unwrap_or(0);
                args.get(pos + 1).filter(|a| !a.starts_with('-')).cloned()
            });
        return cli::export_default_config(output.as_deref());
    }

    let file = flag_value(args, "--run")
        .ok_or("--run requires a file path")?;

    let mem_size = if let Some(s) = flag_value(args, "--mem") {
        parse_mem_arg(&s)?
    } else {
        16 * 1024 * 1024  // 16 MB default for CLI
    };

    let max_cycles = if let Some(s) = flag_value(args, "--max-cycles") {
        parse_max_cycles(&s)?
    } else {
        1_000_000_000u64  // 1 billion instructions safety limit
    };

    let format = match flag_value(args, "--format").as_deref() {
        Some("fstats") => cli::OutputFormat::Fstats,
        Some("csv")    => cli::OutputFormat::Csv,
        Some("json") | None => cli::OutputFormat::Json,
        Some(other) => return Err(format!("unknown --format '{other}' (use json, fstats, or csv)")),
    };

    cli::run_headless(cli::CliArgs {
        file,
        cache_config: flag_value(args, "--cache-config"),
        output: flag_value(args, "--output"),
        format,
        mem_size,
        max_cycles,
    })
}

/// Return the value of `--flag <value>` from the arg list, or None.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).filter(|a| !a.starts_with('-')).cloned()
}
