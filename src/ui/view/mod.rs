use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

pub(super) use super::app::{App, EditorMode, MemRegion, RunButton, Tab};
pub(super) use super::editor::Editor;
use crate::ui::theme;

mod cache;
mod components;
pub mod disasm;
pub mod docs;
mod editor;
mod path_input_overlay;
mod pipeline;
pub(crate) mod run;
mod settings;
mod splash;

use cache::render_cache;
use docs::render_docs;
use editor::{render_editor, render_editor_status};
use path_input_overlay::render_path_input;
use pipeline::render_pipeline;
use run::render_run;
use settings::render_settings;
use splash::render_splash;

pub(crate) const HELP_BTN_W: u16 = 5;

pub fn ui(f: &mut Frame, app: &App) {
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
    let help_btn_w = HELP_BTN_W;
    let tabs_area = Rect::new(
        tab_row.x,
        tab_row.y,
        tab_row.width.saturating_sub(help_btn_w),
        tab_row.height,
    );
    let help_btn_area = Rect::new(
        tab_row.x + tab_row.width.saturating_sub(help_btn_w),
        tab_row.y,
        help_btn_w,
        tab_row.height,
    );

    let tutorial_targets_tabbar = app.tutorial.active && {
        use crate::ui::tutorial::get_steps;
        let steps = get_steps(app.tutorial.tab);
        steps
            .get(app.tutorial.step_idx)
            .and_then(|s| (s.target)(size, app))
            .map(|r| r.height <= 3 && r.y == tab_row.y)
            .unwrap_or(false)
    };
    render_main_tab_bar(f, tabs_area, app, tutorial_targets_tabbar);

    // Help button [?]
    let help_style = if app.help_open {
        Style::default()
            .fg(Color::Rgb(0, 0, 0))
            .bg(theme::ACCENT)
            .bold()
    } else if app.hover_help {
        Style::default()
            .fg(theme::HOVER_FG)
            .bg(theme::HOVER_BG)
            .bold()
    } else {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::DIM)
    };
    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
    let help_para = Paragraph::new(Span::styled("[?]", help_style))
        .block(help_block)
        .alignment(Alignment::Center);
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
        Tab::Pipeline => render_pipeline(f, chunks[1], app),
        Tab::Docs => render_docs(f, chunks[1], app),
        Tab::Config => render_settings(f, chunks[1], app),
    }

    let (footer_text, footer_style) = match app.tab {
        Tab::Editor => {
            let mode = match app.mode { EditorMode::Insert => "INSERT", EditorMode::Command => "COMMAND" };
            (
                format!("{mode}  │  Ctrl+O=Open  Ctrl+S=Save  Ctrl+Z=Undo  Ctrl+F=Find  Ctrl+G=Goto  Ctrl+/=Comment  [?]=Help"),
                Style::default().fg(theme::LABEL),
            )
        }
        Tab::Run => (
            "s=Step  r=Run  p=Pause  R=Restart  f=Speed  v=Sidebar  k=Region  Ctrl+F=Jump RAM  Ctrl+G=Label  [?]=Help".to_string(),
            Style::default().fg(theme::LABEL),
        ),
        Tab::Pipeline => {
            if let Some(ref err) = app.pipeline.status_error {
                (format!("✗  {err}"), Style::default().fg(theme::DANGER))
            } else if let Some(ref ok) = app.pipeline.status_msg {
                (format!("✓  {ok}"), Style::default().fg(theme::RUNNING))
            } else {
                (
                    "s=Step  p/Space=Run/Pause  r=Reset  f=Speed  Tab=Subtab  ↑/↓=Config  Ctrl+E/L=Config  Ctrl+R=Results  [?]=Help".to_string(),
                    Style::default().fg(theme::LABEL),
                )
            }
        }
        Tab::Cache => {
            if let Some(ref err) = app.cache.config_error {
                (format!("✗  {err}"), Style::default().fg(theme::DANGER))
            } else if let Some(ref ok) = app.cache.config_status {
                (format!("✓  {ok}"), Style::default().fg(theme::RUNNING))
            } else {
                (
                    "Tab=Subtabs  Ctrl+E=Export config  Ctrl+L=Import config  Ctrl+R=Results  [?]=Help".to_string(),
                    Style::default().fg(theme::LABEL),
                )
            }
        }
        Tab::Docs => (
            "Ctrl+F=Search  ←/→=Filter  Space=Toggle filter  ↑/↓=Scroll  PgUp/PgDn=Fast scroll  l=Language  [?]=Help".to_string(),
            Style::default().fg(theme::LABEL),
        ),
        Tab::Config => (
            "↑/↓=Navigate  Enter=Edit/Toggle  Esc=Cancel  Click=Toggle bool  Tab=Next field  [?]=Help".to_string(),
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

fn render_main_tab_bar(f: &mut Frame, area: Rect, app: &App, tutorial_targeted: bool) {
    let visible_tabs = app.visible_tabs();
    let mut labels: Vec<Span<'static>> = vec![Span::raw(" ")];
    let mut underlines: Vec<Span<'static>> = vec![Span::raw(" ")];

    for (i, &tab) in visible_tabs.iter().enumerate() {
        let label = format!(" {} ", tab.label());
        let label_w = label.chars().count();
        let text_w = tab.label().chars().count();
        let label_style = if tab == app.tab {
            Style::default()
                .fg(theme::ACTIVE)
                .add_modifier(Modifier::BOLD)
        } else if Some(tab) == app.hover_tab {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::IDLE)
        };
        labels.push(Span::styled(label, label_style));

        underlines.push(underline_cell(tab == app.tab, label_w, text_w));

        if i + 1 < visible_tabs.len() {
            labels.push(Span::raw("  "));
            underlines.push(Span::raw("  "));
        }
    }

    let sep_style = if tutorial_targeted {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(theme::BORDER)
    };
    let lines = vec![
        Line::from(labels),
        Line::from(underlines),
        Line::styled("─".repeat(area.width as usize), sep_style),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn underline_cell(active: bool, total_width: usize, line_width: usize) -> Span<'static> {
    let text = if active && total_width >= line_width {
        let left = (total_width - line_width) / 2;
        let right = total_width.saturating_sub(left + line_width);
        format!(
            "{}{}{}",
            " ".repeat(left),
            "─".repeat(line_width),
            " ".repeat(right)
        )
    } else {
        " ".repeat(total_width)
    };
    Span::styled(text, Style::default().fg(theme::ACCENT))
}

fn render_exit_popup(f: &mut Frame, area: Rect) {
    let popup = centered_rect(area.width / 3, area.height / 4, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::DANGER))
        .title(Span::styled(
            "Confirm Exit",
            Style::default().fg(theme::DANGER),
        ));
    let lines = vec![
        Line::raw("Do you wish to exit?"),
        Line::raw("Check your code is saved before exiting."),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "[Exit]",
                Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::DANGER),
            ),
            Span::styled("  Enter/y  ", Style::default().fg(theme::LABEL)),
            Span::styled(
                "[Cancel]",
                Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::ACCENT),
            ),
            Span::styled("  Esc", Style::default().fg(theme::LABEL)),
        ]),
    ];
    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup);
}

// ── ELF prompt popup ─────────────────────────────────────────────────────────

pub(super) const ELF_BTN_CANCEL: &str = "[ Cancel ]";
pub(super) const ELF_BTN_EDIT: &str = "[ Edit opcodes ]";
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
        let hovered =
            app.mouse_y == btn_y && app.mouse_x >= x && app.mouse_x < x + label.len() as u16;
        if hovered {
            Style::default().fg(Color::Black).bg(theme::ACCENT)
        } else {
            Style::default().fg(Color::Black).bg(theme::IDLE)
        }
    };

    let x_cancel = btn_x0;
    let x_edit = x_cancel + ELF_BTN_CANCEL.len() as u16 + GAP;
    let x_discard = x_edit + ELF_BTN_EDIT.len() as u16 + GAP;

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
            Span::styled(ELF_BTN_CANCEL, btn_style(ELF_BTN_CANCEL, x_cancel)),
            Span::raw("  "),
            Span::styled(ELF_BTN_EDIT, btn_style(ELF_BTN_EDIT, x_edit)),
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
    let popup = help_popup_rect(area, app);
    let pages = help_pages(app.tab);
    let total = pages.len();
    let page = app.help_page.min(total.saturating_sub(1));
    let content = &pages[page];

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
        .title(Span::styled(
            title,
            Style::default().fg(theme::ACCENT).bold(),
        ));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line<'static>> = content
        .iter()
        .map(|(key, desc)| {
            if key.is_empty() {
                Line::from("")
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{key:<18}"),
                        Style::default().fg(theme::LABEL_Y).bold(),
                    ),
                    Span::styled(desc.to_string(), Style::default().fg(theme::TEXT)),
                ])
            }
        })
        .collect();

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

pub(crate) fn help_button_area(area: Rect) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    let tab_row = chunks[0];
    Rect::new(
        tab_row.x + tab_row.width.saturating_sub(HELP_BTN_W),
        tab_row.y,
        HELP_BTN_W,
        tab_row.height,
    )
}

pub(crate) fn help_popup_rect(area: Rect, app: &App) -> Rect {
    let pages = help_pages(app.tab);
    let total = pages.len();
    let page = app.help_page.min(total.saturating_sub(1));
    let content = &pages[page];

    let popup_w = 64u16.min(area.width.saturating_sub(2));
    let popup_h = (content.len() as u16 + 5).min(area.height.saturating_sub(4));
    let anchor = help_button_area(area);
    best_popup_rect(anchor, popup_w, popup_h, area)
}

type HelpEntry = (&'static str, &'static str);

fn help_pages(tab: Tab) -> Vec<Vec<HelpEntry>> {
    match tab {
        Tab::Run => vec![
            vec![
                ("[s]", "step one instruction"),
                ("[r]", "run / stop execution"),
                ("[p]", "pause"),
                ("[R]", "restart from beginning"),
                ("Core [[]/]]", "select which core/hart runtime to observe"),
                ("[f]", "cycle execution speed (1x → 2x → 4x → 8x → GO)"),
                ("[v]", "cycle sidebar: RAM → REGS → Dyn"),
                ("[k]", "cycle RAM region: DATA → STACK → R/W → HEAP"),
                ("[Tab] REGS", "toggle integer / float register bank"),
                ("[F9]", "toggle breakpoint at hovered / PC"),
                ("", ""),
                ("[Ctrl+F]", "jump RAM view to address (type hex, live)"),
                (
                    "[Ctrl+G]",
                    "jump instruction view to label (type name, live)",
                ),
                ("[t]", "toggle instruction trace panel"),
                ("[e]", "toggle execution count display (×N)"),
                ("[y]", "toggle instruction type badge ([R],[I]…)"),
                ("[Ctrl+↑/↓]", "scroll console"),
                ("[↑/↓]", "scroll memory or registers"),
                ("[click]", "select instruction / register"),
                ("[drag]", "resize sidebar / instruction panels"),
            ],
            vec![
                (
                    "Hart",
                    "hardware thread currently bound to the selected core",
                ),
                ("Status [FREE]", "no hart on this core yet"),
                ("[P] register", "pin / unpin register in sidebar"),
                ("[↑/↓] REGS", "navigate register list"),
                ("[↑/↓] RAM", "scroll memory view"),
                ("", ""),
                ("[Dyn] STORE", "sidebar → RAM centered on written address"),
                ("[Dyn] LOAD/ALU", "sidebar → register bank (result visible)"),
                ("[R/W]", "still RAM view; follows last LOAD/STORE address"),
                ("", ""),
                ("Count [ON/OFF]", "show/hide exec count heat map"),
                ("Type [ON/OFF]", "show/hide instruction type badge"),
                ("Speed [1x…GO]", "execution speed control"),
                ("State [RUN]", "pause / resume execution"),
                ("Region [DATA]", "cycle: Data → Stack → R/W → Heap"),
                ("Bytes [4B]", "bytes per memory row"),
                ("Format [HEX]", "display format: HEX / DEC / STR"),
                ("Sign [SGN]", "signed / unsigned display (DEC mode)"),
            ],
        ],
        Tab::Editor => vec![vec![
            ("[Esc]", "switch to Command mode (click editor to return)"),
            ("[Ctrl+Z]", "undo"),
            ("[Ctrl+Y]", "redo"),
            ("[Ctrl+/]", "toggle line comment"),
            ("[Ctrl+F]", "open find bar"),
            ("[Ctrl+H]", "open find & replace bar"),
            ("[Ctrl+G]", "goto line number"),
            ("[Ctrl+O]", "import file"),
            ("[Ctrl+S]", "export / save file"),
            ("", ""),
            ("[Ctrl+A]", "select all"),
            ("[Ctrl+C]", "copy selection"),
            ("[Ctrl+V]", "paste"),
            ("[Ctrl+X]", "cut selection"),
        ]],
        Tab::Cache => vec![vec![
            ("[Tab]", "cycle subtabs: Stats → View → Config"),
            ("[r]", "restart simulation"),
            ("[p]", "pause / resume execution"),
            ("[v]", "cycle sidebar view: RAM → REGS → Dyn"),
            ("[k]", "cycle RAM region: DATA → STACK → R/W → HEAP"),
            ("[f]", "cycle speed: 1x → 2x → 4x → 8x → GO"),
            ("[e]", "toggle exec count display"),
            ("[y]", "toggle instruction type badge"),
            ("[i]", "scope → I-Cache (Stats/View)"),
            ("[d]", "scope → D-Cache (Stats/View)"),
            ("[b]", "scope → Both (Stats/View)"),
            ("[+/-]", "add / remove extra cache level"),
            ("", ""),
            (
                "[m] View",
                "cycle cell data format: HEX → DEC-U → DEC-S → FLOAT",
            ),
            ("[g] View", "cycle byte grouping: 1B → 2B → 4B"),
            ("[t] View", "toggle address / tag display (0x… ↔ t:…)"),
            (
                "[↑/↓] View",
                "scroll the active cache panel; in Both mode this follows the focused panel",
            ),
            ("[←/→] View", "horizontal scroll for the active cache panel"),
            (
                "Stats total",
                "Program total uses the same global clock as Pipeline when pipeline is enabled",
            ),
            (
                "I/D/L2/L3 svc",
                "service cycles are level-local cache work, not slices of program total",
            ),
            (
                "Hit rate History",
                "updates on each sequential step or pipeline commit",
            ),
            ("[Ctrl+E]", "export cache config (.fcache)"),
            ("[Ctrl+L]", "import cache config (.fcache)"),
            ("[Ctrl+R]", "export simulation results (.fstats / .csv)"),
        ]],
        Tab::Docs => vec![vec![
            ("[↑/↓]", "scroll documentation"),
            ("[PgUp/PgDn]", "fast scroll"),
            ("[Ctrl+F]", "open search bar (filter by name/desc)"),
            ("[←/→]", "navigate type filter"),
            ("[Space]", "toggle selected type filter / restore All"),
        ]],
        Tab::Pipeline => vec![vec![
            ("[s]", "step one cycle"),
            ("[Space/p] Main", "run / pause"),
            ("[Tab]", "switch subtab: Main ↔ Config"),
            ("Core [[]/]]", "select which core/hart pipeline to inspect"),
            ("[e]", "toggle pipeline enabled"),
            ("[f]", "cycle speed"),
            ("[b]", "cycle branch resolve stage: ID → EX → MEM"),
            ("[↑/↓] Config", "navigate config fields"),
            ("[Enter] Config", "toggle the selected config row"),
            (
                "Config",
                "edit bypasses, mode, branch resolve stage and predictor",
            ),
            (
                "Branch Predict",
                "cycles Not-Taken → Always-Taken → BTFNT → 2-bit Dynamic",
            ),
            (
                "BTFNT",
                "backward branches taken, forward branches not taken",
            ),
            ("[Ctrl+E]", "export pipeline config (.pcfg)"),
            ("[Ctrl+L]", "import pipeline config (.pcfg)"),
            ("[Ctrl+R]", "export pipeline results (.pstats / .csv)"),
            ("Hazard Map", "shows RAW / load-use / flush / bypass traces"),
            ("History", "last cycles and per-instruction stage timeline"),
        ]],
        Tab::Config => vec![vec![
            ("[↑/↓]", "navigate settings"),
            ("[Enter]", "edit CPI field / toggle bool"),
            ("[Esc]", "cancel edit"),
            ("[Tab]", "confirm edit and move to next field"),
            ("[click]", "toggle bool button / start CPI edit"),
            (
                "Run Scope [ALL/FOCUS]",
                "ALL advances all harts; FOCUS advances only observed hart in Run",
            ),
            ("[Ctrl+E]", "export .rcfg"),
            ("[Ctrl+L]", "import .rcfg"),
        ]],
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

pub(crate) fn best_popup_rect(target: Rect, pw: u16, ph: u16, term: Rect) -> Rect {
    let gap = 1;
    let below_y = target.y + target.height + gap;
    if below_y + ph <= term.y + term.height {
        let x = clamp_x(target.x + target.width.saturating_sub(pw), pw, term);
        return Rect::new(x, below_y, pw, ph);
    }

    if target.y >= term.y + ph + gap {
        let x = clamp_x(target.x + target.width.saturating_sub(pw), pw, term);
        return Rect::new(x, target.y - ph - gap, pw, ph);
    }

    centered_rect(pw, ph, term)
}

fn clamp_x(preferred: u16, pw: u16, term: Rect) -> u16 {
    let max_x = (term.x + term.width).saturating_sub(pw);
    preferred.min(max_x).max(term.x)
}
