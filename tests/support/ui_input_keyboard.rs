
use super::{apply_imem_search, handle_key};
use crate::ui::app::{App, HartLifecycle, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("key handled");

    assert!(app.run.is_running);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}
