use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};

use crate::ui::theme;
pub(super) use super::app::{App, EditorMode, MemRegion, RunButton, Tab};
pub(super) use super::editor::Editor;

pub mod docs;
mod editor;
mod run;
mod components;
pub mod disasm;
mod cache;
mod splash;
mod path_input_overlay;
mod settings;

use docs::render_docs;
use editor::{render_editor, render_editor_status};
use run::render_run;
use cache::render_cache;
use splash::render_splash;
use path_input_overlay::render_path_input;
use settings::render_settings;

pub fn ui(f: &mut Frame, app: &App) {
    // Splash screen takes over the full frame
    if let Some(started) = app.splash_start {
        render_splash(f, started, 4.0, app.run.mem_size);
        return;
    }

    // Apply app-wide dark background
    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG)),
        f.area(),
    );

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
                line = line.style(Style::default().fg(theme::HOVER_FG).bg(theme::HOVER_BG));
            }
            line
        })
        .collect::<Vec<_>>();

    let tutorial_targets_tabbar = app.tutorial.active && {
        use crate::ui::tutorial::get_steps;
        let steps = get_steps(app.tutorial.tab);
        steps.get(app.tutorial.step_idx)
            .and_then(|s| (s.target)(size, app))
            .map(|r| r.height <= 3 && r.y == tab_row.y)
            .unwrap_or(false)
    };
    let tab_border_style = if tutorial_targets_tabbar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(theme::BORDER)
    };

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).border_style(tab_border_style).title("RAVEN"))
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(0, 0, 0))
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" │ ", Style::default().fg(theme::BORDER)))
        .select(app.tab.index());
    f.render_widget(tabs, tabs_area);

    // Help button [?]
    let help_style = if app.help_open {
        Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::ACCENT).bold()
    } else if app.hover_help {
        Style::default().fg(theme::HOVER_FG).bg(theme::HOVER_BG).bold()
    } else {
        Style::default().fg(theme::ACCENT).add_modifier(Modifier::DIM)
    };
    let help_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::BORDER));
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
        Tab::Config => render_settings(f, chunks[1], app),
    }

    let (footer_text, footer_style) = match app.tab {
        Tab::Editor => {
            let mode = match app.mode { EditorMode::Insert => "INSERT", EditorMode::Command => "COMMAND" };
            (
                format!("{mode}  │  Ctrl+O=Open  Ctrl+S=Save  Ctrl+Z=Undo  Ctrl+F=Find  Ctrl+G=Goto  Ctrl+/=Comment"),
                Style::default().fg(theme::LABEL),
            )
        }
        Tab::Run => (
            "s=Step  r=Run  p=Pause  R=Restart  f=Speed  v=Sidebar  k=Region  Ctrl+F=Jump RAM  Ctrl+G=Label  [?]=Help".to_string(),
            Style::default().fg(theme::LABEL),
        ),
        Tab::Cache => {
            if let Some(ref err) = app.cache.config_error {
                (format!("✗  {err}"), Style::default().fg(theme::DANGER))
            } else if let Some(ref ok) = app.cache.config_status {
                (format!("✓  {ok}"), Style::default().fg(theme::RUNNING))
            } else {
                (
                    "Tab=Subtabs  Ctrl+E=Export config  Ctrl+L=Import config  Ctrl+R=Results  Ctrl+M=Baseline  [?]=Help".to_string(),
                    Style::default().fg(theme::LABEL),
                )
            }
        }
        Tab::Docs => (
            "Ctrl+F=Search  ←/→=Filter  Space=Toggle filter  ↑/↓=Scroll  PgUp/PgDn=Fast scroll  l=Language".to_string(),
            Style::default().fg(theme::LABEL),
        ),
        Tab::Config => (
            "↑/↓=Navigate  Enter=Edit/Toggle  Esc=Cancel  Click=Toggle bool  Tab=Next field".to_string(),
            Style::default().fg(theme::LABEL),
        ),
    };

    f.render_widget(Paragraph::new(footer_text).style(footer_style), chunks[2]);

    if app.show_exit_popup {
        render_exit_popup(f, size);
    }

    if app.help_open {
        render_help_popup(f, size, app);
    }

    if app.editor.elf_prompt_open && matches!(app.tab, Tab::Editor) {
        render_elf_prompt(f, size, app);
    }

    render_path_input(f, size, app);

    if app.tutorial.active {
        crate::ui::tutorial::render::render_tutorial_overlay(f, size, app);
    }
}

fn render_exit_popup(f: &mut Frame, area: Rect) {
    let popup = centered_rect(area.width / 3, area.height / 4, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::DANGER))
        .title(Span::styled("Confirm Exit", Style::default().fg(theme::DANGER)));
    let lines = vec![
        Line::raw("Do you wish to exit?"),
        Line::raw("Check your code is saved before exiting."),
        Line::raw(""),
        Line::from(vec![
            Span::styled("[Exit]", Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::DANGER)),
            Span::styled("  Enter/y  ", Style::default().fg(theme::LABEL)),
            Span::styled("[Cancel]", Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::ACCENT)),
            Span::styled("  Esc", Style::default().fg(theme::LABEL)),
        ]),
    ];
    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup);
}

// ── ELF prompt popup ─────────────────────────────────────────────────────────

pub(super) const ELF_BTN_CANCEL:  &str = "[ Cancel ]";
pub(super) const ELF_BTN_EDIT:    &str = "[ Edit opcodes ]";
pub(super) const ELF_BTN_DISCARD: &str = "[ Discard ELF ]";
pub(super) const ELF_POPUP_W: u16 = 62;
pub(super) const ELF_POPUP_H: u16 = 8;
pub(super) const ELF_BTN_ROW: u16 = 4; // inner_y of the button row (0-indexed)

fn render_elf_prompt(f: &mut Frame, area: Rect, app: &App) {
    let popup_w = ELF_POPUP_W.min(area.width.saturating_sub(4));
    let popup = centered_rect(popup_w, ELF_POPUP_H, area);
    f.render_widget(Clear, popup);

    let btn_y = popup.y + 1 + ELF_BTN_ROW; // absolute row of the button line
    let inner_w = popup_w.saturating_sub(2);

    // Compute absolute x positions for each button (left-padded to center all three)
    const GAP: u16 = 2;
    let total_btns = ELF_BTN_CANCEL.len() as u16
        + GAP
        + ELF_BTN_EDIT.len() as u16
        + GAP
        + ELF_BTN_DISCARD.len() as u16;
    let btn_x0 = popup.x + 1 + inner_w.saturating_sub(total_btns) / 2;

    let btn_style = |label: &str, x: u16| {
        let hovered = app.mouse_y == btn_y
            && app.mouse_x >= x
            && app.mouse_x < x + label.len() as u16;
        if hovered {
            Style::default().fg(Color::Black).bg(theme::ACCENT)
        } else {
            Style::default().fg(Color::Black).bg(theme::IDLE)
        }
    };

    let x_cancel  = btn_x0;
    let x_edit    = x_cancel  + ELF_BTN_CANCEL.len()  as u16 + GAP;
    let x_discard = x_edit    + ELF_BTN_EDIT.len()    as u16 + GAP;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::PAUSED))
        .title(Span::styled(
            " ELF Binary ",
            Style::default().fg(theme::PAUSED).bold(),
        ));

    let lines = vec![
        Line::raw(""),
        Line::raw("An ELF binary is loaded — the editor is read-only."),
        Line::raw("How would you like to proceed?"),
        Line::raw(""),
        Line::from(vec![
            Span::raw(" ".repeat(inner_w.saturating_sub(total_btns) as usize / 2)),
            Span::styled(ELF_BTN_CANCEL,  btn_style(ELF_BTN_CANCEL,  x_cancel)),
            Span::raw("  "),
            Span::styled(ELF_BTN_EDIT,    btn_style(ELF_BTN_EDIT,    x_edit)),
            Span::raw("  "),
            Span::styled(ELF_BTN_DISCARD, btn_style(ELF_BTN_DISCARD, x_discard)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Esc", Style::default().fg(theme::LABEL)),
            Span::styled(" = Cancel", Style::default().fg(theme::LABEL)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
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
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(title, Style::default().fg(theme::ACCENT).bold()));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line<'static>> = content.iter().map(|(key, desc)| {
        if key.is_empty() {
            Line::from("")
        } else {
            Line::from(vec![
                Span::styled(format!("{key:<18}"), Style::default().fg(theme::LABEL_Y).bold()),
                Span::styled(desc.to_string(), Style::default().fg(theme::TEXT)),
            ])
        }
    }).collect();

    if total > 1 {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("← → ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("page {}/{total}   ", page + 1),
                Style::default().fg(theme::LABEL),
            ),
            Span::styled("Esc", Style::default().fg(theme::LABEL)),
            Span::styled(" close", Style::default().fg(theme::LABEL)),
        ]));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Esc", Style::default().fg(theme::LABEL)),
            Span::styled(" close", Style::default().fg(theme::LABEL)),
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
                ("[f]",            "cycle execution speed (1x → 2x → 4x → 8x → GO)"),
                ("[v]",            "cycle sidebar: RAM → REGS → Dyn"),
                ("[k]",            "cycle RAM region: DATA → STACK → R/W → HEAP"),
                ("[Tab] REGS",     "toggle integer / float register bank"),
                ("[F9]",           "toggle breakpoint at hovered / PC"),
                ("",               ""),
                ("[Ctrl+F]",       "jump RAM view to address (type hex, live)"),
                ("[Ctrl+G]",       "jump instruction view to label (type name, live)"),
                ("[t]",            "toggle instruction trace panel"),
                ("[e]",            "toggle execution count display (×N)"),
                ("[y]",            "toggle instruction type badge ([R],[I]…)"),
                ("[Tab]",          "collapse / expand panels"),
                ("[↑/↓]",          "scroll memory or registers"),
                ("[click]",        "select instruction / register"),
                ("[drag]",         "resize sidebar / instruction panels"),
            ],
            vec![
                ("[P] register",   "pin / unpin register in sidebar"),
                ("[↑/↓] REGS",     "navigate register list"),
                ("[↑/↓] RAM",      "scroll memory view"),
                ("",               ""),
                ("[Dyn] STORE",    "sidebar → RAM centered on written address"),
                ("[Dyn] LOAD/ALU", "sidebar → register bank (result visible)"),
                ("",               ""),
                ("Count [ON/OFF]", "show/hide exec count heat map"),
                ("Type [ON/OFF]",  "show/hide instruction type badge"),
                ("Speed [1x…GO]",  "execution speed control"),
                ("State [RUN]",    "pause / resume execution"),
                ("Region [DATA]",  "cycle: Data → Stack → R/W → Heap"),
                ("Bytes [4B]",     "bytes per memory row"),
                ("Format [HEX]",   "display format: HEX / DEC / STR"),
                ("Sign [SGN]",     "signed / unsigned display (DEC mode)"),
            ],
        ],
        Tab::Editor => vec![
            vec![
                ("[Esc]",          "switch to Command mode (click editor to return)"),
                ("[Ctrl+Z]",       "undo"),
                ("[Ctrl+Y]",       "redo"),
                ("[Ctrl+/]",       "toggle line comment"),
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
                ("[v]",            "cycle sidebar view: RAM → REGS → Dyn"),
                ("[k]",            "cycle RAM region: DATA → STACK → R/W → HEAP"),
                ("[f]",            "cycle speed: 1x → 2x → 4x → 8x → GO"),
                ("[e]",            "toggle exec count display"),
                ("[y]",            "toggle instruction type badge"),
                ("[i]",            "scope → I-Cache (Stats/View)"),
                ("[d]",            "scope → D-Cache (Stats/View)"),
                ("[b]",            "scope → Both (Stats/View)"),
                ("[+/-]",          "add / remove cache level"),
                ("",               ""),
                ("[m] View",        "cycle data format: HEX → DEC-U → DEC-S → FLOAT"),
                ("[g] View",        "cycle byte grouping: 1B → 2B → 4B"),
                ("[t] View",        "toggle address / tag display (0x… ↔ t:…)"),
                ("[↑/↓]",          "scroll (Stats / View subtabs)"),
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
                ("[Ctrl+F]",       "open search bar (filter by name/desc)"),
                ("[←/→]",          "navigate type filter"),
                ("[Space]",        "toggle selected type filter / restore All"),
            ],
        ],
        Tab::Config => vec![
            vec![
                ("[↑/↓]",          "navigate settings"),
                ("[Enter]",        "edit CPI field / toggle bool"),
                ("[Esc]",          "cancel edit"),
                ("[Tab]",          "confirm edit and move to next field"),
                ("[click]",        "toggle bool button / start CPI edit"),
                ("[Ctrl+E]",       "export .rcfg"),
                ("[Ctrl+L]",       "import .rcfg"),
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
