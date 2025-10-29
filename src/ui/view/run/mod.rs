use ratatui::Frame;
use ratatui::prelude::*;

use super::components::{render_build_status, render_console};
pub(super) use super::{App, MemRegion, RunButton};
pub(super) use crate::ui::app::FormatMode;

mod formatting;
mod instruction_details;
mod instruction_list;
mod memory;
mod registers;
mod sidebar;
mod status;

use instruction_details::render_instruction_details;
use instruction_list::render_instruction_memory;
use sidebar::render_sidebar;
use status::render_run_status;

pub(super) fn render_run(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(app.console_height),
        ])
        .split(area);

    render_build_status(f, layout[0], app);
    render_run_status(f, layout[1], app);

    let main = layout[2];
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(38),
            Constraint::Length(app.imem_width),
            Constraint::Min(46),
        ])
        .split(main);

    render_sidebar(f, columns[0], app);
    render_instruction_memory(f, columns[1], app);
    render_instruction_details(f, columns[2], app);

    render_console(f, layout[3], app);
}
