use super::*;
use crate::ui::pipeline::{GanttCell, GanttRow, InstrClass, Stage, gantt_max_scroll};
use crate::ui::view::run::run_controls_plain_text;
use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::VecDeque;

/// Row the display toggles render on, asked of the renderer rather than counted
/// here — the two groups are separated by a blank row, and a literal `+ 1` in
/// this file would be a third copy of the command bar's layout.
fn display_row(status: Rect) -> u16 {
    status.y + crate::ui::view::run::RUN_DISPLAY_ROW
}

/// Every control the command bar resolves, sweeping both of its rows —
/// transport on the first, display toggles on the second.
fn run_bar_hits(app: &App, status: Rect) -> Vec<RunButton> {
    [status.y, display_row(status)]
        .into_iter()
        .flat_map(|row| {
            (status.x..status.x + status.width)
                .filter_map(move |col| run_status_hit(app, status, row, col))
        })
        .collect()
}

/// The first column on `row` that resolves to `btn`.
fn run_bar_col(app: &App, status: Rect, row: u16, btn: RunButton) -> u16 {
    (status.x..status.x + status.width)
        .find(|&col| run_status_hit(app, status, row, col) == Some(btn))
        .expect("control present on that row")
}

/// Hovering a row in Instruction Memory must highlight *that* row.
///
/// The mouse router used to re-derive the Run tab's vertical split with its own
/// `Constraint::Length(5)` for the command bar. When the bar became three rows
/// the router kept assuming five, so every hover resolved two rows above the
/// cursor — and clicking a breakpoint set it on the wrong instruction.
///
/// The pane's first content row is the anchor: it must resolve to the first
/// instruction, and the border row above it to nothing. Rows below are not
/// checked one-to-one because labels and comments take visual rows of their own
/// — the bug was the anchor being off, not the mapping within the list.
#[test]
fn imem_hover_resolves_the_row_under_the_cursor() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    let area = Rect::new(0, 0, 160, 40);

    // Anchored on the *renderer's* geometry, not the router's. Asking `run_cols`
    // where the pane is would make this test self-consistent and blind: both
    // sides would agree with each other while disagreeing with the screen, which
    // is exactly the bug.
    let imem = {
        use crate::ui::view::run::{run_panel_constraints, run_rows};
        let body = crate::ui::view::components::layout::app_frame_chunks(area)[1];
        ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints(run_panel_constraints(&app))
            .split(run_rows(&app, body)[1])[1]
    };

    let hover_at = |app: &mut App, row: u16| {
        handle_mouse(
            app,
            MouseEvent {
                kind: MouseEventKind::Moved,
                column: imem.x + 6,
                row,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        app.run.hover_imem_addr
    };

    assert_eq!(
        hover_at(&mut app, imem.y + 1),
        Some(app.session.base_pc),
        "the pane's first content row must be the first instruction (pane at y={})",
        imem.y
    );
    assert_eq!(
        hover_at(&mut app, imem.y),
        None,
        "the top border resolved to an instruction"
    );
}

/// The router and the renderer must agree on where the Run tab's three strips
/// are. Both go through `run_rows`; this catches a copy creeping back in.
#[test]
fn run_tab_geometry_is_shared_between_render_and_mouse() {
    let app = App::new(None);
    let area = Rect::new(0, 0, 160, 40);
    let body = crate::ui::view::components::layout::app_frame_chunks(area)[1];
    let rows = crate::ui::view::run::run_rows(&app, body);

    assert_eq!(run_status_area(&app, area), rows[0]);
    assert_eq!(run_cols(&app, area)[0].y, rows[1].y);
    assert_eq!(run_cols(&app, area)[0].height, rows[1].height);
}

#[test]
fn run_status_hit_accounts_for_core_prefix() {
    let app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits = run_bar_hits(&app, status);

    assert!(hits.contains(&RunButton::Core));
    assert!(hits.contains(&RunButton::View));
    assert!(hits.contains(&RunButton::Format));
    assert!(hits.contains(&RunButton::Reset));
}

/// Transport and display live on different rows, and a click must not fall
/// through from one to the other.
#[test]
fn run_status_hit_keeps_transport_and_display_on_their_own_rows() {
    let app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));

    let run_col = run_bar_col(&app, status, status.y, RunButton::Run);
    assert_eq!(
        run_status_hit(&app, status, display_row(status), run_col),
        None,
        "transport leaked onto the display row"
    );

    let fmt_col = run_bar_col(&app, status, display_row(status), RunButton::Format);
    assert_ne!(
        run_status_hit(&app, status, status.y, fmt_col),
        Some(RunButton::Format),
        "display leaked onto the transport row"
    );
}

/// The blank row between the two groups, and the rule row closing the bar, have
/// nothing to click.
#[test]
fn run_status_hit_finds_nothing_on_the_rule_row() {
    let app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));

    for row in [status.y + 1, status.y + status.height - 1] {
        assert!(
            (status.x..status.x + status.width)
                .all(|col| run_status_hit(&app, status, row, col).is_none()),
            "row {} resolved a control",
            row - status.y
        );
    }
}

#[test]
fn run_status_hit_disables_core_selector_in_single_core_mode() {
    let mut app = App::new(None);
    app.max_cores = 1;
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));

    assert!(!run_bar_hits(&app, status).contains(&RunButton::Core));
}

#[test]
fn run_status_hit_accepts_label_portion_of_speed_control() {
    let mut app = App::new(None);
    app.max_cores = 2;
    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));

    // The dim `speed` label is part of the control, matching how a user aims at
    // the word they read rather than at the value alone.
    let col = run_bar_col(&app, status, display_row(status), RunButton::Speed);
    let text = run_controls_plain_text(&app);
    assert!(text.contains("speed"));
    assert_eq!(
        run_status_hit(
            &app,
            status,
            display_row(status),
            col + "speed".len() as u16
        ),
        Some(RunButton::Speed)
    );
}

/// The transport verbs go inert rather than disappearing, so the row does not
/// reflow under the cursor when the machine changes state.
#[test]
fn run_status_transport_disables_the_verb_the_machine_is_already_doing() {
    let mut app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));

    // Paused: run and step are live, pause is not.
    let paused = run_bar_hits(&app, status);
    assert!(paused.contains(&RunButton::Run));
    assert!(paused.contains(&RunButton::Step));
    assert!(!paused.contains(&RunButton::Pause));

    app.toggle_run();
    let running = run_bar_hits(&app, status);
    assert!(running.contains(&RunButton::Pause));
    assert!(!running.contains(&RunButton::Run));
    assert!(!running.contains(&RunButton::Step));
}

#[test]
fn run_status_hit_hides_region_and_bytes_in_dyn_view() {
    let mut app = App::new(None);
    app.run.show_dyn = true;

    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits = run_bar_hits(&app, status);

    assert!(hits.contains(&RunButton::View));
    assert!(!hits.contains(&RunButton::Region));
    assert!(!hits.contains(&RunButton::Bytes));
}

#[test]
fn run_status_hit_shows_region_and_bytes_when_dyn_is_displaying_memory() {
    let mut app = App::new(None);
    app.run.show_dyn = true;
    app.run.show_registers = false;
    app.session.dyn_mem_access = Some((0x100, 4, true));

    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits = run_bar_hits(&app, status);

    assert!(hits.contains(&RunButton::Region));
    assert!(hits.contains(&RunButton::Bytes));
}

#[test]
fn run_status_hit_exposes_stepback_only_when_undoable() {
    use crate::falcon::machine::types::{RegId, RegTarget};

    let mut app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));

    let hits = |app: &App| -> Vec<RunButton> { run_bar_hits(app, status) };

    // Fresh: nothing journaled â†’ step-back renders dim and is not clickable,
    // while the rest of the bar still resolves around it.
    let before = hits(&app);
    assert!(!before.contains(&RunButton::Stepback));
    assert!(before.contains(&RunButton::Reset));

    // Journal a change â†’ step-back becomes clickable without disturbing reset.
    app.rv32_mut()
        .unwrap()
        .write_reg(RegTarget::X(RegId::new(5).unwrap()), 0xABCD)
        .unwrap();
    let after = hits(&app);
    assert!(after.contains(&RunButton::Stepback));
    assert!(after.contains(&RunButton::Reset));
}

#[test]
fn cache_exec_hit_exposes_reset_speed_and_state() {
    let app = App::new(None);
    // Place the exec bar at a known origin, as the renderer would. The hit-test
    // reads that origin — row included — so no layout arithmetic is repeated.
    let (y, x) = (3u16, 1u16);
    app.cache.exec_origin.set((y, x));

    let hits: Vec<RunButton> = (x..x + 80)
        .filter_map(|column| {
            cache_exec_row_hit(
                &app,
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    column,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                },
            )
        })
        .collect();

    assert!(hits.contains(&RunButton::Reset));
    assert!(hits.contains(&RunButton::Speed));
    assert!(hits.contains(&RunButton::State));
}

#[test]
#[ignore = "depends on host terminal size"]
fn run_sidebar_wheel_scrolls_registers_in_dyn_register_view() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_dyn = true;
    app.run.show_registers = false;
    app.session.dyn_mem_access = Some((0x120, 4, false));
    app.run.regs_scroll = 1;
    app.run.mem_view_addr = 0x80;
    let area = Rect::new(0, 0, 160, 40);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert_eq!(app.run.regs_scroll, 2);
    assert_eq!(app.run.mem_view_addr, 0x80);
}

#[test]
fn run_float_register_view_click_does_not_toggle_integer_pins() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_registers = true;
    app.run.reg_bank = 1;
    app.run.pinned_regs.push(3);
    let area = Rect::new(0, 0, 160, 40);
    let cols = run_cols(&app, area);
    let sidebar = cols[0];

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: sidebar.x + 3,
            row: sidebar.y + 3,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert_eq!(app.run.pinned_regs, vec![3]);
}

#[test]
fn run_view_click_closes_mem_search_when_sidebar_leaves_memory() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_registers = false;
    app.run.show_dyn = false;
    app.run.mem_search_open = true;
    app.run.mem_search_query = "1234".into();

    apply_run_button(&mut app, RunButton::View);

    assert!(app.run.show_registers);
    assert!(!app.run.mem_search_open);
    assert!(app.run.mem_search_query.is_empty());
}

#[test]
fn cache_execution_hover_uses_rendered_hitboxes() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    let area = Rect::new(0, 0, 160, 40);
    // Render once so the exec bar records where it actually landed — the hover
    // path reads that same origin, which is the whole point of the assertion.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(
        area.width,
        area.height,
    ))
    .expect("terminal");
    app.splash_start = None;
    terminal
        .draw(|f| crate::ui::view::ui(f, &app))
        .expect("render");
    let (y, x) = app.cache.exec_origin.get();

    // Hover the rendered `state` control on the exec bar.
    use crate::ui::view::cache::build_cache_exec_bar;
    let state_col = (x..x + 60)
        .find(|&c| build_cache_exec_bar(&app).hit(c, x) == Some(RunButton::State))
        .expect("state control present");

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: state_col,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(matches!(app.hover_run_button, Some(RunButton::State)));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 8,
            row: y + 1,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(app.hover_run_button.is_none());
}

#[test]
fn cache_view_mouse_wheel_updates_vertical_scroll() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::View;
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "halt".into(),
    ];
    app.assemble_and_load();
    app.rv32_mut().unwrap().mem_mut_unjournaled().icache.config =
        crate::falcon::cache::CacheConfig {
            size: 512,
            line_size: 16,
            associativity: 1,
            ..crate::falcon::cache::CacheConfig::default()
        };
    app.rv32_mut().unwrap().mem_mut_unjournaled().dcache.config =
        app.rv32().unwrap().mem().icache.config.clone();
    let area = Rect::new(0, 0, 160, 40);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.cache.view_scroll, 1);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.cache.view_scroll, 0);
}

#[test]
fn cache_config_hover_and_click_match_first_row_geometry() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::Config;

    let area = Rect::new(0, 0, 160, 40);
    // Aimed at the rect the renderer recorded for the field, not at a counted
    // row. `body.y + 10` meant "the chrome above is ten rows tall", which is a
    // second copy of the layout — it broke when the tab bar lost a row, and
    // again when the Cache header collapsed from four boxes into two lines.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(
        area.width,
        area.height,
    ))
    .expect("terminal");
    app.splash_start = None;
    terminal
        .draw(|f| crate::ui::view::ui(f, &app))
        .expect("render");
    let (row, x0, _) = app.cache.config_hitboxes_i.get()[crate::ui::app::ConfigField::Size as usize];
    let col = x0 + 2;

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::ConfigField(
            true,
            crate::ui::app::ConfigField::Size,
        ))
    ));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(matches!(
        app.cache.edit_field,
        Some((true, crate::ui::app::ConfigField::Size))
    ));
}

#[test]
fn cache_config_hover_prefers_rendered_hitboxes_for_middle_rows() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::Config;

    let mut hitboxes = [(0, 0, 0); 11];
    hitboxes[crate::ui::app::ConfigField::Associativity.hitbox_index()] = (12, 4, 40);
    hitboxes[crate::ui::app::ConfigField::WritePolicy.hitbox_index()] = (15, 4, 40);
    app.cache.config_hitboxes_i.set(hitboxes);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 10,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );

    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::ConfigField(
            true,
            crate::ui::app::ConfigField::Associativity,
        ))
    ));
}

#[test]
fn cache_view_mouse_wheel_clamps_to_rendered_max_scroll() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::View;
    app.cache.view_num_sets.set(32);
    app.cache.view_visible_sets.set(18);
    app.cache.view_scroll_max.set(14);
    app.cache.view_scroll = 14;

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );

    assert_eq!(app.cache.view_scroll, 14);
}

#[test]
fn pipeline_history_mouse_wheel_clamps_to_rendered_max_scroll() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline_view().gantt_area_rect.set((0, 10, 80, 8));
    app.rv32_mut().unwrap().pipeline_mut().gantt = (0..10)
        .map(|i| GanttRow {
            gantt_id: i + 1,
            pc: (i * 4) as u32,
            disasm: format!("addi x{i}, x{i}, 1"),
            class: InstrClass::Alu,
            cells: VecDeque::from(vec![GanttCell::InStage(Stage::IF); 4]),
            first_cycle: i as u64,
            done: false,
            last_stage: None,
        })
        .collect();
    app.run
        .pipeline_view()
        .gantt_max_scroll_cache
        .set(gantt_max_scroll(&app.rv32().unwrap().pipeline(), 20));
    app.run.pipeline_view_mut().gantt_scroll =
        app.run.pipeline_view_mut().gantt_max_scroll_cache.get();

    // Bottom-anchored: wheel-up digs into scrollback but clamps at the oldest row.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 20),
    );

    assert_eq!(
        app.run.pipeline_view().gantt_scroll,
        app.run.pipeline_view().gantt_max_scroll_cache.get()
    );

    // Wheel-down returns toward follow (0) and saturates there.
    app.run.pipeline_view_mut().gantt_scroll = 1;
    for _ in 0..2 {
        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 20,
                row: 12,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 160, 20),
        );
    }
    assert_eq!(app.run.pipeline_view().gantt_scroll, 0);
}

#[test]
fn pipeline_history_mouse_wheel_ignores_scroll_outside_history_panel() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline_view().gantt_area_rect.set((0, 10, 80, 8));
    app.run.pipeline_view_mut().gantt_scroll = 3;

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 20,
            row: 5,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 20),
    );

    assert_eq!(app.run.pipeline_view().gantt_scroll, 3);
}

#[test]
fn pipeline_state_click_restarts_when_halted() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.rv32_mut().unwrap().pipeline_mut().enabled = true;
    app.rv32_mut().unwrap().pipeline_mut().halted = true;
    app.run.pipeline_view().btn_state_rect.set((6, 20, 31));
    app.rv32_mut().unwrap().cpu_mut_unjournaled().pc = 32;
    app.rv32_mut().unwrap().pipeline_mut().fetch_pc = 32;

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 21,
            row: 6,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );

    assert!(!app.rv32().unwrap().pipeline().halted);
    assert_eq!(app.rv32().unwrap().pipeline().fetch_pc, app.session.base_pc);
}

#[test]
fn pipeline_main_subtab_ignores_stale_config_row_hitboxes() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline_view_mut().subtab = crate::ui::pipeline::PipelineSubtab::Main;
    let original = app.rv32().unwrap().pipeline().bypass.ex_to_ex;
    let mut rects = [(0, 0, 0); crate::ui::pipeline::PIPELINE_CONFIG_ROWS];
    rects[0] = (12, 4, 40);
    app.run.pipeline_view().config_row_rects.set(rects);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 10,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );

    assert_eq!(app.rv32().unwrap().pipeline().bypass.ex_to_ex, original);
}

#[test]
fn cache_config_hover_uses_rendered_preset_and_apply_hitboxes() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::Config;
    app.cache.config_preset_origin_i.set((12, 20));
    app.cache.config_apply_origin.set((14, 20));

    use crate::ui::view::cache::config::{
        CacheApplyBtn, build_cache_apply_bar, build_cache_preset_bar,
    };
    let preset1_col = (20..160)
        .find(|&c| build_cache_preset_bar(&app, true).hit(c, 20) == Some(1))
        .expect("preset 1 present");
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: preset1_col,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::PresetI(1))
    ));

    let keep_col = (20..160)
        .find(|&c| build_cache_apply_bar(&app).hit(c, 20) == Some(CacheApplyBtn::ApplyKeep))
        .expect("apply-keep present");
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: keep_col,
            row: 14,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::ApplyKeep)
    ));
}

#[test]
fn cache_level_selector_uses_rendered_hitboxes() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.add_cache_level(); // one extra level â†’ l1 l2 add remove
    let area = Rect::new(0, 0, 160, 40);
    let (level_area, ..) = cache_content_area(area);
    let origin_x = level_area.x + "level ".len() as u16;
    app.cache.level_origin.set((level_area.y, origin_x));

    use crate::ui::view::cache::{CacheLevelBtn, build_cache_level_bar};
    let l2_col = (origin_x..160)
        .find(|&c| build_cache_level_bar(&app).hit(c, origin_x) == Some(CacheLevelBtn::Level(1)))
        .expect("l2 present");

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: l2_col,
            row: level_area.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::Level(1))
    ));

    let add_col = (origin_x..160)
        .find(|&c| build_cache_level_bar(&app).hit(c, origin_x) == Some(CacheLevelBtn::Add))
        .expect("add present");
    let extras_before = app.cache.extra_pending.len();
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: add_col,
            row: level_area.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.cache.extra_pending.len(), extras_before + 1);
}

#[test]
fn cache_level_selector_help_text_is_not_clickable() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    let area = Rect::new(0, 0, 160, 40);
    let (level_area, ..) = cache_content_area(area);
    app.cache
        .level_origin
        .set((level_area.y, level_area.x + "level ".len() as u16));

    // Far right, over the `+/= add level` help text â€” no control there.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 60,
            row: level_area.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(app.cache.hover.is_none());
}

#[test]
fn cache_view_mouse_wheel_targets_only_panel_under_cursor() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::View;
    app.cache.scope = crate::ui::app::CacheScope::Both;
    app.cache.view_num_sets.set(32);
    app.cache.view_scroll_max.set(14);
    app.cache.view_num_sets_d.set(32);
    app.cache.view_scroll_max_d.set(14);

    let area = Rect::new(0, 0, 160, 40);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.cache.view_scroll, 1);
    assert_eq!(app.cache.view_scroll_d, 0);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 120,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.cache.view_scroll, 1);
    assert_eq!(app.cache.view_scroll_d, 1);
}

#[test]
fn cache_view_hscroll_drag_uses_hovered_panel_max_scroll() {
    use crate::ui::view::components::SbGeom;
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::View;
    // Two side-by-side bars as the View renderer registers them (viewport 0 =
    // ratatui's fall-back). D-cache (slot 1) spans columns 80..130 on row 20.
    let bar = |start: u16, max: usize| SbGeom {
        start,
        len: 50,
        cross: 20,
        content: max,
        viewport: 0,
        offset: 0,
        max,
    };
    app.cache
        .hscroll_bars
        .set([Some(bar(10, 10)), Some(bar(80, 40))]);

    let area = Rect::new(0, 0, 160, 40);

    // Down inside the D-cache thumb (offset 0 â†’ thumb starts at column 81).
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 90,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    // Grabbing the thumb must not jump the view.
    assert_eq!(app.cache.view_h_scroll_d, 0);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 120,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert!(
        app.cache.view_h_scroll_d >= 8,
        "drag should use D-cache max scroll, got {}",
        app.cache.view_h_scroll_d
    );
    assert_eq!(app.cache.view_h_scroll, 0);
}

#[test]
fn run_cols_use_thin_rails_for_collapsed_panels() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.sidebar_collapsed = true;
    app.run.imem_collapsed = true;
    app.run.details_collapsed = true;

    let cols = run_cols(&app, Rect::new(0, 0, 160, 40));

    assert_eq!(cols[0].width, crate::ui::view::run::RUN_COLLAPSED_RAIL_W);
    assert_eq!(cols[1].width, crate::ui::view::run::RUN_COLLAPSED_RAIL_W);
    assert_eq!(cols[2].width, crate::ui::view::run::RUN_COLLAPSED_RAIL_W);
}

#[test]
fn clicking_collapsed_imem_rail_reopens_panel() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.imem_collapsed = true;
    let area = Rect::new(0, 0, 160, 40);
    let cols = run_cols(&app, area);
    let imem = cols[1];

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: imem.x,
            row: imem.y + imem.height / 2,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert!(!app.run.imem_collapsed);
}

#[test]
fn clicking_collapsed_sidebar_rail_reopens_panel() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.sidebar_collapsed = true;
    let area = Rect::new(0, 0, 160, 40);
    let cols = run_cols(&app, area);
    let sidebar = cols[0];

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: sidebar.x,
            row: sidebar.y + sidebar.height / 2,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert!(!app.run.sidebar_collapsed);
}

#[test]
fn clicking_collapsed_details_rail_reopens_panel() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.details_collapsed = true;
    let area = Rect::new(0, 0, 160, 40);
    let cols = run_cols(&app, area);
    let details = cols[2];

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: details.x,
            row: details.y + details.height / 2,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    assert!(!app.run.details_collapsed);
}

#[test]
fn tlb_entries_scrollbar_click_jumps_and_drag_follows() {
    use crate::ui::view::components::SbGeom;
    let mut app = App::new(None);
    app.tab = Tab::Tlb;
    // Geometry as the Entries renderer registers it: 60 rows, 20 visible,
    // bar on column 100 over rows 5..25 (thumb at rows 6..11 for offset 0).
    let bar = SbGeom {
        start: 5,
        len: 20,
        cross: 100,
        content: 60,
        viewport: 20,
        offset: 0,
        max: 40,
    };
    app.tlb.entries_sb.set(Some(bar));

    // Down on the track jumps the thumb under the cursor and starts the drag.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 100,
            row: 15,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    let (grab, jumped) = bar.begin_drag(15);
    assert!(app.tlb.entries_sb_drag.is_some());
    assert_eq!(app.tlb.entries_scroll, jumped);
    let thumb = SbGeom {
        offset: jumped,
        ..bar
    }
    .thumb();
    assert!(
        (thumb.0..thumb.0 + thumb.1).contains(&15),
        "thumb under cursor"
    );

    // Dragging keeps the grabbed thumb cell glued to the cursor (clamped at max).
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 90,
            row: 24,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert_eq!(app.tlb.entries_scroll, bar.drag(24, grab));

    // Up ends the drag; further drags are ignored.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column: 90,
            row: 24,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(app.tlb.entries_sb_drag.is_none());
}

#[test]
fn cache_history_scrollbar_click_moves_selection() {
    use crate::ui::view::components::SbGeom;
    let mut app = App::new(None);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::Stats;
    // Geometry as the Snapshots renderer registers it: 20 entries, 8 visible,
    // bar on column 120 over rows 6..16, window at the top.
    let bar = SbGeom {
        start: 6,
        len: 10,
        cross: 120,
        content: 20,
        viewport: 8,
        offset: 0,
        max: 12,
    };
    app.cache.history_sb.set(Some(bar));

    // Down near the bottom of the track: the window jumps there and the
    // selection pins to the window's bottom edge.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 120,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(app.cache.history_sb_drag.is_some());
    let (grab, start) = bar.begin_drag(12);
    assert!(start > 0);
    assert_eq!(app.cache.history_scroll, (start + 8 - 1).min(19));

    // Dragging back to the top of the track selects the first entry.
    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 120,
            row: 6,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert_eq!(bar.drag(6, grab), 0);
    assert_eq!(app.cache.history_scroll, 0);
}

#[test]
fn run_sidebar_register_scrollbar_click_scrolls_instead_of_editing() {
    use crate::ui::view::components::SbGeom;
    let mut app = App::new(None);
    app.tab = Tab::Run;
    // Geometry as the register-table renderer registers it: 30 rows, 18
    // visible, bar on column 36 over rows 6..24.
    let bar = SbGeom {
        start: 6,
        len: 18,
        cross: 36,
        content: 30,
        viewport: 18,
        offset: 0,
        max: 12,
    };
    app.run.regs_sb.set(Some(bar));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 36,
            row: 14,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    let (grab, jumped) = bar.begin_drag(14);
    assert!(app.run.regs_sb_drag.is_some());
    assert_eq!(app.run.regs_scroll, jumped);
    // The click was consumed by the bar â€” no inline register editor opened.
    assert!(app.run.run_edit.is_none());

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 30,
            row: 22,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert_eq!(app.run.regs_scroll, bar.drag(22, grab));
}
