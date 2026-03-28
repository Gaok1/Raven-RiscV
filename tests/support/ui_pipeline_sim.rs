    use super::*;
    use crate::falcon::asm::assemble;
    use crate::falcon::cache::CacheConfig;
    use crate::falcon::encoder::encode;
    use crate::falcon::exec;
    use crate::falcon::instruction::Instruction;
    use crate::falcon::program::{load_bytes, load_words, zero_bytes};

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

        for _ in 0..16 {
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
        for _ in 0..8 {
            pipeline_tick(&mut state, &mut cpu, &mut mem, &cpi, &mut console);
            if state.halted {
                break;
            }
        }

        assert_eq!(cpu.read(10), 1);
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
                .any(|t| t.kind == TraceKind::Forward && t.to_stage == Stage::ID as usize);
            saw_waw |= state
                .hazard_traces
                .iter()
                .any(|t| t.kind == TraceKind::Hazard(HazardType::Waw));
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
        state.forwarding = false;
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

        for _ in 0..16 {
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
        state.forwarding = false;
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
        for _ in 0..12 {
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
            encode(Instruction::LrW { rd: 5, rs1: 1 }).unwrap(),
            encode(Instruction::Halt).unwrap(),
        ];

        for (i, word) in program.iter().enumerate() {
            mem.store32((i as u32) * 4, *word).unwrap();
        }

        state.reset_stages(0);
        for _ in 0..10 {
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

        let slot = fetch_slot(0, &mut mem).expect("fetch slot");
        assert_eq!(slot.word, encode(Instruction::Halt).unwrap());
        assert_eq!(slot.if_stall_cycles, 3);
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

        let _ = fetch_slot(0, &mut mem).expect("fetch slot");
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
        let mut mem =
            CacheController::new(CacheConfig::default(), dcfg.clone(), vec![l2cfg], 0x4000);
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
        assert_eq!(latency, 8);
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
        for _ in 0..12 {
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
