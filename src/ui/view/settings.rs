// ui/view/settings.rs — Config tab renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::ui::app::{
    App, CpiConfig, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_CPI_START, SETTINGS_ROW_MAX_CORES,
    SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED, SETTINGS_ROW_RUN_SCOPE,
    SETTINGS_ROW_TRACE_SYSCALLS, SETTINGS_ROWS,
};
use crate::ui::theme;
use crate::ui::view::components::{dense_action, dense_value};

pub(super) fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Config ",
            Style::default().fg(theme::ACCENT).bold(),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    // Two-column layout plus bottom controls bar.
    let col_w = inner.width.min(80);
    let col_x = inner.x + (inner.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, inner.y, col_w, inner.height);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(col_area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(35), Constraint::Min(10)])
        .split(layout[0]);

    render_settings_list(f, cols[0], app);
    render_hint_panel(f, cols[1], app);
    render_controls_bar(f, layout[1], app);
}

fn bool_button(value: bool, hovered: bool) -> Span<'static> {
    let (label, color) = if value {
        ("true", theme::RUNNING)
    } else {
        ("false", theme::DANGER)
    };
    dense_value(label, hovered, true, color)
}

fn render_settings_list(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.selected;
    let names = CpiConfig::field_names();
    let descs = CpiConfig::descriptions();
    app.settings
        .list_rect
        .set((area.x, area.y, area.width, area.height));

    // Record geometry for mouse handling
    let mut rows_y = [0u16; 11];

    let mut items: Vec<ListItem> = Vec::new();

    // ── Section: Simulation ──────────────────────────────────────────────

    // Row 0: Cache Enabled toggle
    let is_sel_cache = sel == SETTINGS_ROW_CACHE_ENABLED;
    let is_hov_cache = app.settings.hover_row == Some(SETTINGS_ROW_CACHE_ENABLED);
    let label_style_cache = if is_sel_cache {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_cache {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let cache_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Cache Enabled"), label_style_cache),
        Span::raw("  "),
        bool_button(app.run.cache_enabled, app.settings.hover_cache_enabled),
    ]));
    items.push(cache_item);

    // Row 1: Max cores
    let is_sel_cores = sel == SETTINGS_ROW_MAX_CORES;
    let is_hov_cores = app.settings.hover_row == Some(SETTINGS_ROW_MAX_CORES);
    let is_editing_cores = app.settings.cpi_editing && is_sel_cores;
    let label_style_cores = if is_sel_cores {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_cores {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let cores_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Max Cores"), label_style_cores),
        Span::raw("  "),
        Span::styled(
            if is_editing_cores {
                format!("[ {:>2}_ ]", app.settings.cpi_edit_buf)
            } else {
                format!("[ {:>2} ]", app.max_cores)
            },
            if is_editing_cores {
                Style::default().fg(theme::LABEL_Y).bold()
            } else {
                Style::default().fg(theme::LABEL_Y).bold()
            },
        ),
    ]));
    items.push(cores_item);

    // Row 2: Mem Size
    let is_sel_mem = sel == SETTINGS_ROW_MEM_SIZE;
    let is_hov_mem = app.settings.hover_row == Some(SETTINGS_ROW_MEM_SIZE);
    let is_editing_mem = app.settings.cpi_editing && is_sel_mem;
    let label_style_mem = if is_sel_mem {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_mem {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let mem_kb = app.run.mem_size / 1024;
    let mem_display = if mem_kb % 1024 == 0 {
        format!("{} MB", mem_kb / 1024)
    } else {
        format!("{} KB", mem_kb)
    };
    let mem_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Mem Size"), label_style_mem),
        Span::raw("  "),
        Span::styled(
            if is_editing_mem {
                format!("[ {}_]", app.settings.cpi_edit_buf)
            } else {
                format!("[ {}]", mem_display)
            },
            Style::default().fg(theme::LABEL_Y).bold(),
        ),
    ]));
    items.push(mem_item);

    // Row 3: Run scope
    let is_sel_scope = sel == SETTINGS_ROW_RUN_SCOPE;
    let is_hov_scope = app.settings.hover_row == Some(SETTINGS_ROW_RUN_SCOPE);
    let label_style_scope = if is_sel_scope {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_scope {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let scope_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Run Scope"), label_style_scope),
        Span::raw("  "),
        dense_value(
            app.run_scope.label(),
            app.settings.hover_run_scope,
            true,
            theme::LABEL_Y,
        ),
    ]));
    items.push(scope_item);

    // Row 4: Pipeline Enabled toggle
    let is_sel_pipe = sel == SETTINGS_ROW_PIPELINE_ENABLED;
    let is_hov_pipe = app.settings.hover_row == Some(SETTINGS_ROW_PIPELINE_ENABLED);
    let label_style_pipe = if is_sel_pipe {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_pipe {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let pipe_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Pipeline Enabled"), label_style_pipe),
        Span::raw("  "),
        bool_button(app.pipeline.enabled, app.settings.hover_pipeline_enabled),
    ]));
    items.push(pipe_item);

    // Row 5: Syscall debug log toggle
    let is_sel_trace = sel == SETTINGS_ROW_TRACE_SYSCALLS;
    let is_hov_trace = app.settings.hover_row == Some(SETTINGS_ROW_TRACE_SYSCALLS);
    let label_style_trace = if is_sel_trace {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_trace {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let trace_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Syscall Debug Log"), label_style_trace),
        Span::raw("  "),
        bool_button(
            app.run.trace_syscalls,
            app.settings.hover_trace_syscalls,
        ),
        Span::raw("  "),
        Span::styled(
            "[?]",
            if is_hov_trace {
                Style::default().fg(theme::ACCENT).bold()
            } else {
                Style::default().fg(theme::BORDER)
            },
        ),
    ]));
    items.push(trace_item);

    // Row 6: blank separator
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
            Style::default().fg(theme::CPI_PANEL).bold()
        } else if is_hov {
            Style::default().fg(theme::TEXT).bold()
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
        let item = ListItem::new(line);
        items.push(item);

        // Record y position of each CPI row (offset by the number of bool rows above)
        rows_y[i] = area.y + (SETTINGS_ROW_CPI_START + i) as u16;
    }

    // Record bool button positions for mouse detection
    // Both bool buttons share the same x offset: 20-char label + 2-space gap = column 22
    let bool_btn_x = area.x + 22;
    let bool_btn_label_w = 5u16; // "false"
    app.settings
        .bool_btn_rect
        .set((area.y, bool_btn_x, bool_btn_x + bool_btn_label_w));
    app.settings
        .run_scope_rect
        .set((area.y + 3, bool_btn_x, bool_btn_x + 5));
    app.settings.bool_btn_pipeline_rect.set((
        area.y + 4,
        bool_btn_x,
        bool_btn_x + bool_btn_label_w,
    ));
    app.settings.bool_btn_trace_syscalls_rect.set((
        area.y + 5,
        bool_btn_x,
        bool_btn_x + bool_btn_label_w,
    ));
    app.settings.cpi_rows_y.set(rows_y);

    f.render_widget(List::new(items), area);
}

fn render_hint_panel(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.hover_row.unwrap_or(app.settings.selected);

    let hint = if sel == SETTINGS_ROW_CACHE_ENABLED {
        vec![
            Line::from(Span::styled(
                "Cache Enabled",
                Style::default().fg(theme::ACCENT).bold(),
            )),
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
    } else if sel == SETTINGS_ROW_PIPELINE_ENABLED {
        vec![
            Line::from(Span::styled(
                "Pipeline Enabled",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Enables the Pipeline tab simulator.",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "When ON, the Pipeline tab shows",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "the 5-stage CPU pipeline view.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel == SETTINGS_ROW_TRACE_SYSCALLS {
        vec![
            Line::from(Span::styled(
                "Syscall Debug Log",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Logs each non-I/O syscall to the",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "debug console in yellow.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Read/write-style syscalls stay silent",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                "to avoid console noise.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Hover", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = show this help", Style::default().fg(theme::LABEL)),
            ]),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel == SETTINGS_ROW_MEM_SIZE {
        vec![
            Line::from(Span::styled(
                "Mem Size",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Total RAM available to the simulator.",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "Must be a power of two.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Accepts: 16mb  8192kb  4096",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                "Plain number = KB.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Changing it restarts the simulation.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel == SETTINGS_ROW_RUN_SCOPE {
        vec![
            Line::from(Span::styled(
                "Run Scope",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Controls how the Run tab advances",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "multiple harts when more than one core exists.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "ALL: active harts advance together.",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                "FOCUS: only the observed hart advances.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel == SETTINGS_ROW_MAX_CORES {
        vec![
            Line::from(Span::styled(
                "Max Cores",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Maximum number of physical cores",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "available for harts in this run.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Changing it restarts the simulation.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", Style::default().fg(theme::LABEL)),
            ]),
            Line::from(vec![
                Span::styled("1..32", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = commit value", Style::default().fg(theme::LABEL)),
            ]),
        ]
    } else if sel >= SETTINGS_ROW_CPI_START && sel < SETTINGS_ROWS {
        let i = sel - SETTINGS_ROW_CPI_START;
        let name = CpiConfig::field_names().get(i).copied().unwrap_or("");
        let desc = CpiConfig::descriptions().get(i).copied().unwrap_or("");
        vec![
            Line::from(Span::styled(
                name,
                Style::default().fg(theme::CPI_PANEL).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                desc.to_string(),
                Style::default().fg(theme::TEXT),
            )),
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

fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Actions", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        app.settings.import_rcfg_rect.set((0, 0, 0));
        app.settings.export_rcfg_rect.set((0, 0, 0));
        return;
    }

    let line = Line::from(vec![
        Span::raw(" "),
        dense_action("import", theme::ACCENT, app.settings.hover_import_rcfg),
        Span::raw("   "),
        dense_action("export", theme::ACCENT, app.settings.hover_export_rcfg),
        Span::styled(
            "   Ctrl+l = import  Ctrl+e = export",
            Style::default().fg(theme::LABEL),
        ),
    ]);
    f.render_widget(Paragraph::new(vec![line]), inner);

    let mut x = inner.x + 1;
    let import_x0 = x;
    let import_x1 = import_x0 + "import".len() as u16;
    x = import_x1 + 3;
    let export_x0 = x;
    let export_x1 = export_x0 + "export".len() as u16;
    app.settings
        .import_rcfg_rect
        .set((inner.y, import_x0, import_x1));
    app.settings
        .export_rcfg_rect
        .set((inner.y, export_x0, export_x1));
}
