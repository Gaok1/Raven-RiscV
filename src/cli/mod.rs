mod loaders;
mod output;
use self::loaders::*;
use self::output::*;
pub use self::output::{parse_expect_reg_spec, parse_expect_mem_spec};

// cli.rs — headless CLI commands (build, run, export-config, import-config)

use crate::falcon::cache::{
    CacheConfig, InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy,
};
use crate::falcon::program::{load_bytes, load_elf};
use crate::falcon::registers::HartStartRequest;
use crate::falcon::{self, CacheController, Cpu};
use crate::ui::pipeline::sim::pipeline_tick;
use crate::ui::pipeline::{PipelineConfig, parse_pipeline_config, serialize_pipeline_config};
use crate::ui::{Console, CpiConfig};
use std::collections::HashMap;

const DEFAULT_MAX_CORES: usize = 1;

// ── Public types ──────────────────────────────────────────────────────────────

pub struct RunArgs {
    pub file: String,
    pub cache_config: Option<String>,
    /// Path to a `.rcfg` sim-settings file (CPI + cache_enabled).
    pub settings: Option<String>,
    pub pipeline: bool,
    pub pipeline_config: Option<String>,
    pub pipeline_trace_out: Option<String>,
    pub output: Option<String>,
    /// When true, simulation stats are not written/printed (program stdout still shown).
    pub nout: bool,
    pub format: OutputFormat,
    /// None = not specified on CLI (use rcfg mem_mb, else default 16 MB).
    pub mem_size: Option<usize>,
    pub max_cycles: u64,
    pub max_cores: usize,
    pub expect_exit: Option<u32>,
    pub expect_stdout: Option<String>,
    pub expect_regs: Vec<(u8, u32)>,
    pub expect_mems: Vec<(u32, u32)>,
}

pub enum OutputFormat {
    Json,
    Fstats,
    Csv,
}

#[derive(Clone, Copy)]
struct PipelineReport {
    enabled: bool,
    committed: u64,
    cycles: u64,
    stalls: u64,
    flushes: u64,
    cpi: f64,
}

#[derive(Clone)]
struct PipelineTraceStep {
    cycle: u64,
    committed_pc: Option<u32>,
    committed_class: Option<&'static str>,
    fetch_pc: u32,
    halted: bool,
    faulted: bool,
    stages: Vec<PipelineTraceStage>,
    hazards: Vec<PipelineTraceHazard>,
}

#[derive(Clone)]
struct PipelineTraceStage {
    stage: &'static str,
    pc: Option<u32>,
    disasm: String,
    bubble: bool,
    speculative: bool,
    hazard: Option<&'static str>,
    fu_cycles_left: u8,
    if_stall_cycles: u8,
    mem_stall_cycles: u8,
}

#[derive(Clone)]
struct PipelineTraceHazard {
    kind: &'static str,
    from_stage: &'static str,
    to_stage: &'static str,
    detail: String,
}

struct HeadlessHart {
    hart_id: u32,
    cpu: Cpu,
    active: bool,
    paused: bool,
}

// ── raven build ───────────────────────────────────────────────────────────────

/// Assemble `file` and optionally write a FALC binary.
/// `nout = true` → check-only (no output file written).
pub fn build_program(file: &str, output: Option<&str>, nout: bool) -> Result<(), String> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("cannot read '{}': {e}", file))?;

    let prog = crate::falcon::asm::assemble(&src, 0x0)
        .map_err(|e| {
            eprintln!("error: {}:{}: {}", file, e.line + 1, e.msg);
            String::new() // sentinel; we already printed
        })
        .map_err(|_| String::new())?; // propagate empty sentinel

    let instr = prog.text.len();
    let data = prog.data.len();
    eprintln!(
        "{}: {} instruction{}, {} data byte{}",
        file,
        instr,
        if instr == 1 { "" } else { "s" },
        data,
        if data == 1 { "" } else { "s" },
    );

    if nout {
        return Ok(());
    }

    let out_path = output
        .map(str::to_string)
        .unwrap_or_else(|| replace_ext(file, "bin"));
    write_falc(&prog, &out_path)?;
    eprintln!("  → {out_path}");
    Ok(())
}

// ── raven import-config ───────────────────────────────────────────────────────

/// Parse and validate a .fcache file, print a human-readable summary.
/// Optionally re-export the normalized config to `output`.
pub fn import_config(file: &str, output: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read '{}': {e}", file))?;
    let (icfg, dcfg, extra) = parse_cache_configs(&text)?;

    icfg.validate()
        .map_err(|e| format!("I-cache config error: {e}"))?;
    dcfg.validate()
        .map_err(|e| format!("D-cache config error: {e}"))?;
    for (i, cfg) in extra.iter().enumerate() {
        cfg.validate()
            .map_err(|e| format!("L{} config error: {e}", i + 2))?;
    }

    eprintln!(
        "{}: valid — {} cache level{}",
        file,
        1 + extra.len(),
        if extra.is_empty() { "" } else { "s" }
    );
    print_config_row("  I-Cache L1", &icfg);
    print_config_row("  D-Cache L1", &dcfg);
    for (i, cfg) in extra.iter().enumerate() {
        print_config_row(&format!("  L{} Unified ", i + 2), cfg);
    }

    if let Some(out) = output {
        let normalized = serialize_cache_configs(&icfg, &dcfg, &extra);
        std::fs::write(out, normalized).map_err(|e| format!("cannot write '{}': {e}", out))?;
        eprintln!("  → {out}");
    }
    Ok(())
}

fn print_config_row(label: &str, cfg: &CacheConfig) {
    eprintln!(
        "  {:<14}  {:>5}KB  {}B lines  {}-way  {:?}/{:?}  lat={} pen={}",
        label,
        cfg.size / 1024,
        cfg.line_size,
        cfg.associativity,
        cfg.replacement,
        cfg.write_policy,
        cfg.hit_latency,
        cfg.miss_penalty,
    );
}

// ── raven run ────────────────────────────────────────────────────────────────

pub fn run_headless(args: RunArgs) -> Result<(), String> {
    // ── 1. Load cache config ─────────────────────────────────────────────────
    let (icfg, dcfg, extra_cfgs) = if let Some(path) = &args.cache_config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read cache config '{}': {e}", path))?;
        parse_cache_configs(&text)?
    } else {
        (CacheConfig::default(), CacheConfig::default(), vec![])
    };

    icfg.validate()
        .map_err(|e| format!("I-cache config error: {e}"))?;
    dcfg.validate()
        .map_err(|e| format!("D-cache config error: {e}"))?;
    for (i, cfg) in extra_cfgs.iter().enumerate() {
        cfg.validate()
            .map_err(|e| format!("L{} cache config error: {e}", i + 2))?;
    }

    // ── 2. Apply sim settings (.rcfg) ────────────────────────────────────────
    let (cpi_config, cache_enabled, settings_max_cores, settings_mem_size) =
        if let Some(path) = &args.settings {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("Cannot read settings '{}': {e}", path))?;
            parse_rcfg_cli_full(&text)?
        } else {
            (default_cpi_config(), true, DEFAULT_MAX_CORES, 16 * 1024 * 1024)
        };
    let max_cores = if args.max_cores == 0 {
        settings_max_cores
    } else {
        args.max_cores
    };
    // --mem flag overrides rcfg mem_mb; rcfg overrides built-in default.
    let mem_size = args.mem_size.unwrap_or(settings_mem_size);
    if !(1..=32).contains(&max_cores) {
        return Err(format!("invalid core count {max_cores} (use 1..=32)"));
    }

    // ── 3. Set up simulation ─────────────────────────────────────────────────
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(icfg, dcfg, extra_cfgs, mem_size);
    mem.bypass = !cache_enabled;
    let mut console = Console::default();
    let mut captured_stdout: Vec<u8> = Vec::new();

    // SP = top of RAM
    cpu.write(2, mem_size as u32);

    // ── 3. Load program ──────────────────────────────────────────────────────
    let file_bytes =
        std::fs::read(&args.file).map_err(|e| format!("Cannot read '{}': {e}", args.file))?;

    let file_name = std::path::Path::new(&args.file)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if is_elf(&file_bytes) {
        let info =
            load_elf(&file_bytes, &mut mem.ram).map_err(|e| format!("ELF load error: {e}"))?;
        cpu.pc = info.entry;
        cpu.heap_break = info.heap_start;
        cpu.write(2, mem_size as u32); // restore SP after ELF load
    } else if is_falc(&file_bytes) {
        load_falc(&file_bytes, &mut cpu, &mut mem, mem_size)?;
    } else if looks_like_text(&file_bytes) {
        load_asm_text(&file_bytes, &mut cpu, &mut mem)?;
    } else {
        // Flat binary: load at 0x0
        load_bytes(&mut mem.ram, 0, &file_bytes).map_err(|e| format!("Binary load error: {e}"))?;
        cpu.pc = 0;
        let bss_end = file_bytes.len() as u32;
        cpu.heap_break = (bss_end.wrapping_add(15)) & !15;
    }

    mem.invalidate_all();
    mem.reset_stats();

    if args.pipeline && max_cores > 1 {
        return Err(
            "headless multi-hart execution is not implemented for --pipeline yet; use sequential mode or --cores 1"
                .to_string(),
        );
    }

    let pipeline_report = if args.pipeline {
        let pcfg = if let Some(path) = &args.pipeline_config {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("Cannot read pipeline config '{}': {e}", path))?;
            parse_pipeline_config(&text)?
        } else {
            PipelineConfig::default()
        };
        let (report, trace_steps) = run_headless_pipeline(
            &mut cpu,
            &mut mem,
            &mut console,
            &cpi_config,
            &pcfg,
            args.max_cycles,
            args.pipeline_trace_out.is_some(),
        )?;
        if let Some(path) = &args.pipeline_trace_out {
            std::fs::write(path, format_pipeline_trace_json(&trace_steps))
                .map_err(|e| format!("Cannot write '{}': {e}", path))?;
        }
        Some(report)
    } else {
        if args.pipeline_trace_out.is_some() {
            return Err("--pipeline-trace-out requires --pipeline".to_string());
        }
        run_headless_sequential(
            &mut cpu,
            &mut mem,
            &mut console,
            args.max_cycles,
            &mut captured_stdout,
            max_cores,
        )?;
        None
    };

    // Print remaining program stdout
    flush_cpu_stdout(&mut cpu, &mut captured_stdout);
    // Print any console errors from the simulation
    for line in &console.lines {
        if line.is_error() {
            eprintln!("raven: {}", line.text);
        }
    }

    validate_expectations(
        &cpu,
        &mem,
        &captured_stdout,
        args.expect_exit,
        args.expect_stdout.as_deref(),
        &args.expect_regs,
        &args.expect_mems,
    )?;

    // ── 5. Serialize and output ──────────────────────────────────────────────
    if !args.nout {
        let exit_code = if pipeline_report.as_ref().map(|r| !r.enabled).unwrap_or(true) {
            cpu.exit_code
        } else {
            cpu.exit_code
        };
        let text = match args.format {
            OutputFormat::Json => format_json(&mem, &file_name, exit_code, pipeline_report),
            OutputFormat::Fstats => format_fstats(&mem, &file_name, exit_code, pipeline_report),
            OutputFormat::Csv => format_csv(&mem, &file_name, pipeline_report),
        };
        match &args.output {
            Some(path) => {
                std::fs::write(path, &text).map_err(|e| format!("Cannot write '{}': {e}", path))?
            }
            None => print!("{text}"),
        }
    }

    Ok(())
}

/// Serialize the default cache config to a `.fcache` file (for `--export-config`).
pub fn export_default_config(output: Option<&str>) -> Result<(), String> {
    let text = serialize_cache_configs(&CacheConfig::default(), &CacheConfig::default(), &[]);
    match output {
        Some(path) => {
            std::fs::write(path, &text).map_err(|e| format!("Cannot write '{}': {e}", path))
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

// ── raven export-settings / import-settings ───────────────────────────────────

/// Default `.rcfg` content (same defaults as the TUI).
fn default_rcfg_text() -> String {
    let mut s = String::from(
        "# Raven Sim Config v2\ncache_enabled=true\nmem_mb=16\nmax_cores=1\n\n# CPI (cycles per instruction)\n",
    );
    s.push_str("cpi.alu=1\ncpi.mul=3\ncpi.div=20\ncpi.load=0\ncpi.store=0\n");
    s.push_str("cpi.branch_taken=3\ncpi.branch_not_taken=1\ncpi.jump=2\ncpi.system=10\ncpi.fp=5\n");
    s
}

/// Serialize the default sim settings to a `.rcfg` file.
pub fn export_sim_settings(output: Option<&str>) -> Result<(), String> {
    let text = default_rcfg_text();
    match output {
        Some(path) => {
            std::fs::write(path, &text).map_err(|e| format!("Cannot write '{}': {e}", path))
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

/// Parse and validate a `.rcfg` file, print a human-readable summary.
/// Optionally re-export the normalized settings to `output`.
pub fn import_sim_settings(file: &str, output: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read '{}': {e}", file))?;
    let mut cache_enabled = true;
    let mut max_cores = DEFAULT_MAX_CORES;
    let mut mem_mb: usize = 16;
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    if let Some(v) = kv.get("cache_enabled") {
        cache_enabled = v == "true";
    }
    if let Some(v) = kv.get("max_cores") {
        max_cores = v
            .parse::<usize>()
            .map_err(|_| "invalid max_cores: expected integer".to_string())?;
        if !(1..=32).contains(&max_cores) {
            return Err(format!("invalid max_cores {} (use 1..=32)", max_cores));
        }
    }
    if let Some(v) = kv.get("mem_mb") {
        mem_mb = v
            .parse::<usize>()
            .map_err(|_| "invalid mem_mb: expected integer".to_string())?;
        if !(1..=4096).contains(&mem_mb) {
            return Err(format!("invalid mem_mb {} (use 1..=4096)", mem_mb));
        }
    }
    let cpi_keys = [
        "alu",
        "mul",
        "div",
        "load",
        "store",
        "branch_taken",
        "branch_not_taken",
        "jump",
        "system",
        "fp",
    ];
    let defaults = [1u64, 3, 20, 0, 0, 3, 1, 2, 10, 5];
    eprintln!("{}: valid", file);
    eprintln!("  cache_enabled = {}", cache_enabled);
    eprintln!("  mem_mb        = {}", mem_mb);
    eprintln!("  max_cores     = {}", max_cores);
    for (key, def) in cpi_keys.iter().zip(defaults.iter()) {
        let val: u64 = kv
            .get(&format!("cpi.{}", key))
            .and_then(|v| v.parse().ok())
            .unwrap_or(*def);
        eprintln!("  cpi.{:<20} = {}", key, val);
    }
    if let Some(out) = output {
        // Re-serialize with parsed values
        let mut out_text = String::from("# Raven Sim Config v2\n");
        out_text.push_str(&format!(
            "cache_enabled={}\nmem_mb={}\nmax_cores={}\n\n# CPI (cycles per instruction)\n",
            cache_enabled, mem_mb, max_cores
        ));
        for (key, def) in cpi_keys.iter().zip(defaults.iter()) {
            let val: u64 = kv
                .get(&format!("cpi.{}", key))
                .and_then(|v| v.parse().ok())
                .unwrap_or(*def);
            out_text.push_str(&format!("cpi.{}={}\n", key, val));
        }
        std::fs::write(out, out_text).map_err(|e| format!("Cannot write '{}': {e}", out))?;
        eprintln!("  → {out}");
    }
    Ok(())
}

/// Parse a `.rcfg` and return `cache_enabled` (only field relevant to headless sim).
fn default_cpi_config() -> CpiConfig {
    CpiConfig {
        alu: 1,
        mul: 3,
        div: 20,
        load: 0,
        store: 0,
        branch_taken: 3,
        branch_not_taken: 1,
        jump: 2,
        system: 10,
        fp: 5,
    }
}

fn parse_rcfg_cli_full(text: &str) -> Result<(CpiConfig, bool, usize, usize), String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
        }
    }

    let defaults = default_cpi_config();
    let parse_u64 = |key: &str, default: u64| -> Result<u64, String> {
        match map.get(key) {
            Some(v) => v
                .parse::<u64>()
                .map_err(|_| format!("Invalid {key}: expected integer")),
            None => Ok(default),
        }
    };

    let cpi = CpiConfig {
        alu: parse_u64("cpi.alu", defaults.alu)?,
        mul: parse_u64("cpi.mul", defaults.mul)?,
        div: parse_u64("cpi.div", defaults.div)?,
        load: parse_u64("cpi.load", defaults.load)?,
        store: parse_u64("cpi.store", defaults.store)?,
        branch_taken: parse_u64("cpi.branch_taken", defaults.branch_taken)?,
        branch_not_taken: parse_u64("cpi.branch_not_taken", defaults.branch_not_taken)?,
        jump: parse_u64("cpi.jump", defaults.jump)?,
        system: parse_u64("cpi.system", defaults.system)?,
        fp: parse_u64("cpi.fp", defaults.fp)?,
    };
    let cache_enabled = map
        .get("cache_enabled")
        .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(true);
    let max_cores = match map.get("max_cores") {
        Some(v) => v
            .parse::<usize>()
            .map_err(|_| "Invalid max_cores: expected integer".to_string())?,
        None => DEFAULT_MAX_CORES,
    };
    if !(1..=32).contains(&max_cores) {
        return Err(format!("Invalid max_cores: {max_cores} (use 1..=32)"));
    }
    let mem_size = match map.get("mem_mb") {
        Some(v) => {
            let mb = v
                .parse::<usize>()
                .map_err(|_| "Invalid mem_mb: expected integer".to_string())?;
            if !(1..=4096).contains(&mb) {
                return Err(format!("Invalid mem_mb: {mb} (use 1..=4096)"));
            }
            mb * 1024 * 1024
        }
        None => 16 * 1024 * 1024,
    };
    Ok((cpi, cache_enabled, max_cores, mem_size))
}

fn classify_cpi_cycles_cli(pc: u32, cpu: &Cpu, mem: &CacheController, cpi: &CpiConfig) -> u64 {
    use crate::falcon::instruction::Instruction::*;
    let word = match mem.peek32(pc) {
        Ok(w) => w,
        Err(_) => return 1,
    };
    match crate::falcon::decoder::decode(word) {
        Ok(
            Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Sll { .. }
            | Srl { .. }
            | Sra { .. }
            | Slt { .. }
            | Sltu { .. }
            | Addi { .. }
            | Andi { .. }
            | Ori { .. }
            | Xori { .. }
            | Slti { .. }
            | Sltiu { .. }
            | Slli { .. }
            | Srli { .. }
            | Srai { .. }
            | Lui { .. }
            | Auipc { .. },
        ) => 1 + cpi.alu,
        Ok(Mul { .. } | Mulh { .. } | Mulhsu { .. } | Mulhu { .. }) => 1 + cpi.mul,
        Ok(Div { .. } | Divu { .. } | Rem { .. } | Remu { .. }) => 1 + cpi.div,
        Ok(Lb { .. } | Lh { .. } | Lw { .. } | Lbu { .. } | Lhu { .. } | LrW { .. }) => {
            1 + cpi.load
        }
        Ok(Sb { .. } | Sh { .. } | Sw { .. } | ScW { .. }) => 1 + cpi.store,
        Ok(Jal { .. } | Jalr { .. }) => 1 + cpi.jump,
        Ok(Ecall | Ebreak | Halt) => 1 + cpi.system,
        Ok(Beq { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] == cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bne { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] != cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Blt { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) < (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bge { rs1, rs2, .. }) => {
            if (cpu.x[rs1 as usize] as i32) >= (cpu.x[rs2 as usize] as i32) {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bltu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] < cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(Bgeu { rs1, rs2, .. }) => {
            if cpu.x[rs1 as usize] >= cpu.x[rs2 as usize] {
                1 + cpi.branch_taken
            } else {
                1 + cpi.branch_not_taken
            }
        }
        Ok(
            Flw { .. }
            | Fsw { .. }
            | FaddS { .. }
            | FsubS { .. }
            | FmulS { .. }
            | FdivS { .. }
            | FsqrtS { .. }
            | FminS { .. }
            | FmaxS { .. }
            | FsgnjS { .. }
            | FsgnjnS { .. }
            | FsgnjxS { .. }
            | FeqS { .. }
            | FltS { .. }
            | FleS { .. }
            | FcvtWS { .. }
            | FcvtWuS { .. }
            | FcvtSW { .. }
            | FcvtSWu { .. }
            | FmvXW { .. }
            | FmvWX { .. }
            | FclassS { .. }
            | FmaddS { .. }
            | FmsubS { .. }
            | FnmsubS { .. }
            | FnmaddS { .. },
        ) => 1 + cpi.fp,
        _ => 1,
    }
}

fn run_headless_sequential(
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
    max_cycles: u64,
    captured_stdout: &mut Vec<u8>,
    max_cores: usize,
) -> Result<(), String> {
    if max_cores > 1 {
        return run_headless_multihart_sequential(
            cpu,
            mem,
            console,
            max_cycles,
            captured_stdout,
            max_cores,
        );
    }
    let mut cycles: u64 = 0;
    loop {
        if cycles >= max_cycles {
            eprintln!("raven: warning: --max-cycles limit ({max_cycles}) reached — stopping");
            break;
        }

        match falcon::exec::step(cpu, mem, console) {
            Ok(true) => {}
            Ok(false) => {
                if console.reading {
                    flush_cpu_stdout(cpu, captured_stdout);
                    let mut line = String::new();
                    match std::io::stdin().read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            console.push_input(line.trim_end_matches(['\n', '\r']).to_string())
                        }
                        Err(_) => break,
                    }
                } else if cpu.ebreak_hit || cpu.local_exit || cpu.exit_code.is_some() {
                    break;
                } else {
                    return Err(format!("raven: fault at PC=0x{:08X}", cpu.pc));
                }
            }
            Err(e) => return Err(format!("raven: fault at PC=0x{:08X}: {e}", cpu.pc)),
        }
        cycles += 1;
    }
    if !console.reading && cpu.exit_code.is_none() && (cpu.ebreak_hit || cpu.local_exit) {
        cpu.exit_code = Some(0);
    }
    Ok(())
}

fn run_headless_multihart_sequential(
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
    max_cycles: u64,
    captured_stdout: &mut Vec<u8>,
    max_cores: usize,
) -> Result<(), String> {
    let mut harts = Vec::with_capacity(max_cores);
    harts.push(HeadlessHart {
        hart_id: 0,
        cpu: cpu.clone(),
        active: true,
        paused: false,
    });
    for i in 1..max_cores {
        let mut free = Cpu::default();
        free.write(2, cpu.read(2));
        harts.push(HeadlessHart {
            hart_id: i as u32,
            cpu: free,
            active: false,
            paused: false,
        });
    }

    let mut cycles = 0u64;
    let mut global_exit_hart: Option<usize> = None;
    loop {
        if cycles >= max_cycles {
            eprintln!("raven: warning: --max-cycles limit ({max_cycles}) reached — stopping");
            break;
        }
        let mut any_running = false;
        for idx in 0..max_cores {
            if !harts[idx].active || harts[idx].paused {
                continue;
            }
            any_running = true;
            match falcon::exec::step(&mut harts[idx].cpu, mem, console) {
                Ok(true) => {
                    flush_cpu_stdout(&mut harts[idx].cpu, captured_stdout);
                    service_pending_hart_start(
                        &mut harts,
                        idx,
                        mem,
                        max_cores,
                        mem.ram.data_len(),
                    )?;
                }
                Ok(false) => {
                    flush_cpu_stdout(&mut harts[idx].cpu, captured_stdout);
                    service_pending_hart_start(
                        &mut harts,
                        idx,
                        mem,
                        max_cores,
                        mem.ram.data_len(),
                    )?;
                    if console.reading {
                        let mut line = String::new();
                        match std::io::stdin().read_line(&mut line) {
                            Ok(0) => break,
                            Ok(_) => {
                                console.push_input(line.trim_end_matches(['\n', '\r']).to_string())
                            }
                            Err(_) => break,
                        }
                    } else if harts[idx].cpu.local_exit {
                        // FALCON_HART_EXIT: only this hart exits, others keep running.
                        harts[idx].active = false;
                    } else if harts[idx].cpu.ebreak_hit {
                        harts[idx].paused = true;
                    } else if harts[idx].cpu.exit_code.is_some() {
                        global_exit_hart = Some(idx);
                        any_running = false;
                        break;
                    } else {
                        return Err(format!(
                            "raven: fault at core {} hart {} PC=0x{:08X}",
                            idx, harts[idx].hart_id, harts[idx].cpu.pc
                        ));
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "raven: fault at core {} hart {} PC=0x{:08X}: {e}",
                        idx, harts[idx].hart_id, harts[idx].cpu.pc
                    ));
                }
            }
        }
        if !any_running {
            break;
        }
        if global_exit_hart.is_some() {
            break;
        }
        cycles = cycles.saturating_add(1);
    }

    if let Some(idx) = global_exit_hart {
        *cpu = harts[idx].cpu.clone();
    } else {
        if !console.reading
            && harts.iter().any(|hart| hart.paused || hart.cpu.local_exit)
            && harts.iter().all(|hart| {
                !hart.active || hart.paused || hart.cpu.exit_code.is_some() || hart.cpu.local_exit
            })
        {
            harts[0].cpu.exit_code = Some(0);
        }
        *cpu = harts.remove(0).cpu;
    }
    Ok(())
}

fn service_pending_hart_start(
    harts: &mut [HeadlessHart],
    parent_idx: usize,
    mem: &CacheController,
    max_cores: usize,
    mem_size: usize,
) -> Result<(), String> {
    let Some(HartStartRequest {
        entry_pc,
        stack_ptr,
        arg,
    }) = harts[parent_idx].cpu.pending_hart_start.take()
    else {
        return Ok(());
    };

    let Some(free_idx) = (0..max_cores).find(|&i| i != parent_idx && !harts[i].active) else {
        harts[parent_idx].cpu.write(10, (-1i32) as u32);
        return Ok(());
    };
    if mem.peek32(entry_pc).is_err() {
        harts[parent_idx].cpu.write(10, (-2i32) as u32);
        return Ok(());
    }
    if stack_ptr == 0 || stack_ptr > mem_size as u32 || stack_ptr & 0xF != 0 {
        harts[parent_idx].cpu.write(10, (-3i32) as u32);
        return Ok(());
    }

    let mut child = Cpu::default();
    child.pc = entry_pc;
    child.write(2, stack_ptr);
    child.write(10, arg);
    child.heap_break = harts[parent_idx].cpu.heap_break;

    let next_hart_id = harts
        .iter()
        .map(|h| h.hart_id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    harts[free_idx] = HeadlessHart {
        hart_id: next_hart_id,
        cpu: child,
        active: true,
        paused: false,
    };
    harts[parent_idx].cpu.write(10, next_hart_id);
    Ok(())
}

fn run_headless_pipeline(
    cpu: &mut Cpu,
    mem: &mut CacheController,
    console: &mut Console,
    cpi: &CpiConfig,
    pcfg: &PipelineConfig,
    max_cycles: u64,
    capture_trace: bool,
) -> Result<(PipelineReport, Vec<PipelineTraceStep>), String> {
    let mut state = crate::ui::pipeline::PipelineSimState::new();
    pcfg.apply_to_state(&mut state);
    state.reset_stages(cpu.pc);
    let mut trace_steps = Vec::new();

    while !state.halted && !state.faulted && state.cycle_count < max_cycles {
        let commit = pipeline_tick(&mut state, cpu, mem, cpi, console);
        if capture_trace {
            trace_steps.push(snapshot_pipeline_trace_step(&state, commit.as_ref()));
        }
        if let Some(info) = commit {
            let cpi_cycles = classify_cpi_cycles_cli(info.pc, cpu, mem, cpi);
            mem.add_instruction_cycles(cpi_cycles);
            mem.instruction_count = mem.instruction_count.saturating_add(1);
            mem.snapshot_stats();
        }
    }

    if state.cycle_count >= max_cycles && !state.halted && !state.faulted {
        eprintln!("raven: warning: --max-cycles limit ({max_cycles}) reached — stopping");
    }

    if state.faulted {
        return Err(format!("raven: pipeline fault at PC=0x{:08X}", cpu.pc));
    }

    Ok((
        PipelineReport {
            enabled: true,
            committed: state.instr_committed,
            cycles: state.cycle_count,
            stalls: state.stall_count,
            flushes: state.flush_count,
            cpi: state.cpi(),
        },
        trace_steps,
    ))
}

// ── raven export-pipeline / import-pipeline ──────────────────────────────────

pub fn export_pipeline_settings(output: Option<&str>) -> Result<(), String> {
    let text = serialize_pipeline_config(&PipelineConfig::default());
    match output {
        Some(path) => {
            std::fs::write(path, &text).map_err(|e| format!("Cannot write '{}': {e}", path))
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

pub fn import_pipeline_settings(file: &str, output: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read '{}': {e}", file))?;
    let cfg = parse_pipeline_config(&text)?;

    eprintln!("{}: valid", file);
    eprintln!("  enabled        = {}", cfg.enabled);
    eprintln!("  forwarding     = {}", cfg.forwarding);
    eprintln!("  mode           = {:?}", cfg.mode);
    eprintln!("  branch_resolve = {:?}", cfg.branch_resolve);
    eprintln!("  predict        = {:?}", cfg.predict);
    eprintln!("  speed          = {:?}", cfg.speed);

    if let Some(out) = output {
        let normalized = serialize_pipeline_config(&cfg);
        std::fs::write(out, normalized).map_err(|e| format!("Cannot write '{}': {e}", out))?;
        eprintln!("  → {out}");
    }
    Ok(())
}

// ── Build helpers ─────────────────────────────────────────────────────────────

/// Write a `Program` as a FALC binary (native Raven format).
fn write_falc(prog: &crate::falcon::asm::Program, path: &str) -> Result<(), String> {
    let text_bytes: Vec<u8> = prog.text.iter().flat_map(|w| w.to_le_bytes()).collect();
    let text_size = text_bytes.len() as u32;
    let data_size = prog.data.len() as u32;
    let bss_size = prog.bss_size;

    let mut bytes: Vec<u8> = Vec::with_capacity(16 + text_bytes.len() + prog.data.len());
    bytes.extend_from_slice(b"FALC");
    bytes.extend_from_slice(&text_size.to_le_bytes());
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.extend_from_slice(&bss_size.to_le_bytes());
    bytes.extend_from_slice(&text_bytes);
    bytes.extend_from_slice(&prog.data);

    std::fs::write(path, bytes).map_err(|e| format!("cannot write '{}': {e}", path))
}

fn replace_ext(path: &str, new_ext: &str) -> String {
    std::path::Path::new(path)
        .with_extension(new_ext)
        .to_string_lossy()
        .to_string()
}

// ── Cache config serialization / parsing ─────────────────────────────────────

pub fn serialize_cache_configs(
    icfg: &CacheConfig,
    dcfg: &CacheConfig,
    extra: &[CacheConfig],
) -> String {
    let mut s = String::from("# Raven Cache Config v2\n");
    s.push_str(&format!("levels={}\n", extra.len()));
    serialize_one_config(&mut s, "icache", icfg);
    serialize_one_config(&mut s, "dcache", dcfg);
    for (i, cfg) in extra.iter().enumerate() {
        let prefix = format!("l{}", i + 2);
        serialize_one_config(&mut s, &prefix, cfg);
    }
    s
}

fn serialize_one_config(s: &mut String, prefix: &str, cfg: &CacheConfig) {
    s.push_str(&format!("{prefix}.size={}\n", cfg.size));
    s.push_str(&format!("{prefix}.line_size={}\n", cfg.line_size));
    s.push_str(&format!("{prefix}.associativity={}\n", cfg.associativity));
    s.push_str(&format!("{prefix}.replacement={:?}\n", cfg.replacement));
    s.push_str(&format!("{prefix}.write_policy={:?}\n", cfg.write_policy));
    s.push_str(&format!("{prefix}.write_alloc={:?}\n", cfg.write_alloc));
    s.push_str(&format!("{prefix}.inclusion={:?}\n", cfg.inclusion));
    s.push_str(&format!("{prefix}.hit_latency={}\n", cfg.hit_latency));
    s.push_str(&format!("{prefix}.miss_penalty={}\n", cfg.miss_penalty));
    s.push_str(&format!("{prefix}.assoc_penalty={}\n", cfg.assoc_penalty));
    s.push_str(&format!("{prefix}.transfer_width={}\n", cfg.transfer_width));
}

pub fn parse_cache_configs(
    text: &str,
) -> Result<(CacheConfig, CacheConfig, Vec<CacheConfig>), String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
        }
    }

    let icfg = parse_single_config(&map, "icache")?;
    let dcfg = parse_single_config(&map, "dcache")?;

    let n_extra: usize = map.get("levels").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut extra = Vec::with_capacity(n_extra);
    for i in 0..n_extra {
        let prefix = format!("l{}", i + 2);
        if map.contains_key(&format!("{prefix}.size")) {
            extra.push(parse_single_config(&map, &prefix)?);
        } else {
            extra.push(CacheConfig::default());
        }
    }

    Ok((icfg, dcfg, extra))
}

fn parse_single_config(map: &HashMap<String, String>, prefix: &str) -> Result<CacheConfig, String> {
    let get = |key: &str| -> Result<&str, String> {
        map.get(&format!("{prefix}.{key}"))
            .map(|s| s.as_str())
            .ok_or_else(|| format!("Missing {prefix}.{key}"))
    };
    let get_usize = |key: &str| -> Result<usize, String> {
        get(key)?
            .parse::<usize>()
            .map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };
    let get_u64 = |key: &str| -> Result<u64, String> {
        get(key)?
            .parse::<u64>()
            .map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };

    let replacement = match get("replacement")? {
        "lru" => ReplacementPolicy::Lru,
        "mru" => ReplacementPolicy::Mru,
        "fifo" => ReplacementPolicy::Fifo,
        "random" => ReplacementPolicy::Random,
        "lfu" => ReplacementPolicy::Lfu,
        "clock" => ReplacementPolicy::Clock,
        other => return Err(format!("Unknown replacement policy: {other}")),
    };
    let write_policy = match get("write_policy")? {
        "writethrough" => WritePolicy::WriteThrough,
        "writeback" => WritePolicy::WriteBack,
        other => return Err(format!("Unknown write_policy: {other}")),
    };
    let write_alloc = match get("write_alloc")? {
        "writeallocate" => WriteAllocPolicy::WriteAllocate,
        "nowriteallocate" => WriteAllocPolicy::NoWriteAllocate,
        other => return Err(format!("Unknown write_alloc: {other}")),
    };
    let inclusion = match map
        .get(&format!("{prefix}.inclusion"))
        .map(String::as_str)
        .unwrap_or("noninclusive")
    {
        "inclusive" => InclusionPolicy::Inclusive,
        "exclusive" => InclusionPolicy::Exclusive,
        _ => InclusionPolicy::NonInclusive,
    };
    let assoc_penalty = map
        .get(&format!("{prefix}.assoc_penalty"))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);
    let transfer_width = map
        .get(&format!("{prefix}.transfer_width"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8)
        .max(1);

    Ok(CacheConfig {
        size: get_usize("size")?,
        line_size: get_usize("line_size")?,
        associativity: get_usize("associativity")?,
        replacement,
        write_policy,
        write_alloc,
        inclusion,
        hit_latency: get_u64("hit_latency")?,
        miss_penalty: get_u64("miss_penalty")?,
        assoc_penalty,
        transfer_width,
    })
}

#[cfg(test)]
#[path = "../../tests/support/cli_internal.rs"]
mod tests;
