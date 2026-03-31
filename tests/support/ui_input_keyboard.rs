use super::{
    KeyOutcome, apply_imem_search, capture_snapshot, handle_key, paste_from_terminal,
    paste_imem_search, paste_mem_search, serialize_pipeline_results_pstats,
    serialize_results_csv, serialize_results_fstats,
};
use crate::ui::app::{App, EditorMode, HartLifecycle, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn load_program(app: &mut App, lines: &[&str]) {
    app.editor.buf.lines = lines.iter().map(|line| (*line).to_string()).collect();
    app.assemble_and_load();
}

#[test]
fn imem_search_ignores_non_text_labels() {
    let mut app = App::new(None);
    app.editor.buf.lines = vec![
        ".data".into(),
        "msg: .word 1".into(),
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "entry:".into(),
        "addi a0, zero, 1".into(),
        "loop:".into(),
        "addi a0, a0, 1".into(),
        "halt".into(),
    ];
    app.assemble_and_load();

    let entry_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "entry").then_some(*addr))
        .expect("entry label present");
    let msg_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "msg").then_some(*addr))
        .expect("msg label present");
    assert_ne!(entry_addr, msg_addr, "text and data labels must differ");

    app.run.imem_scroll = 0;
    app.run.imem_search_query = "entry".into();
    apply_imem_search(&mut app);
    let expected = app
        .imem_visual_row_of_addr(entry_addr)
        .expect("entry address is in instruction memory")
        .saturating_sub(2);
    assert_eq!(app.run.imem_scroll, expected);

    let scroll_after_text = app.run.imem_scroll;
    app.run.imem_search_query = "msg".into();
    apply_imem_search(&mut app);
    assert_eq!(app.run.imem_scroll, scroll_after_text);
}

#[test]
fn run_key_resumes_paused_core_even_if_fault_flag_is_set() {
    let mut app = App::new(None);
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "ebreak".into(),
        "addi a0, zero, 7".into(),
    ];
    app.assemble_and_load();
    app.tab = Tab::Run;

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Paused);

    app.run.faulted = true;
    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("key handled");
    assert_eq!(outcome, KeyOutcome::Handled);

    assert!(app.run.is_running);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn run_key_starts_and_stops_continuous_execution_on_run_tab() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "addi a0, zero, 7".into(),
        "halt".into(),
    ];
    app.assemble_and_load();

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("run key handled");
    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(app.run.is_running, "r should start continuous execution");

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("run key handled");
    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(
        !app.run.is_running,
        "pressing r again should stop continuous execution"
    );
}

#[test]
fn ctrl_f_opens_ram_search_even_from_register_view() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    app.run.show_registers = true;
    app.run.show_dyn = false;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
    )
    .expect("ctrl-f handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(app.run.mem_search_open);
    assert!(!app.run.show_registers);
    assert!(!app.run.show_dyn);
}

#[test]
fn k_cycles_region_and_switches_sidebar_back_to_memory() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    app.run.show_registers = true;
    app.run.show_dyn = false;
    app.run.mem_region = crate::ui::app::MemRegion::Data;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    )
    .expect("k handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(matches!(
        app.run.mem_region,
        crate::ui::app::MemRegion::Stack
    ));
    assert!(!app.run.show_registers);
    assert!(!app.run.show_dyn);
}

#[test]
fn run_view_dyn_focuses_last_store_after_mode_switch() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    let store_addr = 0x100;
    app.run.dyn_mem_access = Some((store_addr, 4, true));
    app.run.mem_view_addr = 0;
    app.run.show_registers = true;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
    )
    .expect("v handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(app.run.show_dyn);
    assert_eq!(app.run.mem_view_addr, store_addr & !3);
}

#[test]
fn run_region_rw_focuses_last_memory_access_immediately() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    let load_addr = 0x100;
    app.run.dyn_mem_access = Some((load_addr, 4, false));
    app.run.mem_view_addr = 0;
    app.run.mem_region = crate::ui::app::MemRegion::Stack;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    )
    .expect("k handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(matches!(app.run.mem_region, crate::ui::app::MemRegion::Access));
    assert_eq!(app.run.mem_view_addr, load_addr & !3);
}

#[test]
fn cache_pause_key_resumes_paused_core_even_if_fault_flag_is_set() {
    let mut app = App::new(None);
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "ebreak".into(),
        "addi a0, zero, 7".into(),
    ];
    app.assemble_and_load();
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Paused);

    app.run.faulted = true;
    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
    )
    .expect("cache pause key handled");
    assert_eq!(outcome, KeyOutcome::Handled);

    assert!(app.run.is_running);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn cache_view_key_cycles_sidebar_in_documented_order() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;
    app.run.show_registers = false;
    app.run.show_dyn = false;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
    )
    .expect("cache v handled");
    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(app.run.show_registers);
    assert!(!app.run.show_dyn);

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
    )
    .expect("cache v handled");
    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(!app.run.show_registers);
    assert!(app.run.show_dyn);

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
    )
    .expect("cache v handled");
    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(!app.run.show_registers);
    assert!(!app.run.show_dyn);
}

#[test]
fn cache_view_down_key_clamps_to_rendered_max_scroll() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;
    app.cache.subtab = crate::ui::app::CacheSubtab::View;
    app.cache.view_num_sets.set(32);
    app.cache.view_visible_sets.set(18);
    app.cache.view_scroll_max.set(14);
    app.cache.view_scroll = 14;

    let outcome = handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .expect("down handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert_eq!(app.cache.view_scroll, 14);
}

#[test]
fn cache_view_keyboard_scroll_targets_focused_panel_in_both_scope() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;
    app.cache.subtab = crate::ui::app::CacheSubtab::View;
    app.cache.scope = crate::ui::app::CacheScope::Both;
    app.cache.view_focus = crate::ui::app::CacheViewFocus::DCache;
    app.cache.view_num_sets.set(32);
    app.cache.view_scroll_max.set(14);
    app.cache.view_num_sets_d.set(32);
    app.cache.view_scroll_max_d.set(14);

    let outcome = handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .expect("down handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert_eq!(app.cache.view_scroll, 0);
    assert_eq!(app.cache.view_scroll_d, 1);
}

#[test]
fn cache_scope_keys_update_view_focus_for_single_scope() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;
    app.cache.scope = crate::ui::app::CacheScope::Both;
    app.cache.view_focus = crate::ui::app::CacheViewFocus::ICache;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
    )
    .expect("d handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(matches!(app.cache.scope, crate::ui::app::CacheScope::DCache));
    assert!(matches!(
        app.cache.view_focus,
        crate::ui::app::CacheViewFocus::DCache
    ));
}

#[test]
fn cache_k_cycles_region_and_switches_sidebar_back_to_memory() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;
    app.mode = EditorMode::Command;
    app.run.show_registers = true;
    app.run.show_dyn = false;
    app.run.mem_region = crate::ui::app::MemRegion::Data;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    )
    .expect("cache k handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert!(matches!(
        app.run.mem_region,
        crate::ui::app::MemRegion::Stack
    ));
    assert!(!app.run.show_registers);
    assert!(!app.run.show_dyn);
}

#[test]
fn imem_search_paste_appends_query_and_scrolls_to_match() {
    let mut app = App::new(None);
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "addi a0, zero, 1".into(),
        "entry_loop:".into(),
        "addi a0, a0, 1".into(),
        "halt".into(),
    ];
    app.assemble_and_load();
    app.tab = Tab::Run;
    app.run.imem_search_open = true;
    app.run.imem_scroll = 0;
    app.run.imem_search_query = "entry".into();

    let entry_loop_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "entry_loop").then_some(*addr))
        .expect("entry_loop label present");

    paste_imem_search(&mut app, "_loop\r\n");

    assert_eq!(app.run.imem_search_query, "entry_loop");
    assert_eq!(app.run.imem_search_match_count, 1);
    assert_eq!(app.run.imem_search_matches, vec![entry_loop_addr]);
    let expected = app
        .imem_visual_row_of_addr(entry_loop_addr)
        .expect("entry_loop address is in instruction memory")
        .saturating_sub(2);
    assert_eq!(app.run.imem_scroll, expected);
}

#[test]
fn terminal_paste_targets_imem_search_without_touching_editor() {
    let mut app = App::new(None);
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "addi a0, zero, 1".into(),
        "target_label:".into(),
        "halt".into(),
    ];
    app.assemble_and_load();
    app.tab = Tab::Run;
    app.run.imem_search_open = true;
    let editor_before = app.editor.buf.text();

    paste_from_terminal(&mut app, "target");

    assert_eq!(app.run.imem_search_query, "target");
    assert_eq!(app.editor.buf.text(), editor_before);
}

#[test]
fn mem_search_paste_strips_prefix_and_updates_address() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_registers = false;
    app.run.mem_search_open = true;
    app.run.mem_view_bytes = 4;

    paste_mem_search(&mut app, "0x123\r\n");

    assert_eq!(app.run.mem_search_query, "123");
    assert_eq!(app.run.mem_view_addr, 0x120);
}

#[test]
fn terminal_paste_targets_mem_search_without_touching_editor() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.run.show_registers = false;
    app.run.mem_search_open = true;
    let editor_before = app.editor.buf.text();

    paste_from_terminal(&mut app, "0x40\n");

    assert_eq!(app.run.mem_search_query, "40");
    assert_eq!(app.run.mem_view_addr, 0x40);
    assert_eq!(app.editor.buf.text(), editor_before);
}

#[test]
fn pipeline_step_key_is_handled_without_requesting_quit() {
    let mut app = App::new(None);
    app.tab = Tab::Pipeline;
    app.mode = EditorMode::Command;
    app.pipeline.enabled = true;

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
    )
    .expect("pipeline step key handled");

    assert_eq!(outcome, KeyOutcome::Handled);
}

#[test]
fn run_step_key_advances_even_when_not_paused() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    app.mode = EditorMode::Command;
    app.editor.buf.lines = vec![
        ".text".into(),
        ".globl _start".into(),
        "_start:".into(),
        "addi a0, zero, 7".into(),
        "halt".into(),
    ];
    app.assemble_and_load();

    let outcome = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
    )
    .expect("run step key handled");

    assert_eq!(outcome, KeyOutcome::Handled);
    assert_eq!(app.run.cpu.x[10], 7);
}

#[test]
fn pipeline_snapshot_and_exports_use_pipeline_clock_model() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.set_pipeline_enabled(true);
    app.run.mem.instruction_count = 41;
    app.run.mem.extra_cycles = 99;
    app.pipeline.cycle_count = 12;
    app.pipeline.instr_committed = 3;
    app.pipeline.stall_count = 4;
    app.pipeline.flush_count = 1;
    app.pipeline.branches_executed = 2;
    app.pipeline.stall_by_type = [1, 1, 1, 1, 0];

    let snap = capture_snapshot(&app);
    assert_eq!(snap.instruction_count, 3);
    assert_eq!(snap.total_cycles, 12);
    assert_eq!(snap.base_cycles, 0);
    assert_eq!(snap.pipeline.as_ref().map(|p| p.cycles), Some(12));

    let fstats = serialize_results_fstats(&snap, &[]);
    assert!(fstats.starts_with("# FALCON-ASM Simulation Results v2\n"));
    assert!(fstats.contains("prog.clock_model=pipeline\n"));
    assert!(fstats.contains("prog.total_cycles=12\n"));
    assert!(!fstats.contains("prog.base_cycles="));
    assert!(!fstats.contains("prog.cache_cycles="));

    let csv = serialize_results_csv(&snap, &[]);
    assert!(csv.contains("Clock Model,Instructions,Total Cycles,CPI,IPC\n"));
    assert!(csv.contains("pipeline,3,12,"));
    assert!(!csv.contains("Base Cycles"));
    assert!(!csv.contains("Cache Cycles"));

    let pstats = serialize_pipeline_results_pstats(&snap);
    assert!(pstats.starts_with("# Raven Pipeline Results v2\n"));
    assert!(pstats.contains("prog.clock_model=pipeline\n"));
    assert!(pstats.contains("prog.total_cycles=12\n"));
}
