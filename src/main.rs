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
