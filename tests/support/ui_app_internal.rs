use super::{App, HartLifecycle, RunScope, Tab};
use crate::falcon::memory::Bus;
use crate::ui::view::run::run_controls_plain_text;

fn load_program(app: &mut App, lines: &[&str]) {
    app.editor.buf.lines = lines.iter().map(|line| (*line).to_string()).collect();
    app.assemble_and_load();
}

fn rust_to_raven_elf_bytes() -> Vec<u8> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("rust-to-raven/target/riscv32im-unknown-none-elf/debug/rust-to-raven");
    std::fs::read(path).expect("failed to read rust-to-raven debug ELF")
}

fn console_tail(app: &App) -> String {
    let start = app.console.lines.len().saturating_sub(30);
    app.console.lines[start..]
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn trace_tail(app: &App) -> String {
    let mut lines = Vec::new();
    let start = app.run.exec_trace.len().saturating_sub(12);
    for (pc, disasm) in app.run.exec_trace.iter().skip(start) {
        lines.push(format!("0x{pc:08X}: {disasm}"));
    }
    lines.join("\n")
}

#[test]
fn single_step_advances_from_ebreak_pause_sequential() {
    let mut app = App::new(None);
    app.max_cores = 2;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "ebreak",
            "addi a0, zero, 7",
            "addi a1, zero, 9",
        ],
    );
    app.rebuild_harts_for_debug();

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Paused);
    assert_eq!(app.run.cpu.x[10], 0);

    app.single_step();
    assert_eq!(app.run.cpu.x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn single_step_advances_from_ebreak_pause_pipeline() {
    let mut app = App::new(None);
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "ebreak",
            "addi a0, zero, 7",
            "addi a1, zero, 9",
        ],
    );
    app.rebuild_harts_for_debug();

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Paused);
    assert_eq!(app.run.cpu.x[10], 0);

    app.single_step();
    assert_eq!(app.run.cpu.x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn run_status_shows_ebreak_for_paused_core() {
    let mut app = App::new(None);
    load_program(
        &mut app,
        &[".text", ".globl _start", "_start:", "ebreak", "halt"],
    );

    app.single_step();

    let text = run_controls_plain_text(&app);
    assert!(text.contains("state ebrk"), "{text}");
}

#[test]
fn run_status_shows_halt_for_halted_core() {
    let mut app = App::new(None);
    load_program(&mut app, &[".text", ".globl _start", "_start:", "halt"]);

    app.single_step();

    let text = run_controls_plain_text(&app);
    assert!(text.contains("state halt"), "{text}");
}

#[test]
fn run_status_shows_exit_for_global_exit() {
    let mut app = App::new(None);
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "li a0, 0",
            "li a7, 93",
            "ecall",
        ],
    );

    app.single_step();
    app.single_step();
    app.single_step();

    let text = run_controls_plain_text(&app);
    assert!(text.contains("state exit"), "{text}");
}

#[test]
fn run_status_shows_fault_for_invalid_instruction() {
    let mut app = App::new(None);
    app.run.faulted = true;
    app.finalize_selected_core_after_step();

    let text = run_controls_plain_text(&app);
    assert!(text.contains("state fault"), "{text}");
}

#[test]
fn focused_run_cannot_start_on_free_core() {
    let mut app = App::new(None);
    app.max_cores = 2;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 1",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();
    app.run_scope = RunScope::FocusedHart;
    app.switch_selected_core(1);

    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Free);
    assert!(!app.can_start_run());
}

#[test]
fn all_harts_run_can_start_from_non_selected_running_core() {
    let mut app = App::new(None);
    app.max_cores = 2;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 1",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();
    app.run_scope = RunScope::AllHarts;
    app.switch_selected_core(1);

    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Free);
    assert!(app.can_start_run());
}

#[test]
fn halt_in_source_is_terminal_not_resumable() {
    let mut app = App::new(None);
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 1",
            "halt",
            "addi a0, zero, 9",
        ],
    );

    app.single_step();
    assert_eq!(app.run.cpu.x[10], 1);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu.x[10], 1);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
}

#[test]
fn multi_core_global_step_preserves_exited_core_state() {
    let mut app = App::new(None);
    app.max_cores = 2;
    load_program(&mut app, &[".text", ".globl _start", "_start:", "halt"]);
    app.rebuild_harts_for_debug();
    app.run_scope = RunScope::AllHarts;

    app.single_step();

    assert_eq!(app.core_status(0), HartLifecycle::Exited);
}

#[test]
fn pipeline_halt_is_terminal_not_resumable() {
    let mut app = App::new(None);
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "halt",
            "addi a0, zero, 9",
        ],
    );

    for _ in 0..8 {
        app.single_step();
        if app.core_status(app.selected_core) == HartLifecycle::Exited {
            break;
        }
    }

    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
    assert!(app.run.cpu.local_exit);
    assert!(!app.run.cpu.ebreak_hit);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu.x[10], 0);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
}

#[test]
fn pipeline_all_harts_scope_keeps_halted_hart_exited() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            ".globl worker",
            ".globl trap_word",
            "_start:",
            "halt",
            "worker:",
            "addi a0, zero, 7",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();
    app.run_scope = RunScope::AllHarts;

    let worker_pc = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "worker").then_some(*addr))
        .expect("worker label present");
    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    app.harts[1].cpu.pc = worker_pc;
    app.harts[1].prev_pc = worker_pc;
    if let Some(p) = app.harts[1].pipeline.as_mut() {
        p.enabled = true;
        p.reset_stages(worker_pc);
    }

    for _ in 0..12 {
        app.single_step();
        if app.core_status(0) == HartLifecycle::Exited {
            break;
        }
    }
    assert_eq!(app.core_status(0), HartLifecycle::Exited);

    for _ in 0..6 {
        app.single_step();
    }

    assert_eq!(app.core_status(0), HartLifecycle::Exited);
    assert_eq!(app.harts[0].cpu.pc, app.run.base_pc.wrapping_add(4));
    assert!(matches!(
        app.core_status(1),
        HartLifecycle::Running | HartLifecycle::Exited
    ));
}

#[test]
fn all_harts_step_does_not_auto_resume_non_selected_ebreak_hart() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "ebreak",
            "halt",
            "worker:",
            "addi a0, zero, 7",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();

    let worker_pc = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "worker").then_some(*addr))
        .expect("worker label present");
    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    app.harts[1].cpu.pc = worker_pc;
    app.harts[1].prev_pc = worker_pc;

    app.single_step();
    assert_eq!(app.core_status(0), HartLifecycle::Paused);

    app.switch_selected_core(1);
    app.single_step();

    assert_eq!(app.core_status(0), HartLifecycle::Paused);
    assert_eq!(app.harts[0].cpu.pc, app.run.base_pc.wrapping_add(4));
}

#[test]
fn all_harts_run_cannot_start_from_non_selected_paused_hart() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    load_program(
        &mut app,
        &[".text", ".globl _start", "_start:", "ebreak", "halt"],
    );
    app.rebuild_harts_for_debug();

    app.single_step();
    assert_eq!(app.core_status(0), HartLifecycle::Paused);

    app.switch_selected_core(1);
    assert_eq!(app.core_status(1), HartLifecycle::Free);
    assert!(!app.can_start_run());
}

#[test]
fn focused_secondary_hart_falls_through_unsupported_word_until_halt() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::FocusedHart;
    app.set_cache_enabled(false);
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "halt",
            "worker:",
            "lui t0, 0",
            "addi t0, t0, 32",
            "lui t1, 0xc0001",
            "addi t1, t1, 115",
            "sw t1, 0(t0)",
            "fence",
            "jalr zero, t0, 0",
            "trap_word:",
            "addi a0, zero, 7",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();

    let worker_pc = app.run.base_pc + 4;
    let trap_pc = app.run.base_pc + 32;
    let halt_pc = app.run.base_pc + 36;

    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    app.harts[1].cpu.pc = worker_pc;
    app.harts[1].prev_pc = worker_pc;

    app.switch_selected_core(1);

    for _ in 0..8 {
        if !matches!(app.core_status(1), HartLifecycle::Running) {
            break;
        }
        app.single_step();
    }

    assert_eq!(app.run.cpu.x[5], trap_pc);
    assert_eq!(app.run.mem.peek32(trap_pc).unwrap_or(0), 0xC000_1073);
    assert_eq!(app.core_status(1), HartLifecycle::Exited);
    assert_eq!(app.run.cpu.pc, halt_pc.wrapping_add(4));
    app.sync_selected_core_to_runtime();
    assert_eq!(app.harts[1].cpu.pc, halt_pc.wrapping_add(4));
    assert!(!app.can_start_run());

    let before_pc = app.run.cpu.pc;
    app.single_step();

    assert_eq!(app.run.cpu.pc, before_pc);
    app.sync_selected_core_to_runtime();
    assert_eq!(app.harts[1].cpu.pc, before_pc);
    assert_eq!(app.core_status(1), HartLifecycle::Exited);
    assert!(!app.can_start_run());
}

#[test]
fn focused_secondary_pipeline_ebreak_can_resume_with_step() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.pipeline.enabled = true;
    app.run_scope = RunScope::FocusedHart;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "halt",
            "worker:",
            "ebreak",
            "addi a0, zero, 9",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();

    let worker_pc = app.run.base_pc + 4;
    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    app.harts[1].cpu.pc = worker_pc;
    app.harts[1].prev_pc = worker_pc;
    if let Some(p) = app.harts[1].pipeline.as_mut() {
        p.enabled = true;
        p.reset_stages(worker_pc);
    }

    app.switch_selected_core(1);

    for _ in 0..8 {
        app.single_step();
        if app.core_status(1) == HartLifecycle::Paused {
            break;
        }
    }

    assert_eq!(app.core_status(1), HartLifecycle::Paused);
    assert!(app.can_start_run());

    app.single_step();

    assert_eq!(app.run.cpu.x[10], 9);
    assert_eq!(app.core_status(1), HartLifecycle::Running);
}

#[test]
fn focused_secondary_pipeline_unimp_then_ebreak_can_resume_with_step() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.pipeline.enabled = true;
    app.run_scope = RunScope::FocusedHart;
    app.set_cache_enabled(false);
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "halt",
            "worker:",
            "lui t0, 0",
            "addi t0, t0, 28",
            "lui t1, 0xc0001",
            "addi t1, t1, 115",
            "sw t1, 0(t0)",
            "fence",
            "jalr zero, t0, 0",
            "ebreak",
            "addi a0, zero, 11",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();

    let worker_pc = app.run.base_pc + 4;
    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    app.harts[1].cpu.pc = worker_pc;
    app.harts[1].prev_pc = worker_pc;
    if let Some(p) = app.harts[1].pipeline.as_mut() {
        p.enabled = true;
        p.reset_stages(worker_pc);
    }

    app.switch_selected_core(1);

    for _ in 0..16 {
        app.single_step();
        if app.core_status(1) == HartLifecycle::Paused {
            break;
        }
    }

    assert_eq!(
        app.run.mem.peek32(app.run.base_pc + 28).unwrap_or(0),
        0xC000_1073
    );
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Paused,
        "{}",
        trace_tail(&app)
    );
    assert!(app.can_start_run());

    app.single_step();

    assert_eq!(app.run.cpu.x[10], 11, "{}", trace_tail(&app));
    assert_eq!(app.core_status(1), HartLifecycle::Running);
}

#[test]
fn hart_start_can_reuse_exited_core() {
    let mut app = App::new(None);
    app.max_cores = 2;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "halt",
            "worker:",
            "halt",
        ],
    );
    app.rebuild_harts_for_debug();

    app.harts[1].hart_id = Some(7);
    app.harts[1].lifecycle = HartLifecycle::Exited;
    let (stack_lo, stack_hi) = app.stack_slot_bounds(1);
    let stack_ptr = stack_hi & !0xF;
    assert!(stack_ptr >= stack_lo);
    let worker_pc = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "worker").then_some(*addr))
        .expect("worker label present");
    app.run.cpu.pending_hart_start = Some(crate::falcon::registers::HartStartRequest {
        entry_pc: worker_pc,
        stack_ptr,
        arg: 123,
    });

    app.process_pending_hart_start_for_selected();

    assert_eq!(app.core_status(1), HartLifecycle::Running);
    assert_eq!(app.harts[1].cpu.pc, worker_pc);
    assert_eq!(app.harts[1].cpu.read(10), 123);
}

#[test]
fn disabling_cache_hides_cache_tab_and_falls_back_to_run() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.tab = Tab::Cache;

    app.set_cache_enabled(false);

    assert!(app.tab == Tab::Run);
    assert!(!app.visible_tabs().contains(&Tab::Cache));
}

#[test]
fn disabling_pipeline_hides_pipeline_tab_and_falls_back_to_run() {
    let mut app = App::new(None);
    app.set_pipeline_enabled(true);
    app.tab = Tab::Pipeline;

    app.set_pipeline_enabled(false);

    assert!(app.tab == Tab::Run);
    assert!(!app.visible_tabs().contains(&Tab::Pipeline));
}

#[test]
#[ignore = "Known pipeline issue with Rust-generated pointer-heavy code paths"]
fn rust_to_raven_debug_elf_runs_multihart_in_pipeline_without_fault() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = true;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..20_000 {
        if app.run.faulted || app.pipeline.faulted {
            break;
        }
        if app.run.cpu.exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(
        !app.run.faulted,
        "sequential run state faulted\n{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert!(
        !app.pipeline.faulted,
        "pipeline state faulted\n{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert_eq!(
        app.run.cpu.exit_code,
        Some(0),
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
}

#[test]
#[ignore = "Known pipeline issue with Rust-generated pointer-heavy code paths"]
fn rust_to_raven_debug_elf_single_core_pipeline_does_not_panic() {
    let mut app = App::new(None);
    app.max_cores = 1;
    app.pipeline.enabled = true;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..10_000 {
        if app.run.faulted || app.pipeline.faulted || app.run.cpu.exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(
        !app.run.faulted,
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert!(
        !app.pipeline.faulted,
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert_ne!(
        app.run.cpu.exit_code,
        Some(101),
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
}

#[test]
#[ignore = "Long-running integration probe kept for manual debugging"]
fn rust_to_raven_debug_elf_runs_multihart_sequential_without_fault() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = false;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..50_000 {
        if app.run.faulted {
            break;
        }
        if app.run.cpu.exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu.exit_code, Some(0), "{}", console_tail(&app));
}

#[test]
fn pipeline_ecall_return_updates_a0_before_next_consumer() {
    let mut app = App::new(None);
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "li a0, 0",
            "li a7, 214",
            "ecall",
            "addi t0, a0, 16",
            "addi a0, t0, 0",
            "li a7, 214",
            "ecall",
            "bne a0, t0, fail",
            "li a0, 0",
            "li a7, 93",
            "ecall",
            "fail:",
            "li a0, 99",
            "li a7, 93",
            "ecall",
        ],
    );

    for _ in 0..200 {
        if app.run.faulted || app.pipeline.faulted || app.run.cpu.exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu.exit_code, Some(0), "{}", trace_tail(&app));
}

#[test]
fn pipeline_hart_start_delivers_a0_to_spawned_hart() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".data",
            "result: .word 0",
            ".text",
            ".globl _start",
            "_start:",
            "la a0, worker",
            "li a1, 0x00FFF000",
            "li a2, 0x12345678",
            "li a7, 1100",
            "ecall",
            "halt",
            "worker:",
            "la t0, result",
            "sw a0, 0(t0)",
            "li a7, 1101",
            "ecall",
        ],
    );

    for _ in 0..200 {
        if app.run.faulted || app.pipeline.faulted {
            break;
        }
        if matches!(app.core_status(1), HartLifecycle::Exited) {
            break;
        }
        app.single_step();
    }

    let result_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "result").then_some(*addr))
        .expect("result label present");

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(
        app.run.mem.load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_ecall_reads_fresh_a0_through_a7_values() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".data",
            "result: .word 0",
            ".text",
            ".globl _start",
            "_start:",
            "la t3, worker",
            "addi a0, t3, 0",
            "lui t4, 0x0100",
            "addi a1, t4, -16",
            "lui t5, 0x12345",
            "addi a2, t5, 1656",
            "addi a7, zero, 1100",
            "ecall",
            "halt",
            "worker:",
            "la t0, result",
            "sw a0, 0(t0)",
            "li a7, 1101",
            "ecall",
        ],
    );

    for _ in 0..240 {
        if app.run.faulted || app.pipeline.faulted {
            break;
        }
        if matches!(app.core_status(1), HartLifecycle::Exited) {
            break;
        }
        app.single_step();
    }

    let result_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "result").then_some(*addr))
        .expect("result label present");

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(
        app.run.mem.load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_hart_exit_keeps_worker_pc_on_ecall() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "la a0, worker",
            "li a1, 0x00FFF000",
            "li a2, 0",
            "li a7, 1100",
            "ecall",
            "halt",
            "worker:",
            "li a7, 1101",
            "ecall",
            "ebreak",
        ],
    );

    let hart_exit_pc = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "worker").then_some(*addr + 4))
        .expect("worker ecall present");

    for _ in 0..200 {
        if app.run.faulted || app.pipeline.faulted {
            break;
        }
        if matches!(app.core_status(1), HartLifecycle::Exited) {
            break;
        }
        app.single_step();
    }

    app.switch_selected_core(1);

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(app.run.cpu.pc, hart_exit_pc, "{}", trace_tail(&app));
}

#[test]
fn pipeline_spawned_hart_branch_sees_fresh_andi_result() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".data",
            "result: .word 0",
            ".text",
            ".globl _start",
            "_start:",
            "la a0, worker",
            "li a1, 0x00FFF000",
            "li a2, 0x12345678",
            "li a7, 1100",
            "ecall",
            "halt",
            "worker:",
            "andi t1, a0, 3",
            "bnez t1, bad",
            "la t0, result",
            "sw a0, 0(t0)",
            "li a7, 1101",
            "ecall",
            "bad:",
            "la t0, result",
            "li t1, 0xDEAD",
            "sw t1, 0(t0)",
            "li a7, 1101",
            "ecall",
        ],
    );

    for _ in 0..200 {
        if app.run.faulted || app.pipeline.faulted {
            break;
        }
        if matches!(app.core_status(1), HartLifecycle::Exited) {
            break;
        }
        app.single_step();
    }

    let result_addr = app
        .run
        .labels
        .iter()
        .find_map(|(addr, names)| names.iter().any(|n| n == "result").then_some(*addr))
        .expect("result label present");

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(
        app.run.mem.load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_ret_sees_loaded_ra_with_stack_adjust_between() {
    let mut app = App::new(None);
    app.pipeline.enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "jal ra, jump_through_stack",
            "li a0, 0",
            "li a7, 93",
            "ecall",
            "jump_through_stack:",
            "addi sp, sp, -16",
            "la t0, success",
            "sw t0, 12(sp)",
            "lw ra, 12(sp)",
            "addi sp, sp, 16",
            "ret",
            "success:",
            "li a0, 7",
            "li a7, 93",
            "ecall",
        ],
    );

    for _ in 0..160 {
        if app.run.faulted || app.pipeline.faulted || app.run.cpu.exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.pipeline.faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu.exit_code, Some(7), "{}", trace_tail(&app));
}

#[test]
fn loading_elf_resets_pipeline_to_entry_pc() {
    let mut app = App::new(None);
    app.pipeline.enabled = true;
    app.load_binary(&rust_to_raven_elf_bytes());

    assert_eq!(app.pipeline.fetch_pc, app.run.cpu.pc);
    assert_eq!(app.run.cpu.pc, 0x0001_A398);
    assert_ne!(app.run.cpu.pc, app.run.base_pc);
}

#[test]
#[ignore = "Diagnostic differential run for Rust ELF pipeline divergence"]
fn rust_to_raven_debug_elf_pipeline_matches_sequential_until_exit() {
    let mut seq = App::new(None);
    seq.max_cores = 1;
    seq.pipeline.enabled = false;
    seq.tab = Tab::Run;
    seq.load_binary(&rust_to_raven_elf_bytes());

    let mut pipe = App::new(None);
    pipe.max_cores = 1;
    pipe.pipeline.enabled = true;
    pipe.tab = Tab::Run;
    pipe.load_binary(&rust_to_raven_elf_bytes());

    for step in 0..10_000 {
        if seq.run.faulted || pipe.run.faulted || pipe.pipeline.faulted {
            panic!(
                "fault before divergence check at step {step}\nSEQ:\n{}\n{}\nPIPE:\n{}\n{}",
                console_tail(&seq),
                trace_tail(&seq),
                console_tail(&pipe),
                trace_tail(&pipe)
            );
        }
        if seq.run.cpu.exit_code.is_some() || pipe.run.cpu.exit_code.is_some() {
            assert_eq!(
                pipe.run.cpu.exit_code,
                seq.run.cpu.exit_code,
                "exit mismatch at step {step}\nSEQ:\n{}\n{}\nPIPE:\n{}\n{}",
                console_tail(&seq),
                trace_tail(&seq),
                console_tail(&pipe),
                trace_tail(&pipe)
            );
            return;
        }

        seq.single_step();
        pipe.single_step();

        let same_core = seq.run.cpu.pc == pipe.run.cpu.pc
            && seq.run.cpu.x == pipe.run.cpu.x
            && seq.run.cpu.f == pipe.run.cpu.f
            && seq.run.cpu.heap_break == pipe.run.cpu.heap_break
            && seq.run.cpu.local_exit == pipe.run.cpu.local_exit
            && seq.run.cpu.ebreak_hit == pipe.run.cpu.ebreak_hit
            && seq.run.cpu.exit_code == pipe.run.cpu.exit_code;
        let same_mem_stats = seq.run.mem.instruction_count == pipe.run.mem.instruction_count;

        assert!(
            same_core && same_mem_stats,
            "diverged at step {step}\nSEQ pc=0x{:08X} sp=0x{:08X} ra=0x{:08X} a0=0x{:08X} a1=0x{:08X} a2=0x{:08X}\n{}\n{}\nPIPE pc=0x{:08X} sp=0x{:08X} ra=0x{:08X} a0=0x{:08X} a1=0x{:08X} a2=0x{:08X}\n{}\n{}",
            seq.run.cpu.pc,
            seq.run.cpu.x[2],
            seq.run.cpu.x[1],
            seq.run.cpu.x[10],
            seq.run.cpu.x[11],
            seq.run.cpu.x[12],
            console_tail(&seq),
            trace_tail(&seq),
            pipe.run.cpu.pc,
            pipe.run.cpu.x[2],
            pipe.run.cpu.x[1],
            pipe.run.cpu.x[10],
            pipe.run.cpu.x[11],
            pipe.run.cpu.x[12],
            console_tail(&pipe),
            trace_tail(&pipe)
        );
    }

    panic!(
        "no exit/divergence after limit\nSEQ:\n{}\n{}\nPIPE:\n{}\n{}",
        console_tail(&seq),
        trace_tail(&seq),
        console_tail(&pipe),
        trace_tail(&pipe)
    );
}
