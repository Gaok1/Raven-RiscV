use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};

pub(super) use super::app::{App, DocsPage, EditorMode, MemRegion, RunButton, Tab};
pub(super) use super::editor::Editor;

pub mod docs;
mod editor;
mod run;
mod components;
pub mod disasm;
mod cache;

use docs::render_docs;
use editor::{render_editor, render_editor_status};
use run::render_run;
use cache::render_cache;

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

    // Tab bar: tabs on left, [?] help button on right
    let tab_row = chunks[0];
    let help_btn_w = 5u16; // "[?]  "
    let tabs_area = Rect::new(tab_row.x, tab_row.y, tab_row.width.saturating_sub(help_btn_w), tab_row.height);
    let help_btn_area = Rect::new(tab_row.x + tab_row.width.saturating_sub(help_btn_w), tab_row.y, help_btn_w, tab_row.height);

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
    f.render_widget(tabs, tabs_area);

    // Help button [?]
    let help_style = if app.help_open {
        Style::default().fg(Color::Black).bg(Color::LightCyan).bold()
    } else if app.hover_help {
        Style::default().fg(Color::Black).bg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::Black).bg(Color::LightCyan).add_modifier(Modifier::DIM)
    };
    let help_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray));
    let help_para = Paragraph::new(Span::styled("[?]", help_style)).block(help_block).alignment(Alignment::Center);
    f.render_widget(help_para, help_btn_area);

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
        Tab::Cache => render_cache(f, chunks[1], app),
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

    if app.help_open {
        render_help_popup(f, size, app);
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

// ── Help popup ───────────────────────────────────────────────────────────────

fn render_help_popup(f: &mut Frame, area: Rect, app: &App) {
    let pages = help_pages(app.tab);
    let total = pages.len();
    let page = app.help_page.min(total.saturating_sub(1));
    let content = &pages[page];

    let popup_w = 60u16.min(area.width.saturating_sub(4));
    let popup_h = (content.len() as u16 + 5).min(area.height.saturating_sub(4));
    let popup = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup);

    let tab_label = app.tab.label();
    let title = if total > 1 {
        format!("Help — {tab_label}  [{}/{total}]", page + 1)
    } else {
        format!("Help — {tab_label}")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan))
        .title(Span::styled(title, Style::default().fg(Color::LightCyan).bold()));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line<'static>> = content.iter().map(|(key, desc)| {
        if key.is_empty() {
            Line::from("")
        } else {
            Line::from(vec![
                Span::styled(format!("{key:<18}"), Style::default().fg(Color::Yellow).bold()),
                Span::styled(desc.to_string(), Style::default().fg(Color::White)),
            ])
        }
    }).collect();

    if total > 1 {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("← → ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("page {}/{total}   ", page + 1),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(" close", Style::default().fg(Color::DarkGray)),
        ]));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(" close", Style::default().fg(Color::DarkGray)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

type HelpEntry = (&'static str, &'static str);

fn help_pages(tab: Tab) -> Vec<Vec<HelpEntry>> {
    match tab {
        Tab::Run => vec![
            vec![
                ("[s]",            "step one instruction"),
                ("[r]",            "run / stop execution"),
                ("[p]",            "pause"),
                ("[R]",            "restart from beginning"),
                ("[f]",            "cycle execution speed (1x → 2x → 4x → GO)"),
                ("[v]",            "cycle sidebar view: REGS / RAM / BP"),
                ("[k]",            "toggle stack region in RAM view"),
                ("[F9]",           "toggle breakpoint at hovered / PC"),
                ("",               ""),
                ("[t]",            "toggle instruction trace panel"),
                ("[e]",            "toggle execution count display (×N)"),
                ("[y]",            "toggle instruction type badge ([R],[I]…)"),
                ("[Tab]",          "collapse / expand panels"),
                ("[↑/↓]",          "scroll memory or registers"),
                ("[click]",        "select instruction / register"),
                ("[drag]",         "resize sidebar / instruction panels"),
            ],
            vec![
                ("[p] register",   "pin / unpin register in sidebar"),
                ("[↑/↓] REGS",     "navigate register list"),
                ("[↑/↓] RAM",      "scroll memory view"),
                ("",               ""),
                ("Count [ON/OFF]", "show/hide exec count heat map"),
                ("Type [ON/OFF]",  "show/hide instruction type badge"),
                ("Speed [1x…GO]",  "execution speed control"),
                ("State [RUN]",    "pause / resume execution"),
                ("Region [DATA]",  "switch memory region (Data / Stack)"),
                ("Bytes [4B]",     "bytes per memory row"),
                ("Format [HEX]",   "display format: HEX / DEC / STR"),
                ("Sign [SGN]",     "signed / unsigned display (DEC mode)"),
            ],
        ],
        Tab::Editor => vec![
            vec![
                ("[Tab]",          "toggle Insert / Command mode"),
                ("[Ctrl+Z]",       "undo"),
                ("[Ctrl+Y]",       "redo"),
                ("[Ctrl+F]",       "open find bar"),
                ("[Ctrl+H]",       "open find & replace bar"),
                ("[Ctrl+G]",       "goto line number"),
                ("[Ctrl+O]",       "import file"),
                ("[Ctrl+S]",       "export / save file"),
                ("",               ""),
                ("[Ctrl+A]",       "select all"),
                ("[Ctrl+C]",       "copy selection"),
                ("[Ctrl+V]",       "paste"),
                ("[Ctrl+X]",       "cut selection"),
            ],
        ],
        Tab::Cache => vec![
            vec![
                ("[Tab]",          "cycle subtabs: Stats → View → Config"),
                ("[r]",            "reset statistics"),
                ("[p]",            "pause / resume execution"),
                ("[v]",            "cycle sidebar view: RAM → REGS → BP"),
                ("[k]",            "toggle region: DATA ↔ STACK"),
                ("[f]",            "cycle speed: 1x → 2x → 4x → GO"),
                ("[e]",            "toggle exec count display"),
                ("[y]",            "toggle instruction type badge"),
                ("[i]",            "scope → I-Cache (Stats/View)"),
                ("[d]",            "scope → D-Cache (Stats/View)"),
                ("[b]",            "scope → Both (Stats/View)"),
                ("[+/-]",          "add / remove cache level"),
                ("",               ""),
                ("[↑/↓]",          "scroll (Stats / View subtabs)"),
                ("[↑/↓] Config",   "navigate CPI fields"),
                ("[Enter] Config",  "edit selected CPI field"),
                ("[click] Config",  "click CPI field to edit"),
                ("[Ctrl+E]",       "export cache config (.fcache)"),
                ("[Ctrl+L]",       "import cache config (.fcache)"),
                ("[Ctrl+R]",       "export simulation results (.fstats / .csv)"),
                ("[Ctrl+M]",       "load baseline snapshot for comparison"),
                ("[c] Stats",      "clear loaded baseline"),
            ],
        ],
        Tab::Docs => vec![
            vec![
                ("[↑/↓]",          "scroll documentation"),
                ("[PgUp/PgDn]",    "fast scroll"),
                ("[Ctrl+F]",       "search within docs"),
            ],
        ],
    }
}


fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Rect::new(
        r.x + (r.width.saturating_sub(width)) / 2,
        r.y + (r.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}
