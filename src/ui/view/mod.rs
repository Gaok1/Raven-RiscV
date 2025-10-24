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

    let t_editor = crate::ui::i18n::T::new("Editor", "Editor");
    let t_run = crate::ui::i18n::T::new("Run", "Run");
    let t_docs = crate::ui::i18n::T::new("Docs", "Docs");

    let titles = vec![t_editor.get(app.lang), t_run.get(app.lang), t_docs.get(app.lang)]
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let mut line = Line::from(t);
            let tab = match i {
                0 => Tab::Editor,
                1 => Tab::Run,
                _ => Tab::Docs,
            };
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
        .select(match app.tab {
            Tab::Editor => 0,
            Tab::Run => 1,
            Tab::Docs => 2,
        });
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
        "Mode: {}  |  Auto-assemble on success  |  Ctrl+R=Restart (Run)  |  Ctrl+O=Import  |  Ctrl+S=Export  |  Ctrl+L=Language  |  1/2/3 switch tabs (Command mode)",
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
