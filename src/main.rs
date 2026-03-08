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

fn main() -> io::Result<()> {
    #[cfg(unix)]
    let quit_flag = setup_sigint();

    // Send xterm-compatible maximize hint before entering raw/alternate mode.
    // Works in most modern terminal emulators (alacritty, kitty, xterm, Windows Terminal).
    // Silently ignored by terminals that don't support it.
    print!("\x1b[9;1t");
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let mut terminal: DefaultTerminal = ratatui::init();

    #[cfg(unix)]
    let res = ui::run(&mut terminal, ui::App::new(), quit_flag);
    #[cfg(not(unix))]
    let res = ui::run(&mut terminal, ui::App::new());

    ratatui::restore();

    if let Err(e) = res {
        eprintln!("error: {e}");
    }

    Ok(())
}
