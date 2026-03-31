use super::{CpiConfig, classify_cpi_cycles};
use crate::falcon::{self, CacheController, Cpu};
use crate::ui::console::Console;

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
    imem_start: u32,
    imem_end: u32,
    mem_size: usize,
    pipeline_enabled: bool,
) -> bool {
    hart.prev_pc = hart.cpu.pc;

    // ── Pipeline mode ─────────────────────────────────────────────────────────
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

    // ── Sequential mode ───────────────────────────────────────────────────────
    let step_pc = hart.cpu.pc;
    if step_pc < imem_start || step_pc >= imem_end {
        console.push_error(format!(
            "Hart reached 0x{step_pc:08X}, outside the loaded program. \
             Add `li a7, 93; ecall` to terminate cleanly."
        ));
        hart.faulted = true;
        return true;
    }

    let word = mem.peek32(step_pc).unwrap_or(0);
    let cpi_cycles = classify_cpi_cycles(word, &hart.cpu, cpi);

    let alive = match falcon::exec::step(&mut hart.cpu, mem, console) {
        Ok(v) => v,
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
    // instruction_count was already incremented by fetch32 inside exec::step;
    // do not add 1 again here — pipeline mode increments on commit instead.
    mem.snapshot_stats();

    *hart.exec_counts.entry(step_pc).or_insert(0) += 1;

    if !alive && !console.reading {
        hart.faulted = hart.cpu.exit_code.is_none() && !hart.cpu.ebreak_hit && !hart.cpu.local_exit;
    }

    hart.faulted
}
