use super::{
    HeadlessHart, PipelineReport, format_csv, format_fstats, format_json,
    parse_expect_mem_spec, parse_expect_reg_spec, run_headless_multihart_sequential,
    run_headless_sequential, service_pending_hart_start, validate_expectations,
};
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

    run_headless_sequential(&mut cpu, &mut mem, &mut console, 32, &mut stdout, 1)
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

    run_headless_multihart_sequential(&mut cpu, &mut mem, &mut console, 128, &mut stdout, 2)
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

    run_headless_multihart_sequential(&mut cpu, &mut mem, &mut console, 128, &mut stdout, 2)
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

    let err = run_headless_sequential(&mut cpu, &mut mem, &mut console, 8, &mut stdout, 1)
        .expect_err("invalid instruction should fault");
    assert!(err.contains("fault at PC"));
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

    let json = format_json(&mem, "demo.fas", Some(0), Some(pipeline));
    assert!(json.contains("\"clock_model\": \"pipeline\""));
    assert!(json.contains("\"total_cycles\": 11"));
    assert!(!json.contains("\"base_cycles\""));
    assert!(!json.contains("\"cache_cycles\""));

    let fstats = format_fstats(&mem, "demo.fas", Some(0), Some(pipeline));
    assert!(fstats.starts_with("# FALCON-ASM Simulation Results v2\n"));
    assert!(fstats.contains("prog.clock_model=pipeline\n"));
    assert!(fstats.contains("prog.total_cycles=11\n"));
    assert!(!fstats.contains("prog.base_cycles="));
    assert!(!fstats.contains("prog.cache_cycles="));

    let csv = format_csv(&mem, "demo.fas", Some(pipeline));
    assert!(csv.contains("Clock Model,Instructions,Total Cycles,CPI,IPC\n"));
    assert!(csv.contains("pipeline,3,11,"));
    assert!(!csv.contains("Base Cycles"));
    assert!(!csv.contains("Cache Cycles"));
}
