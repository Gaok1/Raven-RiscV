use super::*;
use crate::falcon::asm::assemble;
use crate::falcon::cache::CacheConfig;
use crate::falcon::encoder::encode;
use crate::falcon::exec;
use crate::falcon::instruction::Instruction;
use crate::falcon::memory::Bus;
use crate::falcon::pipeline::FuState;
use crate::falcon::program::{load_bytes, load_words, zero_bytes};

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

fn load_program_into_mem(
    asm: &str,
    cpu: &mut Cpu,
    mem: &mut CacheController,
) -> crate::falcon::asm::Program {
    let prog = assemble(asm, 0).expect("assemble");
    load_words(&mut mem.ram, 0, &prog.text).expect("load text");
    if !prog.data.is_empty() {
        load_bytes(&mut mem.ram, prog.data_base, &prog.data).expect("load data");
    }
    let bss_base = prog.data_base.wrapping_add(prog.data.len() as u32);
    if prog.bss_size > 0 {
        zero_bytes(&mut mem.ram, bss_base, prog.bss_size).expect("zero bss");
    }
    cpu.pc = 0;
    cpu.write(2, 0x4000);
    prog
}

fn run_sequential(asm: &str) -> (crate::falcon::asm::Program, Cpu, CacheController) {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    mem.bypass = true;
    let prog = load_program_into_mem(asm, &mut cpu, &mut mem);
    for _ in 0..512 {
        match exec::step(&mut cpu, &mut mem, &mut console).expect("sequential step") {
            true => {}
            false => break,
        }
    }
    (prog, cpu, mem)
}

fn run_pipeline_prog(
    asm: &str,
) -> (
    crate::falcon::asm::Program,
    Cpu,
    CacheController,
    PipelineSimState,
) {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    mem.bypass = true;
    let prog = load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);
    for _ in 0..1024 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }
    (prog, cpu, mem, state)
}

#[test]
fn forwarding_prevents_lui_addi_store_corruption() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Lui { rd: 5, imm: 0x1000 }).unwrap(),
        encode(Instruction::Addi {
            rd: 5,
            rs1: 5,
            imm: 4,
        })
        .unwrap(),
        encode(Instruction::Addi {
            rd: 6,
            rs1: 0,
            imm: 0x123,
        })
        .unwrap(),
        encode(Instruction::Sw {
            rs2: 6,
            rs1: 5,
            imm: 0,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }
    state.reset_stages(0);

    for _ in 0..48 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert!(state.halted, "pipeline did not halt");
    assert_eq!(mem.load32(0x1004).unwrap(), 0x123);
    assert_eq!(mem.load32(4).unwrap(), program[1]);
}

#[test]
fn jalr_prediction_waits_for_real_target_when_rs1_is_produced_by_auipc() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Auipc { rd: 1, imm: 1 }).unwrap(),
        encode(Instruction::Jalr {
            rd: 1,
            rs1: 1,
            imm: -4080,
        })
        .unwrap(),
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 99,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }
    state.reset_stages(0);

    for _ in 0..24 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(!state.faulted, "pipeline faulted on AUIPC/JALR thunk");
    assert!(state.halted, "pipeline did not reach the redirected halt");
    assert_eq!(cpu.pc, 20, "jalr should resolve to the AUIPC-derived halt");
    assert_eq!(cpu.read(10), 0, "wrong-path addi must not commit");
}

#[test]
fn fp_compare_reads_both_operands() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::FeqS {
            rd: 10,
            rs1: 1,
            rs2: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(4, encode(Instruction::Halt).unwrap()).unwrap();
    cpu.fwrite_bits(1, 3.5f32.to_bits());
    cpu.fwrite_bits(2, 3.5f32.to_bits());

    state.reset_stages(0);
    for _ in 0..24 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert_eq!(cpu.read(10), 1);
}

#[test]
fn gantt_keeps_effective_ex_cell_when_fu_bank_mirrors_serial_ex() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }
    state.reset_stages(0);

    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.gantt.iter().any(|row| {
            row.disasm.starts_with("addi")
                && row
                    .cells
                    .iter()
                    .any(|cell| matches!(cell, GanttCell::InFu(FuKind::Alu)))
        }) {
            break;
        }
    }

    let addi_row = state
        .gantt
        .iter()
        .find(|row| row.disasm.starts_with("addi"))
        .expect("addi row should be recorded in gantt");

    assert!(
        addi_row
            .cells
            .iter()
            .any(|cell| matches!(cell, GanttCell::InFu(FuKind::Alu))),
        "effective EX cycle should remain visible instead of collapsing into Stall"
    );
}

#[test]
fn branch_prediction_taken_redirects_fetch() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    state.predict = super::super::BranchPredict::Taken;
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 0,
            rs2: 0,
            imm: 8,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        8,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(12, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert_eq!(cpu.read(10), 2);
}

#[test]
fn fence_and_fence_i_flow_through_pipeline_as_system_ops() {
    assert_eq!(
        crate::falcon::pipeline::classify(encode(Instruction::Fence).unwrap()),
        InstrClass::System
    );
    assert_eq!(
        crate::falcon::pipeline::classify(encode(Instruction::FenceI).unwrap()),
        InstrClass::System
    );
}

#[test]
fn parallel_fu_mode_dispatches_fence_to_sys_fu() {
    let mut state = PipelineSimState::new();
    state.mode = super::super::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut older_div = PipeSlot::from_word(
        0,
        encode(Instruction::Div {
            rd: 5,
            rs1: 10,
            rs2: 11,
        })
        .unwrap(),
    );
    older_div.seq = 1;
    older_div.fu_cycles_left = 5;
    state.fu_bank[FuKind::Div.index()].push(super::super::FuState {
        kind: Some(FuKind::Div),
        slot: Some(older_div),
        busy_cycles_left: 4,
    });

    let mut fence = PipeSlot::from_word(4, encode(Instruction::Fence).unwrap());
    fence.seq = 2;
    state.stages[Stage::ID as usize] = Some(fence);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let sys_busy = state.fu_bank[FuKind::Sys.index()].iter().any(|fu| {
        fu.slot
            .as_ref()
            .is_some_and(|slot| matches!(slot.instr, Some(Instruction::Fence)))
    });
    assert!(
        sys_busy,
        "Fence should dispatch to SYS FU while an older DIV is still active"
    );
}

#[test]
fn parallel_fu_mode_keeps_halt_on_serialized_path() {
    let mut state = PipelineSimState::new();
    state.mode = super::super::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut halt = PipeSlot::from_word(0, encode(Instruction::Halt).unwrap());
    halt.seq = 1;
    state.stages[Stage::ID as usize] = Some(halt);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let ex_instr = state.stages[Stage::EX as usize].as_ref().and_then(|slot| {
        slot.instr
            .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    });
    assert!(matches!(ex_instr, Some(Instruction::Halt)));
}

#[test]
fn parallel_fu_mode_dispatches_fence_i_to_sys_fu() {
    let mut state = PipelineSimState::new();
    state.mode = super::super::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig {
        div: 5,
        ..CpiConfig::default()
    };
    let mut console = Console::default();

    let mut older_div = PipeSlot::from_word(
        0,
        encode(Instruction::Div {
            rd: 10,
            rs1: 11,
            rs2: 12,
        })
        .unwrap(),
    );
    older_div.seq = 1;
    older_div.instr = Some(Instruction::Div {
        rd: 10,
        rs1: 11,
        rs2: 12,
    });
    older_div.fu_cycles_left = 5;
    state.fu_bank[FuKind::Div.index()].push(super::super::FuState {
        kind: Some(FuKind::Div),
        slot: Some(older_div),
        busy_cycles_left: 4,
    });

    let mut fence_i = PipeSlot::from_word(4, encode(Instruction::FenceI).unwrap());
    fence_i.seq = 2;
    state.stages[Stage::ID as usize] = Some(fence_i);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let sys_busy = state.fu_bank[FuKind::Sys.index()].iter().any(|fu| {
        fu.slot
            .as_ref()
            .is_some_and(|slot| matches!(slot.instr, Some(Instruction::FenceI)))
    });
    assert!(
        sys_busy,
        "FenceI should dispatch to SYS FU while an older DIV is still active"
    );
}

#[test]
fn invalid_instruction_faults_pipeline_with_console_error() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(0, 0xFFFF_FFFF).unwrap();

    state.reset_stages(0);
    // The fault is raised when the instruction commits, not when it is fetched.
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
    }

    assert!(state.faulted, "invalid opcode should fault pipeline");
    assert!(
        console.lines.iter().any(|line| line
            .text
            .contains("Invalid instruction 0xFFFFFFFF at 0x00000000")),
        "console should report invalid instruction"
    );
}

#[test]
fn fetch_fault_reports_if_error_in_console() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    state.reset_stages(0x8000_0000);
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
    }

    assert!(state.faulted, "fetch fault should fault pipeline");
    assert!(
        console
            .lines
            .iter()
            .any(|line| line.text.contains("IF fault at 0x80000000")),
        "console should report IF fault"
    );
}

#[test]
fn branch_mispredict_populates_branch_stall_bucket() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    state.set_predict(super::super::BranchPredict::NotTaken);
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 0,
            rs2: 0,
            imm: 8,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..24 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.flush_count > 0);
    assert!(
        state.stall_by_type[HazardType::BranchFlush.as_stall_index().unwrap()] > 0,
        "mispredict should populate branch-stall bucket"
    );
}

#[test]
fn pipeline_ecall_get_cycle_count_uses_current_pipeline_wall_clock() {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    mem.bypass = true;
    load_program_into_mem(
        ".text\n.globl _start\n_start:\n    li a7, 1031\n    ecall\n    halt\n",
        &mut cpu,
        &mut mem,
    );
    state.reset_stages(0);

    let mut committed_ecall = false;
    for _ in 0..32 {
        let wb_has_ecall = matches!(
            state.stages[Stage::WB as usize]
                .as_ref()
                .and_then(|slot| slot.instr),
            Some(Instruction::Ecall)
        );
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if wb_has_ecall {
            assert_eq!(cpu.read(10) as u64, state.cycle_count);
            assert!(cpu.read(10) > 0);
            committed_ecall = true;
            break;
        }
    }

    assert!(committed_ecall, "ecall never reached WB");
}

#[test]
fn branch_prediction_mispredict_flushes_wrong_path() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    state.predict = super::super::BranchPredict::Taken;
    cpu.write(1, 1);
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 1,
            rs2: 0,
            imm: 12,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 7,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    mem.store32(
        12,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 9,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(16, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..10 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert_eq!(cpu.read(10), 7);
    assert!(state.flush_count > 0);
}

#[test]
fn branch_prediction_btfnt_takes_backward_loop() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    state.set_predict(super::super::BranchPredict::Btfnt);
    cpu.write(1, 1);
    mem.store32(
        0,
        encode(Instruction::Addi {
            rd: 2,
            rs1: 0,
            imm: 0,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Bne {
            rs1: 1,
            rs2: 0,
            imm: -4,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..4 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
    }

    assert!(
        state.fetch_pc <= 8,
        "btfnt should redirect toward the backward target instead of the forward halt path"
    );
    assert_eq!(state.flush_count, 0);
}

#[test]
fn forwarded_raw_and_waw_are_both_reported_for_same_register() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Lui { rd: 11, imm: 0x1 }).unwrap(),
        encode(Instruction::Addi {
            rd: 11,
            rs1: 11,
            imm: 0,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }
    state.reset_stages(0);

    let mut saw_forward = false;
    let mut saw_waw = false;
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        saw_forward |= state
            .hazard_traces
            .iter()
            .any(|t| t.kind == TraceKind::Forward && t.to_stage == Stage::EX as usize);
        saw_waw |= state
            .hazard_traces
            .iter()
            .any(|t| t.kind == TraceKind::Hazard(HazardType::WriteAfterWrite));
        if saw_forward && saw_waw {
            break;
        }
    }

    assert!(
        saw_forward,
        "expected forwarding trace for self-dependent addi"
    );
    assert!(
        saw_waw,
        "expected WAW trace for repeated writes to the same rd"
    );
}

#[test]
fn no_forwarding_still_reads_fresh_value_after_writeback() {
    let mut state = PipelineSimState::new();
    state.set_legacy_forwarding(false);
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Lui {
            rd: 11,
            imm: 0x1000,
        })
        .unwrap(),
        encode(Instruction::Addi {
            rd: 12,
            rs1: 11,
            imm: 4,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }
    state.reset_stages(0);

    for _ in 0..48 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert!(state.halted, "pipeline did not halt");
    assert_eq!(cpu.read(11), 0x1000);
    assert_eq!(cpu.read(12), 0x1004);
}

#[test]
fn print_example_runs_with_forwarding_disabled() {
    let asm = r#"
.data
msg: .asciz "Hello, Raven!\n"

.text
    la   t0, msg
    li   t1, 0
count_loop:
    lb   t2, 0(t0)
    beq  t2, zero, count_done
    addi t0, t0, 1
    addi t1, t1, 1
    j    count_loop
count_done:
    li   a0, 1
    la   a1, msg
    mv   a2, t1
    li   a7, 64
    ecall
    li   a0, 0
    li   a7, 93
    ecall
"#;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.set_legacy_forwarding(false);
    mem.bypass = true;
    load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);

    for _ in 0..1024 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.halted, "pipeline did not halt");
    assert!(!state.faulted, "pipeline faulted");
    let output = console
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        output.contains("Hello, Raven!"),
        "expected hello-world output, got: {output:?}"
    );
    assert!(
        output.contains("Exit 0"),
        "expected clean exit, got: {output:?}"
    );
}

#[test]
fn no_forwarding_count_loop_reaches_done() {
    let asm = r#"
.data
msg: .asciz "A"

.text
    la   t0, msg
    li   t1, 0
count_loop:
    lb   t2, 0(t0)
    beq  t2, zero, count_done
    addi t0, t0, 1
    addi t1, t1, 1
    j    count_loop
count_done:
    li   a0, 99
    halt
"#;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.set_legacy_forwarding(false);
    mem.bypass = true;
    load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);

    for _ in 0..64 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.halted, "loop with no forwarding did not halt");
    assert_eq!(cpu.read(10), 99);
}

#[test]
fn no_forwarding_ecall_sequence_halts_cleanly() {
    let asm = r#"
.data
msg: .asciz "A"

.text
    li   a0, 1
    la   a1, msg
    li   a2, 1
    li   a7, 64
    ecall
    li   a0, 0
    li   a7, 93
    ecall
"#;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.set_legacy_forwarding(false);
    mem.bypass = true;
    load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);

    for _ in 0..96 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.halted, "no-forwarding ecall sequence did not halt");
    assert!(!state.faulted);
}

#[test]
fn flw_use_stalls_until_fp_load_data_is_ready() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(0x100, 1.5f32.to_bits()).unwrap();
    let program = [
        encode(Instruction::Flw {
            rd: 1,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
        encode(Instruction::FaddS {
            rd: 2,
            rs1: 1,
            rs2: 1,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }

    state.reset_stages(0);
    for _ in 0..48 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.halted, "pipeline did not halt");
    assert!(!state.faulted, "pipeline faulted");
    assert_eq!(cpu.fread_bits(2), 3.0f32.to_bits());
}

#[test]
fn lrw_executes_in_pipeline() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    cpu.write(1, 0x100);
    mem.store32(0x100, 0xCAFE_BABE).unwrap();
    let program = [
        encode(Instruction::LrW {
            rd: 5,
            rs1: 1,
            aq: false,
            rl: false,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];

    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }

    state.reset_stages(0);
    for _ in 0..32 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(state.halted, "pipeline did not halt");
    assert!(!state.faulted, "pipeline faulted");
    assert_eq!(cpu.read(5), 0xCAFE_BABE);
    assert_eq!(cpu.lr_reservation, Some(0x100));
}

#[test]
fn fetch_slot_tracks_icache_latency() {
    let icfg = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        hit_latency: 3,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
        ..CacheConfig::default()
    };
    let mut mem = CacheController::new(icfg, CacheConfig::default(), vec![], 0x4000);
    mem.bypass = false;
    mem.ram
        .store32(0, encode(Instruction::Halt).unwrap())
        .unwrap();

    let slot = fetch_slot(0, &mut mem);
    assert_eq!(slot.word, encode(Instruction::Halt).unwrap());
    assert_eq!(slot.if_stall_cycles, 3);
}

#[test]
fn fetch_slot_tracks_l1_l2_icache_latency() {
    let l1 = slow_level(4, 4, 1);
    let l2 = slow_level(4, 8, 5);
    let mut mem = CacheController::new(l1, CacheConfig::default(), vec![l2], 0x4000);
    mem.bypass = false;
    mem.ram
        .store32(0, encode(Instruction::Halt).unwrap())
        .unwrap();

    let slot = fetch_slot(0, &mut mem);
    assert_eq!(slot.word, encode(Instruction::Halt).unwrap());
    assert_eq!(slot.if_stall_cycles, 7);
}

#[test]
fn fetch_slot_tracks_l1_l2_l3_icache_latency() {
    let l1 = slow_level(4, 4, 1);
    let l2 = slow_level(4, 4, 5);
    let l3 = slow_level(4, 8, 9);
    let mut mem = CacheController::new(l1, CacheConfig::default(), vec![l2, l3], 0x4000);
    mem.bypass = false;
    mem.ram
        .store32(0, encode(Instruction::Halt).unwrap())
        .unwrap();

    let slot = fetch_slot(0, &mut mem);
    assert_eq!(slot.word, encode(Instruction::Halt).unwrap());
    assert_eq!(slot.if_stall_cycles, 17);
}

#[test]
fn pipeline_fetch_does_not_count_as_retired_instruction() {
    let icfg = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        hit_latency: 1,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
        ..CacheConfig::default()
    };
    let mut mem = CacheController::new(icfg, CacheConfig::default(), vec![], 0x4000);
    mem.bypass = false;
    mem.ram
        .store32(0, encode(Instruction::Halt).unwrap())
        .unwrap();

    let _ = fetch_slot(0, &mut mem);
    assert_eq!(mem.instruction_count, 0);
}

#[test]
fn mem_stall_does_not_consume_if_cache_stall_cycles() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut if_slot = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 1,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    );
    if_slot.if_stall_cycles = 3;

    let mut mem_slot = PipeSlot::from_word(
        4,
        encode(Instruction::Lw {
            rd: 2,
            rs1: 0,
            imm: 0,
        })
        .unwrap(),
    );
    mem_slot.mem_stall_cycles = 2;

    state.stages[Stage::IF as usize] = Some(if_slot);
    state.stages[Stage::MEM as usize] = Some(mem_slot);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let if_cycles = state.stages[Stage::IF as usize]
        .as_ref()
        .map(|s| s.if_stall_cycles)
        .expect("IF slot should remain in place");
    let mem_cycles = state.stages[Stage::MEM as usize]
        .as_ref()
        .map(|s| s.mem_stall_cycles)
        .expect("MEM slot should remain in place");

    assert_eq!(if_cycles, 3, "MEM stall should freeze IF cache countdown");
    assert_eq!(
        mem_cycles, 1,
        "MEM stall should consume one MEM latency cycle"
    );
}

#[test]
fn if_cache_stall_inserts_frontend_bubble_into_id() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut if_slot = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 3,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    );
    if_slot.if_stall_cycles = 2;

    let ex_slot = PipeSlot::from_word(
        4,
        encode(Instruction::Addi {
            rd: 4,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    );

    state.stages[Stage::IF as usize] = Some(if_slot);
    state.stages[Stage::EX as usize] = Some(ex_slot);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let id_slot = state.stages[Stage::ID as usize]
        .as_ref()
        .expect("ID should contain an explicit front-end bubble");
    assert!(id_slot.is_bubble);
    assert_eq!(id_slot.hazard, Some(HazardType::MemoryLatency));
    assert!(
        state.hazard_msgs.iter().any(|(kind, msg)| {
            *kind == HazardType::MemoryLatency && msg.contains("front-end bubble")
        }),
        "expected a textual explanation for IF-cache bubble injection"
    );
    assert!(
        state.gantt.iter().any(|row| row
            .cells
            .iter()
            .any(|cell| matches!(cell, GanttCell::Bubble))),
        "expected front-end bubble to appear in gantt history"
    );
}

#[test]
fn if_cache_stall_bubble_propagates_into_ex() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut if_slot = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 3,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    );
    if_slot.if_stall_cycles = 2;

    let ex_slot = PipeSlot::from_word(
        4,
        encode(Instruction::Addi {
            rd: 4,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    );

    state.stages[Stage::IF as usize] = Some(if_slot);
    state.stages[Stage::EX as usize] = Some(ex_slot);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let ex_slot = state.stages[Stage::EX as usize]
        .as_ref()
        .expect("EX should contain the propagated front-end bubble");
    assert!(ex_slot.is_bubble);
    assert_eq!(ex_slot.hazard, Some(HazardType::MemoryLatency));
}

#[test]
fn stage_mem_counts_extra_level_latency() {
    let dcfg = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        hit_latency: 1,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
        ..CacheConfig::default()
    };
    let l2cfg = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        hit_latency: 5,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
        ..CacheConfig::default()
    };
    let mut mem = CacheController::new(CacheConfig::default(), dcfg.clone(), vec![l2cfg], 0x4000);
    let mut cpu = Cpu::default();
    let mut console = Console::default();
    mem.bypass = false;
    mem.ram.store32(0x100, 0x1234_5678).unwrap();

    let mut slot = PipeSlot::from_word(
        0,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 0,
            imm: 0,
        })
        .unwrap(),
    );
    slot.instr = Some(Instruction::Lw {
        rd: 5,
        rs1: 0,
        imm: 0,
    });
    slot.mem_addr = Some(0x100);

    let latency = stage_mem(&mut slot, &mut cpu, &mut mem, &mut console);
    assert_eq!(slot.mem_result, Some(0x1234_5678));
    assert_eq!(latency.0, 8);
}

#[test]
fn stage_mem_counts_two_extra_levels_latency() {
    let dcfg = slow_level(4, 4, 1);
    let l2cfg = slow_level(4, 4, 5);
    let l3cfg = slow_level(4, 8, 9);
    let mut mem = CacheController::new(CacheConfig::default(), dcfg, vec![l2cfg, l3cfg], 0x4000);
    let mut cpu = Cpu::default();
    let mut console = Console::default();
    mem.bypass = false;
    mem.ram.store32(0x100, 0x1234_5678).unwrap();

    let mut slot = PipeSlot::from_word(
        0,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 0,
            imm: 0,
        })
        .unwrap(),
    );
    slot.instr = Some(Instruction::Lw {
        rd: 5,
        rs1: 0,
        imm: 0,
    });
    slot.mem_addr = Some(0x100);

    let latency = stage_mem(&mut slot, &mut cpu, &mut mem, &mut console);
    assert_eq!(slot.mem_result, Some(0x1234_5678));
    assert_eq!(latency.0, 18);
}

#[test]
fn branch_resolve_id_redirects_before_wrong_path_commits() {
    let mut state = PipelineSimState::new();
    state.branch_resolve = super::super::BranchResolve::Id;
    state.predict = super::super::BranchPredict::NotTaken;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 0,
            rs2: 0,
            imm: 8,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        8,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(12, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..20 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted {
            break;
        }
    }

    assert_eq!(cpu.read(10), 2);
    assert_eq!(state.flush_count, 1, "ID resolve should only flush IF");
}

#[test]
fn mispredict_marks_flush_in_gantt_history() {
    let mut state = PipelineSimState::new();
    state.predict = super::super::BranchPredict::Taken;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    cpu.write(1, 1);
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 1,
            rs2: 0,
            imm: 12,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 7,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    mem.store32(
        12,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 9,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(16, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..10 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.flush_count > 0 {
            break;
        }
    }

    assert!(
        state.gantt.iter().any(|row| row
            .cells
            .iter()
            .any(|cell| matches!(cell, GanttCell::Flush))),
        "expected flushed wrong-path instruction to appear as Flush in gantt"
    );
}

#[test]
fn bubble_sort_matches_sequential_execution() {
    let asm = r#"
.data
arr: .word 5, 1, 4, 2, 8
.text
    la   t0, arr
    li   t1, 5
    mv   t4, t1
    li   s2, 0
outer_loop:
    li   s3, 0
    li   t2, 0
    addi t5, t4, -1
inner_loop:
    bge  t2, t5, inner_done
    slli t3, t2, 2
    add  t3, t0, t3
    lw   s0, 0(t3)
    lw   s1, 4(t3)
    ble  s0, s1, no_swap
    sw   s1, 0(t3)
    sw   s0, 4(t3)
    addi s2, s2, 1
    addi s3, s3, 1
no_swap:
    addi t2, t2, 1
    j    inner_loop
inner_done:
    addi t4, t4, -1
    beq  s3, zero, sort_done
    bgt  t4, zero, outer_loop
sort_done:
    halt
"#;

    let (prog_seq, _, mem_seq) = run_sequential(asm);
    let (prog_pipe, _, mem_pipe, state) = run_pipeline_prog(asm);
    let arr_base = prog_seq.data_base;

    let seq_vals: Vec<u32> = (0..5)
        .map(|i| mem_seq.load32(arr_base + i * 4).unwrap())
        .collect();
    let pipe_vals: Vec<u32> = (0..5)
        .map(|i| mem_pipe.load32(arr_base + i * 4).unwrap())
        .collect();

    assert_eq!(prog_pipe.data_base, prog_seq.data_base);
    assert_eq!(seq_vals, vec![1, 2, 4, 5, 8]);
    assert_eq!(
        pipe_vals, seq_vals,
        "pipeline diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count, state.flush_count, state.cycle_count
    );
}

#[test]
fn branch_after_addi_dependency_matches_sequential_behavior() {
    let asm = r#"
.text
    li   t4, 5
    addi t5, t4, -1
    bge  zero, t5, done
    li   a0, 7
done:
    halt
"#;
    let (_, cpu_seq, _) = run_sequential(asm);
    let (_, cpu_pipe, _, state) = run_pipeline_prog(asm);
    assert_eq!(
        cpu_pipe.read(10),
        cpu_seq.read(10),
        "branch-after-addi diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count,
        state.flush_count,
        state.cycle_count
    );
}

#[test]
fn csr_instructions_and_fp_flags_match_sequential_behavior() {
    // Zicsr used to classify as Unknown in the pipeline and retire as a no-op.
    let asm = r#"
.text
    li   t0, 1
    fcvt.s.w f1, t0
    li   t0, 3
    fcvt.s.w f2, t0
    fdiv.s f3, f1, f2
    csrr a0, fflags
    mv   a4, a0
    csrw fflags, zero
    csrr a1, fflags
    li   t1, 0x1234
    csrw fcsr, t1
    csrr a2, fcsr
    csrr a3, frm
    halt
"#;
    let (_, cpu_seq, _) = run_sequential(asm);
    let (_, cpu_pipe, _, _) = run_pipeline_prog(asm);
    let regs = |cpu: &Cpu| [10, 11, 12, 13, 14].map(|r| cpu.read(r));
    // 1/3 is inexact: NX. fcsr keeps 8 bits; frm is fcsr[7:5].
    assert_eq!(regs(&cpu_seq), [0x01, 0, 0x34, 1, 0x01]);
    assert_eq!(regs(&cpu_pipe), regs(&cpu_seq));
}

#[test]
fn f0_result_is_forwarded_like_any_float_register() {
    // f0 is not hardwired to zero; the bypass used to skip it like x0.
    let asm = r#"
.text
    li   a1, 0x12345678
    li   a2, 0
    fmv.w.x f1, a1
    fmv.w.x f2, a2
    fsgnj.s f0, f1, f2
    fmv.x.w a0, f0
    halt
"#;
    let (_, cpu_pipe, _, _) = run_pipeline_prog(asm);
    assert_eq!(cpu_pipe.read(10), 0x12345678);
}

#[test]
fn parallel_fu_mode_sends_float_loads_and_stores_through_mem() {
    // flw/fsw are FloatingPoint by class; they used to go from the FPU
    // straight to WB, so the load read nothing and the store wrote nothing.
    let asm = r#"
.data
tdat:
.word 0xbf800000
.word 0x40000000
.word 0
.word 0
.word 0
.word 0
.text
    la   a1, tdat
    flw  f1, 4(a1)
    fsw  f1, 20(a1)
    lw   a0, 20(a1)
    flw  f2, 0(a1)
    fadd.s f3, f1, f2
    fmv.x.w a2, f3
    halt
"#;
    let (_, cpu_seq, _) = run_sequential(asm);
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    mem.bypass = true;
    load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);
    for _ in 0..1024 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }
    // 2.0 stored and reloaded; 2.0 + (-1.0) = 1.0.
    assert_eq!(
        [cpu_seq.read(10), cpu_seq.read(12)],
        [0x4000_0000, 0x3F80_0000]
    );
    assert_eq!(
        [cpu.read(10), cpu.read(12)],
        [cpu_seq.read(10), cpu_seq.read(12)]
    );
}

#[test]
fn branch_after_two_loads_matches_sequential_behavior() {
    let asm = r#"
.data
arr: .word 5, 1
.text
    la   t0, arr
    lw   s0, 0(t0)
    lw   s1, 4(t0)
    ble  s0, s1, done
    li   a0, 9
done:
    halt
"#;
    let (_, cpu_seq, _) = run_sequential(asm);
    let (_, cpu_pipe, _, state) = run_pipeline_prog(asm);
    assert_eq!(
        cpu_pipe.read(10),
        cpu_seq.read(10),
        "branch-after-two-loads diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count,
        state.flush_count,
        state.cycle_count
    );
}

#[test]
fn swap_pair_matches_sequential_execution() {
    let asm = r#"
.data
arr: .word 5, 1
.text
    la   t0, arr
    lw   s0, 0(t0)
    lw   s1, 4(t0)
    ble  s0, s1, done
    sw   s1, 0(t0)
    sw   s0, 4(t0)
done:
    halt
"#;
    let (prog_seq, _, mem_seq) = run_sequential(asm);
    let (prog_pipe, _, mem_pipe, state) = run_pipeline_prog(asm);
    let arr_base = prog_seq.data_base;
    let seq_vals: Vec<u32> = (0..2)
        .map(|i| mem_seq.load32(arr_base + i * 4).unwrap())
        .collect();
    let pipe_vals: Vec<u32> = (0..2)
        .map(|i| mem_pipe.load32(arr_base + i * 4).unwrap())
        .collect();
    assert_eq!(prog_pipe.data_base, prog_seq.data_base);
    assert_eq!(seq_vals, vec![1, 5]);
    assert_eq!(
        pipe_vals, seq_vals,
        "pipeline diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count, state.flush_count, state.cycle_count
    );
}

#[test]
fn looped_swap_pair_matches_sequential_execution() {
    let asm = r#"
.data
arr: .word 5, 1
.text
    la   t0, arr
    li   t2, 0
    li   t5, 1
loop:
    bge  t2, t5, done
    lw   s0, 0(t0)
    lw   s1, 4(t0)
    ble  s0, s1, no_swap
    sw   s1, 0(t0)
    sw   s0, 4(t0)
no_swap:
    addi t2, t2, 1
    j    loop
done:
    halt
"#;
    let (prog_seq, _, mem_seq) = run_sequential(asm);
    let (prog_pipe, _, mem_pipe, state) = run_pipeline_prog(asm);
    let arr_base = prog_seq.data_base;
    let seq_vals: Vec<u32> = (0..2)
        .map(|i| mem_seq.load32(arr_base + i * 4).unwrap())
        .collect();
    let pipe_vals: Vec<u32> = (0..2)
        .map(|i| mem_pipe.load32(arr_base + i * 4).unwrap())
        .collect();
    assert_eq!(prog_pipe.data_base, prog_seq.data_base);
    assert_eq!(seq_vals, vec![1, 5]);
    assert_eq!(
        pipe_vals, seq_vals,
        "looped swap diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count, state.flush_count, state.cycle_count
    );
}

#[test]
fn indexed_looped_swap_pair_matches_sequential_execution() {
    let asm = r#"
.data
arr: .word 5, 1
.text
    la   t0, arr
    li   t2, 0
    li   t5, 1
loop:
    bge  t2, t5, done
    slli t3, t2, 2
    add  t3, t0, t3
    lw   s0, 0(t3)
    lw   s1, 4(t3)
    ble  s0, s1, no_swap
    sw   s1, 0(t3)
    sw   s0, 4(t3)
no_swap:
    addi t2, t2, 1
    j    loop
done:
    halt
"#;
    let (prog_seq, _, mem_seq) = run_sequential(asm);
    let (prog_pipe, _, mem_pipe, state) = run_pipeline_prog(asm);
    let arr_base = prog_seq.data_base;
    let seq_vals: Vec<u32> = (0..2)
        .map(|i| mem_seq.load32(arr_base + i * 4).unwrap())
        .collect();
    let pipe_vals: Vec<u32> = (0..2)
        .map(|i| mem_pipe.load32(arr_base + i * 4).unwrap())
        .collect();
    assert_eq!(prog_pipe.data_base, prog_seq.data_base);
    assert_eq!(seq_vals, vec![1, 5]);
    assert_eq!(
        pipe_vals, seq_vals,
        "indexed looped swap diverged; stalls={}, flushes={}, cycles={}",
        state.stall_count, state.flush_count, state.cycle_count
    );
}

#[test]
fn gantt_retains_up_to_200_cycles_per_row() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 10,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(4, encode(Instruction::Jal { rd: 0, imm: -4 }).unwrap())
        .unwrap();

    state.reset_stages(0);
    for _ in 0..260 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.faulted || state.halted {
            break;
        }
    }

    let longest = state
        .gantt
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0);
    assert!(
        longest <= crate::falcon::pipeline::MAX_GANTT_COLS,
        "gantt row exceeded retention: {longest}"
    );
    assert!(
        state.gantt.iter().any(|row| row.first_cycle > 0),
        "expected at least one row to have advanced first_cycle after long retention"
    );
}

#[test]
fn gantt_retains_far_more_than_old_12_rows_for_vertical_scroll() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    for i in 0..48u32 {
        mem.store32(
            i * 4,
            encode(Instruction::Addi {
                rd: 10,
                rs1: 10,
                imm: 1,
            })
            .unwrap(),
        )
        .unwrap();
    }
    mem.store32(48 * 4, encode(Instruction::Halt).unwrap())
        .unwrap();

    state.reset_stages(0);
    for _ in 0..256 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(
        state.gantt.len() > 12,
        "vertical history should retain far more than 12 rows, got {}",
        state.gantt.len()
    );
}

#[test]
fn functional_units_keep_div_in_ex_for_multiple_cycles() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    cpu.write(11, 12);
    cpu.write(12, 3);
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.div = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Div {
            rd: 10,
            rs1: 11,
            rs2: 12,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 13,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);

    let mut div_pc = None;
    let mut div_cycles_left = 0;
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if let Some(slot) = state.fu_bank[crate::falcon::pipeline::FuKind::Div.index()]
            .iter()
            .filter_map(|fu| fu.slot.as_ref())
            .find(|slot| !slot.is_bubble && slot.disasm.starts_with("div"))
        {
            div_pc = Some(slot.pc);
            div_cycles_left = slot.fu_cycles_left;
            break;
        }
    }

    let div_pc = div_pc.expect("div should reach DIV functional unit");
    assert!(
        div_cycles_left > 1,
        "div should have multi-cycle latency in functional-units mode"
    );

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let div_slot = state.fu_bank[crate::falcon::pipeline::FuKind::Div.index()]
        .iter()
        .filter_map(|fu| fu.slot.as_ref())
        .find(|slot| !slot.is_bubble)
        .expect("div should still occupy DIV functional unit");
    assert_eq!(div_slot.pc, div_pc);
    assert!(
        div_slot.fu_cycles_left < div_cycles_left,
        "remaining DIV cycles should decrease while div stays in its functional unit"
    );
    assert!(
        !state.stages[crate::falcon::pipeline::Stage::ID as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("addi")),
        "younger independent addi should not remain blocked in ID while div runs in its own FU"
    );
}

#[test]
fn single_cycle_mode_holds_div_in_ex_for_configured_cpi() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::SingleCycle;
    let mut cpu = Cpu::default();
    cpu.write(11, 12);
    cpu.write(12, 3);
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.div = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Div {
            rd: 10,
            rs1: 11,
            rs2: 12,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(4, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);

    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("div"))
        {
            break;
        }
    }

    assert!(
        state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("div")),
        "div should reach EX"
    );

    let ex_cycles_left = state.stages[crate::falcon::pipeline::Stage::EX as usize]
        .as_ref()
        .map(|slot| slot.fu_cycles_left)
        .expect("div in EX");
    assert!(
        ex_cycles_left > 1,
        "single-cycle mode must still honor CPI in EX"
    );

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    assert!(
        state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("div")),
        "single-cycle mode should keep div in EX while CPI cycles remain"
    );
    assert!(
        state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| slot.fu_cycles_left < ex_cycles_left),
        "remaining EX cycles should decrease while div stays in EX"
    );
}

#[test]
fn single_cycle_mode_holds_alu_in_ex_for_configured_cpi() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::SingleCycle;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.alu = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(4, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);

    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("addi"))
        {
            break;
        }
    }

    let ex_cycles_left = state.stages[crate::falcon::pipeline::Stage::EX as usize]
        .as_ref()
        .map(|slot| slot.fu_cycles_left)
        .expect("addi should reach EX");
    assert!(ex_cycles_left > 1);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    assert!(
        state.stages[crate::falcon::pipeline::Stage::EX as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("addi")),
        "ALU instruction should remain in EX while CPI cycles remain"
    );
}

#[test]
fn ex_to_ex_toggle_changes_back_to_back_alu_stalls() {
    fn run_with_ex_to_ex(ex_to_ex: bool) -> (PipelineSimState, Cpu) {
        let mut state = PipelineSimState::new();
        state.bypass.ex_to_ex = ex_to_ex;
        state.bypass.mem_to_ex = false;
        state.bypass.wb_to_id = false;
        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        let cpi = CpiConfig::default();
        let mut console = Console::default();

        mem.bypass = true;
        let program = [
            encode(Instruction::Addi {
                rd: 11,
                rs1: 0,
                imm: 7,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 12,
                rs1: 11,
                imm: 3,
            })
            .unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];
        for (i, word) in program.iter().enumerate() {
            mem.store32((i as u32) * 4, *word).unwrap();
        }

        state.reset_stages(0);
        for _ in 0..16 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted || state.faulted {
                break;
            }
        }

        (state, cpu)
    }

    let (with_forward, cpu_with) = run_with_ex_to_ex(true);
    let (without_forward, cpu_without) = run_with_ex_to_ex(false);

    assert_eq!(cpu_with.read(12), 10);
    assert_eq!(cpu_without.read(12), 10);
    assert!(
        with_forward.stall_count < without_forward.stall_count,
        "disabling EX->EX should increase stalls: with={}, without={}",
        with_forward.stall_count,
        without_forward.stall_count
    );
}

#[test]
fn mem_to_ex_toggle_changes_load_use_stalls() {
    fn run_with_mem_to_ex(mem_to_ex: bool) -> (PipelineSimState, Cpu) {
        let mut state = PipelineSimState::new();
        state.bypass.ex_to_ex = true;
        state.bypass.mem_to_ex = mem_to_ex;
        state.bypass.wb_to_id = false;
        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        let cpi = CpiConfig::default();
        let mut console = Console::default();

        mem.bypass = true;
        mem.store32(0x100, 5).unwrap();
        let program = [
            encode(Instruction::Lw {
                rd: 11,
                rs1: 0,
                imm: 0x100,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 12,
                rs1: 11,
                imm: 3,
            })
            .unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];
        for (i, word) in program.iter().enumerate() {
            mem.store32((i as u32) * 4, *word).unwrap();
        }

        state.reset_stages(0);
        for _ in 0..20 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted || state.faulted {
                break;
            }
        }

        (state, cpu)
    }

    let (with_forward, cpu_with) = run_with_mem_to_ex(true);
    let (without_forward, cpu_without) = run_with_mem_to_ex(false);

    assert_eq!(cpu_with.read(12), 8);
    assert_eq!(cpu_without.read(12), 8);
    assert!(
        with_forward.stall_count < without_forward.stall_count,
        "disabling MEM->EX should increase stalls: with={}, without={}",
        with_forward.stall_count,
        without_forward.stall_count
    );
}

#[test]
fn mem_to_ex_trace_keeps_mem_to_ex_path_label() {
    let mut state = PipelineSimState::new();
    state.bypass.ex_to_ex = true;
    state.bypass.mem_to_ex = true;
    state.bypass.wb_to_id = false;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(0x100, 5).unwrap();
    let program = [
        encode(Instruction::Lw {
            rd: 11,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
        encode(Instruction::Addi {
            rd: 12,
            rs1: 11,
            imm: 3,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];
    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }

    state.reset_stages(0);
    let mut saw_mem_to_ex = false;
    for _ in 0..20 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        saw_mem_to_ex |= state.hazard_traces.iter().any(|trace| {
            trace.kind == TraceKind::Forward
                && trace.to_stage == Stage::EX as usize
                && trace.detail.contains("MEM->EX")
        });
        if saw_mem_to_ex || state.halted || state.faulted {
            break;
        }
    }

    assert!(
        saw_mem_to_ex,
        "expected MEM->EX path label in forwarding trace"
    );
}

#[test]
fn store_to_load_forward_extracts_supported_sizes_without_masking_latency() {
    let bypass = super::super::PipelineBypassConfig {
        store_to_load: true,
        ..super::super::PipelineBypassConfig::default()
    };

    let mk_slot = |instr, addr, value| {
        let word = encode(instr).unwrap_or(0);
        let mut slot = PipeSlot::from_word(0, word);
        slot.instr = Some(instr);
        slot.mem_addr = Some(addr);
        slot.rs2_val = value;
        slot
    };

    let producer = Some(mk_slot(
        Instruction::Sw {
            rs2: 11,
            rs1: 0,
            imm: 0,
        },
        0x100,
        0x4433_2211,
    ));
    let load_w = mk_slot(
        Instruction::Lw {
            rd: 10,
            rs1: 0,
            imm: 0,
        },
        0x100,
        0,
    );
    let load_b = mk_slot(
        Instruction::Lb {
            rd: 10,
            rs1: 0,
            imm: 2,
        },
        0x102,
        0,
    );
    let load_bu = mk_slot(
        Instruction::Lbu {
            rd: 10,
            rs1: 0,
            imm: 3,
        },
        0x103,
        0,
    );
    let load_h = mk_slot(
        Instruction::Lh {
            rd: 10,
            rs1: 0,
            imm: 0,
        },
        0x100,
        0,
    );
    let load_hu = mk_slot(
        Instruction::Lhu {
            rd: 10,
            rs1: 0,
            imm: 2,
        },
        0x102,
        0,
    );

    assert_eq!(
        super::forwarding::try_store_to_load_forward(&load_w, bypass, &producer),
        Some(0x4433_2211)
    );
    assert_eq!(
        super::forwarding::try_store_to_load_forward(&load_b, bypass, &producer),
        Some(0x33)
    );
    assert_eq!(
        super::forwarding::try_store_to_load_forward(&load_bu, bypass, &producer),
        Some(0x44)
    );
    assert_eq!(
        super::forwarding::try_store_to_load_forward(&load_h, bypass, &producer),
        Some(0x2211)
    );
    assert_eq!(
        super::forwarding::try_store_to_load_forward(&load_hu, bypass, &producer),
        Some(0x4433)
    );
}

#[test]
fn store_to_load_forward_overrides_stale_ram_value_but_keeps_dcache_latency() {
    let mut state = PipelineSimState::new();
    state.bypass.store_to_load = true;
    let mut cpu = Cpu::default();
    let dcfg = CacheConfig {
        size: 16,
        line_size: 4,
        associativity: 1,
        hit_latency: 3,
        miss_penalty: 0,
        assoc_penalty: 0,
        transfer_width: 4,
        ..CacheConfig::default()
    };
    let mut mem = CacheController::new(CacheConfig::default(), dcfg, vec![], 0x4000);
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = false;
    mem.store32(0x100, 0xDEAD_BEEF).unwrap();

    let mut store = PipeSlot::from_word(
        0,
        encode(Instruction::Sw {
            rs2: 11,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
    );
    store.instr = Some(Instruction::Sw {
        rs2: 11,
        rs1: 0,
        imm: 0x100,
    });
    store.mem_addr = Some(0x100);
    store.rs2_val = 0x1122_3344;

    let mut load = PipeSlot::from_word(
        4,
        encode(Instruction::Lw {
            rd: 10,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
    );
    load.instr = Some(Instruction::Lw {
        rd: 10,
        rs1: 0,
        imm: 0x100,
    });
    load.mem_addr = Some(0x100);

    state.stages[Stage::EX as usize] = Some(load);
    state.stages[Stage::MEM as usize] = Some(store);

    advance_stages(
        &mut state,
        &cpu.clone(),
        None,
        &mut cpu,
        &mut mem,
        &cpi,
        &mut console,
    );

    let mem_slot = state.stages[Stage::MEM as usize]
        .as_ref()
        .expect("load should advance into MEM");
    assert_eq!(mem_slot.mem_result, Some(0x1122_3344));
    assert_eq!(mem_slot.mem_stall_cycles, 2);
}

#[test]
fn store_data_hazard_does_not_use_hidden_mem_bypass() {
    let mut state = PipelineSimState::new();
    state.bypass.ex_to_ex = true;
    state.bypass.mem_to_ex = true;
    state.bypass.wb_to_id = false;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    let program = [
        encode(Instruction::Addi {
            rd: 11,
            rs1: 0,
            imm: 42,
        })
        .unwrap(),
        encode(Instruction::Sw {
            rs2: 11,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
        encode(Instruction::Halt).unwrap(),
    ];
    for (i, word) in program.iter().enumerate() {
        mem.store32((i as u32) * 4, *word).unwrap();
    }

    state.reset_stages(0);
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        let id_is_store = state.stages[Stage::ID as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("sw"));
        let mem_has_producer = state.stages[Stage::MEM as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("addi"));
        if id_is_store && mem_has_producer {
            break;
        }
    }

    assert!(
        state.stages[Stage::ID as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("sw")),
        "store should still be waiting in ID"
    );
    assert!(
        state.stages[Stage::MEM as usize]
            .as_ref()
            .is_some_and(|slot| !slot.is_bubble && slot.disasm.starts_with("addi")),
        "producer should have advanced to MEM"
    );
}

#[test]
fn wb_to_id_toggle_changes_same_cycle_decode_visibility() {
    fn run_with_wb_to_id(wb_to_id: bool) -> (PipelineSimState, Cpu) {
        let mut state = PipelineSimState::new();
        state.bypass.ex_to_ex = false;
        state.bypass.mem_to_ex = false;
        state.bypass.wb_to_id = wb_to_id;
        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        let cpi = CpiConfig::default();
        let mut console = Console::default();

        mem.bypass = true;
        let program = [
            encode(Instruction::Lui {
                rd: 11,
                imm: 0x1000,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 12,
                rs1: 11,
                imm: 4,
            })
            .unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];
        for (i, word) in program.iter().enumerate() {
            mem.store32((i as u32) * 4, *word).unwrap();
        }

        state.reset_stages(0);
        for _ in 0..20 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted || state.faulted {
                break;
            }
        }

        (state, cpu)
    }

    let (with_forward, cpu_with) = run_with_wb_to_id(true);
    let (without_forward, cpu_without) = run_with_wb_to_id(false);
    assert_eq!(cpu_with.read(12), 0x1004);
    assert_eq!(cpu_without.read(12), 0x1004);
    assert!(
        with_forward.stall_count < without_forward.stall_count,
        "disabling WB->ID should increase stalls: with={}, without={}",
        with_forward.stall_count,
        without_forward.stall_count
    );
}

#[test]
fn branch_in_id_does_not_false_stall_on_value_committed_from_wb_this_cycle() {
    fn run_with_wb_to_id(wb_to_id: bool) -> PipelineSimState {
        let mut state = PipelineSimState::new();
        state.branch_resolve = super::super::BranchResolve::Id;
        state.bypass.ex_to_ex = false;
        state.bypass.mem_to_ex = false;
        state.bypass.wb_to_id = wb_to_id;
        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        let cpi = CpiConfig::default();
        let mut console = Console::default();

        mem.bypass = true;
        let program = [
            encode(Instruction::Addi {
                rd: 5,
                rs1: 0,
                imm: 1,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 0,
                rs1: 0,
                imm: 0,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 0,
                rs1: 0,
                imm: 0,
            })
            .unwrap(),
            encode(Instruction::Beq {
                rs1: 5,
                rs2: 0,
                imm: 8,
            })
            .unwrap(),
            encode(Instruction::Addi {
                rd: 10,
                rs1: 0,
                imm: 7,
            })
            .unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];
        for (i, word) in program.iter().enumerate() {
            mem.store32((i as u32) * 4, *word).unwrap();
        }

        state.reset_stages(0);
        for _ in 0..32 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted || state.faulted {
                break;
            }
        }

        assert!(state.halted, "pipeline did not halt");
        assert_eq!(cpu.read(10), 7, "fall-through instruction should commit");
        state
    }

    let with_forward = run_with_wb_to_id(true);
    let without_forward = run_with_wb_to_id(false);

    assert_eq!(
        with_forward.stall_count, without_forward.stall_count,
        "same-cycle WB commit should satisfy branch-in-ID without needing WB->ID bypass"
    );
    assert_eq!(
        without_forward.stall_by_type[HazardType::ReadAfterWrite.as_stall_index().unwrap()],
        0
    );
}

#[test]
fn raw_stall_still_consumes_if_cache_countdown() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut if_slot = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 3,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    );
    if_slot.if_stall_cycles = 3;

    let mut ex_slot = PipeSlot::from_word(
        4,
        encode(Instruction::Addi {
            rd: 1,
            rs1: 0,
            imm: 7,
        })
        .unwrap(),
    );
    ex_slot.instr = Some(Instruction::Addi {
        rd: 1,
        rs1: 0,
        imm: 7,
    });

    let mut id_slot = PipeSlot::from_word(
        8,
        encode(Instruction::Addi {
            rd: 2,
            rs1: 1,
            imm: 1,
        })
        .unwrap(),
    );
    id_slot.instr = Some(Instruction::Addi {
        rd: 2,
        rs1: 1,
        imm: 1,
    });

    state.stages[Stage::IF as usize] = Some(if_slot);
    state.stages[Stage::ID as usize] = Some(id_slot);
    state.stages[Stage::EX as usize] = Some(ex_slot);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    assert_eq!(
        state.stages[Stage::IF as usize]
            .as_ref()
            .map(|s| s.if_stall_cycles),
        Some(2),
        "RAW stall should not freeze an in-flight IF cache countdown"
    );
}

#[test]
fn scw_without_shared_reservation_fails_cleanly() {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();

    cpu.lr_reservation = Some(0x5000);
    let mut slot = PipeSlot::from_word(
        0,
        encode(Instruction::ScW {
            rd: 5,
            rs1: 1,
            rs2: 2,
            aq: false,
            rl: false,
        })
        .unwrap(),
    );
    slot.instr = Some(Instruction::ScW {
        rd: 5,
        rs1: 1,
        rs2: 2,
        aq: false,
        rl: false,
    });
    slot.mem_addr = Some(0x5000);
    slot.rs2_val = 0xDEAD_BEEF;

    let (_latency, faulted, _trap) = stage_mem(&mut slot, &mut cpu, &mut mem, &mut console);
    assert!(
        !faulted,
        "sc.w without a shared reservation should fail cleanly"
    );
    assert_eq!(slot.alu_result, 1);
}

#[test]
fn amoswap_mem_read_fault_sets_faulted() {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();

    let mut slot = PipeSlot::from_word(
        0,
        encode(Instruction::AmoswapW {
            rd: 5,
            rs1: 1,
            rs2: 2,
            aq: false,
            rl: false,
        })
        .unwrap(),
    );
    slot.instr = Some(Instruction::AmoswapW {
        rd: 5,
        rs1: 1,
        rs2: 2,
        aq: false,
        rl: false,
    });
    slot.mem_addr = Some(0x5000);
    slot.rs2_val = 0x1234_5678;

    let (_latency, faulted, _trap) = stage_mem(&mut slot, &mut cpu, &mut mem, &mut console);
    assert!(faulted, "AMO read fault should fault the pipeline");
}

#[test]
fn gantt_keeps_distinct_rows_for_repeated_same_pc() {
    let mut state = PipelineSimState::new();
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(0, encode(Instruction::Jal { rd: 0, imm: 0 }).unwrap())
        .unwrap();

    state.reset_stages(0);
    for _ in 0..20 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.faulted || state.halted {
            break;
        }
    }

    let pc_zero_rows = state.gantt.iter().filter(|row| row.pc == 0).count();
    assert!(
        pc_zero_rows > 1,
        "repeated executions of the same PC should occupy distinct Gantt rows"
    );
}

#[test]
fn parallel_fu_mode_dispatches_independent_alu_work_while_load_is_still_in_lsu() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.load = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 1,
            imm: 0,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 6,
            rs1: 0,
            imm: 7,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    mem.store32(0x100, 0xCAFE_BABE).unwrap();
    cpu.write(1, 0x100);

    state.reset_stages(0);
    for _ in 0..4 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    let lsu_slot = state.fu_bank[crate::falcon::pipeline::FuKind::Lsu.index()]
        .first()
        .and_then(|fu| fu.slot.as_ref())
        .expect("load should dispatch into LSU bank");
    assert!(matches!(lsu_slot.class, InstrClass::Load));

    let alu_slot = state.fu_bank[crate::falcon::pipeline::FuKind::Alu.index()]
        .first()
        .and_then(|fu| fu.slot.as_ref())
        .expect("independent addi should dispatch into ALU bank");
    assert!(matches!(alu_slot.class, InstrClass::Alu));
    assert_eq!(alu_slot.rd, Some(6));
    assert_eq!(
        cpu.read(6),
        0,
        "newer ALU work must not commit before the older load"
    );
}

#[test]
fn parallel_fu_mode_dispatches_load_to_lsu_bank() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.load = 3;
    let mut console = Console::default();

    let mut older_div = PipeSlot::from_word(
        0,
        encode(Instruction::Div {
            rd: 10,
            rs1: 11,
            rs2: 12,
        })
        .unwrap(),
    );
    older_div.seq = 1;
    older_div.instr = Some(Instruction::Div {
        rd: 10,
        rs1: 11,
        rs2: 12,
    });
    older_div.fu_cycles_left = 5;
    state.fu_bank[FuKind::Div.index()].push(super::super::FuState {
        kind: Some(FuKind::Div),
        slot: Some(older_div),
        busy_cycles_left: 4,
    });

    let mut load = PipeSlot::from_word(
        4,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 1,
            imm: 0,
        })
        .unwrap(),
    );
    load.seq = 2;
    state.stages[Stage::ID as usize] = Some(load);
    cpu.write(1, 0x100);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let lsu_busy = state.fu_bank[FuKind::Lsu.index()].iter().any(|fu| {
        fu.slot
            .as_ref()
            .is_some_and(|slot| matches!(slot.instr, Some(Instruction::Lw { .. })))
    });
    assert!(
        lsu_busy,
        "load should dispatch to LSU FU while an older DIV is still active"
    );
}

#[test]
fn parallel_fu_mode_serializes_lsu_ops_to_preserve_store_then_load_order() {
    let asm = r#"
.text
    li   sp, 0x100
    li   t0, 0x1234
    sw   t0, 0(sp)
    lw   a0, 0(sp)
    halt
"#;

    let (_prog_seq, cpu_seq, _mem_seq) = run_sequential(asm);
    let (_prog_pipe, cpu_pipe, _mem_pipe, state) = run_pipeline_prog(asm);

    assert_eq!(cpu_seq.read(10), 0x1234);
    assert_eq!(
        cpu_pipe.read(10),
        cpu_seq.read(10),
        "parallel LSU must not let a younger load bypass an older store; stalls={}, flushes={}, cycles={}",
        state.stall_count,
        state.flush_count,
        state.cycle_count
    );
}

#[test]
fn parallel_fu_mode_keeps_younger_result_uncommitted_while_older_load_finishes() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.load = 3;
    cpi.alu = 1;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 1,
            imm: 0,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 6,
            rs1: 0,
            imm: 7,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    mem.store32(0x100, 0xABCD_EF01).unwrap();
    cpu.write(1, 0x100);

    state.reset_stages(0);
    for _ in 0..4 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert_eq!(
        cpu.read(6),
        0,
        "a younger ALU result must not commit while the older load is still ahead in program order"
    );

    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert_eq!(
        cpu.read(5),
        0xABCD_EF01,
        "older load should eventually commit"
    );
    assert_eq!(
        cpu.read(6),
        7,
        "younger ALU result should commit after the older load"
    );
}

#[test]
fn parallel_fu_mode_keeps_dependent_consumer_blocked_until_producer_finishes() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.mul = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Mul {
            rd: 5,
            rs1: 1,
            rs2: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 6,
            rs1: 5,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    cpu.write(1, 3);
    cpu.write(2, 4);

    state.reset_stages(0);
    for _ in 0..4 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    let mul_slot = state.fu_bank[crate::falcon::pipeline::FuKind::Mul.index()]
        .first()
        .and_then(|fu| fu.slot.as_ref())
        .expect("mul should still be active in MUL bank");
    assert!(matches!(mul_slot.class, InstrClass::Multiply));

    assert!(
        state.fu_bank[crate::falcon::pipeline::FuKind::Alu.index()]
            .iter()
            .all(|fu| fu.slot.is_none()),
        "dependent addi must not dispatch into ALU bank while mul still owns x5"
    );
    let id_slot = state.stages[Stage::ID as usize]
        .as_ref()
        .expect("dependent addi should remain blocked in ID");
    assert!(matches!(id_slot.class, InstrClass::Alu));
    assert_eq!(id_slot.rs1, Some(5));
}

#[test]
fn parallel_fu_mode_respects_configured_alu_capacity() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.fu_capacity[crate::falcon::pipeline::FuKind::Alu.index()] = 2;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.alu = 3;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Addi {
            rd: 5,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 6,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    let mut max_active_alu = 0usize;
    for _ in 0..8 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        max_active_alu = max_active_alu.max(
            state.fu_bank[crate::falcon::pipeline::FuKind::Alu.index()]
                .iter()
                .filter(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble))
                .count(),
        );
        if state.halted || state.faulted {
            break;
        }
    }

    assert_eq!(
        max_active_alu, 2,
        "two ALU operations should coexist when ALU capacity is configured to 2"
    );
}

#[test]
fn parallel_fu_mode_stalls_in_id_when_parallel_alu_bank_is_full() {
    let mut state = PipelineSimState::new();
    state.mode = super::super::PipelineMode::FunctionalUnits;
    state.fu_capacity[FuKind::Alu.index()] = 1;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut older_addi = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 10,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    );
    older_addi.seq = 1;
    older_addi.instr = Some(Instruction::Addi {
        rd: 10,
        rs1: 0,
        imm: 1,
    });
    older_addi.fu_cycles_left = 2;
    state.fu_bank[FuKind::Alu.index()].push(super::super::FuState {
        kind: Some(FuKind::Alu),
        slot: Some(older_addi),
        busy_cycles_left: 1,
    });

    let mut younger_addi = PipeSlot::from_word(
        4,
        encode(Instruction::Addi {
            rd: 11,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    );
    younger_addi.seq = 2;
    state.stages[Stage::ID as usize] = Some(younger_addi);

    let mut if_addi = PipeSlot::from_word(
        8,
        encode(Instruction::Addi {
            rd: 12,
            rs1: 0,
            imm: 3,
        })
        .unwrap(),
    );
    if_addi.seq = 3;
    state.stages[Stage::IF as usize] = Some(if_addi);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    // With swap_remove cleanup, the promoted ALU entry is removed from fu_bank.
    assert!(
        state.fu_bank[FuKind::Alu.index()].is_empty(),
        "promoted ALU entry must be removed from fu_bank after swap_remove cleanup"
    );
    let id_instr = state.stages[Stage::ID as usize].as_ref().and_then(|slot| {
        slot.instr
            .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    });
    assert!(matches!(id_instr, Some(Instruction::Addi { rd: 11, .. })));
    assert!(
        state.stages[Stage::EX as usize]
            .as_ref()
            .is_none_or(|slot| slot.is_bubble)
    );
    assert!(
        state.hazard_msgs.iter().any(
            |(hazard, msg)| *hazard == HazardType::FunctionalUnitBusy && msg.contains("ALU is at capacity")
        )
    );
}

#[test]
fn store_data_hazard_does_not_emit_fu_to_id_bypass_trace() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut producer = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 11,
            rs1: 0,
            imm: 42,
        })
        .unwrap(),
    );
    producer.seq = 1;
    producer.instr = Some(Instruction::Addi {
        rd: 11,
        rs1: 0,
        imm: 42,
    });
    producer.alu_result = 42;
    producer.fu_cycles_left = 1;
    state.fu_bank[FuKind::Alu.index()].push(super::super::FuState {
        kind: Some(FuKind::Alu),
        slot: Some(producer),
        busy_cycles_left: 0,
    });

    let mut store = PipeSlot::from_word(
        4,
        encode(Instruction::Sw {
            rs2: 11,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
    );
    store.seq = 2;
    store.instr = Some(Instruction::Sw {
        rs2: 11,
        rs1: 0,
        imm: 0x100,
    });
    state.stages[Stage::ID as usize] = Some(store);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    assert!(
        state.hazard_traces.iter().all(|trace| {
            !(matches!(trace.kind, TraceKind::Forward)
                && trace.from_stage == Stage::EX as usize
                && trace.to_stage == Stage::ID as usize)
        }),
        "store-data dependency should not be rendered as a dispatchable FU->ID bypass"
    );
}

#[test]
fn id_raw_stall_does_not_freeze_parallel_fu_progress() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    let mut producer = PipeSlot::from_word(
        0,
        encode(Instruction::Addi {
            rd: 11,
            rs1: 0,
            imm: 42,
        })
        .unwrap(),
    );
    producer.seq = 1;
    producer.instr = Some(Instruction::Addi {
        rd: 11,
        rs1: 0,
        imm: 42,
    });
    producer.alu_result = 42;
    producer.fu_cycles_left = 2;
    state.fu_bank[FuKind::Alu.index()].push(super::super::FuState {
        kind: Some(FuKind::Alu),
        slot: Some(producer),
        busy_cycles_left: 1,
    });

    let mut store = PipeSlot::from_word(
        4,
        encode(Instruction::Sw {
            rs2: 11,
            rs1: 0,
            imm: 0x100,
        })
        .unwrap(),
    );
    store.seq = 2;
    store.instr = Some(Instruction::Sw {
        rs2: 11,
        rs1: 0,
        imm: 0x100,
    });
    state.stages[Stage::ID as usize] = Some(store);

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let wb_instr = state.stages[Stage::WB as usize].as_ref().and_then(|slot| {
        slot.instr
            .or_else(|| crate::falcon::decoder::decode(slot.word).ok())
    });
    assert!(
        matches!(wb_instr, Some(Instruction::Addi { rd: 11, .. })),
        "parallel FU work must continue advancing and may already promote to WB while ID is stalled on RAW"
    );
}

#[test]
fn remu_store_data_dependency_eventually_completes() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.div = 20;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Remu {
            rd: 10,
            rs1: 10,
            rs2: 11,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Sw {
            rs2: 10,
            rs1: 2,
            imm: 20,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(8, encode(Instruction::Halt).unwrap()).unwrap();
    cpu.write(10, 123);
    cpu.write(11, 10);
    cpu.write(2, 0x100);

    state.reset_stages(0);
    for _ in 0..80 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(
        state.halted,
        "remu -> sw -> halt should complete under skip"
    );
    assert_eq!(mem.load32(0x114).unwrap(), 3);
}

#[test]
fn ready_fu_result_can_promote_into_bubbled_wb() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.stages[Stage::WB as usize] = Some(PipeSlot::bubble());
    state.fu_bank[FuKind::Div.index()].push(FuState {
        kind: Some(FuKind::Div),
        slot: Some(PipeSlot {
            is_bubble: false,
            instr: Some(Instruction::Remu {
                rd: 10,
                rs1: 10,
                rs2: 11,
            }),
            class: InstrClass::Divide,
            rd: Some(10),
            alu_result: 3,
            fu_cycles_left: 1,
            seq: 1,
            ..PipeSlot::bubble()
        }),
        busy_cycles_left: 0,
    });

    promote_ready_fu_result_to_wb(&mut state);

    let wb = state.stages[Stage::WB as usize].as_ref().expect("wb slot");
    assert!(
        !wb.is_bubble,
        "ready FU result must replace a bubbled WB slot"
    );
    // With swap_remove cleanup, the promoted entry is removed entirely from fu_bank.
    assert!(state.fu_bank[FuKind::Div.index()].is_empty());
}

#[test]
fn parallel_fu_mode_keeps_correct_jump_after_taken_branch_flush() {
    let asm = r#"
        .text
        .globl _start
    _start:
        li a1, 0
        beqz a1, taken
        j wrong
    taken:
        li a0, 1
        j done
    wrong:
        li a0, 2
    done:
        li a7, 93
        ecall
    "#;

    let (_prog_seq, cpu_seq, _mem_seq) = run_sequential(asm);
    let (_prog_pipe, mut cpu_pipe, mut mem_pipe, mut state) = run_pipeline_prog(asm);
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.reset_stages(cpu_pipe.pc);
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    for _ in 0..256 {
        pipeline_tick(&mut state, &mut cpu_pipe, &mut mem_pipe, &cpi, &mut console);
        if state.halted || state.faulted || cpu_pipe.exit_code.is_some() {
            break;
        }
    }

    assert!(!state.faulted, "console faulted during branch/jump repro");
    assert_eq!(cpu_pipe.exit_code, Some(1), "parallel path diverged");
    assert_eq!(cpu_pipe.exit_code, cpu_seq.exit_code);
}

#[test]
fn parallel_fu_mode_does_not_duplicate_taken_jump_after_remu_branch_path() {
    let asm = r#"
        .text
        .globl _start
    _start:
        lw a0, 28(sp)
        lw a1, 24(sp)
        remu a0, a0, a1
        sw a0, 20(sp)
        sw a0, 44(sp)
        beqz a0, taken
        j wrong
    taken:
        lw a0, 28(sp)
        sw a0, 32(sp)
        j done
    wrong:
        li a0, 2
        sw a0, 32(sp)
    done:
        lw a0, 32(sp)
        halt
    "#;

    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    mem.bypass = true;
    let _prog = load_program_into_mem(asm, &mut cpu, &mut mem);
    cpu.write(2, 0x1000);
    mem.store32(0x1000 + 28, 0x2000).unwrap();
    mem.store32(0x1000 + 24, 0x1000).unwrap();

    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.reset_stages(0);
    let cpi = CpiConfig::default();
    let mut committed_pcs = Vec::new();

    for _ in 0..128 {
        let commit = pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if let Some(ref info) = commit {
            committed_pcs.push(info.pc);
        }
        if state.halted || state.faulted {
            break;
        }
    }

    let jump_pc = 9 * 4;
    let jump_commits = committed_pcs.iter().filter(|&&pc| pc == jump_pc).count();
    assert_eq!(
        jump_commits, 1,
        "taken-path jump must commit exactly once; got sequence {:?}",
        committed_pcs
    );
    assert!(!state.faulted);
    assert!(state.halted);
    assert_eq!(cpu.read(10), 0x2000);
}

#[test]
fn ready_lsu_result_can_promote_into_bubbled_mem() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.stages[Stage::MEM as usize] = Some(PipeSlot::bubble());
    state.fu_bank[FuKind::Lsu.index()].push(FuState {
        kind: Some(FuKind::Lsu),
        slot: Some(PipeSlot {
            is_bubble: false,
            instr: Some(Instruction::Lw {
                rd: 10,
                rs1: 2,
                imm: 0,
            }),
            class: InstrClass::Load,
            rd: Some(10),
            mem_addr: Some(0x100),
            fu_cycles_left: 1,
            seq: 1,
            ..PipeSlot::bubble()
        }),
        busy_cycles_left: 0,
    });

    promote_ready_lsu_to_mem(&mut state);

    let mem = state.stages[Stage::MEM as usize]
        .as_ref()
        .expect("mem slot");
    assert!(
        !mem.is_bubble,
        "ready LSU result must replace a bubbled MEM slot"
    );
    // With swap_remove cleanup, the promoted entry is removed entirely from fu_bank.
    assert!(state.fu_bank[FuKind::Lsu.index()].is_empty());
}

#[test]
fn mem_cache_stall_does_not_freeze_parallel_fu_progress() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.stages[Stage::MEM as usize] = Some(PipeSlot {
        is_bubble: false,
        instr: Some(Instruction::Lw {
            rd: 5,
            rs1: 2,
            imm: 0,
        }),
        class: InstrClass::Load,
        rd: Some(5),
        mem_addr: Some(0x100),
        mem_stall_cycles: 3,
        seq: 1,
        ..PipeSlot::bubble()
    });
    state.fu_bank[FuKind::Div.index()].push(FuState {
        kind: Some(FuKind::Div),
        slot: Some(PipeSlot {
            is_bubble: false,
            instr: Some(Instruction::Remu {
                rd: 10,
                rs1: 10,
                rs2: 11,
            }),
            class: InstrClass::Divide,
            rd: Some(10),
            alu_result: 3,
            fu_cycles_left: 4,
            seq: 2,
            ..PipeSlot::bubble()
        }),
        busy_cycles_left: 3,
    });

    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut console = Console::default();

    pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);

    let div_slot = state.fu_bank[FuKind::Div.index()][0]
        .slot
        .as_ref()
        .expect("div slot remains in flight");
    assert_eq!(
        div_slot.fu_cycles_left, 3,
        "parallel FU countdown must continue while MEM is stalled"
    );
}

#[test]
fn parallel_fu_mode_allows_mixed_fu_types_to_run_together() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.alu = 3;
    cpi.mul = 4;
    cpi.load = 4;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Lw {
            rd: 5,
            rs1: 1,
            imm: 0,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Mul {
            rd: 6,
            rs1: 2,
            rs2: 3,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        8,
        encode(Instruction::Addi {
            rd: 7,
            rs1: 0,
            imm: 9,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(12, encode(Instruction::Halt).unwrap()).unwrap();
    mem.store32(0x100, 0xCAFE_BABE).unwrap();
    cpu.write(1, 0x100);
    cpu.write(2, 6);
    cpu.write(3, 7);

    state.reset_stages(0);
    let mut saw_mixed_parallelism = false;
    for _ in 0..10 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        let lsu_active = state.fu_bank[FuKind::Lsu.index()]
            .iter()
            .any(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble));
        let mul_active = state.fu_bank[FuKind::Mul.index()]
            .iter()
            .any(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble));
        let alu_active = state.fu_bank[FuKind::Alu.index()]
            .iter()
            .any(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble));
        if lsu_active && mul_active && alu_active {
            saw_mixed_parallelism = true;
            break;
        }
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(
        saw_mixed_parallelism,
        "load, mul, and addi should be able to coexist across LSU, MUL, and ALU banks"
    );
}

#[test]
fn parallel_fu_mode_respects_per_type_capacities_independently() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    state.fu_capacity[FuKind::Alu.index()] = 2;
    state.fu_capacity[FuKind::Mul.index()] = 1;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.alu = 4;
    cpi.mul = 4;
    let mut console = Console::default();

    mem.bypass = true;
    mem.store32(
        0,
        encode(Instruction::Mul {
            rd: 10,
            rs1: 1,
            rs2: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 11,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        8,
        encode(Instruction::Addi {
            rd: 12,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        12,
        encode(Instruction::Mul {
            rd: 13,
            rs1: 3,
            rs2: 4,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(16, encode(Instruction::Halt).unwrap()).unwrap();
    cpu.write(1, 2);
    cpu.write(2, 3);
    cpu.write(3, 4);
    cpu.write(4, 5);

    state.reset_stages(0);
    let mut saw_capacity_shape = false;
    for _ in 0..12 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        let alu_count = state.fu_bank[FuKind::Alu.index()]
            .iter()
            .filter(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble))
            .count();
        let mul_count = state.fu_bank[FuKind::Mul.index()]
            .iter()
            .filter(|fu| fu.slot.as_ref().is_some_and(|slot| !slot.is_bubble))
            .count();
        if alu_count == 2 && mul_count == 1 {
            saw_capacity_shape = true;
        }
        assert!(
            alu_count <= 2,
            "ALU occupancy must respect its configured capacity"
        );
        assert!(
            mul_count <= 1,
            "MUL occupancy must respect its configured capacity"
        );
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(
        saw_capacity_shape,
        "the pipeline should allow two ALU ops and one MUL op in flight together when capacities permit"
    );
}

#[test]
fn forwarding_prefers_youngest_ready_fu_writer_for_same_register() {
    let mut consumer = PipeSlot {
        is_bubble: false,
        instr: Some(Instruction::Addi {
            rd: 6,
            rs1: 5,
            imm: 1,
        }),
        class: InstrClass::Alu,
        rs1: Some(5),
        rs1_val: 0,
        seq: 10,
        ..PipeSlot::bubble()
    };
    let older_writer = PipeSlot {
        is_bubble: false,
        instr: Some(Instruction::Addi {
            rd: 5,
            rs1: 0,
            imm: 1,
        }),
        class: InstrClass::Alu,
        rd: Some(5),
        alu_result: 1,
        fu_cycles_left: 1,
        seq: 3,
        ..PipeSlot::bubble()
    };
    let younger_writer = PipeSlot {
        is_bubble: false,
        instr: Some(Instruction::Lui { rd: 5, imm: 3 }),
        class: InstrClass::Alu,
        rd: Some(5),
        alu_result: 0x3000,
        fu_cycles_left: 1,
        seq: 4,
        ..PipeSlot::bubble()
    };

    crate::falcon::pipeline::forwarding::apply_forwarding_to_id(
        &mut consumer,
        crate::falcon::pipeline::PipelineBypassConfig::legacy_enabled(),
        &None,
        &[older_writer, younger_writer],
    );

    assert_eq!(
        consumer.rs1_val, 0x3000,
        "forwarding must pick the youngest ready writer of x5, not the older addi"
    );
}

#[test]
fn branch_flush_clears_speculative_work_already_dispatched_to_fu_bank() {
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::FunctionalUnits;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut cpi = CpiConfig::default();
    cpi.alu = 3;
    let mut console = Console::default();

    mem.bypass = true;
    state.predict = super::super::BranchPredict::NotTaken;
    mem.store32(
        0,
        encode(Instruction::Beq {
            rs1: 0,
            rs2: 0,
            imm: 8,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        4,
        encode(Instruction::Addi {
            rd: 5,
            rs1: 0,
            imm: 1,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(
        8,
        encode(Instruction::Addi {
            rd: 6,
            rs1: 0,
            imm: 2,
        })
        .unwrap(),
    )
    .unwrap();
    mem.store32(12, encode(Instruction::Halt).unwrap()).unwrap();

    state.reset_stages(0);
    for _ in 0..12 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }

    assert!(
        state.flush_count > 0,
        "taken branch with not-taken prediction should flush wrong-path work"
    );
    assert_eq!(
        cpu.read(5),
        0,
        "wrong-path addi must not commit from ALU bank"
    );
    assert_eq!(cpu.read(6), 2, "correct-path addi should still commit");
    assert!(
        state
            .fu_bank
            .iter()
            .flatten()
            .all(|fu| fu.slot.as_ref().is_none_or(|slot| !slot.is_speculative)),
        "branch flush must not leave speculative work alive in any FU bank"
    );
}

/// Run `asm` with the caches on until the hart stops: sequentially when
/// `mode` is `None`, otherwise under that pipeline mode. `setup` sees the
/// machine after the load and before the first instruction.
fn run_until_stopped(
    asm: &str,
    mode: Option<crate::falcon::pipeline::PipelineMode>,
    setup: impl FnOnce(&crate::falcon::asm::Program, &mut Cpu, &mut CacheController),
) -> (crate::falcon::asm::Program, Cpu, CacheController, Console) {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    let prog = load_program_into_mem(asm, &mut cpu, &mut mem);
    setup(&prog, &mut cpu, &mut mem);
    match mode {
        None => {
            for _ in 0..256 {
                if !exec::step(&mut cpu, &mut mem, &mut console).expect("sequential step") {
                    break;
                }
            }
        }
        Some(mode) => {
            let cpi = CpiConfig::default();
            let mut state = PipelineSimState::new();
            state.set_mode(mode, &cpi);
            state.reset_stages(0);
            if let Some(engine) = state.ooo.as_mut() {
                crate::capability::PipelineControl::reset(engine, 0);
            }
            for _ in 0..4096 {
                pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
                if state.halted || state.faulted {
                    break;
                }
            }
            assert!(
                state.halted && !state.faulted,
                "{mode:?} did not stop cleanly: {:?}",
                console.lines.iter().map(|l| &l.text).collect::<Vec<_>>()
            );
        }
    }
    (prog, cpu, mem, console)
}

/// Sequential mode, then every pipeline mode.
fn every_execution_model() -> Vec<Option<crate::falcon::pipeline::PipelineMode>> {
    std::iter::once(None)
        .chain(crate::falcon::pipeline::PipelineMode::all().into_iter().map(Some))
        .collect()
}

fn label_address(prog: &crate::falcon::asm::Program, label: &str) -> u32 {
    prog.labels
        .iter()
        .find(|(_, names)| names.iter().any(|name| name == label))
        .map(|(&addr, _)| addr)
        .unwrap_or_else(|| panic!("no label {label}"))
}

// The first `.data` word stands in for an ELF's `tohost` symbol. The third,
// at `data_base + 8`, is plain memory for the store behind the exit.
const TOHOST_EXIT: &str = r#"
.data
tohost:
    .word 0
    .word 0
after:
    .word 0
.text
    li   a0, 5
    la   t0, tohost
    sw   zero, 0(t0)    # clearing the word asks nothing
    li   t1, 7          # (3 << 1) | 1: exit with code 3
    sw   t1, 4(t0)      # the upper half carries no request
report:
    sw   t1, 0(t0)      # MEM while the store above is still in WB
    sw   t1, 8(t0)      # never runs: the run ends at the store above
    li   a0, 1
    halt
"#;

#[test]
fn a_store_to_tohost_exits_in_every_mode() {
    // riscv-tests' standard environment, and any program built for an HTIF
    // host, ends by storing to `tohost`; RAVEN used to run on past it. With
    // the caches bypassed a store spends a single cycle in MEM, and the
    // serialized datapath then has the store behind the exit in MEM while
    // the exit is still in WB.
    for bypass in [false, true] {
        for model in every_execution_model() {
            let (prog, cpu, mem, console) =
                run_until_stopped(TOHOST_EXIT, model, |prog, _, mem| {
                    mem.bypass = bypass;
                    mem.set_tohost(Some(prog.data_base))
                });
            let run = format!("{model:?}, bypass {bypass}");
            assert_eq!(cpu.exit_code, Some(3), "{run}");
            assert_eq!(cpu.read(10), 5, "{run}: ran past the exit");
            assert_eq!(
                cpu.pc,
                label_address(&prog, "report"),
                "{run}: the hart parks on the store that exited"
            );
            assert_eq!(mem.load32(prog.data_base).unwrap(), 7, "{run}");
            assert_eq!(
                mem.load32(prog.data_base + 8).unwrap(),
                0,
                "{run}: the store behind the exit wrote memory"
            );
            assert!(
                console.lines.iter().any(|l| l.text.contains("Exit 3")),
                "{run}"
            );
        }
    }
}

#[test]
fn without_tohost_the_same_store_is_plain_memory() {
    for model in every_execution_model() {
        let (prog, cpu, mem, _) = run_until_stopped(TOHOST_EXIT, model, |_, _, _| {});
        assert_eq!(cpu.exit_code, None, "{model:?}");
        assert_eq!(cpu.read(10), 1, "{model:?}");
        assert_eq!(mem.load32(prog.data_base).unwrap(), 7, "{model:?}");
        assert_eq!(mem.load32(prog.data_base + 8).unwrap(), 7, "{model:?}");
    }
}

#[test]
fn a_tohost_exit_is_imprecise_only_under_the_scoreboard() {
    // The scoreboard writes results in the order they finish, not in program
    // order. A store to `tohost` can then end the run while an older `div` is
    // still in its unit, so `a1` is never written, or after a younger `li`
    // has already written `a0`. That is the model's imprecision, not a lost
    // exit: the exit code and the PC are right. Every other model stops with
    // exactly what program order says.
    let program = |body: &str| {
        format!(
            r#"
.data
tohost:
    .word 0
    .word 0
.text
    la   t0, tohost
    li   t1, 7
    li   t3, 100
    li   t4, 7
{body}
    li   a0, 1
    halt
"#
        )
    };
    let older_unfinished = program("    div  a1, t3, t4\nreport:\n    sw   t1, 0(t0)");
    let younger_first = program(
        "    li   t3, 21\n    li   t4, 3\n    div  t1, t3, t4\nreport:\n    sw   t1, 0(t0)",
    );
    let scoreboard = Some(crate::falcon::pipeline::PipelineMode::Scoreboard);
    for model in every_execution_model() {
        let (prog, cpu, _, _) = run_until_stopped(&older_unfinished, model, |prog, _, mem| {
            mem.set_tohost(Some(prog.data_base))
        });
        assert_eq!(cpu.exit_code, Some(3), "{model:?}");
        assert_eq!(cpu.pc, label_address(&prog, "report"), "{model:?}");
        assert_eq!(cpu.read(10), 0, "{model:?}");
        let quotient = if model == scoreboard { 0 } else { 14 };
        assert_eq!(cpu.read(11), quotient, "{model:?}: a1");

        let (prog, cpu, _, _) = run_until_stopped(&younger_first, model, |prog, _, mem| {
            mem.set_tohost(Some(prog.data_base))
        });
        assert_eq!(cpu.exit_code, Some(3), "{model:?}");
        assert_eq!(cpu.pc, label_address(&prog, "report"), "{model:?}");
        let a0 = if model == scoreboard { 1 } else { 0 };
        assert_eq!(cpu.read(10), a0, "{model:?}: a0");
    }
}

#[test]
fn an_htif_system_call_through_tohost_is_reported_once_and_ignored() {
    // An even value points at a system-call block for the host to run.
    let asm = r#"
.data
tohost:
    .word 0
    .word 0
.text
    la   t0, tohost
    li   t1, 0x100
    sw   t1, 0(t0)
    sw   t1, 0(t0)
    li   a0, 4
    halt
"#;
    for model in every_execution_model() {
        let (_, cpu, _, console) = run_until_stopped(asm, model, |prog, _, mem| {
            mem.set_tohost(Some(prog.data_base))
        });
        assert_eq!(cpu.exit_code, None, "{model:?}");
        assert_eq!(cpu.read(10), 4, "{model:?}");
        let reports = console
            .lines
            .iter()
            .filter(|l| l.text.contains("not supported"))
            .count();
        assert_eq!(reports, 1, "{model:?}");
    }
}

#[test]
fn flw_and_fsw_keep_memory_order_in_every_mode() {
    // riscv-tests rv32uf ldst, case 2. The out-of-order models sent `flw` and
    // `fsw` to the FPU as float work, outside the order the load/store unit
    // keeps, and the scoreboard let the `lw` read the word before the `fsw`
    // had stored it. The nops give `la` time to finish, so the base address
    // is ready whether or not `flw` names it (#86).
    let asm = r#"
.data
two:  .float 2.0
copy: .word 0
.text
    la   a1, two
    nop
    nop
    nop
    nop
    flw  ft1, 0(a1)
    fsw  ft1, 4(a1)
    lw   a2, 4(a1)
    halt
"#;
    for model in every_execution_model() {
        let (_, cpu, _, _) = run_until_stopped(asm, model, |_, _, _| {});
        assert_eq!(cpu.read(12), 2.0f32.to_bits(), "{model:?}: lw ran before fsw");
    }
}

#[test]
fn mhartid_reads_the_hart_id_in_every_mode() {
    // riscv-tests' environment parks every hart that does not read 0 here.
    let asm = r#"
.text
    csrr a0, mhartid
    li   t0, 9
    csrw mhartid, t0    # read-only
    csrr a1, mhartid
    halt
"#;
    for model in every_execution_model() {
        let (_, cpu, _, _) = run_until_stopped(asm, model, |_, cpu, _| cpu.hart_id = 2);
        assert_eq!(cpu.read(10), 2, "{model:?}");
        assert_eq!(cpu.read(11), 2, "{model:?}");
        assert_eq!(cpu.hart_id, 2, "{model:?}");
    }
}

/// Run `asm` under `mode` until it halts, with the caches on (`bypass` off) so
/// the D-cache and I-cache really are separate.
fn run_pipeline_mode_cached(
    asm: &str,
    mode: crate::falcon::pipeline::PipelineMode,
    console: &mut Console,
) -> (Cpu, PipelineSimState) {
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.set_mode(mode, &cpi);
    load_program_into_mem(asm, &mut cpu, &mut mem);
    state.reset_stages(0);
    for _ in 0..4096 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, console);
        if state.halted || state.faulted || console.reading {
            break;
        }
    }
    (cpu, state)
}

#[test]
fn mret_and_sret_jump_to_the_saved_pc_in_every_mode() {
    // Both used to classify as Unknown and retire as no-ops.
    for (csr, ret) in [("mepc", "mret"), ("sepc", "sret")] {
        let asm = format!(
            r#"
.text
    la   t0, target
    csrw {csr}, t0
    {ret}
    li   a0, 1
    halt
target:
    li   a0, 7
    halt
"#
        );
        for mode in crate::falcon::pipeline::PipelineMode::all() {
            let mut console = Console::default();
            let (cpu, state) = run_pipeline_mode_cached(&asm, mode, &mut console);
            assert!(
                state.halted && !state.faulted,
                "{ret} under {mode:?} did not halt"
            );
            assert_eq!(cpu.read(10), 7, "{ret} under {mode:?} fell through");
        }
    }
}

#[test]
fn sfence_vma_runs_in_the_pipeline() {
    let asm = r#"
.text
    li   t0, 0x1000
    sfence.vma t0, zero
    sfence.vma
    li   a0, 3
    halt
"#;
    for mode in crate::falcon::pipeline::PipelineMode::all() {
        let mut console = Console::default();
        let (cpu, state) = run_pipeline_mode_cached(asm, mode, &mut console);
        assert!(state.halted && !state.faulted, "{mode:?}");
        assert_eq!(cpu.read(10), 3, "{mode:?}");
    }
}

#[test]
fn fence_i_makes_stored_code_visible_in_every_mode() {
    // riscv-tests rv32ui-p-fence_i, case 2: copy `addi a3, a3, 333` over the
    // first instruction at `patch`, fence.i, and run it. The store sits dirty
    // in the write-back D-cache, and the I-cache fills from RAM.
    let asm = r#"
.data
insn:
    .word 0
.text
    li   a3, 111
    la   t0, new_insn
    lw   a0, 0(t0)
    la   t1, patch
    sw   a0, 0(t1)
    fence.i
    jalr ra, t1, 0
    mv   a0, a3
    halt
new_insn:
    addi a3, a3, 333
patch:
    addi a3, a3, 222
    ret
"#;
    let (_, cpu_seq, _) = run_sequential(asm);
    assert_eq!(cpu_seq.read(10), 444);
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x4000,
    );
    let mut console = Console::default();
    load_program_into_mem(asm, &mut cpu, &mut mem);
    // Warm the I-cache with the old code first, the way the test does.
    let patch = cpu.pc + 4 * 12;
    let _ = mem.fetch32(patch);
    for _ in 0..64 {
        if !exec::step(&mut cpu, &mut mem, &mut console).expect("step") {
            break;
        }
    }
    assert_eq!(cpu.read(10), 444, "sequential, caches on");
    for mode in crate::falcon::pipeline::PipelineMode::all() {
        let mut console = Console::default();
        let (cpu, state) = run_pipeline_mode_cached(asm, mode, &mut console);
        assert!(
            state.halted && !state.faulted,
            "{mode:?} pc={:#x} halted={} faulted={} a0={} a3={} cycles={} console={:?}",
            cpu.pc,
            state.halted,
            state.faulted,
            cpu.read(10),
            cpu.read(13),
            state.cycle_count,
            console
                .lines
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(cpu.read(10), 444, "{mode:?} ran the stale instruction");
    }
}

#[test]
fn blocking_read_parks_the_ecall_until_input_arrives_in_every_mode() {
    // The ecall used to retire with no line read, and the program went on
    // past it with an empty buffer.
    let asm = r#"
.data
buf: .space 16
.text
    la   a0, buf
    li   a7, 1003
    ecall
    la   t0, buf
    lbu  a1, 0(t0)
    halt
"#;
    for mode in crate::falcon::pipeline::PipelineMode::all() {
        let mut console = Console::default();
        let mut cpu = Cpu::default();
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        let cpi = CpiConfig::default();
        let mut state = PipelineSimState::new();
        state.set_mode(mode, &cpi);
        load_program_into_mem(asm, &mut cpu, &mut mem);
        state.reset_stages(0);
        let mut tick = |cpu: &mut Cpu, console: &mut Console| {
            for _ in 0..512 {
                pipeline_tick(&mut state, cpu, &mut mem, &cpi, console);
                if state.halted || state.faulted || console.reading {
                    break;
                }
            }
        };
        tick(&mut cpu, &mut console);
        assert!(console.reading, "{mode:?} never blocked on the read");
        let committed_while_blocked = cpu.read(11);
        // The host keeps ticking while it waits; the ecall stays parked.
        console.reading = false;
        tick(&mut cpu, &mut console);
        assert!(console.reading, "{mode:?} ran past the parked ecall");
        assert_eq!(cpu.read(11), committed_while_blocked);
        console.push_input("A");
        console.reading = false;
        tick(&mut cpu, &mut console);
        assert_eq!(cpu.read(11), u32::from(b'A'), "{mode:?}");
    }
}

#[test]
fn a_failed_fetch_on_the_wrong_path_does_not_fault_in_any_mode() {
    // The front end runs past `ret` into the zero word after it before the
    // return resolves. That fetch used to fault the whole pipeline.
    let asm = r#"
.text
    jal  ra, last
    li   a0, 9
    halt
last:
    ret
"#;
    for mode in crate::falcon::pipeline::PipelineMode::all() {
        let mut console = Console::default();
        let (cpu, state) = run_pipeline_mode_cached(asm, mode, &mut console);
        assert!(
            state.halted && !state.faulted,
            "{mode:?}: {:?}",
            console.lines.iter().map(|l| &l.text).collect::<Vec<_>>()
        );
        assert_eq!(cpu.read(10), 9, "{mode:?}");
    }
}

/// Run hand-placed code in U-mode under Sv32, with VA 0 → PA 0 mapped RX+U
/// and nothing else. The M-mode handler at 0x3000 sets a0 = 99 and halts.
fn run_user_code_under_sv32(
    code: &[(u32, Instruction)],
    mode: crate::falcon::pipeline::PipelineMode,
) -> (Cpu, PipelineSimState) {
    use crate::falcon::mmu::PrivMode;
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(
        CacheConfig::default(),
        CacheConfig::default(),
        vec![],
        0x10000,
    );
    let mut console = Console::default();
    let cpi = CpiConfig::default();
    let (root, leaf) = (0x6000u32, 0x7000u32);
    mem.ram.store32(root, ((leaf >> 12) << 10) | 0x1).unwrap();
    mem.ram.store32(leaf, (0x2 | 0x8 | 0x10) | 0x1).unwrap();
    let satp = (1u32 << 31) | (root >> 12);
    mem.mmu_mut().enabled = true;
    mem.set_satp(satp);
    cpu.satp = satp;
    for &(addr, instr) in code {
        mem.ram.store32(addr, encode(instr).unwrap()).unwrap();
    }
    mem.ram
        .store32(
            0x3000,
            encode(Instruction::Addi {
                rd: 10,
                rs1: 0,
                imm: 99,
            })
            .unwrap(),
        )
        .unwrap();
    mem.ram
        .store32(0x3004, encode(Instruction::Halt).unwrap())
        .unwrap();
    cpu.mtvec = 0x3000;
    cpu.priv_mode = PrivMode::U;
    mem.set_priv_mode(PrivMode::U);

    let mut state = PipelineSimState::new();
    state.set_mode(mode, &cpi);
    state.reset_stages(0);
    for _ in 0..4096 {
        pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
        if state.halted || state.faulted {
            break;
        }
    }
    (cpu, state)
}

#[test]
fn a_fetch_page_fault_enters_the_handler_only_when_the_program_gets_there() {
    for mode in crate::falcon::pipeline::PipelineMode::all() {
        // `jalr` at the end of the only mapped page: the front end fetches
        // the unmapped VA 0x1000 behind it before the jump resolves.
        let wrong_path = [
            (
                0x000,
                Instruction::Addi {
                    rd: 6,
                    rs1: 0,
                    imm: 0x100,
                },
            ),
            (0x004, Instruction::Jal { rd: 0, imm: 0xFF8 }),
            (
                0xFFC,
                Instruction::Jalr {
                    rd: 0,
                    rs1: 6,
                    imm: 0,
                },
            ),
            (
                0x100,
                Instruction::Addi {
                    rd: 10,
                    rs1: 0,
                    imm: 5,
                },
            ),
            (0x104, Instruction::Halt),
        ];
        let (cpu, state) = run_user_code_under_sv32(&wrong_path, mode);
        assert!(state.halted && !state.faulted, "{mode:?}");
        assert_eq!(cpu.read(10), 5, "{mode:?}: a wrong-path fetch trapped");
        assert_eq!(cpu.mcause, 0, "{mode:?}");

        let reached = [(0x000, Instruction::Jal { rd: 0, imm: 0x5000 })];
        let (cpu, state) = run_user_code_under_sv32(&reached, mode);
        assert!(state.halted && !state.faulted, "{mode:?}");
        assert_eq!(cpu.read(10), 99, "{mode:?}: the handler did not run");
        assert_eq!(cpu.mcause, 12, "{mode:?}: instruction page fault");
        assert_eq!(cpu.mepc, 0x5000, "{mode:?}");
    }
}

#[test]
fn reset_and_redirect_reach_the_out_of_order_engine() {
    // reset_stages and redirect_pc used to touch only the in-order latches,
    // so under Scoreboard or Tomasulo a reload, a reset or a PC edit left the
    // engine fetching where the old program was.
    let asm = r#"
.text
    li   a0, 1
    li   a0, 2
    li   a0, 3
    halt
"#;
    for mode in [
        crate::falcon::pipeline::PipelineMode::Scoreboard,
        crate::falcon::pipeline::PipelineMode::TomasuloRob,
    ] {
        let mut console = Console::default();
        let (mut cpu, mut state) = run_pipeline_mode_cached(asm, mode, &mut console);
        assert!(state.halted, "{mode:?}");
        let engine = |state: &PipelineSimState| {
            let engine = state.ooo.as_ref().expect("a dynamic mode has an engine");
            (
                crate::capability::PipelineInspect::stats(engine).cycles,
                engine.fetch_pc(),
            )
        };
        assert_ne!(engine(&state).0, 0, "{mode:?}");

        state.reset_stages(0);
        assert_eq!(engine(&state), (0, 0), "{mode:?}: reset left the engine running");

        state.redirect_pc(8);
        assert_eq!(engine(&state).1, 8, "{mode:?}: redirect left the engine behind");

        // And a fresh run from the reset really runs the program again.
        let mut mem = CacheController::new(
            CacheConfig::default(),
            CacheConfig::default(),
            vec![],
            0x4000,
        );
        load_program_into_mem(asm, &mut cpu, &mut mem);
        cpu.write(10, 0);
        state.reset_stages(0);
        let cpi = CpiConfig::default();
        for _ in 0..256 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted || state.faulted {
                break;
            }
        }
        assert!(state.halted && !state.faulted, "{mode:?}");
        assert_eq!(cpu.read(10), 3, "{mode:?}");
    }
}

#[test]
fn set_mode_builds_the_engine_even_if_mode_was_assigned_directly() {
    let cpi = CpiConfig::default();
    let mut state = PipelineSimState::new();
    state.mode = crate::falcon::pipeline::PipelineMode::TomasuloRob;
    assert!(state.ooo.is_none());
    state.set_mode(crate::falcon::pipeline::PipelineMode::TomasuloRob, &cpi);
    assert!(state.ooo.is_some());
}
