use super::*;
use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::VecDeque;
use crate::ui::pipeline::{GanttCell, GanttRow, InstrClass, Stage, gantt_max_scroll};

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
fn cache_exec_hit_exposes_reset_speed_and_state() {
    let app = App::new(None);
    let status = cache_run_status_area(Rect::new(0, 0, 160, 40));
    let y = status.y + 1;
    app.cache.exec_speed_btn.set((y, 10, 12));
    app.cache.exec_state_btn.set((y, 20, 25));
    app.cache.exec_reset_btn.set((y, 30, 35));

    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| cache_exec_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Reset));
    assert!(hits.contains(&RunButton::Speed));
    assert!(hits.contains(&RunButton::State));
}

#[test]
fn cache_execution_hover_uses_rendered_hitboxes() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    let area = Rect::new(0, 0, 160, 40);
    let status = cache_run_status_area(area);
    let y = status.y + 1;
    app.cache.exec_speed_btn.set((y, 10, 12));
    app.cache.exec_state_btn.set((y, 20, 25));
    app.cache.exec_reset_btn.set((y, 30, 35));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 21,
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
    app.run.mem.icache.config = crate::falcon::cache::CacheConfig {
        size: 512,
        line_size: 16,
        associativity: 1,
        ..crate::falcon::cache::CacheConfig::default()
    };
    app.run.mem.dcache.config = app.run.mem.icache.config.clone();
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
    let row = 10;
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
    app.pipeline.gantt = (0..10)
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
    app.pipeline.gantt_max_scroll_cache.set(gantt_max_scroll(&app.pipeline, 20));
    app.pipeline.gantt_scroll = app.pipeline.gantt_max_scroll_cache.get();

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

    assert_eq!(
        app.pipeline.gantt_scroll,
        app.pipeline.gantt_max_scroll_cache.get()
    );
}

#[test]
fn cache_config_hover_uses_rendered_preset_and_apply_hitboxes() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.cache.subtab = CacheSubtab::Config;
    app.cache.config_preset_btns_i.set([(12, 20, 25), (12, 26, 32), (12, 33, 38)]);
    app.cache.config_apply_btns.set([(14, 20, 39), (14, 42, 60)]);

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 27,
            row: 12,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(matches!(
        app.cache.hover,
        Some(crate::ui::app::CacheHoverTarget::PresetI(1))
    ));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 45,
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
    *app.cache.level_btns.borrow_mut() = vec![(0, 10, 12), (0, 15, 17)];
    app.cache.add_level_btn.set((0, 20, 23));
    app.cache.remove_level_btn.set((0, 26, 32));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 16,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert!(matches!(app.cache.hover, Some(crate::ui::app::CacheHoverTarget::Level(1))));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 21,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
    );
    assert_eq!(app.cache.extra_pending.len(), 1);
}

#[test]
fn cache_level_selector_help_text_is_not_clickable() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    *app.cache.level_btns.borrow_mut() = vec![(0, 10, 12)];
    app.cache.add_level_btn.set((0, 20, 23));
    app.cache.remove_level_btn.set((0, 0, 0));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 40,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 160, 40),
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
