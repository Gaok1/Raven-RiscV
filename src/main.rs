mod falcon;
mod ui;

use ratatui::DefaultTerminal;
use std::io;

fn main() -> io::Result<()> {
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = ui::run(&mut terminal, ui::App::new());
    ratatui::restore();

    if let Err(e) = res {
        eprintln!("error: {e}");
    }
    Ok(())
}
