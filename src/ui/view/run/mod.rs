use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use super::components::render_console;
pub(super) use super::{App, MemRegion, RunButton};
pub(super) use crate::ui::app::FormatMode;
use crate::ui::app::HartLifecycle;
use crate::ui::theme;

mod formatting;
mod instruction_details;
mod instruction_list;
mod memory;
mod registers;
mod sidebar;
mod status;

use instruction_details::render_instruction_details;
use instruction_list::{render_exec_trace, render_instruction_memory};
use sidebar::render_sidebar;
pub(crate) use status::{render_run_status, run_controls_plain_text, state_text};

pub(crate) const RUN_COLLAPSED_RAIL_W: u16 = 2;
pub(crate) const RUN_SIDEBAR_MIN_W: u16 = 20;
pub(crate) const RUN_IMEM_MIN_W: u16 = 20;
pub(crate) const RUN_DETAILS_MIN_W: u16 = 40;

pub(crate) fn run_panel_constraints(app: &App) -> [Constraint; 3] {
    let sidebar = if app.run.sidebar_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Length(app.run.sidebar_width)
    };
    let imem = if app.run.imem_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Length(app.run.imem_width)
    };
    let details = if app.run.details_collapsed {
        Constraint::Length(RUN_COLLAPSED_RAIL_W)
    } else {
        Constraint::Min(RUN_DETAILS_MIN_W)
    };
    [sidebar, imem, details]
}

pub(super) fn render_run(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(app.run.console_height),
        ])
        .split(area);

    render_run_status(f, layout[0], app);

    let main = layout[1];
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(run_panel_constraints(app))
        .split(main);

    if app.core_status(app.selected_core) == HartLifecycle::Free {
        render_free_core_panel(
            f,
            columns[0],
            "Core State",
            "No hart is bound to this core.",
        );
        render_free_core_panel(
            f,
            columns[1],
            "Instruction Memory",
            "This core is currently FREE, so there is no PC or instruction stream to follow.",
        );
        render_free_core_panel(
            f,
            columns[2],
            "Details",
            "Spawn a hart onto this core first. If spawn failed, check the console for the reason.",
        );
        render_console(f, layout[2], app);
        return;
    }

    if app.run.sidebar_collapsed {
        render_collapsed(f, columns[0], "S", "collapsed", '▶');
    } else {
        render_sidebar(f, columns[0], app);
        render_sidebar_drag_arrow(f, columns[0], app);
    }

    if app.run.imem_collapsed {
        render_collapsed(f, columns[1], "I", "collapsed", '▶');
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
        render_collapsed(f, columns[2], "D", "collapsed", '◀');
    } else {
        render_instruction_details(f, columns[2], app);
    }

    render_console(f, layout[2], app);
}

fn render_collapsed(
    f: &mut Frame,
    area: Rect,
    short: &'static str,
    state: &'static str,
    arrow: char,
) {
    let bg = Style::default().bg(Color::Rgb(20, 26, 38));
    f.render_widget(Block::default().style(bg), area);

    if area.height == 0 || area.width == 0 {
        return;
    }

    let title_style = Style::default()
        .fg(theme::ACCENT)
        .bg(Color::Rgb(28, 38, 54))
        .add_modifier(Modifier::BOLD);
    let title = if area.width >= 2 { "[]" } else { "•" };
    f.render_widget(
        Paragraph::new(title).style(title_style),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let mid = area.y + area.height / 2;
    if mid < area.y + area.height {
        f.render_widget(
            Paragraph::new(short).style(
                Style::default()
                    .fg(theme::LABEL_Y)
                    .bg(Color::Rgb(20, 26, 38))
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(area.x, mid.saturating_sub(1).max(area.y), area.width, 1),
        );
        f.render_widget(
            Paragraph::new(arrow.to_string()).style(
                Style::default()
                    .fg(theme::RUNNING)
                    .bg(Color::Rgb(20, 26, 38))
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(area.x, mid, area.width, 1),
        );
        if mid + 1 < area.y + area.height {
            let state_label = if area.width >= 2 { "×" } else { state };
            f.render_widget(
                Paragraph::new(state_label).style(
                    Style::default()
                        .fg(theme::PAUSED)
                        .bg(Color::Rgb(20, 26, 38)),
                ),
                Rect::new(area.x, mid + 1, area.width, 1),
            );
        }
    }
}

fn render_free_core_panel(f: &mut Frame, area: Rect, title: &'static str, body: &'static str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(theme::LABEL))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn render_sidebar_drag_arrow(f: &mut Frame, area: Rect, app: &App) {
    let style = if app.run.hover_sidebar_bar {
        Style::default().fg(theme::HOVER_BG)
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
