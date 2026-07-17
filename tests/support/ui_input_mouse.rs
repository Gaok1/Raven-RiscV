use super::*;
use crate::ui::pipeline::{GanttCell, GanttRow, InstrClass, Stage, gantt_max_scroll};
use crate::ui::view::run::run_controls_plain_text;
use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::VecDeque;

#[test]
fn run_status_hit_accounts_for_core_prefix() {
    let app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));

    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| run_status_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Core));
    assert!(hits.contains(&RunButton::View));
    assert!(hits.contains(&RunButton::Format));
    assert!(hits.contains(&RunButton::Reset));
}

#[test]
fn run_status_hit_disables_core_selector_in_single_core_mode() {
    let mut app = App::new(None);
    app.max_cores = 1;
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let text = run_controls_plain_text(&app);
    let core_col = text.find("core").expect("core label present") as u16;

    assert!(!matches!(
        run_status_hit(&app, status, status.x + 1 + core_col),
        Some(RunButton::Core)
    ));
}

#[test]
fn run_status_hit_accepts_label_portion_of_speed_control() {
    let mut app = App::new(None);
    app.max_cores = 2;
    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));
    let text = run_controls_plain_text(&app);
    let speed_col = text.find("speed").expect("speed label present") as u16;

    assert!(matches!(
        run_status_hit(&app, status, status.x + 1 + speed_col),
        Some(RunButton::Speed)
    ));
}

#[test]
fn run_status_hit_covers_full_rendered_state_control_width() {
    let app = App::new(None);

    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));
    let text = run_controls_plain_text(&app);
    let state_start = text.find("state ").expect("state label present");
    let state_tail = &text[state_start..];
    let state_width = state_tail.find("   ").unwrap_or(state_tail.len());
    let state_end_col = state_start as u16 + state_width as u16 - 1;

    assert!(matches!(
        run_status_hit(&app, status, status.x + 1 + state_end_col),
        Some(RunButton::State)
    ));
}

#[test]
fn run_status_hit_hides_region_and_bytes_in_dyn_view() {
    let mut app = App::new(None);
    app.run.show_dyn = true;

    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| run_status_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::View));
    assert!(!hits.contains(&RunButton::Region));
    assert!(!hits.contains(&RunButton::Bytes));
}

#[test]
fn run_status_hit_shows_region_and_bytes_when_dyn_is_displaying_memory() {
    let mut app = App::new(None);
    app.run.show_dyn = true;
    app.run.show_registers = false;
    app.run.dyn_mem_access = Some((0x100, 4, true));

    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| run_status_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Region));
    assert!(hits.contains(&RunButton::Bytes));
}

#[test]
fn run_status_hit_exposes_stepback_only_when_undoable() {
    use crate::falcon::machine::types::{RegId, RegTarget};

    let mut app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 200, 40));

    let hits = |app: &App| -> Vec<RunButton> {
        (status.x..status.x + status.width)
            .filter_map(|col| run_status_hit(app, status, col))
            .collect()
    };

    // Fresh: nothing journaled → step-back renders dim and is not clickable,
    // while the rest of the bar still resolves around it.
    let before = hits(&app);
    assert!(!before.contains(&RunButton::Stepback));
    assert!(before.contains(&RunButton::Reset));

    // Journal a change → step-back becomes clickable without disturbing reset.
    app.run
        .machine
        .write_reg(RegTarget::X(RegId::new(5).unwrap()), 0xABCD)
        .unwrap();
    let after = hits(&app);
    assert!(after.contains(&RunButton::Stepback));
    assert!(after.contains(&RunButton::Reset));
}

#[test]
fn cache_exec_hit_exposes_reset_speed_and_state() {
    let app = App::new(None);
    let status = cache_run_status_area(Rect::new(0, 0, 160, 40));
    // Place the exec bar at a known origin, as the renderer would.
    app.cache.exec_origin.set((status.y + 1, status.x));

    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| cache_exec_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Reset));
    assert!(hits.contains(&RunButton::Speed));
    assert!(hits.contains(&RunButton::State));
}

#[test]
fn run_sidebar_wheel_scrolls_registers_in_dyn_register_view() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_dyn = true;
    app.run.show_registers = false;
    app.run.dyn_mem_access = Some((0x120, 4, false));
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
    app.run.show_float_regs = true;
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
    let status = cache_run_status_area(area);
    let y = status.y + 1;
    app.cache.exec_origin.set((y, status.x));

    // Hover the rendered `state` control on the exec bar.
    use crate::ui::view::cache::build_cache_exec_bar;
    let state_col = (status.x..status.x + status.width)
        .find(|&c| build_cache_exec_bar(&app).hit(c, status.x) == Some(RunButton::State))
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
    app.run.machine.mem_mut_unjournaled().icache.config = crate::falcon::cache::CacheConfig {
        size: 512,
        line_size: 16,
        associativity: 1,
        ..crate::falcon::cache::CacheConfig::default()
    };
    app.run.machine.mem_mut_unjournaled().dcache.config = app.run.mem().icache.config.clone();
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
    let row = 13;
    let col = 10;

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
    app.run.pipeline().gantt_area_rect.set((0, 10, 80, 8));
    app.run.pipeline_mut().gantt = (0..10)
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
    app.run.pipeline()
        .gantt_max_scroll_cache
        .set(gantt_max_scroll(&app.run.pipeline(), 20));
    app.run.pipeline_mut().gantt_scroll = app.run.pipeline_mut().gantt_max_scroll_cache.get();

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
        app.run.pipeline().gantt_scroll,
        app.run.pipeline().gantt_max_scroll_cache.get()
    );

    // Wheel-down returns toward follow (0) and saturates there.
    app.run.pipeline_mut().gantt_scroll = 1;
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
    assert_eq!(app.run.pipeline().gantt_scroll, 0);
}

#[test]
fn pipeline_history_mouse_wheel_ignores_scroll_outside_history_panel() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline().gantt_area_rect.set((0, 10, 80, 8));
    app.run.pipeline_mut().gantt_scroll = 3;

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

    assert_eq!(app.run.pipeline().gantt_scroll, 3);
}

#[test]
fn pipeline_state_click_restarts_when_halted() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline_mut().enabled = true;
    app.run.pipeline_mut().halted = true;
    app.run.pipeline().btn_state_rect.set((6, 20, 31));
    app.run.machine.cpu_mut_unjournaled().pc = 32;
    app.run.pipeline_mut().fetch_pc = 32;

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

    assert!(!app.run.pipeline().halted);
    assert_eq!(app.run.pipeline().fetch_pc, app.run.base_pc);
}

#[test]
fn pipeline_main_subtab_ignores_stale_config_row_hitboxes() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.run.pipeline_mut().subtab = crate::ui::pipeline::PipelineSubtab::Main;
    let original = app.run.pipeline().bypass.ex_to_ex;
    let mut rects = [(0, 0, 0); crate::ui::pipeline::PipelineBypassConfig::CONFIG_ROWS];
    rects[0] = (12, 4, 40);
    app.run.pipeline().config_row_rects.set(rects);

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

    assert_eq!(app.run.pipeline().bypass.ex_to_ex, original);
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
    app.add_cache_level(); // one extra level → l1 l2 add remove
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

    // Far right, over the `+/= add level` help text — no control there.
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
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::View;
    app.cache.hscroll_row.set(20);
    app.cache.hscroll_tracks.set([(10, 50), (80, 50)]);
    app.cache.hscroll_max_by_panel.set([10, 40]);

    let area = Rect::new(0, 0, 160, 40);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 80,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 90,
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
