// output.rs — JSON, fstats, and CSV output formatters; expectation validators

use super::{
    PipelineReport, PipelineTraceHazard, PipelineTraceStage, PipelineTraceStep, SchedReport,
};
use crate::falcon::sched::ThreadInfo;
use crate::falcon::asm::utils::parse_reg;
use crate::falcon::cache::Cache;
use raven_engine::capability::{CacheHierarchy, CoherenceCoreStats};
use crate::falcon::memory::Bus;
use crate::falcon::{CacheController, Cpu};
use crate::ui::pipeline::{HazardType, Stage, TraceKind};
use raven_engine::{Machine, MachineSnapshot, StepOutcome};

// ── JSON output ───────────────────────────────────────────────────────────────

pub(super) fn format_json(
    mem: &CacheController,
    file: &str,
    exit_code: Option<u32>,
    pipeline: Option<PipelineReport>,
    sched: Option<&SchedReport>,
) -> String {
    let (instr, total_cyc, base_cyc, cpi, ipc, clock_model) = if let Some(p) = pipeline {
        let ipc = if p.cycles > 0 {
            p.committed as f64 / p.cycles as f64
        } else {
            0.0
        };
        (p.committed, p.cycles, None, p.cpi, ipc, "pipeline")
    } else {
        (
            mem.instruction_count,
            mem.total_program_cycles(),
            Some(mem.extra_cycles),
            mem.overall_cpi(),
            mem.ipc(),
            "serial",
        )
    };

    let mut s = String::from("{\n");
    s.push_str(&format!("  \"file\": \"{}\",\n", json_escape(file)));
    match exit_code {
        Some(c) => s.push_str(&format!("  \"exit_code\": {c},\n")),
        None => s.push_str("  \"exit_code\": null,\n"),
    }
    s.push_str(&format!("  \"instructions\": {instr},\n"));
    s.push_str(&format!("  \"clock_model\": \"{clock_model}\",\n"));
    s.push_str(&format!("  \"total_cycles\": {total_cyc},\n"));
    if let Some(base_cyc) = base_cyc {
        s.push_str(&format!("  \"base_cycles\": {base_cyc},\n"));
        s.push_str(&format!(
            "  \"cache_cycles\": {},\n",
            total_cyc.saturating_sub(base_cyc)
        ));
    }
    s.push_str(&format!("  \"cpi\": {cpi:.4},\n"));
    s.push_str(&format!("  \"ipc\": {ipc:.4},\n"));
    if let Some(p) = pipeline {
        s.push_str("  \"pipeline\": {\n");
        s.push_str(&format!("    \"enabled\": {},\n", p.enabled));
        s.push_str(&format!("    \"committed\": {},\n", p.committed));
        s.push_str(&format!("    \"cycles\": {},\n", p.cycles));
        s.push_str(&format!("    \"stalls\": {},\n", p.stalls));
        s.push_str(&format!("    \"flushes\": {},\n", p.flushes));
        s.push_str(&format!("    \"cpi\": {:.4}\n", p.cpi));
        s.push_str("  },\n");
    }

    s.push_str("  \"icache\": ");
    append_cache_json(&mut s, &mem.icache, "I-Cache L1", instr, mem.icache_amat());
    s.push_str(",\n");

    s.push_str("  \"dcache\": ");
    append_cache_json(&mut s, &mem.dcache, "D-Cache L1", instr, mem.dcache_amat());

    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let name = format!("L{} (Unified)", i + 2);
        let key = format!("l{}", i + 2);
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 {
            lvl.config.hit_latency as f64
        } else {
            lvl.stats.total_cycles as f64 / total as f64
        };
        s.push_str(",\n");
        s.push_str(&format!("  \"{key}\": "));
        append_cache_json(&mut s, lvl, &name, instr, amat);
    }

    append_coherence_json(&mut s, mem, instr);
    if let Some(report) = sched {
        append_sched_json(&mut s, report);
    }
    s.push_str("\n}\n");
    s
}

/// One thread's numbers, as `(key, value)` pairs, in the order every output
/// format lists them. A time the thread has not reached yet is `None`.
fn thread_counters(t: &ThreadInfo) -> [(&'static str, Option<u64>); 13] {
    let st = &t.stats;
    [
        ("priority", Some(u64::from(t.priority))),
        ("arrival", Some(st.arrival)),
        ("first_run", st.first_run),
        ("finish", st.finish),
        ("response", st.response()),
        ("turnaround", st.turnaround()),
        ("run_ticks", Some(st.run_ticks)),
        ("waiting_ticks", Some(st.ready_ticks)),
        ("blocked_ticks", Some(st.blocked_ticks)),
        ("dispatches", Some(st.dispatches)),
        ("preemptions", Some(st.preemptions)),
        ("yields", Some(st.yields)),
        ("entry_pc", Some(u64::from(t.entry_pc))),
    ]
}

fn append_sched_json(s: &mut String, report: &SchedReport) {
    let cfg = &report.config;
    let sum = &report.summary;
    s.push_str(",\n  \"scheduler\": {\n");
    s.push_str(&format!("    \"policy\": \"{}\",\n", cfg.policy.as_str()));
    s.push_str(&format!("    \"quantum\": {},\n", cfg.quantum));
    s.push_str(&format!("    \"switch_cost\": {},\n", cfg.switch_cost));
    s.push_str(&format!("    \"ticks\": {},\n", sum.ticks));
    s.push_str(&format!("    \"context_switches\": {},\n", sum.context_switches));
    s.push_str(&format!("    \"preemptions\": {},\n", sum.preemptions));
    s.push_str(&format!("    \"avg_waiting\": {:.4},\n", sum.avg_waiting));
    s.push_str(&format!("    \"avg_response\": {:.4},\n", sum.avg_response));
    s.push_str(&format!("    \"avg_turnaround\": {:.4},\n", sum.avg_turnaround));
    s.push_str(&format!("    \"utilization\": {:.4},\n", sum.utilization));
    s.push_str("    \"cores\": [");
    for (core, ticks) in report.cores.iter().enumerate() {
        if core > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "\n      {{ \"core\": {core}, \"busy\": {}, \"switching\": {}, \"idle\": {} }}",
            ticks.busy, ticks.switching, ticks.idle
        ));
    }
    s.push_str("\n    ],\n    \"threads\": [");
    for (n, t) in report.threads.iter().enumerate() {
        if n > 0 {
            s.push(',');
        }
        s.push_str(&format!("\n      {{ \"tid\": {}", t.tid));
        match t.parent {
            Some(p) => s.push_str(&format!(", \"parent\": {p}")),
            None => s.push_str(", \"parent\": null"),
        }
        s.push_str(&format!(", \"state\": \"{}\"", t.state.label().to_ascii_lowercase()));
        for (key, value) in thread_counters(t) {
            match value {
                Some(v) => s.push_str(&format!(", \"{key}\": {v}")),
                None => s.push_str(&format!(", \"{key}\": null")),
            }
        }
        s.push_str(" }");
    }
    s.push_str("\n    ]\n  }");
}

/// What one core did on the coherence bus, as `(key, value)` pairs, in the
/// order every output format lists them.
fn coherence_counters(c: &CoherenceCoreStats) -> [(&'static str, u64); 10] {
    [
        ("bus_transactions", c.transactions()),
        ("bus_reads", c.bus_reads),
        ("bus_read_exclusive", c.bus_read_exclusive),
        ("bus_upgrades", c.bus_upgrades),
        ("bus_writes", c.bus_writes),
        ("invalidations_received", c.invalidations_received),
        ("flushes", c.flushes),
        ("cache_to_cache", c.cache_to_cache),
        ("coherence_misses", c.coherence_misses),
        ("bus_cycles", c.bus_cycles),
    ]
}

/// The cores with a private L1, each with its L1 pair and bus counters.
/// Empty with a shared L1.
fn private_cores(
    mem: &CacheController,
) -> Vec<(usize, &Cache, &Cache, CoherenceCoreStats)> {
    if !mem.coherence_protocol().private_l1() {
        return Vec::new();
    }
    let tracker = mem.coherence_tracker();
    (0..mem.core_count())
        .filter_map(|core| {
            let (icache, dcache) = mem.l1_of(core)?;
            let counters = tracker.cores.get(core).copied().unwrap_or_default();
            Some((core, icache, dcache, counters))
        })
        .collect()
}

fn append_coherence_json(s: &mut String, mem: &CacheController, instr: u64) {
    let cores = private_cores(mem);
    if cores.is_empty() {
        return;
    }
    let cfg = mem.coherence_config();
    s.push_str(",\n  \"coherence\": {\n");
    s.push_str(&format!("    \"protocol\": \"{}\",\n", cfg.protocol.as_str()));
    s.push_str(&format!("    \"snoop_latency\": {},\n", cfg.snoop_latency));
    s.push_str(&format!("    \"c2c_latency\": {},\n", cfg.cache_to_cache_latency));
    s.push_str("    \"cores\": [");
    for (n, (core, icache, dcache, counters)) in cores.iter().enumerate() {
        if n > 0 {
            s.push(',');
        }
        s.push_str(&format!("\n    {{\n    \"core\": {core},\n"));
        for (key, value) in coherence_counters(counters) {
            s.push_str(&format!("    \"{key}\": {value},\n"));
        }
        s.push_str("    \"icache\": ");
        append_cache_json(s, icache, &format!("Core {core} I-Cache L1"), instr, mem.l1_amat(icache));
        s.push_str(",\n    \"dcache\": ");
        append_cache_json(s, dcache, &format!("Core {core} D-Cache L1"), instr, mem.l1_amat(dcache));
        s.push_str("\n    }");
    }
    s.push_str("\n    ]\n  }");
}

pub(super) fn append_cache_json(
    s: &mut String,
    cache: &Cache,
    name: &str,
    instructions: u64,
    amat: f64,
) {
    let stats = &cache.stats;
    let cfg = &cache.config;
    let total = stats.total_accesses();
    let hit_rate = stats.hit_rate();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let mpki = stats.mpki(instructions);

    let mut hotspots: Vec<(u32, u64)> = stats.miss_pcs.iter().map(|(&k, &v)| (k, v)).collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(5);

    s.push_str("{\n");
    s.push_str(&format!("    \"name\": \"{name}\",\n"));
    s.push_str(&format!("    \"config\": {{ \"size\": {}, \"line_size\": {}, \"associativity\": {}, \"replacement\": \"{:?}\", \"write_policy\": \"{:?}\", \"hit_latency\": {}, \"miss_penalty\": {} }},\n",
        cfg.size, cfg.line_size, cfg.associativity, cfg.replacement, cfg.write_policy, cfg.hit_latency, cfg.miss_penalty));
    s.push_str(&format!("    \"hits\": {},\n", stats.hits));
    s.push_str(&format!("    \"misses\": {},\n", stats.misses));
    s.push_str(&format!("    \"total_accesses\": {},\n", total));
    s.push_str(&format!("    \"hit_rate_pct\": {hit_rate:.2},\n"));
    s.push_str(&format!("    \"miss_rate_pct\": {miss_rate:.2},\n"));
    s.push_str(&format!("    \"mpki\": {mpki:.2},\n"));
    s.push_str(&format!("    \"amat_cycles\": {amat:.4},\n"));
    s.push_str(&format!("    \"total_cycles\": {},\n", stats.total_cycles));
    s.push_str(&format!("    \"evictions\": {},\n", stats.evictions));
    s.push_str(&format!("    \"writebacks\": {},\n", stats.writebacks));
    s.push_str(&format!("    \"bytes_loaded\": {},\n", stats.bytes_loaded));
    s.push_str(&format!(
        "    \"ram_write_bytes\": {},\n",
        stats.ram_write_bytes
    ));
    s.push_str(&format!("    \"bytes_stored\": {},\n", stats.bytes_stored));

    // Top miss PCs
    s.push_str("    \"top_miss_pcs\": [");
    for (j, (pc, count)) in hotspots.iter().enumerate() {
        let pct = if stats.misses == 0 {
            0.0
        } else {
            *count as f64 / stats.misses as f64 * 100.0
        };
        if j > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!(
            "{{ \"pc\": \"0x{pc:08x}\", \"count\": {count}, \"pct\": {pct:.1} }}"
        ));
    }
    s.push_str("]\n  }");
}

pub(super) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

pub(super) fn flush_cpu_stdout(cpu: &mut Cpu, captured_stdout: &mut Vec<u8>) {
    if cpu.stdout.is_empty() {
        return;
    }
    captured_stdout.extend_from_slice(&cpu.stdout);
    use std::io::Write;
    let _ = std::io::stdout().write_all(&cpu.stdout);
    let _ = std::io::stdout().flush();
    cpu.stdout.clear();
}

pub(super) fn parse_u32_value(raw: &str, what: &str) -> Result<u32, String> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("invalid {what} '{raw}'"))
    } else {
        s.parse::<u32>()
            .map_err(|_| format!("invalid {what} '{raw}'"))
    }
}

pub fn parse_expect_reg_spec(spec: &str) -> Result<(u8, u32), String> {
    let (reg, value) = spec
        .split_once('=')
        .ok_or_else(|| format!("invalid --expect-reg '{spec}' (use reg=value)"))?;
    let name = reg.trim();
    let reg = parse_reg(name)
        .or_else(|| parse_numbered_reg(name))
        .ok_or_else(|| format!("invalid register '{name}'"))?;
    let value = parse_u32_value(value, "register value")?;
    Ok((reg, value))
}

/// `r0`..`r31`, the backend-neutral spelling used by non-RISC-V architectures.
///
/// The upper bound matters: the index is used to address a 32-entry register
/// file, so an out-of-range one has to be rejected here rather than panic later.
fn parse_numbered_reg(name: &str) -> Option<u8> {
    name.strip_prefix(['r', 'R'])?
        .parse::<u8>()
        .ok()
        .filter(|index| *index < 32)
}

pub fn parse_expect_mem_spec(spec: &str) -> Result<(u32, u32), String> {
    let (addr, value) = spec
        .split_once('=')
        .ok_or_else(|| format!("invalid --expect-mem '{spec}' (use addr=value)"))?;
    Ok((
        parse_u32_value(addr, "memory address")?,
        parse_u32_value(value, "memory value")?,
    ))
}

pub(super) fn validate_expectations(
    cpu: &Cpu,
    mem: &CacheController,
    captured_stdout: &[u8],
    expect_exit: Option<u32>,
    expect_stdout: Option<&str>,
    expect_regs: &[(u8, u32)],
    expect_mems: &[(u32, u32)],
) -> Result<(), String> {
    if let Some(expected) = expect_exit {
        match cpu.exit_code {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return Err(format!(
                    "exit-code assertion failed: expected {expected}, got {actual}"
                ));
            }
            None => {
                return Err(format!(
                    "exit-code assertion failed: expected {expected}, but program did not exit"
                ));
            }
        }
    }

    if let Some(expected) = expect_stdout {
        if captured_stdout != expected.as_bytes() {
            return Err(format!(
                "stdout assertion failed: expected {:?}, got {:?}",
                expected,
                String::from_utf8_lossy(captured_stdout)
            ));
        }
    }

    for &(reg, expected) in expect_regs {
        let actual = cpu.read(reg);
        if actual != expected {
            return Err(format!(
                "register assertion failed: {} expected 0x{expected:08X}, got 0x{actual:08X}",
                reg_name_cli(reg)
            ));
        }
    }

    for &(addr, expected) in expect_mems {
        let actual = mem
            .load32(addr)
            .map_err(|e| format!("memory assertion failed at 0x{addr:08X}: {e}"))?;
        if actual != expected {
            return Err(format!(
                "memory assertion failed: [0x{addr:08X}] expected 0x{expected:08X}, got 0x{actual:08X}"
            ));
        }
    }

    Ok(())
}

/// Trait-backed twin of [`validate_expectations`], for the architectures the
/// CLI drives through [`Machine`] instead of through the RV32 [`Cpu`].
pub(super) fn validate_machine_expectations(
    machine: &dyn Machine,
    snapshot: &MachineSnapshot,
    outcome: StepOutcome,
    expect_exit: Option<u32>,
    expect_stdout: Option<&str>,
    expect_regs: &[(u8, u32)],
    expect_mems: &[(u32, u32)],
) -> Result<(), String> {
    if let Some(expected) = expect_exit {
        let actual = match outcome {
            StepOutcome::Exited(code) => code as u32,
            StepOutcome::Halted => 0,
            _ => {
                return Err(format!(
                    "exit-code assertion failed: expected {expected}, but program did not exit"
                ));
            }
        };
        if actual != expected {
            return Err(format!(
                "exit-code assertion failed: expected {expected}, got {actual}"
            ));
        }
    }

    if let Some(expected) = expect_stdout {
        if snapshot.stdout != expected.as_bytes() {
            return Err(format!(
                "stdout assertion failed: expected {:?}, got {:?}",
                expected,
                String::from_utf8_lossy(&snapshot.stdout)
            ));
        }
    }

    for &(index, expected) in expect_regs {
        let register = snapshot.registers.get(index as usize).ok_or_else(|| {
            format!(
                "register assertion failed: {} has no register {index}",
                snapshot.architecture
            )
        })?;
        if register.value != u64::from(expected) {
            return Err(format!(
                "register assertion failed: {} expected 0x{expected:08X}, got 0x{:08X}",
                register.name, register.value
            ));
        }
    }

    for &(address, expected) in expect_mems {
        let bytes = machine
            .read_memory(u64::from(address), 4)
            .map_err(|e| format!("memory assertion failed at 0x{address:08X}: {e}"))?;
        let actual = u32::from_le_bytes(bytes.try_into().expect("read_memory returned 4 bytes"));
        if actual != expected {
            return Err(format!(
                "memory assertion failed: [0x{address:08X}] expected 0x{expected:08X}, got 0x{actual:08X}"
            ));
        }
    }

    Ok(())
}

/// Minimal run report for trait-driven backends: they have no cache or pipeline
/// statistics to report, so [`format_json`] does not apply.
pub(super) fn format_machine_json(architecture: &str, snapshot: &MachineSnapshot) -> String {
    format!(
        "{{\n  \"architecture\": \"{architecture}\",\n  \"instructions\": {},\n  \"pc\": {},\n  \"state\": \"{}\"\n}}\n",
        snapshot.instructions, snapshot.pc, snapshot.state
    )
}

pub(super) fn reg_name_cli(r: u8) -> &'static str {
    raven_engine::falcon::pipeline::sim::reg_name(r)
}

pub(super) fn hazard_type_label(hazard: HazardType) -> &'static str {
    match hazard {
        HazardType::ReadAfterWrite => "RAW",
        HazardType::LoadUse => "LOAD",
        HazardType::BranchFlush => "CTRL",
        HazardType::FunctionalUnitBusy => "FU",
        HazardType::MemoryLatency => "STALL",
        HazardType::WriteAfterWrite => "WAW",
        HazardType::WriteAfterRead => "WAR",
    }
}

pub(super) fn snapshot_pipeline_trace_step(
    state: &raven_engine::falcon::pipeline::PipelineSimState,
    commit: Option<&raven_engine::falcon::pipeline::sim::CommitInfo>,
) -> PipelineTraceStep {
    let mut stages = Vec::with_capacity(5);
    for (idx, stage) in Stage::all().iter().enumerate() {
        let slot = state.stages[idx].as_ref();
        let (
            pc,
            disasm,
            bubble,
            speculative,
            hazard,
            fu_cycles_left,
            if_stall_cycles,
            mem_stall_cycles,
        ) = if let Some(slot) = slot {
            (
                Some(slot.pc),
                slot.disasm.clone(),
                slot.is_bubble,
                slot.is_speculative,
                slot.hazard.map(hazard_type_label),
                slot.fu_cycles_left,
                slot.if_stall_cycles,
                slot.mem_stall_cycles,
            )
        } else {
            (None, String::new(), true, false, None, 0, 0, 0)
        };
        stages.push(PipelineTraceStage {
            stage: stage.label(),
            pc,
            disasm,
            bubble,
            speculative,
            hazard,
            fu_cycles_left,
            if_stall_cycles,
            mem_stall_cycles,
        });
    }

    let mut hazards = Vec::with_capacity(state.hazard_traces.len());
    for trace in &state.hazard_traces {
        let kind = match trace.kind {
            TraceKind::Hazard(h) => hazard_type_label(h),
            TraceKind::Forward => "FWD",
        };
        hazards.push(PipelineTraceHazard {
            kind,
            from_stage: Stage::all()[trace.from_stage].label(),
            to_stage: Stage::all()[trace.to_stage].label(),
            detail: trace.detail.clone(),
        });
    }

    PipelineTraceStep {
        cycle: state.cycle_count,
        committed_pc: commit.map(|c| c.pc),
        committed_class: commit.map(|c| c.class.label()),
        fetch_pc: state.fetch_pc,
        halted: state.halted,
        faulted: state.faulted,
        stages,
        hazards,
    }
}

pub(super) fn format_pipeline_trace_json(steps: &[PipelineTraceStep]) -> String {
    let mut s = String::from("{\n  \"format\": \"raven-pipeline-trace-v1\",\n  \"cycles\": [\n");
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            s.push_str(",\n");
        }
        s.push_str("    {\n");
        s.push_str(&format!("      \"cycle\": {},\n", step.cycle));
        match step.committed_pc {
            Some(pc) => s.push_str(&format!("      \"committed_pc\": \"0x{pc:08X}\",\n")),
            None => s.push_str("      \"committed_pc\": null,\n"),
        }
        match step.committed_class {
            Some(class) => s.push_str(&format!(
                "      \"committed_class\": \"{}\",\n",
                json_escape(class)
            )),
            None => s.push_str("      \"committed_class\": null,\n"),
        }
        s.push_str(&format!(
            "      \"fetch_pc\": \"0x{:08X}\",\n",
            step.fetch_pc
        ));
        s.push_str(&format!("      \"halted\": {},\n", step.halted));
        s.push_str(&format!("      \"faulted\": {},\n", step.faulted));
        s.push_str("      \"stages\": [\n");
        for (j, stage) in step.stages.iter().enumerate() {
            if j > 0 {
                s.push_str(",\n");
            }
            s.push_str("        {");
            s.push_str(&format!("\"stage\": \"{}\", ", stage.stage));
            match stage.pc {
                Some(pc) => s.push_str(&format!("\"pc\": \"0x{pc:08X}\", ")),
                None => s.push_str("\"pc\": null, "),
            }
            s.push_str(&format!(
                "\"disasm\": \"{}\", \"bubble\": {}, \"speculative\": {}, ",
                json_escape(&stage.disasm),
                stage.bubble,
                stage.speculative
            ));
            match stage.hazard {
                Some(hazard) => s.push_str(&format!("\"hazard\": \"{}\", ", hazard)),
                None => s.push_str("\"hazard\": null, "),
            }
            s.push_str(&format!(
                "\"fu_cycles_left\": {}, \"if_stall_cycles\": {}, \"mem_stall_cycles\": {}",
                stage.fu_cycles_left, stage.if_stall_cycles, stage.mem_stall_cycles
            ));
            s.push('}');
        }
        s.push_str("\n      ],\n");
        s.push_str("      \"hazards\": [\n");
        for (j, hazard) in step.hazards.iter().enumerate() {
            if j > 0 {
                s.push_str(",\n");
            }
            s.push_str(&format!(
                "        {{\"kind\": \"{}\", \"from\": \"{}\", \"to\": \"{}\", \"detail\": \"{}\"}}",
                hazard.kind,
                hazard.from_stage,
                hazard.to_stage,
                json_escape(&hazard.detail)
            ));
        }
        s.push_str("\n      ]\n");
        s.push_str("    }");
    }
    s.push_str("\n  ]\n}\n");
    s
}

// ── fstats output (compatible with TUI import) ────────────────────────────────

pub(super) fn format_rstats(
    mem: &CacheController,
    file: &str,
    exit_code: Option<u32>,
    pipeline: Option<PipelineReport>,
    sched: Option<&SchedReport>,
) -> String {
    let (instr, total_cyc, base_cyc, cpi, ipc, clock_model) = if let Some(p) = pipeline {
        let ipc = if p.cycles > 0 {
            p.committed as f64 / p.cycles as f64
        } else {
            0.0
        };
        (p.committed, p.cycles, None, p.cpi, ipc, "pipeline")
    } else {
        (
            mem.instruction_count,
            mem.total_program_cycles(),
            Some(mem.extra_cycles),
            mem.overall_cpi(),
            mem.ipc(),
            "serial",
        )
    };

    let mut s = String::from("# Raven Results v1\n");
    s.push_str("\n[program]\n");
    s.push_str(&format!("label={file}\n"));
    s.push_str(&format!("prog.clock_model={clock_model}\n"));
    if let Some(code) = exit_code {
        s.push_str(&format!("prog.exit_code={code}\n"));
    }
    s.push_str(&format!("prog.instructions={instr}\n"));
    s.push_str(&format!("prog.total_cycles={total_cyc}\n"));
    if let Some(base_cyc) = base_cyc {
        s.push_str(&format!("prog.base_cycles={base_cyc}\n"));
        s.push_str(&format!(
            "prog.cache_cycles={}\n",
            total_cyc.saturating_sub(base_cyc)
        ));
    }
    s.push_str(&format!("prog.cpi={cpi:.4}\n"));
    s.push_str(&format!("prog.ipc={ipc:.4}\n"));
    if let Some(p) = pipeline {
        s.push_str("\n[pipeline]\n");
        s.push_str("pipeline.enabled=true\n");
        s.push_str(&format!("pipeline.committed={}\n", p.committed));
        s.push_str(&format!("pipeline.cycles={}\n", p.cycles));
        s.push_str(&format!("pipeline.stalls={}\n", p.stalls));
        s.push_str(&format!("pipeline.flushes={}\n", p.flushes));
        s.push_str(&format!("pipeline.cpi={:.4}\n", p.cpi));
    }

    s.push_str("\n[cache]\n");
    s.push_str(&format!("extra_levels={}\n", mem.extra_levels.len()));
    write_cache_level_fstats(
        &mut s,
        "icache",
        "I-Cache L1",
        &mem.icache,
        instr,
        mem.icache_amat(),
    );
    write_cache_level_fstats(
        &mut s,
        "dcache",
        "D-Cache L1",
        &mem.dcache,
        instr,
        mem.dcache_amat(),
    );
    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let key = format!("l{}", i + 2);
        let name = format!("{} Unified", CacheController::extra_level_name(i));
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 {
            lvl.config.hit_latency as f64
        } else {
            lvl.stats.total_cycles as f64 / total as f64
        };
        write_cache_level_fstats(&mut s, &key, &name, lvl, instr, amat);
    }

    let cores = private_cores(mem);
    if !cores.is_empty() {
        let cfg = mem.coherence_config();
        s.push_str(&format!("coherence.protocol={}\n", cfg.protocol.as_str()));
        s.push_str(&format!("coherence.snoop_latency={}\n", cfg.snoop_latency));
        s.push_str(&format!("coherence.c2c_latency={}\n", cfg.cache_to_cache_latency));
        s.push_str(&format!("coherence.cores={}\n", cores.len()));
        for (core, icache, dcache, counters) in &cores {
            for (key, value) in coherence_counters(counters) {
                s.push_str(&format!("core{core}.{key}={value}\n"));
            }
            let amat_i = mem.l1_amat(icache);
            let amat_d = mem.l1_amat(dcache);
            let name_i = format!("Core {core} I-Cache L1");
            let name_d = format!("Core {core} D-Cache L1");
            write_cache_level_fstats(&mut s, &format!("core{core}.icache"), &name_i, icache, instr, amat_i);
            write_cache_level_fstats(&mut s, &format!("core{core}.dcache"), &name_d, dcache, instr, amat_d);
        }
    }

    // Miss hotspots (I-cache)
    let mut hotspots: Vec<(u32, u64)> = mem
        .icache
        .stats
        .miss_pcs
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(10);
    s.push_str(&format!("miss_hotspot_count={}\n", hotspots.len()));
    for (i, (pc, count)) in hotspots.iter().enumerate() {
        s.push_str(&format!("miss_hotspot.{i}.pc=0x{pc:08x}\n"));
        s.push_str(&format!("miss_hotspot.{i}.count={count}\n"));
    }

    // Hit-rate history
    let hist_i: Vec<(f64, f64)> = mem.icache.stats.history.iter().cloned().collect();
    let hist_d: Vec<(f64, f64)> = mem.dcache.stats.history.iter().cloned().collect();
    s.push_str(&format!("history_i_count={}\n", hist_i.len()));
    for (i, (x, y)) in hist_i.iter().enumerate() {
        s.push_str(&format!("history_i.{i}={x}:{y}\n"));
    }
    s.push_str(&format!("history_d_count={}\n", hist_d.len()));
    for (i, (x, y)) in hist_d.iter().enumerate() {
        s.push_str(&format!("history_d.{i}={x}:{y}\n"));
    }

    // CPI config (defaults)
    s.push_str("cpi.alu=1\ncpi.mul=3\ncpi.div=20\ncpi.load=0\ncpi.store=0\n");
    s.push_str("cpi.branch_taken=3\ncpi.branch_not_taken=1\ncpi.jump=2\ncpi.system=10\n");

    if let Some(report) = sched {
        let cfg = &report.config;
        let sum = &report.summary;
        s.push_str("\n[sched]\n");
        s.push_str(&format!("sched.policy={}\n", cfg.policy.as_str()));
        s.push_str(&format!("sched.quantum={}\n", cfg.quantum));
        s.push_str(&format!("sched.switch_cost={}\n", cfg.switch_cost));
        s.push_str(&format!("sched.ticks={}\n", sum.ticks));
        s.push_str(&format!("sched.context_switches={}\n", sum.context_switches));
        s.push_str(&format!("sched.preemptions={}\n", sum.preemptions));
        s.push_str(&format!("sched.avg_waiting={:.4}\n", sum.avg_waiting));
        s.push_str(&format!("sched.avg_response={:.4}\n", sum.avg_response));
        s.push_str(&format!("sched.avg_turnaround={:.4}\n", sum.avg_turnaround));
        s.push_str(&format!("sched.utilization={:.4}\n", sum.utilization));
        s.push_str(&format!("sched.cores={}\n", report.cores.len()));
        for (core, ticks) in report.cores.iter().enumerate() {
            s.push_str(&format!("core{core}.busy_ticks={}\n", ticks.busy));
            s.push_str(&format!("core{core}.switch_ticks={}\n", ticks.switching));
            s.push_str(&format!("core{core}.idle_ticks={}\n", ticks.idle));
        }
        s.push_str(&format!("sched.threads={}\n", report.threads.len()));
        for t in &report.threads {
            let tid = t.tid;
            if let Some(p) = t.parent {
                s.push_str(&format!("thread{tid}.parent={p}\n"));
            }
            s.push_str(&format!(
                "thread{tid}.state={}\n",
                t.state.label().to_ascii_lowercase()
            ));
            for (key, value) in thread_counters(t) {
                if let Some(v) = value {
                    s.push_str(&format!("thread{tid}.{key}={v}\n"));
                }
            }
        }
    }
    s
}

pub(super) fn write_cache_level_fstats(
    s: &mut String,
    key: &str,
    name: &str,
    cache: &Cache,
    _instr: u64,
    amat: f64,
) {
    let st = &cache.stats;
    let cfg = &cache.config;
    s.push_str(&format!("{key}.name={name}\n"));
    s.push_str(&format!("{key}.size={}\n", cfg.size));
    s.push_str(&format!("{key}.line_size={}\n", cfg.line_size));
    s.push_str(&format!("{key}.associativity={}\n", cfg.associativity));
    s.push_str(&format!("{key}.replacement={:?}\n", cfg.replacement));
    s.push_str(&format!("{key}.write_policy={:?}\n", cfg.write_policy));
    s.push_str(&format!("{key}.hit_latency={}\n", cfg.hit_latency));
    s.push_str(&format!("{key}.miss_penalty={}\n", cfg.miss_penalty));
    s.push_str(&format!("{key}.hits={}\n", st.hits));
    s.push_str(&format!("{key}.misses={}\n", st.misses));
    s.push_str(&format!("{key}.evictions={}\n", st.evictions));
    s.push_str(&format!("{key}.writebacks={}\n", st.writebacks));
    s.push_str(&format!("{key}.bytes_loaded={}\n", st.bytes_loaded));
    s.push_str(&format!("{key}.bytes_stored={}\n", st.bytes_stored));
    s.push_str(&format!("{key}.total_cycles={}\n", st.total_cycles));
    s.push_str(&format!("{key}.ram_write_bytes={}\n", st.ram_write_bytes));
    s.push_str(&format!("{key}.amat={amat:.4}\n"));
}

// ── CSV output ────────────────────────────────────────────────────────────────

pub(super) fn format_csv(
    mem: &CacheController,
    file: &str,
    pipeline: Option<PipelineReport>,
    sched: Option<&SchedReport>,
) -> String {
    let (instr, total_cyc, base_cyc, cpi, ipc, clock_model) = if let Some(p) = pipeline {
        let ipc = if p.cycles > 0 {
            p.committed as f64 / p.cycles as f64
        } else {
            0.0
        };
        (p.committed, p.cycles, None, p.cpi, ipc, "pipeline")
    } else {
        (
            mem.instruction_count,
            mem.total_program_cycles(),
            Some(mem.extra_cycles),
            mem.overall_cpi(),
            mem.ipc(),
            "serial",
        )
    };

    let mut s = String::new();
    s.push_str(&format!("# {file}\n"));
    s.push_str("PROGRAM SUMMARY\n");
    if let Some(base_cyc) = base_cyc {
        s.push_str("Clock Model,Instructions,Total Cycles,Base Cycles,Cache Cycles,CPI,IPC\n");
        s.push_str(&format!(
            "{clock_model},{instr},{total_cyc},{base_cyc},{},{cpi:.4},{ipc:.4}\n",
            total_cyc.saturating_sub(base_cyc)
        ));
    } else {
        s.push_str("Clock Model,Instructions,Total Cycles,CPI,IPC\n");
        s.push_str(&format!(
            "{clock_model},{instr},{total_cyc},{cpi:.4},{ipc:.4}\n"
        ));
    }
    s.push('\n');

    if let Some(p) = pipeline {
        s.push_str("PIPELINE SUMMARY\n");
        s.push_str("Committed,Cycles,Stalls,Flushes,CPI\n");
        s.push_str(&format!(
            "{},{},{},{},{:.4}\n\n",
            p.committed, p.cycles, p.stalls, p.flushes, p.cpi
        ));
    }

    s.push_str("CACHE LEVELS\n");
    s.push_str("Level,Hits,Misses,Total Accesses,Hit Rate (%),Miss Rate (%),MPKI,AMAT (cycles),Evictions,Writebacks,RAM Reads (B),RAM Writes (B),Total Cycles\n");

    let write_level = |s: &mut String, label: &str, cache: &Cache, instructions: u64, amat: f64| {
        let st = &cache.stats;
        let total = st.total_accesses();
        let hit_rate = st.hit_rate();
        let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
        let mpki = st.mpki(instructions);
        s.push_str(&format!(
            "{label},{},{},{},{:.1},{:.1},{:.2},{:.4},{},{},{},{},{}\n",
            st.hits,
            st.misses,
            total,
            hit_rate,
            miss_rate,
            mpki,
            amat,
            st.evictions,
            st.writebacks,
            st.bytes_loaded,
            st.ram_write_bytes,
            st.total_cycles
        ));
    };

    write_level(&mut s, "I-Cache L1", &mem.icache, instr, mem.icache_amat());
    write_level(&mut s, "D-Cache L1", &mem.dcache, instr, mem.dcache_amat());
    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let label = format!("L{} Unified", i + 2);
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 {
            lvl.config.hit_latency as f64
        } else {
            lvl.stats.total_cycles as f64 / total as f64
        };
        write_level(&mut s, &label, lvl, instr, amat);
    }

    let cores = private_cores(mem);
    if !cores.is_empty() {
        for (core, icache, dcache, _) in &cores {
            let label_i = format!("Core {core} I-Cache L1");
            let label_d = format!("Core {core} D-Cache L1");
            write_level(&mut s, &label_i, icache, instr, mem.l1_amat(icache));
            write_level(&mut s, &label_d, dcache, instr, mem.l1_amat(dcache));
        }
        s.push('\n');
        s.push_str(&format!(
            "COHERENCE ({})\n",
            mem.coherence_config().protocol.as_str()
        ));
        s.push_str("Core,Bus Transactions,BusRd,BusRdX,BusUpgr,BusWr,Invalidations Received,Flushes,Cache-to-Cache,Coherence Misses,Bus Cycles\n");
        for (core, _, _, counters) in &cores {
            let values: Vec<String> = coherence_counters(counters)
                .iter()
                .map(|(_, v)| v.to_string())
                .collect();
            s.push_str(&format!("{core},{}\n", values.join(",")));
        }
    }

    // Miss hotspots
    let mut hotspots: Vec<(u32, u64)> = mem
        .icache
        .stats
        .miss_pcs
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(5);
    if !hotspots.is_empty() {
        s.push('\n');
        s.push_str("TOP I-CACHE MISS PCs\n");
        s.push_str("PC,Miss Count,% of Misses\n");
        let total_misses = mem.icache.stats.misses;
        for (pc, count) in &hotspots {
            let pct = if total_misses == 0 {
                0.0
            } else {
                *count as f64 / total_misses as f64 * 100.0
            };
            s.push_str(&format!("0x{pc:08x},{count},{pct:.1}\n"));
        }
    }

    if let Some(report) = sched {
        let cfg = &report.config;
        let sum = &report.summary;
        s.push('\n');
        s.push_str("SCHEDULER\n");
        s.push_str("Policy,Quantum,Switch Cost,Ticks,Context Switches,Preemptions,Avg Waiting,Avg Response,Avg Turnaround,Utilization (%)\n");
        s.push_str(&format!(
            "{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.1}\n",
            cfg.policy.as_str(),
            cfg.quantum,
            cfg.switch_cost,
            sum.ticks,
            sum.context_switches,
            sum.preemptions,
            sum.avg_waiting,
            sum.avg_response,
            sum.avg_turnaround,
            sum.utilization * 100.0
        ));
        s.push('\n');
        s.push_str("CORES\nCore,Busy Ticks,Switch Ticks,Idle Ticks\n");
        for (core, ticks) in report.cores.iter().enumerate() {
            s.push_str(&format!(
                "{core},{},{},{}\n",
                ticks.busy, ticks.switching, ticks.idle
            ));
        }
        s.push('\n');
        s.push_str("THREADS\n");
        s.push_str("TID,Parent,State,Priority,Arrival,First Run,Finish,Response,Turnaround,Run Ticks,Waiting Ticks,Blocked Ticks,Dispatches,Preemptions,Yields,Entry PC\n");
        for t in &report.threads {
            let cell = |v: Option<u64>| v.map_or_else(|| "-".to_string(), |v| v.to_string());
            let counters = thread_counters(t);
            let values: Vec<String> = counters[..12].iter().map(|(_, v)| cell(*v)).collect();
            s.push_str(&format!(
                "{},{},{},{},0x{:08x}\n",
                t.tid,
                cell(t.parent.map(u64::from)),
                t.state.label(),
                values.join(","),
                t.entry_pc
            ));
        }
    }

    s
}
