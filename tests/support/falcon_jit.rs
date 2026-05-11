use crate::falcon::asm::assemble;
use crate::falcon::errors::FalconError;
use crate::falcon::jit::{
    BackendKind, ExecCtx, ExecOutcome, ExecutionBackend, InterpreterBackend, make_backend,
};
use crate::falcon::program::load_words;
use crate::falcon::{Cpu, Ram};
use crate::ui::console::Console;

#[test]
fn factory_returns_interpreter_for_none() {
    let backend = make_backend(BackendKind::None).expect("interpreter is always available");
    assert_eq!(backend.kind(), BackendKind::None);
}

#[test]
fn factory_rejects_hot_mode_as_unsupported() {
    match make_backend(BackendKind::Hot) {
        Ok(_) => panic!("Hot mode should be unimplemented in Phase A"),
        Err(FalconError::Unsupported(_)) => {}
        Err(other) => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn factory_rejects_full_mode_as_unsupported() {
    match make_backend(BackendKind::Full) {
        Ok(_) => panic!("Full mode should be unimplemented in Phase A"),
        Err(FalconError::Unsupported(_)) => {}
        Err(other) => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn backend_kind_as_str_round_trips_cli_values() {
    assert_eq!(BackendKind::None.as_str(), "none");
    assert_eq!(BackendKind::Hot.as_str(), "hot");
    assert_eq!(BackendKind::Full.as_str(), "full");
}

#[test]
fn interpreter_run_until_yield_matches_exec_step() {
    // Two CPUs running the same arithmetic program: one via the raw
    // `exec::step` entry point, the other via `InterpreterBackend`. Final
    // register state must be byte-identical.
    let asm = r#"
.text
    addi x5, x0, 7
    addi x6, x0, 35
    add  x7, x5, x6
    sub  x8, x6, x5
    halt
"#;
    let prog = assemble(asm, 0).expect("assemble small arithmetic program");

    let mut cpu_raw = Cpu::default();
    let mut mem_raw = Ram::new(4096);
    let mut con_raw = Console::default();
    load_words(&mut mem_raw, 0, &prog.text).expect("load text raw");
    cpu_raw.pc = 0;
    cpu_raw.write(2, 4096);

    let mut cpu_be = Cpu::default();
    let mut mem_be = Ram::new(4096);
    let mut con_be = Console::default();
    load_words(&mut mem_be, 0, &prog.text).expect("load text backend");
    cpu_be.pc = 0;
    cpu_be.write(2, 4096);

    let mut backend = InterpreterBackend::new();

    for _ in 0..64 {
        let raw_alive = crate::falcon::exec::step(&mut cpu_raw, &mut mem_raw, &mut con_raw)
            .expect("raw step");

        let mut ctx = ExecCtx::new(&mut cpu_be, &mut mem_be, &mut con_be);
        let outcome = backend.run_until_yield(&mut ctx).expect("backend step");
        let be_alive = matches!(outcome, ExecOutcome::Stepped { .. });

        assert_eq!(raw_alive, be_alive, "alive flags must agree per step");
        assert_eq!(cpu_raw.pc, cpu_be.pc, "pc must agree per step");
        assert_eq!(cpu_raw.x, cpu_be.x, "x regs must agree per step");
        assert_eq!(
            cpu_raw.instr_count, cpu_be.instr_count,
            "instr_count must agree per step"
        );

        if !raw_alive {
            break;
        }
    }

    assert!(
        cpu_raw.local_exit && cpu_be.local_exit,
        "both must halt (Halt sets local_exit)"
    );
    assert_eq!(cpu_raw.read(7), 42, "x7 = 7 + 35 = 42");
    assert_eq!(cpu_raw.read(8), 28, "x8 = 35 - 7 = 28");
}

#[test]
fn hot_profile_records_branch_targets() {
    // Small countdown loop: x5 = 5; loop: x5 -= 1; bnez x5, loop; halt.
    // Loop body executes 5 iterations; the loop head (target of the taken
    // branch) is hit 4 times by the branch, plus the initial fall-through
    // (not a "taken" transfer → not recorded). So the count at loop_head_pc
    // should be 4 — once per taken backward branch.
    let asm = r#"
.text
_start:
    addi x5, x0, 5
loop:
    addi x5, x5, -1
    bnez x5, loop
    halt
"#;
    let prog = assemble(asm, 0).expect("assemble loop");

    let mut cpu = Cpu::default();
    let mut mem = Ram::new(4096);
    let mut con = Console::default();
    load_words(&mut mem, 0, &prog.text).expect("load loop");
    cpu.pc = 0;
    cpu.write(2, 4096);

    // `loop:` label is the second instruction (offset 4).
    let loop_head_pc = 4u32;

    let mut backend = InterpreterBackend::new();
    for _ in 0..200 {
        let mut ctx = ExecCtx::new(&mut cpu, &mut mem, &mut con);
        let outcome = backend.run_until_yield(&mut ctx).expect("step");
        if matches!(outcome, ExecOutcome::Halted | ExecOutcome::AwaitingInput) {
            break;
        }
    }

    let profile = backend.profile();
    let head_count = profile.get(loop_head_pc);
    assert_eq!(
        head_count, 4,
        "loop head should record one entry per taken backward branch"
    );
    // The halt at offset 12 is reached by fall-through (pc + 4), so it should
    // not appear as a branch target.
    assert_eq!(
        profile.get(12),
        0,
        "fall-through targets must not be recorded"
    );
}
