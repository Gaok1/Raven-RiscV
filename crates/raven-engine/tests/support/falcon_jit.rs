use crate::falcon::asm::assemble;
use crate::falcon::cache::{CacheConfig, CacheController};
use crate::falcon::errors::FalconError;
use crate::falcon::jit::{
    BackendKind, BlockTerminator, ExecCtx, ExecOutcome, ExecutionBackend, InterpreterBackend,
    make_backend, scan_block,
};
use crate::falcon::memory::Bus;
use crate::falcon::program::load_words;
use crate::falcon::{Cpu, Ram};
use crate::ui::console::Console;

// ---------------------------------------------------------------------------
// Helpers compartilhados pelos testes de scan_block
// ---------------------------------------------------------------------------

/// Cria um `CacheController` com caches default e carrega `words` na RAM
/// diretamente (via `mem.ram`, não via Bus), garantindo que `peek32` — que
/// lê da RAM bruta — veja os dados imediatamente.
fn make_cc(words: &[u32]) -> CacheController {
    let mut mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
    load_words(&mut mem.ram, 0, words).expect("load words into RAM");
    mem
}

/// Monta o trecho `asm` e retorna as palavras encodadas.
fn asm_words(asm: &str) -> Vec<u32> {
    assemble(asm, 0).expect("assemble").text
}

#[test]
fn factory_returns_interpreter_for_none() {
    let backend = make_backend(BackendKind::None).expect("interpreter is always available");
    assert_eq!(backend.kind(), BackendKind::None);
}

#[test]
#[cfg(not(feature = "jit"))]
fn factory_rejects_hot_mode_as_unsupported() {
    match make_backend(BackendKind::Hot) {
        Ok(_) => panic!("Hot mode should be unsupported without the jit feature"),
        Err(FalconError::Unsupported(_)) => {}
        Err(other) => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
#[cfg(feature = "jit")]
fn factory_returns_hot_backend() {
    assert!(make_backend(BackendKind::Hot).is_ok());
}

#[test]
fn factory_rejects_full_mode_via_make_backend() {
    // Full mode requires cpu+mem for eager scan; make_backend always returns Unsupported.
    // Use make_full_backend(cpu, mem) to construct it.
    match make_backend(BackendKind::Full) {
        Ok(_) => panic!("make_backend should not construct Full; use make_full_backend"),
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
        let raw_alive =
            crate::falcon::exec::step(&mut cpu_raw, &mut mem_raw, &mut con_raw).expect("raw step");

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

// ---------------------------------------------------------------------------
// Testes de scan_block
// ---------------------------------------------------------------------------

#[test]
fn scan_block_alu_then_halt() {
    // addi x5, x0, 1  /  add x6, x5, x5  /  halt
    // O scan deve incluir as duas instruções ALU e parar no halt.
    let words = asm_words(".text\n addi x5, x0, 1\n add x6, x5, x5\n halt");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.start_pc, 0);
    assert_eq!(block.terminator, BlockTerminator::Halt);
    assert_eq!(block.words.len(), 3, "2 ALU + 1 halt");
    assert_eq!(block.end_pc, 8, "halt está em PC=8");
}

#[test]
fn scan_block_single_halt() {
    // Bloco de uma única instrução: o terminador é também a primeira instrução.
    // Invariante: start_pc == end_pc.
    let words = asm_words(".text\n halt");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::Halt);
    assert_eq!(block.words.len(), 1);
    assert_eq!(block.start_pc, block.end_pc);
}

#[test]
fn scan_block_ends_on_branch() {
    // addi x5, x5, -1  /  bnez x5, -4 (backward branch)
    // O scan deve incluir o addi e parar no bnez.
    let words = asm_words(".text\nloop:\n addi x5, x5, -1\n bnez x5, loop");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::Branch);
    assert_eq!(block.words.len(), 2, "addi + bnez");
    assert_eq!(block.end_pc, 4, "bnez está em PC=4");
}

#[test]
fn scan_block_ends_on_jal() {
    // addi x1, x0, 0  /  jal x0, 0  (salto no lugar)
    let words = asm_words(".text\n addi x1, x0, 0\n jal x0, 0");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::Jal);
    assert_eq!(block.words.len(), 2);
    assert_eq!(block.end_pc, 4);
}

#[test]
fn scan_block_ends_on_jalr() {
    // jalr x0, x1, 0  — típico de `ret` em RISC-V (rd=0, rs1=ra)
    let words = asm_words(".text\n jalr x0, x1, 0");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::Jalr);
    assert_eq!(block.words.len(), 1);
    assert_eq!(block.start_pc, block.end_pc);
}

#[test]
fn scan_block_fence_terminates() {
    // addi x1, x0, 1  /  fence  — fence encerra o bloco
    let words = asm_words(".text\n addi x1, x0, 1\n fence");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::Fence);
    assert_eq!(block.words.len(), 2);
    assert_eq!(block.end_pc, 4);
}

#[test]
fn scan_block_cap_at_64() {
    // 70 instruções `addi x0, x0, 0` (nop) sem terminador explícito.
    // O scan deve parar na 64ª e retornar FallThrough.
    let nop = asm_words(".text\n addi x0, x0, 0")[0];
    let words: Vec<u32> = std::iter::repeat(nop).take(70).collect();
    let mem = make_cc(&words);
    let block = scan_block(&mem, 0);

    assert_eq!(block.terminator, BlockTerminator::FallThrough);
    assert_eq!(block.words.len(), 64);
    assert_eq!(block.end_pc, 63 * 4, "PC da 64ª instrução (índice 63)");
}

#[test]
fn scan_block_start_pc_respected() {
    // Carrega 3 instruções a partir do byte 0, mas escaneia a partir de PC=4
    // (segunda instrução). O bloco resultante deve ter start_pc=4 e end_pc=8.
    let words = asm_words(".text\n addi x1, x0, 1\n addi x2, x0, 2\n halt");
    let mem = make_cc(&words);
    let block = scan_block(&mem, 4); // começa na segunda instrução

    assert_eq!(block.start_pc, 4);
    assert_eq!(block.end_pc, 8, "halt está em PC=8");
    assert_eq!(block.terminator, BlockTerminator::Halt);
    assert_eq!(block.words.len(), 2, "addi x2 + halt");
}

// ---------------------------------------------------------------------------
// tohost at the end of a compiled block
// ---------------------------------------------------------------------------

/// A bus with `tohost` at 0x100 and a store to it already made, the way a
/// compiled block's `sw` leaves it when the block returns to the dispatcher.
fn bus_after_tohost_store(value: u32) -> CacheController {
    let mut mem = make_cc(&[]);
    mem.set_tohost(Some(0x100));
    mem.store32(0x100, value).expect("store to tohost");
    mem
}

#[test]
fn a_tohost_exit_in_a_compiled_block_wins_however_the_block_ended() {
    // The block stored exit(3) to `tohost` and went on: to its end, to a
    // `halt` or exit `ecall`, to a blocking read, or to a fault. Only the
    // first was answered; the request was dropped on a halt or a read, and
    // on a fault left on the bus for whichever hart stepped next.
    use crate::falcon::jit::backend::answer_host_request;
    let outcomes = [
        Ok(ExecOutcome::Stepped { instructions: 4 }),
        Ok(ExecOutcome::Halted),
        Ok(ExecOutcome::AwaitingInput),
        Err(FalconError::Bus("JIT fault: invalid memory access".into())),
    ];
    for outcome in outcomes {
        let ended = format!("{outcome:?}");
        let mut mem = bus_after_tohost_store(7);
        let mut cpu = Cpu::default();
        cpu.pc = 0x40;
        let mut console = Console::default();
        console.reading = matches!(outcome, Ok(ExecOutcome::AwaitingInput));

        let mut ctx = ExecCtx::new(&mut cpu, &mut mem, &mut console);
        let answered = answer_host_request(&mut ctx, outcome);

        assert!(
            matches!(answered, Ok(ExecOutcome::Halted)),
            "{ended}: {answered:?}"
        );
        assert_eq!(cpu.exit_code, Some(3), "{ended}");
        assert_eq!(cpu.pc, 0x40, "{ended}: where the block left it");
        assert!(!console.reading, "{ended}: still waiting for input");
        assert!(mem.take_host_request().is_none(), "{ended}: left on the bus");
    }
}

#[test]
fn an_unsupported_tohost_request_in_a_compiled_block_leaves_its_outcome_alone() {
    use crate::falcon::jit::backend::answer_host_request;
    let outcomes = [
        Ok(ExecOutcome::Stepped { instructions: 4 }),
        Err(FalconError::Bus("JIT fault: invalid memory access".into())),
    ];
    for outcome in outcomes {
        let ended = format!("{outcome:?}");
        let mut mem = bus_after_tohost_store(0x200);
        let mut cpu = Cpu::default();
        let mut console = Console::default();

        let mut ctx = ExecCtx::new(&mut cpu, &mut mem, &mut console);
        let answered = answer_host_request(&mut ctx, outcome);

        assert_eq!(format!("{answered:?}"), ended);
        assert_eq!(cpu.exit_code, None, "{ended}");
        assert!(mem.take_host_request().is_none(), "{ended}: left on the bus");
        let reports = console
            .lines
            .iter()
            .filter(|line| line.text.contains("not supported"))
            .count();
        assert_eq!(reports, 1, "{ended}");
    }
}
