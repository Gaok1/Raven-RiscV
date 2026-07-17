// ui/view/settings.rs — Settings tab renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{List, ListItem, Paragraph},
};

use crate::ui::app::{
    App, CpiConfig, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_CPI_START, SETTINGS_ROW_JIT_MODE,
    SETTINGS_ROW_MAX_CORES, SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED,
    SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROW_SCREEN_TARGET, SETTINGS_ROW_TLB_ENABLED,
    SETTINGS_ROW_TRACE_SYSCALLS, SETTINGS_ROW_VM_ENABLED, SETTINGS_ROWS,
};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, bool_value, dense_action, dense_value, label_span};
use crate::ui::view::style;

pub(super) fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel(" Settings ", PanelKind::Accent));
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
    let cache_state = ControlState::from(
        sel == SETTINGS_ROW_CACHE_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_CACHE_ENABLED),
    );
    let cache_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Cache Enabled"), cache_state, theme::LABEL),
        Span::raw("  "),
        bool_value(app.run.cache_enabled, app.settings.hover_cache_enabled),
    ]));
    items.push(cache_item);

    // Row 1: Max cores
    let is_sel_cores = sel == SETTINGS_ROW_MAX_CORES;
    let is_editing_cores = app.settings.cpi_editing && is_sel_cores;
    let cores_state = ControlState::from(
        is_sel_cores,
        app.settings.hover_row == Some(SETTINGS_ROW_MAX_CORES),
    );
    let cores_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Max Cores"), cores_state, theme::LABEL),
        Span::raw("  "),
        Span::styled(
            if is_editing_cores {
                format!("[ {:>2}█ ]", app.settings.cpi_edit_buf)
            } else {
                format!("[ {:>2} ]", app.max_cores)
            },
            Style::default().fg(theme::LABEL_Y).bold(),
        ),
    ]));
    items.push(cores_item);

    // Row 2: Mem Size
    let is_sel_mem = sel == SETTINGS_ROW_MEM_SIZE;
    let is_editing_mem = app.settings.cpi_editing && is_sel_mem;
    let mem_state = ControlState::from(
        is_sel_mem,
        app.settings.hover_row == Some(SETTINGS_ROW_MEM_SIZE),
    );
    let mem_kb = app.run.mem_size / 1024;
    let mem_display = if mem_kb % 1024 == 0 {
        format!("{} MB", mem_kb / 1024)
    } else {
        format!("{} KB", mem_kb)
    };
    let mem_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Mem Size"), mem_state, theme::LABEL),
        Span::raw("  "),
        Span::styled(
            if is_editing_mem {
                format!("[ {}█]", app.settings.cpi_edit_buf)
            } else {
                format!("[ {}]", mem_display)
            },
            Style::default().fg(theme::LABEL_Y).bold(),
        ),
    ]));
    items.push(mem_item);

    // Row 3: Run scope
    let scope_state = ControlState::from(
        sel == SETTINGS_ROW_RUN_SCOPE,
        app.settings.hover_row == Some(SETTINGS_ROW_RUN_SCOPE),
    );
    let scope_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Run Scope"), scope_state, theme::LABEL),
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
    let pipe_state = ControlState::from(
        sel == SETTINGS_ROW_PIPELINE_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_PIPELINE_ENABLED),
    );
    let pipe_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Pipeline Enabled"), pipe_state, theme::LABEL),
        Span::raw("  "),
        bool_value(app.run.pipeline().enabled, app.settings.hover_pipeline_enabled),
    ]));
    items.push(pipe_item);

    // Row 5: VM Enabled toggle (Sv32 + TLB)
    let vm_state = ControlState::from(
        sel == SETTINGS_ROW_VM_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_VM_ENABLED),
    );
    let vm_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Virtual Memory"), vm_state, theme::LABEL),
        Span::raw("  "),
        dense_value(
            app.vm_mode().as_str(),
            app.settings.hover_vm_enabled,
            true,
            theme::LABEL_Y,
        ),
    ]));
    items.push(vm_item);

    // Row 6: TLB Enabled toggle (cache the page-table walks, or always walk).
    let vm_off = !app.run.vm_enabled();
    let tlb_state = ControlState::from(
        sel == SETTINGS_ROW_TLB_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_TLB_ENABLED),
    )
    .disabled_if(vm_off);
    let mut tlb_spans = vec![
        label_span(format!("{:<20}", "  TLB Enabled"), tlb_state, theme::LABEL),
        Span::raw("  "),
        bool_value(app.run.tlb_enabled, app.settings.hover_tlb_enabled),
    ];
    if vm_off {
        tlb_spans.push(Span::raw("  "));
        tlb_spans.push(Span::styled(
            "(no effect — VM off)",
            Style::default().fg(theme::BORDER),
        ));
    }
    items.push(ListItem::new(Line::from(tlb_spans)));

    // Row 7: JIT mode selector
    let jit_state = ControlState::from(
        sel == SETTINGS_ROW_JIT_MODE,
        app.settings.hover_row == Some(SETTINGS_ROW_JIT_MODE),
    );
    let jit_label = app.run.jit_kind.as_str().to_uppercase();
    #[cfg(feature = "jit")]
    let jit_unavailable = false;
    #[cfg(not(feature = "jit"))]
    let jit_unavailable = app.run.jit_kind != crate::falcon::jit::BackendKind::None;
    let mut jit_spans = vec![
        label_span(format!("{:<20}", "  JIT Mode"), jit_state, theme::LABEL),
        Span::raw("  "),
        dense_value(&jit_label, app.settings.hover_jit_mode, true, theme::LABEL_Y),
    ];
    if jit_unavailable {
        jit_spans.push(Span::raw("  "));
        jit_spans.push(Span::styled(
            "recompile com --features jit",
            style::danger(),
        ));
    }
    let jit_item = ListItem::new(Line::from(jit_spans));
    items.push(jit_item);

    // Row 6: Syscall debug log toggle
    let is_hov_trace = app.settings.hover_row == Some(SETTINGS_ROW_TRACE_SYSCALLS);
    let trace_state = ControlState::from(sel == SETTINGS_ROW_TRACE_SYSCALLS, is_hov_trace);
    let trace_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  Syscall Debug Log"), trace_state, theme::LABEL),
        Span::raw("  "),
        bool_value(app.run.trace_syscalls, app.settings.hover_trace_syscalls),
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

    // Row 9: screen output selector (graphics syscalls 2000+)
    let is_sel_screen = sel == SETTINGS_ROW_SCREEN_TARGET;
    let is_hov_screen = app.settings.hover_row == Some(SETTINGS_ROW_SCREEN_TARGET);
    let label_style_screen = if is_sel_screen {
        Style::default().fg(theme::ACCENT).bold()
    } else if is_hov_screen {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::LABEL)
    };
    let screen_item = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<20}", "  Screen Output"), label_style_screen),
        Span::raw("  "),
        dense_value(
            app.console.screen_target.label(),
            app.settings.hover_screen_target,
            true,
            theme::LABEL_Y,
        ),
    ]));
    items.push(screen_item);

    // Row 10: blank separator
    items.push(ListItem::new(Line::raw("")));

    // ── Section: CPI Config ──────────────────────────────────────────────
    for (i, &name) in names.iter().enumerate() {
        let row_idx = SETTINGS_ROW_CPI_START + i;
        let is_sel = sel == row_idx;
        let is_hov = app.settings.hover_cpi_field == Some(i);
        let is_editing = app.settings.cpi_editing && is_sel;

        let val_str = if is_editing {
            format!("{}█", app.settings.cpi_edit_buf)
        } else {
            format!("{}", app.run.cpi_config.get(i))
        };

        let val_style = if is_sel && is_editing {
            Style::default().fg(theme::LABEL_Y).bold()
        } else if is_sel || is_hov {
            Style::default().fg(theme::LABEL_Y)
        } else {
            style::value()
        };
        let desc_style = if is_hov {
            style::label()
        } else {
            Style::default().fg(theme::BORDER)
        };
        let desc = descs.get(i).copied().unwrap_or("");

        let line = Line::from(vec![
            label_span(
                format!("  {name:<10}"),
                ControlState::from(is_sel, is_hov),
                theme::CPI_PANEL,
            ),
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
    app.settings
        .bool_btn_vm_rect
        .set((area.y + 5, bool_btn_x, bool_btn_x + bool_btn_label_w));
    app.settings
        .bool_btn_tlb_rect
        .set((area.y + 6, bool_btn_x, bool_btn_x + bool_btn_label_w));
    app.settings.bool_btn_trace_syscalls_rect.set((
        area.y + 8,
        bool_btn_x,
        bool_btn_x + bool_btn_label_w,
    ));
    app.settings.screen_target_rect.set((
        area.y + 9,
        bool_btn_x,
        bool_btn_x + 6, // "WINDOW"
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
                style::value(),
            )),
            Line::from(Span::styled(
                "go directly to RAM — no cache",
                style::value(),
            )),
            Line::from(Span::styled("latency, no statistics.", style::value())),
            Line::raw(""),
            Line::from(Span::styled("CPI config still applies.", style::label())),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", style::label()),
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
                style::value(),
            )),
            Line::from(Span::styled(
                "When ON, the Pipeline tab shows",
                style::value(),
            )),
            Line::from(Span::styled(
                "the 5-stage CPU pipeline view.",
                style::value(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", style::label()),
            ]),
        ]
    } else if sel == SETTINGS_ROW_TLB_ENABLED {
        vec![
            Line::from(Span::styled(
                "TLB Enabled",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "When ON, translations are cached in the",
                style::value(),
            )),
            Line::from(Span::styled(
                "TLB: repeat accesses hit (1 cyc).",
                style::value(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "When OFF, every access walks the page",
                style::value(),
            )),
            Line::from(Span::styled(
                "table — all misses, miss penalty each time.",
                style::value(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Only matters while Virtual Memory is on.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", style::label()),
            ]),
        ]
    } else if sel == SETTINGS_ROW_JIT_MODE {
        vec![
            Line::from(Span::styled(
                "JIT Mode",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "none  — interpreter puro (padrão)",
                style::value(),
            )),
            Line::from(Span::styled(
                "hot   — compila blocos quentes",
                style::value(),
            )),
            Line::from(Span::styled(
                "full  — scan eager ao carregar",
                style::value(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "hot/full requerem --features jit.",
                style::label(),
            )),
            Line::from(Span::styled(
                "Aviso vermelho = sem efeito nesta build.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = ciclar modo", style::label()),
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
                style::value(),
            )),
            Line::from(Span::styled("debug console in yellow.", style::value())),
            Line::raw(""),
            Line::from(Span::styled(
                "Read/write-style syscalls stay silent",
                style::label(),
            )),
            Line::from(Span::styled("to avoid console noise.", style::label())),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Hover", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = show this help", style::label()),
            ]),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", style::label()),
            ]),
        ]
    } else if sel == SETTINGS_ROW_SCREEN_TARGET {
        vec![
            Line::from(Span::styled(
                "Screen Output",
                Style::default().fg(theme::ACCENT).bold(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Where a program's screen (graphics",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "syscalls 2000+) is displayed.",
                Style::default().fg(theme::TEXT),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "TUI: Screen sub-view of the Run tab.",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                "WINDOW: native OS window (not on macOS).",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Applies at the next screen_init / reset.",
                Style::default().fg(theme::LABEL),
            )),
            Line::raw(""),
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
                style::value(),
            )),
            Line::from(Span::styled("Must be a power of two.", style::value())),
            Line::raw(""),
            Line::from(Span::styled("Accepts: 16mb  8192kb  4096", style::label())),
            Line::from(Span::styled("Plain number = KB.", style::label())),
            Line::raw(""),
            Line::from(Span::styled(
                "Changing it restarts the simulation.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", style::label()),
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
                style::value(),
            )),
            Line::from(Span::styled(
                "multiple harts when more than one core exists.",
                style::value(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "ALL: active harts advance together.",
                style::label(),
            )),
            Line::from(Span::styled(
                "FOCUS: only the observed hart advances.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" / Click = toggle", style::label()),
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
                style::value(),
            )),
            Line::from(Span::styled(
                "available for harts in this run.",
                style::value(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Changing it restarts the simulation.",
                style::label(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", style::label()),
            ]),
            Line::from(vec![
                Span::styled("1..32", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = commit value", style::label()),
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
            Line::from(Span::styled(desc.to_string(), style::value())),
            Line::raw(""),
            Line::from(Span::styled(
                format!("Current: {}", app.run.cpi_config.get(i)),
                Style::default().fg(theme::LABEL_Y),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = edit", style::label()),
            ]),
            Line::from(vec![
                Span::styled("↑/↓  ", Style::default().fg(theme::LABEL_Y)),
                Span::styled(" = navigate", style::label()),
            ]),
        ]
    } else {
        vec![]
    };

    f.render_widget(
        Paragraph::new(hint).wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("Actions", PanelKind::Plain));
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
        Span::styled("   Ctrl+l = import  Ctrl+e = export", style::label()),
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
