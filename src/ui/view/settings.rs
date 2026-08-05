// ui/view/settings.rs — Settings tab renderer
use ratatui::{
    Frame,
    prelude::*,
    widgets::{List, ListItem, Paragraph},
};

use crate::ui::app::{
    App, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_JIT_MODE,
    SETTINGS_ROW_MAX_CORES, SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED,
    SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROW_SCREEN_TARGET, SETTINGS_ROW_TLB_ENABLED,
    SETTINGS_ROW_TRACE_SYSCALLS, SETTINGS_ROW_VM_ENABLED, SETTINGS_ROWS,
};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{
    ControlState, Toolbar, bool_value, dense_value, edit_value, label_span,
};
use crate::ui::view::style;

/// Columns the explanation column claims on the right.
const HINT_W: u16 = 46;

/// Columns the settings column claims. Wide enough for the longest CPI
/// description, so the explanation beside it lands next to the rows it explains
/// instead of drifting to the far edge of a 158-column panel.
const LIST_W: u16 = 76;

/// A section heading inside the settings list, set in from the panel edge like
/// every other labelled rule in the app.
fn section_rule(name: &str, width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(panel::section_rule(name, width.saturating_sub(2)).spans);
    Line::from(spans)
}

pub(super) fn render_settings(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("Settings", PanelKind::Accent));
    if inner.height == 0 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    // The architecture is state, so it reads as a labelled value. `[a] switch`
    // used to trail it — a key, in the chrome, which is the one place the design
    // system does not put keys. Rendered after the columns below, so it can line
    // its left edge up with theirs.
    let head_line = {
        let mut head = vec![Span::raw(" ")];
        head.extend(style::readout(
            "architecture",
            app.architecture.descriptor().display_name,
            theme::ACCENT,
        ));
        Line::from(head)
    };

    // The settings take the width they need and the explanation takes the rest.
    // Both used to be squeezed into an 80-column strip centred in a 158-column
    // panel, then split again — which left the rows ~40 columns, so the CPI
    // descriptions were cut mid-word ("branch when not take") and the TLB row's
    // note ran straight into the explanation beside it.
    // The block is sized to its content and then centred, rather than pinned to
    // the left edge with the slack piling up on the right. Centring a *narrow*
    // strip is what truncated the descriptions before; centring one wide enough
    // to hold them costs nothing.
    let list_w = LIST_W.min(layout[1].width);
    let hint_w = HINT_W.min(layout[1].width.saturating_sub(list_w));
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(list_w),
            Constraint::Length(hint_w),
            Constraint::Min(0),
        ])
        .split(layout[1]);
    let (body, hint, head_x) = (cols[1], cols[2], cols[1].x);

    f.render_widget(
        Paragraph::new(head_line),
        Rect::new(
            head_x,
            layout[0].y,
            layout[0].width.saturating_sub(head_x - layout[0].x),
            layout[0].height,
        ),
    );
    render_settings_list(f, body, app);
    render_hint_panel(f, hint, app);
    render_controls_bar(f, body, layout[2], app);
}

fn render_settings_list(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.selected;
    app.settings
        .list_rect
        .set((area.x, area.y, area.width, area.height));

    let mut items: Vec<ListItem> = Vec::new();

    // Where each logical settings row ended up on screen. Filled as the rows are
    // pushed, so the headings below can shift everything without any hitbox
    // needing to be re-counted.
    let mut row_of = [0usize; SETTINGS_ROWS];
    macro_rules! at {
        ($idx:expr) => {{
            row_of[$idx] = items.len();
        }};
    }

    // ── Section: Simulation ──────────────────────────────────────────────
    items.push(ListItem::new(section_rule("SIMULATION", area.width)));
    at!(SETTINGS_ROW_CACHE_ENABLED);

    // Row 0: Cache Enabled toggle
    let cache_state = ControlState::from(
        sel == SETTINGS_ROW_CACHE_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_CACHE_ENABLED),
    );
    let cache_item = ListItem::new(Line::from(vec![
        label_span(
            format!("{:<20}", "  cache"),
            cache_state,
            theme::LABEL,
        ),
        Span::raw("  "),
        bool_value(app.session.cache_enabled, app.settings.hover_cache_enabled),
    ]));
    items.push(cache_item);

    // Row 1: Max cores
    at!(SETTINGS_ROW_MAX_CORES);
    let is_sel_cores = sel == SETTINGS_ROW_MAX_CORES;
    let is_editing_cores = app.settings.num_editing && is_sel_cores;
    let cores_state = ControlState::from(
        is_sel_cores,
        app.settings.hover_row == Some(SETTINGS_ROW_MAX_CORES),
    );
    // A pill, not `[ 4 ]`. The brackets said "keyboard key" on a value you edit
    // by clicking, and `edit_value` already owns the one `█` edit cursor.
    let cores_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  max cores"), cores_state, theme::LABEL),
        Span::raw("  "),
        edit_value(
            &app.max_cores.to_string(),
            is_editing_cores.then_some(app.settings.num_edit_buf.as_str()),
            app.settings.hover_row == Some(SETTINGS_ROW_MAX_CORES),
            theme::LABEL_Y,
        ),
    ]));
    items.push(cores_item);

    // Row 2: Mem Size
    at!(SETTINGS_ROW_MEM_SIZE);
    let is_sel_mem = sel == SETTINGS_ROW_MEM_SIZE;
    let is_editing_mem = app.settings.num_editing && is_sel_mem;
    let mem_state = ControlState::from(
        is_sel_mem,
        app.settings.hover_row == Some(SETTINGS_ROW_MEM_SIZE),
    );
    let mem_kb = app.session.mem_size / 1024;
    let mem_display = if mem_kb % 1024 == 0 {
        format!("{} MB", mem_kb / 1024)
    } else {
        format!("{} KB", mem_kb)
    };
    let mem_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  memory size"), mem_state, theme::LABEL),
        Span::raw("  "),
        edit_value(
            &mem_display,
            is_editing_mem.then_some(app.settings.num_edit_buf.as_str()),
            app.settings.hover_row == Some(SETTINGS_ROW_MEM_SIZE),
            theme::LABEL_Y,
        ),
    ]));
    items.push(mem_item);

    // Row 3: Run scope
    at!(SETTINGS_ROW_RUN_SCOPE);
    let scope_state = ControlState::from(
        sel == SETTINGS_ROW_RUN_SCOPE,
        app.settings.hover_row == Some(SETTINGS_ROW_RUN_SCOPE),
    );
    let scope_item = ListItem::new(Line::from(vec![
        label_span(format!("{:<20}", "  run scope"), scope_state, theme::LABEL),
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
    at!(SETTINGS_ROW_PIPELINE_ENABLED);
    let pipe_state = ControlState::from(
        sel == SETTINGS_ROW_PIPELINE_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_PIPELINE_ENABLED),
    );
    let pipe_item = ListItem::new(Line::from(vec![
        label_span(
            format!("{:<20}", "  pipeline"),
            pipe_state,
            theme::LABEL,
        ),
        Span::raw("  "),
        bool_value(
            app.pipeline_status().is_some_and(|status| status.enabled),
            app.settings.hover_pipeline_enabled,
        ),
    ]));
    items.push(pipe_item);

    // Row 5: VM Enabled toggle (Sv32 + TLB)
    at!(SETTINGS_ROW_VM_ENABLED);
    let vm_state = ControlState::from(
        sel == SETTINGS_ROW_VM_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_VM_ENABLED),
    );
    let vm_item = ListItem::new(Line::from(vec![
        label_span(
            format!("{:<20}", "  virtual memory"),
            vm_state,
            theme::LABEL,
        ),
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
    at!(SETTINGS_ROW_TLB_ENABLED);
    let vm_off = !app.session.vm_enabled();
    let tlb_state = ControlState::from(
        sel == SETTINGS_ROW_TLB_ENABLED,
        app.settings.hover_row == Some(SETTINGS_ROW_TLB_ENABLED),
    )
    .disabled_if(vm_off);
    let mut tlb_spans = vec![
        label_span(format!("{:<20}", "  TLB"), tlb_state, theme::LABEL),
        Span::raw("  "),
        bool_value(app.session.tlb_enabled, app.settings.hover_tlb_enabled),
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
    at!(SETTINGS_ROW_JIT_MODE);
    let jit_state = ControlState::from(
        sel == SETTINGS_ROW_JIT_MODE,
        app.settings.hover_row == Some(SETTINGS_ROW_JIT_MODE),
    );
    let jit_label = app.session.jit_kind.as_str().to_uppercase();
    #[cfg(feature = "jit")]
    let jit_unavailable = false;
    #[cfg(not(feature = "jit"))]
    let jit_unavailable = app.session.jit_kind != crate::falcon::jit::BackendKind::None;
    let mut jit_spans = vec![
        label_span(format!("{:<20}", "  JIT mode"), jit_state, theme::LABEL),
        Span::raw("  "),
        dense_value(
            &jit_label,
            app.settings.hover_jit_mode,
            true,
            theme::LABEL_Y,
        ),
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

    // Row 8: Syscall debug log toggle
    at!(SETTINGS_ROW_TRACE_SYSCALLS);
    let is_hov_trace = app.settings.hover_row == Some(SETTINGS_ROW_TRACE_SYSCALLS);
    let trace_state = ControlState::from(sel == SETTINGS_ROW_TRACE_SYSCALLS, is_hov_trace);
    let trace_item = ListItem::new(Line::from(vec![
        label_span(
            format!("{:<20}", "  syscall debug log"),
            trace_state,
            theme::LABEL,
        ),
        Span::raw("  "),
        bool_value(
            app.session.trace_syscalls,
            app.settings.hover_trace_syscalls,
        ),
        Span::raw("  "),
        // Clickable, so it wears the button surface rather than brackets.
        style::button_span("?", false, is_hov_trace),
    ]));
    items.push(trace_item);

    // Row 9: screen output selector (graphics syscalls 2000+)
    at!(SETTINGS_ROW_SCREEN_TARGET);
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
        Span::styled(format!("{:<20}", "  screen output"), label_style_screen),
        Span::raw("  "),
        dense_value(
            app.console.screen_target.label(),
            app.settings.hover_screen_target,
            true,
            theme::LABEL_Y,
        ),
    ]));
    items.push(screen_item);

    // The `CYCLES PER INSTRUCTION` section used to sit here. It moved to the
    // Pipeline tab's settings page, beside the datapath whose timing it sets —
    // this tab could only list the numbers, while the page that actually models
    // them had to show them read-only under a note pointing back here.

    // Value-chip hitboxes, measured off the rows as they were actually pushed.
    //
    // Two things were wrong before. The row was a counted constant (`area.y + 6`
    // for the TLB toggle), so inserting anything above it — a section heading,
    // say — silently moved every control's hitbox off its control. And the width
    // was a flat `5` ("false") when a chip renders one padding column either
    // side, leaving the last two columns of every toggle dead to the mouse and
    // most of `WINDOW` unhoverable.
    let value_x = area.x + 22;
    let chip = |row: usize, text: &str| {
        (
            area.y + row as u16,
            value_x,
            value_x + text.chars().count() as u16 + 2,
        )
    };
    let bool_w = |on: bool| if on { "true" } else { "false" };

    app.settings
        .bool_btn_rect
        .set(chip(row_of[SETTINGS_ROW_CACHE_ENABLED], bool_w(app.session.cache_enabled)));
    app.settings
        .run_scope_rect
        .set(chip(row_of[SETTINGS_ROW_RUN_SCOPE], app.run_scope.label()));
    app.settings.bool_btn_pipeline_rect.set(chip(
        row_of[SETTINGS_ROW_PIPELINE_ENABLED],
        bool_w(app.pipeline_status().is_some_and(|status| status.enabled)),
    ));
    app.settings
        .bool_btn_vm_rect
        .set(chip(row_of[SETTINGS_ROW_VM_ENABLED], app.vm_mode().as_str()));
    app.settings
        .bool_btn_tlb_rect
        .set(chip(row_of[SETTINGS_ROW_TLB_ENABLED], bool_w(app.session.tlb_enabled)));
    app.settings.bool_btn_trace_syscalls_rect.set(chip(
        row_of[SETTINGS_ROW_TRACE_SYSCALLS],
        bool_w(app.session.trace_syscalls),
    ));
    app.settings.screen_target_rect.set(chip(
        row_of[SETTINGS_ROW_SCREEN_TARGET],
        app.console.screen_target.label(),
    ));
    app.settings
        .row_ys
        .set(row_of.map(|row| area.y + row as u16));

    f.render_widget(List::new(items), area);
}

fn render_hint_panel(f: &mut Frame, area: Rect, app: &App) {
    let sel = app.settings.hover_row.unwrap_or(app.settings.selected);

    let hint = if sel == SETTINGS_ROW_CACHE_ENABLED {
        vec![
            Line::from(Span::styled(
                "cache",
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
                "pipeline",
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
                "TLB",
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
                "JIT mode",
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
                "syscall debug log",
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
                "screen output",
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
                "memory size",
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
                "run scope",
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
                "max cores",
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
    } else {
        vec![]
    };

    // Every block above ended with its own copy of `Enter / Click = toggle`.
    // How you operate a row is the same on all of them, so ten repetitions of it
    // are noise crowding out the explanation — and it is a key, which the footer
    // and the help overlay already carry. Filtered here rather than deleted from
    // ten places, so a new block cannot quietly reintroduce it.
    let mut hint: Vec<Line> = hint
        .into_iter()
        .filter(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            !(text.contains(" = toggle")
                || text.contains(" = cycle")
                || text.contains(" = edit")
                || text.contains(" = navigate")
                || text.contains(" = show this help"))
        })
        .collect();
    while hint.last().is_some_and(|line| line.width() == 0) {
        hint.pop();
    }

    // Indented off the settings column so the two do not read as one wide row.
    let body = Rect::new(
        area.x + 2,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    f.render_widget(
        Paragraph::new(hint).wrap(ratatui::widgets::Wrap { trim: false }),
        body,
    );
}

/// `col` gives the bar the same left edge and width as the settings column, so
/// the whole page reads as one centred block rather than a centred list with a
/// button row wandering off under it.
fn render_controls_bar(f: &mut Frame, col: Rect, row: Rect, app: &App) {
    let area = Rect::new(col.x, row.y, col.width, row.height);
    if area.height < 2 {
        app.settings.import_rcfg_rect.set((0, 0, 0));
        app.settings.export_rcfg_rect.set((0, 0, 0));
        return;
    }
    // A labelled rule, not a second bordered box nested inside the Settings
    // panel: that box spent two of its three rows drawing a frame around one
    // line holding two buttons.
    let inner = Rect::new(area.x, area.y + 1, area.width, 1);

    // Laid out once and then asked where each control landed, instead of
    // building spans here and re-deriving `x + "import".len()` below — those two
    // drifted the moment the buttons gained their surface. The `Ctrl+l = import`
    // hint is gone with them: keys belong in the footer, which already lists
    // these two.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Btn {
        Import,
        Export,
    }
    let mut bar = Toolbar::with_gap(1);
    bar.action(
        Btn::Import,
        "import",
        ControlState::chip(false, app.settings.hover_import_rcfg),
        theme::ACCENT,
    )
    .action(
        Btn::Export,
        "export",
        ControlState::chip(false, app.settings.hover_export_rcfg),
        theme::ACCENT,
    );

    let mut rule = vec![Span::raw(" ")];
    rule.extend(panel::section_rule("CONFIG FILE", area.width.saturating_sub(2)).spans);
    let mut spans = vec![Span::raw(" ")];
    spans.extend(bar.spans());
    f.render_widget(
        Paragraph::new(vec![Line::from(rule), Line::from(spans)]),
        area,
    );

    let origin = inner.x + 1;
    for (id, start, end) in bar.cells() {
        let rect = match id {
            Btn::Import => &app.settings.import_rcfg_rect,
            Btn::Export => &app.settings.export_rcfg_rect,
        };
        rect.set((inner.y, origin + start, origin + end));
    }
}
