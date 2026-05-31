use super::{CpiConfig, classify_cpi_cycles};
use crate::falcon::jit::{ExecCtx, ExecOutcome, ExecutionBackend};
use crate::falcon::memory::Bus;
use crate::falcon::mmu::AccessType;
use crate::falcon::{self, CacheController, Cpu, Instruction, registers::ExecRegion};
use crate::ui::console::Console;

/// Push this hart's `satp` and `priv_mode` into the shared MMU so the next
/// translation uses the per-hart CSRs. Required before any step on a hart
/// other than the one whose CSRs the MMU currently mirrors. Does NOT flush
/// the TLB — translations are tagged by ASID, so cross-hart sharing is fine
/// as long as the OS uses distinct ASIDs (or sfence.vma is issued).
pub(crate) fn sync_mmu_to_cpu(mem: &mut CacheController, cpu: &Cpu) {
    let mmu = mem.mmu_mut();
    mmu.satp = crate::falcon::mmu::Satp::new(cpu.satp);
    mmu.priv_mode = cpu.priv_mode;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HartLifecycle {
    Free,
    Running,
    Paused,
    Exited,
    Faulted,
}

impl HartLifecycle {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Free => "FREE",
            Self::Running => "RUN",
            Self::Paused => "BRK",
            Self::Exited => "EXIT",
            Self::Faulted => "FAULT",
        }
    }
}

pub(crate) struct HartCoreRuntime {
    pub(crate) hart_id: Option<u32>,
    pub(crate) lifecycle: HartLifecycle,
    pub(crate) cpu: Cpu,
    pub(crate) prev_x: [u32; 32],
    pub(crate) prev_f: [u32; 32],
    pub(crate) prev_pc: u32,
    pub(crate) faulted: bool,
    pub(crate) reg_age: [u8; 32],
    pub(crate) f_age: [u8; 32],
    pub(crate) reg_last_write_pc: [Option<u32>; 32],
    pub(crate) f_last_write_pc: [Option<u32>; 32],
    pub(crate) exec_counts: std::collections::HashMap<u32, u64>,
    pub(crate) exec_trace: std::collections::VecDeque<(u32, String)>,
    pub(crate) dyn_mem_access: Option<(u32, u32, bool)>,
    pub(crate) mem_access_log: Vec<(u32, u32, u8)>,
    pub(crate) pipeline: Option<crate::ui::pipeline::PipelineSimState>,
}

impl HartCoreRuntime {
    pub(crate) fn free(base_pc: u32, mem_size: usize) -> Self {
        let mut cpu = Cpu::default();
        cpu.pc = base_pc;
        cpu.write(2, mem_size as u32);
        Self {
            hart_id: None,
            lifecycle: HartLifecycle::Free,
            prev_x: cpu.x,
            prev_f: cpu.f,
            prev_pc: cpu.pc,
            cpu,
            faulted: false,
            reg_age: [255u8; 32],
            f_age: [255u8; 32],
            reg_last_write_pc: [None; 32],
            f_last_write_pc: [None; 32],
            exec_counts: std::collections::HashMap::new(),
            exec_trace: std::collections::VecDeque::new(),
            dyn_mem_access: None,
            mem_access_log: Vec::new(),
            pipeline: Some(crate::ui::pipeline::PipelineSimState::new()),
        }
    }
}

pub(crate) fn is_transparent_single_step_word(word: u32) -> bool {
    matches!(
        falcon::decoder::decode(word),
        Ok(Instruction::Fence | Instruction::FenceI)
    )
}

// ── Background-hart step (Phase-2 optimisation) ──────────────────────────────
//
// Steps one non-selected hart directly, without routing state through `self.run`.
// Only fields required for forward progress are touched: cpu, pipeline,
// exec_counts, and the shared mem/console.
// Display-only fields (reg_age, exec_trace, mem_access_log, etc.) are skipped
// intentionally — they are refreshed lazily when the user selects this core.
//
// Returns `true` if the hart has faulted during this step.
pub(crate) fn step_hart_bg_inner(
    hart: &mut HartCoreRuntime,
    mem: &mut CacheController,
    console: &mut Console,
    cpi: &CpiConfig,
    exec_regions: &[ExecRegion],
    mem_size: usize,
    pipeline_enabled: bool,
    backend: &mut dyn ExecutionBackend<CacheController>,
) -> bool {
    // Ensure the shared MMU reflects *this* hart's satp/priv_mode before any
    // translation. Otherwise round-robin scheduling lets the previous hart's
    // page table leak into this hart's loads/stores.
    sync_mmu_to_cpu(mem, &hart.cpu);
    for _ in 0..16 {
        hart.prev_pc = hart.cpu.pc;

        // ── Pipeline mode ─────────────────────────────────────────────────────
        if pipeline_enabled {
            let Some(pipe) = hart.pipeline.as_mut() else {
                return false;
            };
            if pipe.halted || pipe.faulted {
                if pipe.faulted {
                    hart.faulted = true;
                }
                return hart.faulted;
            }
            let commit =
                crate::ui::pipeline::sim::pipeline_tick(pipe, &mut hart.cpu, mem, cpi, console);
            if let Some(info) = commit {
                *hart.exec_counts.entry(info.pc).or_insert(0) += 1;
                mem.instruction_count = mem.instruction_count.saturating_add(1);
                mem.snapshot_stats();
            }
            if pipe.faulted {
                hart.faulted = true;
            }
            return hart.faulted;
        }

        // ── Sequential mode ───────────────────────────────────────────────────
        let step_pc = hart.cpu.pc;
        if !exec_regions.iter().any(|region| region.contains(step_pc)) {
            console.push_error(format!(
                "Hart reached 0x{step_pc:08X}, outside any executable region. \
                 Add `li a7, 93; ecall` to terminate cleanly."
            ));
            hart.faulted = true;
            return true;
        }

        // Translate via the MMU so CPI classification reads the correct
        // opcode under VM. Failure falls back to identity to avoid charging
        // extra cycles for what would have been a fetch-side page fault
        // (the real fault is raised by the backend a few lines below).
        let fetch_pa = match mem.translate(step_pc, AccessType::Fetch) {
            Ok((pa, _stall)) => pa,
            Err(_) => step_pc,
        };
        let word = mem.peek32(fetch_pa).unwrap_or(0);
        let cpi_cycles = classify_cpi_cycles(word, &hart.cpu, cpi);

        let outcome = {
            let mut ctx = ExecCtx::new(&mut hart.cpu, mem, console);
            backend.run_until_yield(&mut ctx)
        };
        let alive = match outcome {
            Ok(ExecOutcome::Stepped { .. }) => true,
            Ok(ExecOutcome::Halted | ExecOutcome::AwaitingInput) => false,
            Err(e) => {
                use crate::falcon::errors::FalconError;
                let msg = if matches!(&e, FalconError::Bus(_)) {
                    let ram_kb = mem_size / 1024;
                    let suggest = if ram_kb < 1024 {
                        "16mb"
                    } else if ram_kb < 65536 {
                        "128mb"
                    } else {
                        "512mb"
                    };
                    format!("{e} (RAM is {ram_kb} KB — run with --mem {suggest} to increase)")
                } else {
                    e.to_string()
                };
                console.push_error(msg);
                hart.faulted = true;
                return true;
            }
        };

        mem.add_instruction_cycles(cpi_cycles);
        mem.snapshot_stats();
        *hart.exec_counts.entry(step_pc).or_insert(0) += 1;

        if !alive && !console.reading {
            hart.faulted =
                hart.cpu.exit_code.is_none() && !hart.cpu.ebreak_hit && !hart.cpu.local_exit;
        }

        if hart.faulted || !alive || !is_transparent_single_step_word(word) {
            return hart.faulted;
        }
    }

    hart.faulted
}
