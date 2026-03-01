use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::components::{render_build_status, render_console};
pub(super) use super::{App, MemRegion, RunButton};
pub(super) use crate::ui::app::{FormatMode, RunSpeed};

mod formatting;
mod instruction_details;
mod instruction_list;
mod memory;
mod registers;
mod sidebar;
mod status;

use instruction_details::render_instruction_details;
use instruction_list::{render_instruction_memory, render_exec_trace};
use sidebar::render_sidebar;
use status::render_run_status;

pub(super) fn render_run(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(area);

    render_build_status(f, layout[0], app);
    render_run_status(f, layout[1], app);

    let main = layout[2];
    let sidebar_w  = if app.run.sidebar_collapsed  { 3 } else { app.run.sidebar_width };
    let imem_w     = if app.run.imem_collapsed     { 3 } else { app.run.imem_width    };
    let details_min = if app.run.details_collapsed { 3 } else { 40 };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w),
            Constraint::Length(imem_w),
            Constraint::Min(details_min),
        ])
        .split(main);

    if app.run.sidebar_collapsed {
        render_collapsed(f, columns[0], "◄ S");
    } else {
        render_sidebar(f, columns[0], app);
        render_sidebar_drag_arrow(f, columns[0], app);
    }

    if app.run.imem_collapsed {
        render_collapsed(f, columns[1], "◄ I");
    } else if app.run.show_trace && columns[1].height >= 10 {
        // Split instruction memory column: top = imem, bottom = trace
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(columns[1]);
        render_instruction_memory(f, split[0], app);
        render_exec_trace(f, split[1], app);
    } else {
        render_instruction_memory(f, columns[1], app);
    }

    if app.run.details_collapsed {
        render_collapsed(f, columns[2], "► D");
    } else {
        render_instruction_details(f, columns[2], app);
    }

    render_console(f, layout[3], app);
}

fn render_collapsed(f: &mut Frame, area: Rect, label: &'static str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Render label vertically centered
    let mid = inner.height / 2;
    if mid < inner.height {
        let label_area = Rect::new(inner.x, inner.y + mid, inner.width, 1);
        f.render_widget(
            Paragraph::new(label).style(Style::default().fg(Color::DarkGray)),
            label_area,
        );
    }
}

fn render_sidebar_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    let style = if app.run.hover_sidebar_bar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let arrow_area = Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y + area.height / 2,
        1,
        1,
    );
    f.render_widget(Paragraph::new(">").style(style), arrow_area);
}
