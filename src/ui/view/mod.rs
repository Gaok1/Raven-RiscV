use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};

pub(super) use super::app::{App, EditorMode, MemRegion, RunButton, Tab};
pub(super) use super::editor::Editor;

pub mod docs;
mod editor;
mod run;
mod components;
pub mod disasm;

use docs::render_docs;
use editor::{render_editor, render_editor_status};
use run::render_run;

pub fn ui(f: &mut Frame, app: &App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(size);

    // Build tab titles from Tab::all() — adding a new tab only requires
    // extending the Tab enum and Tab::all()/Tab::label().
    let titles = Tab::all()
        .iter()
        .map(|&tab| {
            let mut line = Line::from(tab.label());
            if Some(tab) == app.hover_tab && tab != app.tab {
                line = line.style(Style::default().fg(Color::Black).bg(Color::Gray));
            }
            line
        })
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Falcon ASM"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" │ ", Style::default().fg(Color::DarkGray)))
        .select(app.tab.index());
    f.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Editor => {
            let editor_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(3)])
                .split(chunks[1]);
            render_editor_status(f, editor_chunks[0], app);
            render_editor(f, editor_chunks[1], app);
        }
        Tab::Run => render_run(f, chunks[1], app),
        Tab::Docs => render_docs(f, chunks[1], app),
    }

    let mode = match app.mode {
        EditorMode::Insert => "INSERT",
        EditorMode::Command => "COMMAND",
    };
    let status = format!(
        "Mode: {}  |  Auto-assemble  |  Ctrl+O=Import  |  Ctrl+S=Export",
        mode
    );

    let status = Paragraph::new(status).block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[2]);

    if app.show_exit_popup {
        render_exit_popup(f, size);
    }
}

fn render_exit_popup(f: &mut Frame, area: Rect) {
    let popup = centered_rect(area.width / 3, area.height / 4, area);
    f.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).title("Confirm Exit");
    let lines = vec![
        Line::raw("Do you wish to exit?"),
        Line::raw("Check your code is saved before exiting."),
        Line::raw(""),
        Line::from(vec![
            Span::styled("[Exit]", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::raw("   "),
            Span::styled(
                "[Cancel]",
                Style::default().fg(Color::Black).bg(Color::Blue),
            ),
        ]),
    ];
    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup);
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Rect::new(
        r.x + (r.width.saturating_sub(width)) / 2,
        r.y + (r.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}
