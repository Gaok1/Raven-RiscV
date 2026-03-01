use crate::ui::app::App;
use crossterm::terminal;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub mod keyboard;
pub mod mouse;

pub use keyboard::handle_key;
pub use mouse::handle_mouse;

pub(crate) fn max_regs_scroll(app: &App) -> usize {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let area = Rect::new(0, 0, width, height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(area);
    let main = chunks[2];
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(38),
            Constraint::Length(app.run.imem_width),
            Constraint::Min(46),
        ])
        .split(main);
    let side = cols[0];
    let lines = side.height.saturating_sub(4) as usize;
    let total = 37usize; // idk why this number, just works
    total.saturating_sub(lines)
}
