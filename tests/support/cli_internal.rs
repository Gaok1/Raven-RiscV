use super::{
    HeadlessHart, PipelineReport, SchedReport, format_csv, format_json, format_rstats, load_falc,
    parse_expect_mem_spec, parse_expect_reg_spec, run_headless_multihart_sequential,
    run_headless_pipeline, run_headless_scheduled, run_headless_sequential,
    service_pending_hart_start, validate_expectations,
};
use crate::falcon::sched::{SchedConfig, SchedPolicy, ThreadState};
use crate::falcon::asm::assemble;
use crate::falcon::cache::{CacheConfig, CacheController};
use crate::falcon::memory::Bus;
use crate::falcon::program::load_words;
use crate::falcon::{Cpu, registers::HartStartRequest};
use crate::ui::console::Console;

#[test]
fn expect_reg_spec_supports_alias_and_hex() {
    let (reg, value) = parse_expect_reg_spec("a0=0x2a").expect("spec should parse");
    assert_eq!(reg, 10);
    assert_eq!(value, 42);
}

#[test]
fn expect_reg_spec_accepts_numbered_form_within_the_register_file() {
    assert_eq!(parse_expect_reg_spec("r2=1").expect("r2 is valid").0, 2);
    // Out-of-range indices must be rejected here: the index later addresses a
    // 32-entry array, so letting one through is a panic, not a wrong answer.
    assert!(parse_expect_reg_spec("r99=1").is_err());
    assert!(parse_expect_reg_spec("r32=1").is_err());
}

/// `raven build` emits FALC v2, so the v2 loader has to set up the same guest
/// state the v1 loader did — including the program break that `brk`/`sbrk`
/// hand out.
#[test]
fn falc_v2_load_sets_the_program_break_past_the_image() {
    let source = ".data\nbuf: .space 64\n.text\n    li a0, 1\n    halt\n";
    let image = crate::riscv32::architecture()
        .assembler()
        .assemble(source, 0)
        .expect("program should assemble");
    let bytes = image.to_falc_v2().expect("image should serialize");

    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        64 * 1024,
    );
    load_falc(&bytes, &mut cpu, &mut mem, 64 * 1024).expect("v2 image should load");

    let end = image.end_address() as u32;
    assert!(end > 0, "the fixture must actually occupy memory");
    assert!(cpu.heap_break >= end, "heap must start past the image");
    assert_eq!(cpu.heap_break % 16, 0, "heap break must be 16-byte aligned");
}

#[test]
fn expect_mem_spec_supports_hex_pairs() {
    let (addr, value) = parse_expect_mem_spec("0x1000=0xDEADBEEF").expect("spec should parse");
    assert_eq!(addr, 0x1000);
    assert_eq!(value, 0xDEADBEEF);
}

#[test]
fn expect_mem_reads_cache_aware_value() {
    let cpu = Cpu::default();
    let mut mem = CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 256);
    mem.store32(0x20, 0xDEAD_BEEF)
        .expect("store should succeed");
    assert_eq!(mem.peek32(0x20).expect("raw ram"), 0);

    validate_expectations(&cpu, &mem, &[], None, None, &[], &[(0x20, 0xDEAD_BEEF)])
        .expect("expect-mem should use cache-aware reads");
}

#[test]
fn hart_start_rejects_misaligned_and_oob_stack() {
    let mem_size = 16 * 1024 * 1024usize;
    let max_cores = 4usize;
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        mem_size,
    );
    mem.ram.store32(0, 0x0010_0073).expect("store text");

    let make_harts = |stack_ptr: u32| {
        let mut parent = Cpu::default();
        parent.heap_break = 0x00FF_F000;
        parent.pending_hart_start = Some(HartStartRequest {
            entry_pc: 0,
            stack_ptr,
            arg: 0,
        });
        vec![
            HeadlessHart {
                hart_id: 0,
                cpu: parent,
                active: true,
                paused: false,
            },
            HeadlessHart {
                hart_id: 1,
                cpu: Cpu::default(),
                active: false,
                paused: false,
            },
            HeadlessHart {
                hart_id: 2,
                cpu: Cpu::default(),
                active: false,
                paused: false,
            },
            HeadlessHart {
                hart_id: 3,
                cpu: Cpu::default(),
                active: false,
                paused: false,
            },
        ]
    };

    let mut harts = make_harts(0x00FF_FFF1);
    service_pending_hart_start(&mut harts, 0, &mem, max_cores, mem_size)
        .expect("service should succeed");
    assert_eq!(harts[0].cpu.read(10) as i32, -3);
    assert!(!harts[1].active);

    let mut harts = make_harts(0);
    service_pending_hart_start(&mut harts, 0, &mem, max_cores, mem_size)
        .expect("service should succeed");
    assert_eq!(harts[0].cpu.read(10) as i32, -3);

    let mut harts = make_harts(mem_size as u32 + 16);
    service_pending_hart_start(&mut harts, 0, &mem, max_cores, mem_size)
        .expect("service should succeed");
    assert_eq!(harts[0].cpu.read(10) as i32, -3);

    let mut harts = make_harts(0x00FF_FFF0);
    service_pending_hart_start(&mut harts, 0, &mem, max_cores, mem_size)
        .expect("service should succeed");
    assert!(harts[0].cpu.read(10) as i32 >= 0);
    assert!(harts[1].active);
}

#[test]
fn headless_single_halt_is_treated_as_clean_exit() {
    let mut cpu = Cpu::default();
    let mut mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
    let mut console = Console::default();
    let mut stdout = Vec::new();
    let prog = assemble(".text\nhalt", 0).expect("assemble halt");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.pc = 0;
    cpu.write(2, 4096);

    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_sequential(
        &mut cpu,
        &mut mem,
        &mut console,
        32,
        &mut stdout,
        1,
        backend.as_mut(),
    )
    .expect("headless run");

    assert_eq!(cpu.exit_code, Some(0));
}

#[test]
fn headless_multihart_halt_completion_is_treated_as_clean_exit() {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        16 * 1024 * 1024,
    );
    let mut console = Console::default();
    let mut stdout = Vec::new();
    let asm = r#"
.text
.globl _start
_start:
    li   t5, -4096
    add  t5, sp, t5
    la   a0, worker
    li   t6, 0x00010000
    sub  a1, t5, t6
    li   a2, 1
    li   a7, 1100
    ecall
    halt
worker:
    halt
"#;
    let prog = assemble(asm, 0).expect("assemble multi-hart halt");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.pc = 0;
    cpu.write(2, (16 * 1024 * 1024) as u32);
    cpu.heap_break = prog.data_base;

    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_multihart_sequential(
        &mut cpu,
        &mut mem,
        &mut console,
        128,
        &mut stdout,
        2,
        backend.as_mut(),
    )
    .expect("headless multi-hart run");

    assert_eq!(cpu.exit_code, Some(0));
}

#[test]
fn headless_hart_start_uses_memory_size_not_parent_sp_for_stack_validation() {
    let mem_size = 16 * 1024 * 1024usize;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        mem_size,
    );
    let mut console = Console::default();
    let mut stdout = Vec::new();
    let asm = r#"
.text
.globl _start
_start:
    la   a0, worker
    li   a1, 0x00C00000
    li   a2, 1
    li   a7, 1100
    ecall
    halt
worker:
    halt
"#;
    let prog = assemble(asm, 0).expect("assemble multi-hart spawn");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.pc = 0;
    cpu.write(2, 0x0080_0000);
    cpu.heap_break = prog.data_base;

    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_multihart_sequential(
        &mut cpu,
        &mut mem,
        &mut console,
        128,
        &mut stdout,
        2,
        backend.as_mut(),
    )
    .expect("headless multi-hart run");

    assert_eq!(cpu.exit_code, Some(0));
}

#[test]
fn headless_invalid_instruction_is_reported_as_fault() {
    let mut cpu = Cpu::default();
    let mut mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
    let mut console = Console::default();
    let mut stdout = Vec::new();
    mem.ram.store32(0, 0xFFFF_FFFF).expect("store invalid");
    cpu.pc = 0;
    cpu.write(2, 4096);

    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    let err = run_headless_sequential(
        &mut cpu,
        &mut mem,
        &mut console,
        8,
        &mut stdout,
        1,
        backend.as_mut(),
    )
    .expect_err("invalid instruction should fault");
    assert!(err.contains("fault at PC"));
}

#[test]
fn headless_trap_without_a_handler_names_the_same_pc_in_both_modes() {
    // A reserved rounding mode with no mtvec: the trap is fatal, its reason is
    // on the console for `run_headless` to print, and both runs stop on the
    // illegal instruction rather than on the one after it.
    let prog = assemble(
        ".text\n    li t0, 0x3F800000\n    fmv.w.x f1, t0\n    csrrwi zero, frm, 6\nbad:\n    fadd.s f10, f1, f1, dyn\n    halt",
        0,
    )
    .expect("assemble");
    let bad = prog
        .labels
        .iter()
        .find(|(_, names)| names.iter().any(|name| name == "bad"))
        .map(|(addr, _)| *addr)
        .expect("label bad");
    for pipeline in [false, true] {
        let mut cpu = Cpu::default();
        let mut mem =
            CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
        let mut console = Console::default();
        load_words(&mut mem.ram, 0, &prog.text).expect("load text");
        cpu.pc = 0;
        cpu.write(2, 4096);

        let outcome = if pipeline {
            run_headless_pipeline(
                &mut cpu,
                &mut mem,
                &mut console,
                &crate::ui::CpiConfig::default(),
                &crate::ui::pipeline::PipelineConfig::default(),
                4096,
                false,
            )
            .0
            .map(|_| ())
        } else {
            let mut stdout = Vec::new();
            let mut backend =
                crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None)
                    .expect("backend");
            run_headless_sequential(
                &mut cpu,
                &mut mem,
                &mut console,
                64,
                &mut stdout,
                1,
                backend.as_mut(),
            )
        };
        let err = outcome.expect_err("the trap should fault");
        assert!(
            err.contains(&format!("fault at PC=0x{bad:08X}")),
            "pipeline={pipeline}: {err}"
        );
        assert!(
            console
                .lines
                .iter()
                .any(|line| line.is_error()
                    && line.text.contains("trap (no mtvec handler): cause=2")),
            "pipeline={pipeline}: the trap was not reported"
        );
    }
}

#[test]
fn pipeline_output_formats_do_not_emit_serial_cycle_breakdown() {
    let mut mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
    mem.instruction_count = 77;
    mem.extra_cycles = 55;
    let pipeline = PipelineReport {
        enabled: true,
        committed: 3,
        cycles: 11,
        stalls: 4,
        flushes: 1,
        cpi: 11.0 / 3.0,
    };

    let json = format_json(&mem, "demo.s", Some(0), Some(pipeline), None);
    assert!(json.contains("\"clock_model\": \"pipeline\""));
    assert!(json.contains("\"total_cycles\": 11"));
    assert!(!json.contains("\"base_cycles\""));
    assert!(!json.contains("\"cache_cycles\""));

    let rstats = format_rstats(&mem, "demo.s", Some(0), Some(pipeline), None);
    assert!(rstats.starts_with("# Raven Results v1\n"));
    assert!(rstats.contains("prog.clock_model=pipeline\n"));
    assert!(rstats.contains("prog.total_cycles=11\n"));
    assert!(!rstats.contains("prog.base_cycles="));
    assert!(!rstats.contains("prog.cache_cycles="));

    let csv = format_csv(&mem, "demo.s", Some(pipeline), None);
    assert!(csv.contains("Clock Model,Instructions,Total Cycles,CPI,IPC\n"));
    assert!(csv.contains("pipeline,3,11,"));
    assert!(!csv.contains("Base Cycles"));
    assert!(!csv.contains("Cache Cycles"));
}

/// Core 0 writes a value and raises a flag; core 1 waits for the flag, copies
/// the value and says it is done; core 0 exits with the copy. Every word sits
/// on its own cache line.
const FLAG_HANDOFF: &str = r#"
.text
.globl _start
_start:
    li   t5, -4096
    add  a1, sp, t5
    la   a0, consumer
    li   a2, 0
    li   a7, 1100
    ecall
    li   t0, 0x8000
    li   t1, 42
    sw   t1, 0(t0)
    li   t1, 1
    sw   t1, 16(t0)
wait:
    lw   t1, 48(t0)
    beqz t1, wait
    lw   a0, 32(t0)
    li   a7, 93
    ecall
consumer:
    li   t0, 0x8000
spin:
    lw   t1, 16(t0)
    beqz t1, spin
    lw   t1, 0(t0)
    sw   t1, 32(t0)
    li   t1, 1
    sw   t1, 48(t0)
    li   a7, 1101
    ecall
"#;

fn run_flag_handoff(protocol: crate::falcon::cache::CoherenceProtocol) -> Cpu {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        16 * 1024 * 1024,
    );
    mem.set_coherence(crate::falcon::cache::CoherenceConfig {
        protocol,
        ..Default::default()
    });
    mem.set_core_count(2);
    let mut console = Console::default();
    let mut stdout = Vec::new();
    let prog = assemble(FLAG_HANDOFF, 0).expect("assemble flag handoff");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.pc = 0;
    cpu.write(2, (16 * 1024 * 1024) as u32);
    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_multihart_sequential(
        &mut cpu,
        &mut mem,
        &mut console,
        20_000,
        &mut stdout,
        2,
        backend.as_mut(),
    )
    .expect("headless multi-hart run");
    cpu
}

#[test]
fn a_flag_handoff_between_cores_works_under_every_coherent_protocol() {
    use crate::falcon::cache::CoherenceProtocol::*;
    for protocol in [Shared, Msi, Mesi, Moesi] {
        let cpu = run_flag_handoff(protocol);
        assert_eq!(cpu.exit_code, Some(42), "{protocol:?}");
    }
}

#[test]
fn without_coherence_the_waiting_core_never_sees_the_flag() {
    // Core 1 read the flag before core 0 raised it and keeps its own copy;
    // core 0 waits on core 1 in turn, so the run reaches its cycle limit.
    let cpu = run_flag_handoff(crate::falcon::cache::CoherenceProtocol::None);
    assert_eq!(cpu.exit_code, None);
}

/// Main starts two workers, A and B, and joins both; each worker prints its
/// letter after each of three rounds of busy work. Main exits with 7.
const TWO_WORKERS: &str = r#"
.text
_start:
    li   t5, -8192
    add  a1, sp, t5
    la   a0, worker
    li   a2, 65
    li   a7, 1100
    ecall
    mv   s0, a0
    li   t5, -16384
    add  a1, sp, t5
    la   a0, worker
    li   a2, 66
    li   a7, 1100
    ecall
    mv   s1, a0
    mv   a0, s0
    li   a7, 1104
    ecall
    mv   a0, s1
    li   a7, 1104
    ecall
    li   a0, 7
    li   a7, 93
    ecall
worker:
    mv   s1, a0
    li   s2, 3
round:
    li   t0, 10
busy:
    addi t0, t0, -1
    bnez t0, busy
    mv   a0, s1
    li   a7, 1006
    ecall
    addi s2, s2, -1
    bnez s2, round
    li   a7, 1101
    ecall
"#;

/// Run `asm` under `policy` on `cores` cores. Returns the final CPU, what the
/// program printed, and the scheduler's report or the error.
fn run_scheduled(
    asm: &str,
    policy: SchedPolicy,
    quantum: u32,
    cores: usize,
) -> (Cpu, String, Result<SchedReport, String>) {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        16 * 1024 * 1024,
    );
    mem.set_core_count(cores);
    let mut console = Console::default();
    let mut stdout = Vec::new();
    let prog = assemble(asm, 0).expect("assemble");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.pc = 0;
    cpu.write(2, (16 * 1024 * 1024) as u32);
    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    let cfg = SchedConfig {
        policy,
        quantum,
        switch_cost: 0,
    };
    let result = run_headless_scheduled(
        &mut cpu,
        &mut mem,
        &mut console,
        100_000,
        &mut stdout,
        cores,
        cfg,
        backend.as_mut(),
    )
    .map(|sched| SchedReport::from_scheduler(&sched));
    (cpu, String::from_utf8_lossy(&stdout).into_owned(), result)
}

#[test]
fn fifo_runs_each_worker_to_completion_on_one_core() {
    let (cpu, out, report) = run_scheduled(TWO_WORKERS, SchedPolicy::Fifo, 5, 1);
    let report = report.expect("scheduled run");
    assert_eq!(out, "AAABBB");
    assert_eq!(cpu.exit_code, Some(7));
    assert_eq!(report.summary.preemptions, 0);
    assert_eq!(report.threads.len(), 3);
    assert!(report.threads.iter().all(|t| t.state == ThreadState::Exited));
}

#[test]
fn round_robin_interleaves_the_workers() {
    let (cpu, out, report) = run_scheduled(TWO_WORKERS, SchedPolicy::RoundRobin, 5, 1);
    let report = report.expect("scheduled run");
    assert_eq!(out, "ABABAB");
    assert_eq!(cpu.exit_code, Some(7));
    assert!(report.summary.preemptions > 0);
}

#[test]
fn a_more_urgent_worker_runs_first() {
    // Main gives B priority 1 right after starting it: B takes the core from
    // main at once, then A and main share by round robin.
    let asm = TWO_WORKERS.replace(
        "    mv   s1, a0\n    mv   a0, s0",
        "    mv   s1, a0\n    li   a1, 1\n    li   a7, 1105\n    ecall\n    mv   a0, s0",
    );
    let (cpu, out, report) = run_scheduled(&asm, SchedPolicy::Priority, 1000, 1);
    report.expect("scheduled run");
    assert_eq!(out, "BBBAAA");
    assert_eq!(cpu.exit_code, Some(7));
}

#[test]
fn every_thread_gets_a_core_when_there_are_enough() {
    let (cpu, out, report) = run_scheduled(TWO_WORKERS, SchedPolicy::RoundRobin, 5, 3);
    let report = report.expect("scheduled run");
    assert_eq!(out, "ABABAB");
    assert_eq!(cpu.exit_code, Some(7));
    assert_eq!(report.summary.preemptions, 0, "no one ever waits for a core");
}

#[test]
fn threads_joined_on_each_other_are_reported_as_deadlocked() {
    let asm = r#"
.text
_start:
    li   t5, -8192
    add  a1, sp, t5
    la   a0, worker
    li   a2, 0
    li   a7, 1100
    ecall
    li   a7, 1104          # join the worker
    ecall
    li   a7, 93
    ecall
worker:
    li   a0, 0
    li   a7, 1104          # join main
    ecall
    li   a7, 1101
    ecall
"#;
    let (_, _, report) = run_scheduled(asm, SchedPolicy::RoundRobin, 5, 1);
    let err = report.err().expect("a deadlock");
    assert!(err.contains("deadlock"), "{err}");
    assert!(err.contains("thread 0 joins 1"), "{err}");
}

#[test]
fn joining_itself_or_no_one_returns_minus_one() {
    let asm = ".text\n_start:\n    li a0, 0\n    li a7, 1104\n    ecall\n    li a7, 93\n    ecall\n";
    let (cpu, _, report) = run_scheduled(asm, SchedPolicy::Fifo, 5, 1);
    report.expect("scheduled run");
    assert_eq!(cpu.exit_code, Some(u32::MAX));

    // The same without a scheduler.
    let mut cpu = Cpu::default();
    let mut mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);
    let prog = assemble(asm, 0).expect("assemble");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.write(2, 4096);
    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_sequential(
        &mut cpu,
        &mut mem,
        &mut Console::default(),
        64,
        &mut Vec::new(),
        1,
        backend.as_mut(),
    )
    .expect("headless run");
    assert_eq!(cpu.exit_code, Some(u32::MAX));
}

#[test]
fn without_a_scheduler_join_waits_on_its_own_core() {
    // Each hart has a core; main's join keeps it off its core until both
    // workers are done, then main exits with 7.
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        16 * 1024 * 1024,
    );
    mem.set_core_count(3);
    let mut stdout = Vec::new();
    let prog = assemble(TWO_WORKERS, 0).expect("assemble");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    cpu.write(2, (16 * 1024 * 1024) as u32);
    let mut backend =
        crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None).expect("backend");
    run_headless_multihart_sequential(
        &mut cpu,
        &mut mem,
        &mut Console::default(),
        100_000,
        &mut stdout,
        3,
        backend.as_mut(),
    )
    .expect("headless multi-hart run");
    assert_eq!(cpu.exit_code, Some(7));
    assert_eq!(String::from_utf8_lossy(&stdout), "ABABAB");
}

#[test]
fn the_scheduler_report_reaches_every_output_format() {
    let (_, _, report) = run_scheduled(TWO_WORKERS, SchedPolicy::RoundRobin, 5, 1);
    let report = report.expect("scheduled run");
    let mem =
        CacheController::new(CacheConfig::default(), CacheConfig::default(), vec![], 4096);

    let json = format_json(&mem, "t.s", Some(7), None, Some(&report));
    assert!(json.contains("\"scheduler\": {"));
    assert!(json.contains("\"policy\": \"rr\""));
    assert!(json.contains("\"tid\": 2, \"parent\": 0"));

    let rstats = format_rstats(&mem, "t.s", Some(7), None, Some(&report));
    assert!(rstats.contains("\n[sched]\nsched.policy=rr\n"));
    assert!(rstats.contains("thread1.turnaround="));

    let csv = format_csv(&mem, "t.s", None, Some(&report));
    assert!(csv.contains("SCHEDULER\n"));
    assert!(csv.contains("\nTHREADS\nTID,Parent,State,"));
    assert!(csv.contains("\n2,0,EXIT,10,"));

    let plain = format_json(&mem, "t.s", Some(7), None, None);
    assert!(!plain.contains("scheduler"));
}
