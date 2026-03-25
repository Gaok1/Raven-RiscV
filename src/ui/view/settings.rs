// ui/view/settings.rs — Config tab renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::ui::app::{App, CpiConfig, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_CPI_START, SETTINGS_ROWS};
use crate::ui::theme;

pub(super) fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(" Config ", Style::default().fg(theme::ACCENT).bold()));

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 { return; }

    // Two-column layout: settings list (left) | description panel (right)
    let col_w = inner.width.min(80);
    let col_x = inner.x + (inner.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, inner.y, col_w, inner.height);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(35), Constraint::Min(10)])
        .split(col_area);

    render_settings_list(f, cols[0], app);
    render_hint_panel(f, cols[1], app);
}

fn bool_button(value: bool, hovered: bool) -> Span<'static> {
    let (label, bg) = if value {
        ("[ TRUE  ]", theme::RUNNING)
    } else {
        ("[ FALSE ]", theme::DANGER)
    };
    let style = if hovered {
        Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::HOVER_BG)
    } else {
        Style::default().fg(Color::Rgb(0, 0, 0)).bg(bg)
    };
    Span::styled(label, style)
}

fn render_settings_list(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.selected;
    let names = CpiConfig::field_names();
    let descs = CpiConfig::descriptions();

    // Record geometry for mouse handling
    let mut rows_y = [0u16; 10];

    let mut items: Vec<ListItem> = Vec::new();

    // ── Section: Simulation ──────────────────────────────────────────────

    // Row 0: Cache Enabled toggle
    let is_sel_bool = sel == SETTINGS_ROW_CACHE_ENABLED;
    let label_style = if is_sel_bool {
        Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::ACCENT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let bool_row = Line::from(vec![
        Span::styled(format!("{:<20}", "  Cache Enabled"), label_style),
        Span::raw("  "),
        bool_button(app.run.cache_enabled, app.settings.hover_cache_enabled),
    ]);
    items.push(ListItem::new(bool_row));

    // Row 1: blank separator
    items.push(ListItem::new(Line::raw("")));

    // ── Section: CPI Config ──────────────────────────────────────────────
    for (i, &name) in names.iter().enumerate() {
        let row_idx = SETTINGS_ROW_CPI_START + i;
        let is_sel = sel == row_idx;
        let is_hov = app.settings.hover_cpi_field == Some(i);
        let is_editing = app.settings.cpi_editing && is_sel;

        let val_str = if is_editing {
            format!("{}_", app.settings.cpi_edit_buf)
        } else {
            format!("{}", app.run.cpi_config.get(i))
        };

        let name_style = if is_sel {
            Style::default().fg(Color::Rgb(0, 0, 0)).bg(theme::CPI_PANEL).bold()
        } else if is_hov {
            Style::default().fg(theme::CPI_PANEL).bg(Color::Rgb(30, 50, 40))
        } else {
            Style::default().fg(theme::CPI_PANEL)
        };
        let val_style = if is_sel && is_editing {
            Style::default().fg(theme::LABEL_Y).bold()
        } else if is_sel || is_hov {
            Style::default().fg(theme::LABEL_Y)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let desc_style = if is_hov {
            Style::default().fg(theme::LABEL)
        } else {
            Style::default().fg(theme::BORDER)
        };
        let desc = descs.get(i).copied().unwrap_or("");

        let line = Line::from(vec![
            Span::styled(format!("  {name:<10}"), name_style),
            Span::styled(format!("{val_str:>6}  "), val_style),
            Span::styled(desc.to_string(), desc_style),
        ]);
        let mut item = ListItem::new(line);
        if !is_sel && is_hov {
            item = item.style(Style::default().bg(Color::Rgb(30, 50, 40)));
        }
        items.push(item);

        // Record y position of each CPI row
        rows_y[i] = area.y + (SETTINGS_ROW_CPI_START + i) as u16;
    }

    // Record the bool button y for mouse detection
    let bool_btn_y = area.y;
    // Bool button starts after: 20-char padded label + 2-space gap = column 22
    let bool_btn_x = area.x + 22;
    let bool_btn_label_w = 9u16; // "[ TRUE  ]" or "[ FALSE ]"
    app.settings.bool_btn_rect.set((bool_btn_y, bool_btn_x, bool_btn_x + bool_btn_label_w));
    app.settings.cpi_rows_y.set(rows_y);

    f.render_widget(List::new(items), area);
}

fn render_hint_panel(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.selected;

    let hint = if sel == SETTINGS_ROW_CACHE_ENABLED {
        vec![
            Line::from(Span::styled("Cache Enabled", Style::default().fg(theme::ACCENT).bold())),
            Line::raw(""),
            Line::from(Span::styled(
                "When disabled, all memory accesses",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "go directly to RAM — no cache",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "latency, no statistics.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "CPI config still applies.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel >= SETTINGS_ROW_CPI_START && sel < SETTINGS_ROWS {
        let i = sel - SETTINGS_ROW_CPI_START;
        let name = CpiConfig::field_names().get(i).copied().unwrap_or("");
        let desc = CpiConfig::descriptions().get(i).copied().unwrap_or("");
        vec![
            Line::from(Span::styled(name, Style::default().fg(theme::CPI_PANEL).bold())),
            Line::raw(""),
            Line::from(Span::styled(desc.to_string(), Style::default().fg(theme::TEXT))),
            Line::raw(""),
            Line::from(Span::styled(
                format!("Current: {}", app.run.cpi_config.get(i)),
                Style::default().fg(theme::LABEL_Y),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", Style::default().fg(theme::LABEL)),
            ]),
            Line::from(vec![
                Span::styled("↑/↓  ", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = navigate", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else {
        vec![]
    };

    f.render_widget(Paragraph::new(hint).wrap(ratatui::widgets::Wrap { trim: false }), area);
}
