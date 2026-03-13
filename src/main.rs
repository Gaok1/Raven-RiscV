mod falcon;
mod ui;

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

fn main() -> io::Result<()> {
    #[cfg(unix)]
    let quit_flag = setup_sigint();

    let mut ram_override: Option<usize> = None;
    let args: Vec<String> = std::env::args().collect();
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
    // Works in most modern terminal emulators (alacritty, kitty, xterm, Windows Terminal).
    // Silently ignored by terminals that don't support it.
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
