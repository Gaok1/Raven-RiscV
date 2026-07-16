use super::{App, HartLifecycle, RunScope, Tab};
use crate::falcon::cache::CacheConfig;
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

fn slow_level(line_size: usize, size: usize, hit_latency: u64) -> CacheConfig {
    CacheConfig {
        size,
        line_size,
        associativity: 1,
        hit_latency,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: line_size as u32,
        ..CacheConfig::default()
    }
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
    assert_eq!(app.run.cpu().x[10], 0);

    app.single_step();
    assert_eq!(app.run.cpu().x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn single_step_advances_from_ebreak_pause_pipeline() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
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
    assert_eq!(app.run.cpu().x[10], 0);

    app.single_step();
    assert_eq!(app.run.cpu().x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);
}

#[test]
fn screen_sleep_then_exit_parks_on_the_ecall() {
    // Regression: a game loop that sleeps (screen_sleep_ms) and then exits
    // must park on the exit ecall — never run past the end of .text
    // ("outside any executable region").
    let mut app = App::new(None);
    load_program(
        &mut app,
        &[
            ".text",
            "li a0, 32",
            "li a1, 32",
            "li a7, 2000", // screen_init(32, 32)
            "ecall",
            "li a0, 5",
            "li a7, 2007", // screen_sleep_ms(5)
            "ecall",
            "li a0, 0",
            "li a7, 93", // exit(0)
            "ecall",
        ],
    );
    app.run.speed = super::RunSpeed::X8;
    app.run.is_running = true;
    for _ in 0..500 {
        app.tick();
        if app.run.cpu().exit_code.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        app.run.cpu().exit_code,
        Some(0),
        "console: {}\ntrace: {}",
        console_tail(&app),
        trace_tail(&app)
    );

    // The bug showed up on ticks after the exit: keep ticking.
    for _ in 0..20 {
        app.tick();
    }
    let tail = console_tail(&app);
    assert!(
        !tail.contains("outside any executable region"),
        "executed past the exit ecall:\n{tail}"
    );
}

#[test]
fn pipeline_tab_single_step_without_cache_stall_advances_one_cycle() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
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
    app.run.machine.mem_mut_unjournaled().bypass = true;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    let cycle_before = app.run.pipeline().cycle_count;
    app.single_step();

    assert_eq!(
        app.run.pipeline().cycle_count,
        cycle_before + 1,
        "pipeline tab step should advance exactly one cycle when there is no cache stall"
    );
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::IF as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1"),
        "the first step should only fetch the first instruction"
    );
}

#[test]
fn pipeline_tab_single_step_skips_consecutive_icache_stalls() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
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
    app.run.machine.mem_mut_unjournaled().icache.config.hit_latency = 3;
    app.run.machine.mem_mut_unjournaled().icache.config.miss_penalty = 0;
    app.run.machine.mem_mut_unjournaled().icache.config.assoc_penalty = 0;
    app.run.machine.mem_mut_unjournaled().icache.config.transfer_width = 4;
    app.run.machine.mem_mut_unjournaled().bypass = false;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    app.single_step();
    let if_slot = app.run.pipeline().stages[crate::ui::pipeline::Stage::IF as usize]
        .as_ref()
        .expect("instruction should be fetched once IF cache stalls are consumed");
    assert_eq!(if_slot.disasm, "addi a0, zero, 1");
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::ID as usize].is_none(),
        "after skipping cache stalls the step should stop on the first non-stall cycle"
    );
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize].is_none(),
        "no extra cycle should be executed after the first useful fetch"
    );
    app.single_step();
    let id_slot = app.run.pipeline().stages[crate::ui::pipeline::Stage::ID as usize]
        .as_ref()
        .expect("the next step should move the fetched instruction into ID");
    assert_eq!(id_slot.disasm, "addi a0, zero, 1");
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::IF as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "halt"),
        "the following instruction should now be in IF"
    );

    app.single_step();
    let advanced_or_committed = app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
        .as_ref()
        .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1")
        || app.run.pipeline().stages[crate::ui::pipeline::Stage::MEM as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1")
        || app.run.pipeline().stages[crate::ui::pipeline::Stage::WB as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1")
        || app.run.cpu().x[10] == 1;
    assert!(
        advanced_or_committed,
        "instruction should continue to advance on the next user step"
    );
}

#[test]
fn sequential_single_step_updates_cache_history_each_instruction() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
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

    app.single_step();

    assert_eq!(app.run.mem().instruction_count, 1);
    assert_eq!(app.run.mem().icache.stats.history.len(), 1);
    assert_eq!(app.run.mem().dcache.stats.history.len(), 1);
}

#[test]
fn stepback_reverts_sequential_step_state_and_trace() {
    let mut app = App::new(None);
    app.tab = Tab::Run;
    // This covers the sequential interpreter path specifically (pipeline off);
    // `stepback_reverts_pipeline_cycle_state` covers the pipeline path.
    app.run.pipeline_mut().enabled = false;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 7",
            "addi a1, zero, 9",
        ],
    );
    assert!(!app.can_stepback_now(), "nothing to undo before stepping");

    app.single_step();
    assert_eq!(app.run.cpu().x[10], 7);
    app.single_step();
    assert_eq!(app.run.cpu().x[11], 9);
    let pc_after_two = app.run.cpu().pc;
    assert_eq!(app.run.exec_trace.len(), 2);
    assert!(app.can_stepback_now());

    // Undo the second instruction: x11 reverts, PC backs up, trace shrinks.
    app.stepback_one();
    assert_eq!(app.run.cpu().x[11], 0, "x11 reverted");
    assert_eq!(app.run.exec_trace.len(), 1);
    assert!(app.run.cpu().pc < pc_after_two);

    // Undo the first: x10 reverts and the journal empties.
    app.stepback_one();
    assert_eq!(app.run.cpu().x[10], 0, "x10 reverted");
    assert_eq!(app.run.exec_trace.len(), 0);
    assert!(!app.can_stepback_now(), "journal drained");
}

/// Step-back in **pipeline** mode rewinds one clock cycle: the pipeline's own
/// state (stages, fetch PC, cycle counter) reverts together with the CPU and
/// memory, because the pipeline now lives inside `Machine` and every cycle is
/// journaled. This is the case that used to be impossible — the journal stayed
/// empty in pipeline mode, so `can_stepback_now()` never became true.
#[test]
fn stepback_reverts_pipeline_cycle_state() {
    let mut app = App::new(None);
    // default max_cores (4), AllHarts — the real-world config
    app.tab = Tab::Run;
    // Pipeline on (the default). Drive the journaling per-cycle path.
    app.run.pipeline_mut().enabled = true;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 1",
            "addi a1, zero, 2",
            "addi a2, zero, 3",
            "addi a3, zero, 4",
            "addi a4, zero, 5",
            "addi a5, zero, 6",
            "addi a6, zero, 7",
            "addi a7, zero, 8",
        ],
    );

    // Fingerprint the full reversible state before any cycle runs.
    let x0 = app.run.cpu().x;
    let pc0 = app.run.cpu().pc;
    let cyc0 = app.run.pipeline().cycle_count;
    let fetch0 = app.run.pipeline().fetch_pc;
    let stages_empty0 = app.run.pipeline().stages.iter().all(|s| s.is_none());

    assert!(!app.can_stepback_now(), "nothing to undo before stepping");

    // Single-step advances whole instructions; in pipeline mode each is several
    // journaled clock cycles. Two steps leaves a multi-cycle journal.
    app.single_step();
    app.single_step();

    // The headline fix: pipeline mode now fills the journal, so step-back is live.
    assert!(
        app.can_stepback_now(),
        "pipeline mode must be reversible now"
    );
    assert!(
        app.run.pipeline().cycle_count > cyc0,
        "the clock advanced"
    );
    assert!(app.run.cpu().x[10] >= 1, "first instruction took effect");

    // Drain the journal one cycle at a time; the clock must move strictly
    // backward and never get stuck.
    let mut prev = app.run.pipeline().cycle_count;
    let mut guard = 0;
    while app.can_stepback_now() {
        app.stepback_one();
        let now = app.run.pipeline().cycle_count;
        assert!(now < prev, "each step-back rewinds at least one cycle");
        prev = now;
        guard += 1;
        assert!(guard < 1000, "stepback failed to drain");
    }

    // Back to the exact pre-run state — CPU and pipeline both.
    assert_eq!(app.run.cpu().x, x0, "registers round-tripped");
    assert_eq!(app.run.cpu().pc, pc0, "PC round-tripped");
    assert_eq!(app.run.pipeline().cycle_count, cyc0, "clock round-tripped");
    assert_eq!(app.run.pipeline().fetch_pc, fetch0, "fetch PC round-tripped");
    assert_eq!(
        app.run.pipeline().stages.iter().all(|s| s.is_none()),
        stages_empty0,
        "pipeline stages round-tripped"
    );
    assert_eq!(app.run.exec_trace.len(), 0, "exec trace drained");
}

#[test]
fn pipeline_single_step_updates_cache_history_on_commit() {
    let mut app = App::new(None);
    app.set_cache_enabled(true);
    app.run.pipeline_mut().enabled = true;
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

    app.single_step();

    assert_eq!(app.run.mem().instruction_count, 1);
    assert_eq!(app.run.mem().icache.stats.history.len(), 1);
    assert_eq!(app.run.mem().dcache.stats.history.len(), 1);
}

#[test]
fn pipeline_tab_single_step_does_not_skip_useful_cycle_while_if_cache_stalls() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi sp, sp, -4",
            "sw ra, 0(sp)",
            "halt",
        ],
    );
    app.run.machine.mem_mut_unjournaled().icache.config.hit_latency = 3;
    app.run.machine.mem_mut_unjournaled().icache.config.miss_penalty = 0;
    app.run.machine.mem_mut_unjournaled().icache.config.assoc_penalty = 0;
    app.run.machine.mem_mut_unjournaled().icache.config.transfer_width = 4;
    app.run.machine.mem_mut_unjournaled().bypass = false;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    app.single_step();
    app.single_step();

    let id_before = app.run.pipeline().stages[crate::ui::pipeline::Stage::ID as usize]
        .as_ref()
        .expect("first instruction should be in ID before the useful cycle");
    let if_before = app.run.pipeline().stages[crate::ui::pipeline::Stage::IF as usize]
        .as_ref()
        .expect("second instruction should still be in IF cache stall");
    assert_eq!(id_before.disasm, "addi sp, sp, -4");
    assert!(
        if_before.disasm.starts_with("sw"),
        "expected IF to hold the store instruction, got {:?}",
        if_before.disasm
    );
    assert!(
        if_before.if_stall_cycles > 0,
        "setup requires IF to still be stalled by cache latency"
    );

    let cycle_before = app.run.pipeline().cycle_count;
    app.single_step();

    assert_eq!(
        app.run.pipeline().cycle_count,
        cycle_before + 1,
        "a useful cycle with IF cache pressure must not be auto-skipped"
    );
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi sp, sp, -4"),
        "the user step must stop with the instruction still visible in EX"
    );
    assert!(
        !app.run.pipeline().stages[crate::ui::pipeline::Stage::MEM as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi sp, sp, -4"),
        "the step must not jump past EX into MEM with the same instruction"
    );
    assert!(
        !app.run.pipeline().stages[crate::ui::pipeline::Stage::WB as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi sp, sp, -4"),
        "the step must not jump past EX into WB with the same instruction"
    );
}

#[test]
fn pipeline_tab_single_step_does_not_skip_useful_cycle_while_multilevel_if_stalls() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi sp, sp, -4",
            "sw ra, 0(sp)",
            "halt",
        ],
    );
    let icfg = slow_level(4, 4, 1);
    let dcfg = slow_level(16, 16, 1);
    let l2 = slow_level(4, 4, 5);
    let l3 = slow_level(4, 8, 9);
    app.run.machine.mem_mut_unjournaled().apply_config(icfg, dcfg, vec![l2, l3]);
    app.run.machine.mem_mut_unjournaled().bypass = false;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    app.single_step();
    app.single_step();

    let id_before = app.run.pipeline().stages[crate::ui::pipeline::Stage::ID as usize]
        .as_ref()
        .expect("first instruction should be in ID before the useful cycle");
    let if_before = app.run.pipeline().stages[crate::ui::pipeline::Stage::IF as usize]
        .as_ref()
        .expect("second instruction should still be in IF cache stall");
    assert_eq!(id_before.disasm, "addi sp, sp, -4");
    assert!(if_before.disasm.starts_with("sw"));
    assert!(if_before.if_stall_cycles > 0);

    let cycle_before = app.run.pipeline().cycle_count;
    app.single_step();

    assert_eq!(app.run.pipeline().cycle_count, cycle_before + 1);
    assert!(!app.run.pipeline().last_cycle_cache_only);
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi sp, sp, -4")
    );
}

#[test]
fn pipeline_tab_single_step_does_not_skip_useful_cycle_while_multilevel_mem_stalls() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
    load_program(
        &mut app,
        &[
            ".data",
            "val: .word 0x12345678",
            ".text",
            ".globl _start",
            "_start:",
            "la t0, val",
            "lw a0, 0(t0)",
            "addi a1, zero, 1",
            "halt",
        ],
    );
    let icfg = slow_level(16, 16, 1);
    let dcfg = slow_level(4, 4, 1);
    let l2 = slow_level(4, 4, 5);
    let l3 = slow_level(4, 8, 9);
    app.run.machine.mem_mut_unjournaled().apply_config(icfg, dcfg, vec![l2, l3]);
    app.run.machine.mem_mut_unjournaled().bypass = false;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    for _ in 0..16 {
        if app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("lw"))
        {
            break;
        }
        app.single_step();
    }

    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("lw")),
        "load should be poised to enter MEM"
    );

    let cycle_before = app.run.pipeline().cycle_count;
    app.single_step();

    let mem_slot = app.run.pipeline().stages[crate::ui::pipeline::Stage::MEM as usize]
        .as_ref()
        .expect("load should become visible in MEM");
    assert_eq!(app.run.pipeline().cycle_count, cycle_before + 1);
    assert!(!app.run.pipeline().last_cycle_cache_only);
    assert!(mem_slot.disasm.starts_with("lw"));
    assert!(mem_slot.mem_stall_cycles > 0);
}

#[test]
fn pipeline_tab_single_step_keeps_single_cycle_alu_latency_visible_in_ex() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Pipeline;
    app.run.cpi_config.alu = 3;
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
    app.run.machine.mem_mut_unjournaled().bypass = true;
    let __pc = app.run.cpu().pc; app.run.pipeline_mut().reset_stages(__pc);

    for _ in 0..8 {
        app.single_step();
        if app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1")
        {
            break;
        }
    }

    let ex_cycles_left = app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
        .as_ref()
        .map(|slot| slot.fu_cycles_left)
        .expect("addi should become visible in EX");
    assert!(ex_cycles_left > 1);

    let cycle_before = app.run.pipeline().cycle_count;
    app.single_step();

    assert_eq!(app.run.pipeline().cycle_count, cycle_before + 1);
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm == "addi a0, zero, 1"),
        "step should stop with the ALU instruction still visible in EX"
    );
    assert!(
        app.run.pipeline().stages[crate::ui::pipeline::Stage::MEM as usize]
            .as_ref()
            .is_none_or(|slot| slot.is_bubble || slot.disasm != "addi a0, zero, 1"),
        "single-step must not skip the ALU instruction past EX"
    );
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
    assert_eq!(app.run.cpu().x[10], 1);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Running);

    app.single_step();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu().x[10], 1);
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
fn sequential_linux_exit_is_terminal_not_resumable() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = false;
    app.tab = Tab::Run;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "li a0, 7",
            "li a7, 93",
            "ecall",
            "addi a0, zero, 9",
        ],
    );

    for _ in 0..12 {
        app.single_step();
        if app.core_status(app.selected_core) == HartLifecycle::Exited {
            break;
        }
    }

    let exit_pc = app.run.cpu().pc;
    assert_eq!(app.run.cpu().exit_code, Some(7));
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu().pc, exit_pc);
    assert_eq!(app.run.cpu().x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
    assert!(!app.run.faulted, "{}", console_tail(&app));
}

#[test]
fn pipeline_halt_is_terminal_not_resumable() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
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
    assert!(app.run.cpu().local_exit);
    assert!(!app.run.cpu().ebreak_hit);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu().x[10], 0);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
}

#[test]
fn pipeline_linux_exit_in_run_tab_is_terminal_not_resumable() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Run;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "li a0, 7",
            "li a7, 93",
            "ecall",
            "addi a0, zero, 9",
        ],
    );

    for _ in 0..32 {
        app.single_step();
        if app.core_status(app.selected_core) == HartLifecycle::Exited {
            break;
        }
    }

    let exit_pc = app.run.cpu().pc;
    assert_eq!(app.run.cpu().exit_code, Some(7), "{}", console_tail(&app));
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.resume_selected_hart();
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);

    app.single_step();
    assert_eq!(app.run.cpu().pc, exit_pc);
    assert_eq!(app.run.cpu().x[10], 7);
    assert_eq!(app.core_status(app.selected_core), HartLifecycle::Exited);
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert!(!app.run.faulted, "{}", console_tail(&app));
}

#[test]
fn pipeline_all_harts_scope_keeps_halted_hart_exited() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run.pipeline_mut().enabled = true;
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
    if let Some(pipe) = app.harts[1].pipeline.as_mut() {
        pipe.enabled = true;
        pipe.reset_stages(worker_pc);
    }
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
fn all_harts_step_advances_selected_and_non_selected_harts() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    load_program(
        &mut app,
        &[
            ".text",
            ".globl _start",
            "_start:",
            "addi a0, zero, 1",
            "halt",
            "worker:",
            "addi a1, zero, 9",
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

    app.switch_selected_core(1);
    for _ in 0..4 {
        app.single_step();
        app.sync_selected_core_to_runtime();
        if app.harts[0].cpu.x[10] == 1 && app.harts[1].cpu.x[11] == 9 {
            break;
        }
    }

    assert_eq!(app.harts[0].cpu.x[10], 1);
    assert_eq!(app.harts[1].cpu.x[11], 9);
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

    assert_eq!(app.run.cpu().x[5], trap_pc);
    assert_eq!(app.run.mem().peek32(trap_pc).unwrap_or(0), 0xC000_1073);
    assert_eq!(app.core_status(1), HartLifecycle::Exited);
    assert_eq!(app.run.cpu().pc, halt_pc.wrapping_add(4));
    app.sync_selected_core_to_runtime();
    assert_eq!(app.harts[1].cpu.pc, halt_pc.wrapping_add(4));
    assert!(!app.can_start_run());

    let before_pc = app.run.cpu().pc;
    app.single_step();

    assert_eq!(app.run.cpu().pc, before_pc);
    app.sync_selected_core_to_runtime();
    assert_eq!(app.harts[1].cpu.pc, before_pc);
    assert_eq!(app.core_status(1), HartLifecycle::Exited);
    assert!(!app.can_start_run());
}

#[test]
fn focused_secondary_pipeline_ebreak_can_resume_with_step() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run.pipeline_mut().enabled = true;
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

    assert_eq!(app.run.cpu().x[10], 9);
    assert_eq!(app.core_status(1), HartLifecycle::Running);
}

#[test]
fn focused_secondary_pipeline_unimp_then_ebreak_can_resume_with_step() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run.pipeline_mut().enabled = true;
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
        app.run.mem().peek32(app.run.base_pc + 28).unwrap_or(0),
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

    assert_eq!(app.run.cpu().x[10], 11, "{}", trace_tail(&app));
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
    app.run.machine.cpu_mut_unjournaled().pending_hart_start = Some(crate::falcon::registers::HartStartRequest {
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
fn disabling_pipeline_keeps_pipeline_tab_visible_for_sequential_mode() {
    // Pipeline tab is always visible — when disabled it shows sequential mode.
    let mut app = App::new(None);
    app.set_pipeline_enabled(true);
    app.tab = Tab::Pipeline;

    app.set_pipeline_enabled(false);

    // Tab stays on Pipeline; it is still reachable (sequential-mode view).
    assert!(app.tab == Tab::Pipeline);
    assert!(app.visible_tabs().contains(&Tab::Pipeline));
    // Sequential mode flag is set when pipeline is disabled
    assert!(app.run.pipeline().sequential_mode);
    assert!(!app.run.pipeline().enabled);
}

#[test]
fn toggling_pipeline_reconfigures_all_hart_pipeline_states() {
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
    app.set_pipeline_enabled(true);
    app.run.pipeline_mut().cycle_count = 9;
    app.run.pipeline_mut().bypass.store_to_load = true;
    app.run.pipeline_mut()
        .set_predict(crate::ui::pipeline::BranchPredict::TwoBit);
    app.reconfigure_pipeline_model();

    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    if let Some(p) = app.harts[1].pipeline.as_mut() {
        assert!(p.enabled);
        assert_eq!(p.predict, crate::ui::pipeline::BranchPredict::TwoBit);
        assert!(p.bypass.store_to_load);
        assert_eq!(p.cycle_count, 0);
    }

    app.set_pipeline_enabled(false);

    assert!(!app.run.pipeline().enabled);
    assert_eq!(app.run.pipeline().cycle_count, 0);
    if let Some(p) = app.harts[1].pipeline.as_ref() {
        assert!(!p.enabled);
        assert_eq!(p.cycle_count, 0);
    }
}

#[test]
fn aggregate_pipeline_snapshot_includes_non_selected_harts() {
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
    app.set_pipeline_enabled(true);

    app.run.pipeline_mut().cycle_count = 7;
    app.run.pipeline_mut().instr_committed = 3;
    app.run.pipeline_mut().stall_count = 2;
    app.run.pipeline_mut().flush_count = 1;
    app.run.pipeline_mut().branches_executed = 2;
    app.run.pipeline_mut().stall_by_type = [1, 0, 1, 0, 0];

    app.harts[1].hart_id = Some(1);
    app.harts[1].lifecycle = HartLifecycle::Running;
    let other = app.harts[1]
        .pipeline
        .as_mut()
        .expect("background hart has pipeline state");
    other.cycle_count = 11;
    other.instr_committed = 5;
    other.stall_count = 4;
    other.flush_count = 2;
    other.branches_executed = 3;
    other.stall_by_type = [2, 1, 0, 1, 0];

    let snap = app
        .aggregate_pipeline_snapshot()
        .expect("pipeline snapshot present");

    assert_eq!(
        snap.cycles, 11,
        "program wall-clock should follow the slowest hart"
    );
    assert_eq!(snap.committed, 8);
    assert_eq!(snap.stalls, 6);
    assert_eq!(snap.flushes, 3);
    assert_eq!(snap.branches, 5);
    assert_eq!(snap.raw_stalls, 3);
    assert_eq!(snap.load_use_stalls, 1);
    assert_eq!(snap.branch_stalls, 1);
    assert_eq!(snap.fu_stalls, 1);
    assert_eq!(snap.mem_stalls, 0);
    assert!((snap.cpi - (11.0 / 8.0)).abs() < f64::EPSILON);
}

#[test]
fn rust_to_raven_debug_elf_runs_multihart_in_pipeline_without_fault() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..100_000 {
        if app.run.faulted || app.run.pipeline().faulted {
            break;
        }
        if app.run.cpu().exit_code.is_some() {
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
        !app.run.pipeline().faulted,
        "pipeline state faulted\n{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert_eq!(
        app.run.cpu().exit_code,
        Some(0),
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
}

fn rust_to_raven_debug_elf_single_core_pipeline_does_not_panic() {
    let mut app = App::new(None);
    app.max_cores = 1;
    app.run.pipeline_mut().enabled = true;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..10_000 {
        if app.run.faulted || app.run.pipeline().faulted || app.run.cpu().exit_code.is_some() {
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
        !app.run.pipeline().faulted,
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
    assert_ne!(
        app.run.cpu().exit_code,
        Some(101),
        "{}\n{}",
        console_tail(&app),
        trace_tail(&app)
    );
}

#[test]
fn rebuild_harts_copies_parallel_fu_config_to_background_cores() {
    let mut app = App::new(None);
    app.max_cores = 3;
    app.run.pipeline_mut().enabled = true;
    app.run.pipeline_mut().mode = crate::ui::pipeline::PipelineMode::FunctionalUnits;
    app.run.pipeline_mut().fu_capacity[crate::ui::pipeline::FuKind::Alu.index()] = 3;
    app.run.pipeline_mut().fu_capacity[crate::ui::pipeline::FuKind::Lsu.index()] = 2;
    app.rebuild_harts_for_debug();

    let bg = app.harts[1].pipeline.as_ref().expect("background pipeline");
    assert_eq!(bg.mode, app.run.pipeline().mode);
    assert_eq!(bg.fu_capacity, app.run.pipeline().fu_capacity);
    assert_eq!(bg.exec_regions, app.run.pipeline().exec_regions);
}

#[test]
fn hart_start_child_inherits_parallel_fu_config() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run.pipeline_mut().enabled = true;
    app.run.pipeline_mut().mode = crate::ui::pipeline::PipelineMode::FunctionalUnits;
    app.run.pipeline_mut().fu_capacity[crate::ui::pipeline::FuKind::Div.index()] = 4;
    app.run.pipeline_mut().fu_capacity[crate::ui::pipeline::FuKind::Lsu.index()] = 2;
    app.run.machine.cpu_mut_unjournaled().pending_hart_start = Some(crate::falcon::registers::HartStartRequest {
        entry_pc: app.run.base_pc,
        stack_ptr: 0x0010_0000,
        arg: 0x1234_5678,
    });

    app.process_pending_hart_start_for_selected();

    let child = app.harts[1].pipeline.as_ref().expect("child pipeline");
    assert_eq!(child.mode, app.run.pipeline().mode);
    assert_eq!(child.fu_capacity, app.run.pipeline().fu_capacity);
    assert_eq!(app.harts[1].cpu.read(10), 0x1234_5678);
}

#[test]
#[ignore = "Long-running integration probe kept for manual debugging"]
fn rust_to_raven_debug_elf_runs_multihart_sequential_without_fault() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = false;
    app.tab = Tab::Run;
    app.load_binary(&rust_to_raven_elf_bytes());

    for _ in 0..50_000 {
        if app.run.faulted {
            break;
        }
        if app.run.cpu().exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu().exit_code, Some(0), "{}", console_tail(&app));
}

#[test]
fn pipeline_ecall_return_updates_a0_before_next_consumer() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted || app.run.cpu().exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu().exit_code, Some(0), "{}", trace_tail(&app));
}

#[test]
fn pipeline_hart_start_delivers_a0_to_spawned_hart() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted {
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
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(
        app.run.mem().load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_ecall_reads_fresh_a0_through_a7_values() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted {
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
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(
        app.run.mem().load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_hart_exit_keeps_worker_pc_on_ecall() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted {
            break;
        }
        if matches!(app.core_status(1), HartLifecycle::Exited) {
            break;
        }
        app.single_step();
    }

    app.switch_selected_core(1);

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(
        app.core_status(1),
        HartLifecycle::Exited,
        "{}",
        trace_tail(&app)
    );
    assert_eq!(app.run.cpu().pc, hart_exit_pc, "{}", trace_tail(&app));
}

#[test]
fn pipeline_spawned_hart_branch_sees_fresh_andi_result() {
    let mut app = App::new(None);
    app.max_cores = 2;
    app.run_scope = RunScope::AllHarts;
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted {
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
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(
        app.run.mem().load32(result_addr).expect("result word"),
        0x1234_5678
    );
}

#[test]
fn pipeline_ret_sees_loaded_ra_with_stack_adjust_between() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
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
        if app.run.faulted || app.run.pipeline().faulted || app.run.cpu().exit_code.is_some() {
            break;
        }
        app.single_step();
    }

    assert!(!app.run.faulted, "{}", console_tail(&app));
    assert!(!app.run.pipeline().faulted, "{}", console_tail(&app));
    assert_eq!(app.run.cpu().exit_code, Some(7), "{}", trace_tail(&app));
}

#[test]
fn loading_elf_resets_pipeline_to_entry_pc() {
    let mut app = App::new(None);
    app.run.pipeline_mut().enabled = true;
    let elf = rust_to_raven_elf_bytes();
    let mut ram = crate::falcon::memory::Ram::new(16 * 1024 * 1024);
    let info = crate::falcon::program::load_elf(&elf, &mut ram).expect("parse rust-to-raven ELF");
    app.load_binary(&elf);

    assert_eq!(app.run.pipeline().fetch_pc, app.run.cpu().pc);
    assert_eq!(app.run.cpu().pc, info.entry);
    assert_ne!(app.run.cpu().pc, app.run.base_pc);
}

#[test]
#[ignore = "Diagnostic differential run for Rust ELF pipeline divergence"]
fn rust_to_raven_debug_elf_pipeline_matches_sequential_until_exit() {
    let mut seq = App::new(None);
    seq.max_cores = 1;
    seq.run.pipeline_mut().enabled = false;
    seq.tab = Tab::Run;
    seq.load_binary(&rust_to_raven_elf_bytes());

    let mut pipe = App::new(None);
    pipe.max_cores = 1;
    pipe.run.pipeline_mut().enabled = true;
    pipe.tab = Tab::Run;
    pipe.load_binary(&rust_to_raven_elf_bytes());

    for step in 0..10_000 {
        if seq.run.faulted || pipe.run.faulted || pipe.run.pipeline().faulted {
            panic!(
                "fault before divergence check at step {step}\nSEQ:\n{}\n{}\nPIPE:\n{}\n{}",
                console_tail(&seq),
                trace_tail(&seq),
                console_tail(&pipe),
                trace_tail(&pipe)
            );
        }
        if seq.run.cpu().exit_code.is_some() || pipe.run.cpu().exit_code.is_some() {
            assert_eq!(
                pipe.run.cpu().exit_code,
                seq.run.cpu().exit_code,
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

        let same_core = seq.run.cpu().pc == pipe.run.cpu().pc
            && seq.run.cpu().x == pipe.run.cpu().x
            && seq.run.cpu().f == pipe.run.cpu().f
            && seq.run.cpu().heap_break == pipe.run.cpu().heap_break
            && seq.run.cpu().local_exit == pipe.run.cpu().local_exit
            && seq.run.cpu().ebreak_hit == pipe.run.cpu().ebreak_hit
            && seq.run.cpu().exit_code == pipe.run.cpu().exit_code;
        let same_mem_stats = seq.run.mem().instruction_count == pipe.run.mem().instruction_count;

        assert!(
            same_core && same_mem_stats,
            "diverged at step {step}\nSEQ pc=0x{:08X} sp=0x{:08X} ra=0x{:08X} a0=0x{:08X} a1=0x{:08X} a2=0x{:08X}\n{}\n{}\nPIPE pc=0x{:08X} sp=0x{:08X} ra=0x{:08X} a0=0x{:08X} a1=0x{:08X} a2=0x{:08X}\n{}\n{}",
            seq.run.cpu().pc,
            seq.run.cpu().x[2],
            seq.run.cpu().x[1],
            seq.run.cpu().x[10],
            seq.run.cpu().x[11],
            seq.run.cpu().x[12],
            console_tail(&seq),
            trace_tail(&seq),
            pipe.run.cpu().pc,
            pipe.run.cpu().x[2],
            pipe.run.cpu().x[1],
            pipe.run.cpu().x[10],
            pipe.run.cpu().x[11],
            pipe.run.cpu().x[12],
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

// ── Phase 5: inline editing (registers / PC / floats / RAM) ─────────────────

mod run_edit {
    use super::super::{App, FormatMode, RunEditTarget};
    use super::load_program;
    use crate::falcon::machine::types::{FRegId, MemWidth, RegId, RegTarget};

    /// A minimal loaded program so the machine has a CPU + memory to edit.
    fn loaded_app() -> App {
        let mut app = App::new(None);
        load_program(
            &mut app,
            &[".text", ".globl _start", "_start:", "addi a0, zero, 1"],
        );
        app.rebuild_harts_for_debug();
        app
    }

    fn x(index: u8) -> RunEditTarget {
        RunEditTarget::Reg(RegTarget::X(RegId::new(index).unwrap()))
    }

    #[test]
    fn commit_writes_integer_register() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        app.begin_run_edit(x(5));
        app.run.run_edit_buf = "deadbeef".to_string();
        app.commit_run_edit();
        assert!(app.run.run_edit.is_none(), "editor closes on success");
        assert_eq!(app.run.cpu().x[5], 0xdead_beef);
    }

    #[test]
    fn commit_writes_pc() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        app.begin_run_edit(RunEditTarget::Reg(RegTarget::Pc));
        app.run.run_edit_buf = "100".to_string();
        app.commit_run_edit();
        assert_eq!(app.run.cpu().pc, 0x100);
    }

    /// Editing the PC must also steer the pipeline's fetch, otherwise the next
    /// step keeps fetching the stale `fetch_pc` (the "PC doesn't advance" bug).
    #[test]
    fn commit_pc_redirects_pipeline_fetch() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        let entry = app.run.base_pc;
        app.begin_run_edit(RunEditTarget::Reg(RegTarget::Pc));
        app.run.run_edit_buf = format!("{entry:x}");
        app.commit_run_edit();
        if app.run.pipeline().enabled {
            assert_eq!(app.run.pipeline().fetch_pc, entry);
        }
    }

    #[test]
    fn commit_rejects_x0_and_keeps_editor_open() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        app.begin_run_edit(x(0));
        app.run.run_edit_buf = "5".to_string();
        app.commit_run_edit();
        assert!(app.run.run_edit.is_some(), "editor stays open on rejection");
        assert!(app.run.run_edit_error.is_some());
        assert_eq!(app.run.cpu().x[0], 0);
    }

    #[test]
    fn commit_rejects_out_of_range_byte_cell() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        app.run.mem_view_bytes = 1;
        let addr = app.run.data_base;
        app.begin_run_edit(RunEditTarget::Mem { addr, width: MemWidth::B1 });
        app.run.run_edit_buf = "1ff".to_string(); // 0x1FF > 1 byte
        app.commit_run_edit();
        assert!(app.run.run_edit.is_some());
        assert!(app.run.run_edit_error.is_some());
    }

    #[test]
    fn commit_writes_memory_then_stepback_restores() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        app.run.mem_view_bytes = 4;
        let addr = app.run.data_base & !3;
        let before = app.run.mem().effective_read32(addr).unwrap_or(0);

        app.begin_run_edit(RunEditTarget::Mem { addr, width: MemWidth::B4 });
        app.run.run_edit_buf = "cafebabe".to_string();
        app.commit_run_edit();
        assert_eq!(app.run.mem().effective_read32(addr).unwrap(), 0xcafe_babe);

        app.stepback_one();
        assert_eq!(app.run.mem().effective_read32(addr).unwrap_or(0), before);
    }

    #[test]
    fn commit_writes_register_in_binary() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Bin;
        app.begin_run_edit(x(7));
        app.run.run_edit_buf = "0b1010_1010".to_string();
        app.commit_run_edit();
        assert!(app.run.run_edit.is_none());
        assert_eq!(app.run.cpu().x[7], 0b1010_1010);
    }

    #[test]
    fn commit_writes_float_register() {
        let mut app = loaded_app();
        app.run.show_float_regs = true;
        app.begin_run_edit(RunEditTarget::FReg(FRegId::new(3).unwrap()));
        app.run.run_edit_buf = "1.5".to_string();
        app.commit_run_edit();
        assert_eq!(f32::from_bits(app.run.cpu().f[3]), 1.5);
    }

    #[test]
    fn register_edit_is_undone_by_stepback() {
        let mut app = loaded_app();
        app.run.fmt_mode = FormatMode::Hex;
        let before = app.run.cpu().x[6];
        app.begin_run_edit(x(6));
        app.run.run_edit_buf = "abc".to_string();
        app.commit_run_edit();
        assert_eq!(app.run.cpu().x[6], 0xabc);
        app.stepback_one();
        assert_eq!(app.run.cpu().x[6], before);
    }

    #[test]
    fn begin_run_edit_is_noop_while_running() {
        let mut app = loaded_app();
        app.run.is_running = true;
        app.begin_run_edit(x(5));
        assert!(app.run.run_edit.is_none());
    }
}
