// cli.rs — headless CLI commands (build, run, export-config, import-config)

use std::collections::HashMap;
use crate::falcon::{self, Cpu, CacheController};
use crate::falcon::cache::{CacheConfig, Cache, ReplacementPolicy, WritePolicy, WriteAllocPolicy, InclusionPolicy};
use crate::falcon::program::{load_words, load_bytes, load_elf, zero_bytes};
use crate::ui::Console;

// ── Public types ──────────────────────────────────────────────────────────────

pub struct RunArgs {
    pub file: String,
    pub cache_config: Option<String>,
    pub output: Option<String>,
    /// When true, simulation stats are not written/printed (program stdout still shown).
    pub nout: bool,
    pub format: OutputFormat,
    pub mem_size: usize,
    pub max_cycles: u64,
}

pub enum OutputFormat { Json, Fstats, Csv }

// ── raven build ───────────────────────────────────────────────────────────────

/// Assemble `file` and optionally write a FALC binary.
/// `nout = true` → check-only (no output file written).
pub fn build_program(file: &str, output: Option<&str>, nout: bool) -> Result<(), String> {
    let src = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read '{}': {e}", file))?;

    let prog = crate::falcon::asm::assemble(&src, 0x0)
        .map_err(|e| {
            eprintln!("error: {}:{}: {}", file, e.line + 1, e.msg);
            String::new() // sentinel; we already printed
        })
        .map_err(|_| String::new())?; // propagate empty sentinel

    let instr = prog.text.len();
    let data  = prog.data.len();
    eprintln!("{}: {} instruction{}, {} data byte{}",
        file,
        instr, if instr == 1 { "" } else { "s" },
        data,  if data  == 1 { "" } else { "s" },
    );

    if nout { return Ok(()); }

    let out_path = output.map(str::to_string)
        .unwrap_or_else(|| replace_ext(file, "bin"));
    write_falc(&prog, &out_path)?;
    eprintln!("  → {out_path}");
    Ok(())
}

// ── raven import-config ───────────────────────────────────────────────────────

/// Parse and validate a .fcache file, print a human-readable summary.
/// Optionally re-export the normalized config to `output`.
pub fn import_config(file: &str, output: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read '{}': {e}", file))?;
    let (icfg, dcfg, extra) = parse_cache_configs(&text)?;

    icfg.validate().map_err(|e| format!("I-cache config error: {e}"))?;
    dcfg.validate().map_err(|e| format!("D-cache config error: {e}"))?;
    for (i, cfg) in extra.iter().enumerate() {
        cfg.validate().map_err(|e| format!("L{} config error: {e}", i + 2))?;
    }

    eprintln!("{}: valid — {} cache level{}", file, 1 + extra.len(), if extra.is_empty() { "" } else { "s" });
    print_config_row("  I-Cache L1", &icfg);
    print_config_row("  D-Cache L1", &dcfg);
    for (i, cfg) in extra.iter().enumerate() {
        print_config_row(&format!("  L{} Unified ", i + 2), cfg);
    }

    if let Some(out) = output {
        let normalized = serialize_cache_configs(&icfg, &dcfg, &extra);
        std::fs::write(out, normalized)
            .map_err(|e| format!("cannot write '{}': {e}", out))?;
        eprintln!("  → {out}");
    }
    Ok(())
}

fn print_config_row(label: &str, cfg: &CacheConfig) {
    eprintln!("  {:<14}  {:>5}KB  {}B lines  {}-way  {:?}/{:?}  lat={} pen={}",
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

    icfg.validate().map_err(|e| format!("I-cache config error: {e}"))?;
    dcfg.validate().map_err(|e| format!("D-cache config error: {e}"))?;
    for (i, cfg) in extra_cfgs.iter().enumerate() {
        cfg.validate().map_err(|e| format!("L{} cache config error: {e}", i + 2))?;
    }

    // ── 2. Set up simulation ─────────────────────────────────────────────────
    let mut cpu = Cpu::default();
    let mut mem = CacheController::new(icfg, dcfg, extra_cfgs, args.mem_size);
    let mut console = Console::default();

    // SP = top of RAM
    cpu.write(2, args.mem_size as u32);

    // ── 3. Load program ──────────────────────────────────────────────────────
    let file_bytes = std::fs::read(&args.file)
        .map_err(|e| format!("Cannot read '{}': {e}", args.file))?;

    let file_name = std::path::Path::new(&args.file)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if is_elf(&file_bytes) {
        let info = load_elf(&file_bytes, &mut mem.ram)
            .map_err(|e| format!("ELF load error: {e}"))?;
        cpu.pc = info.entry;
        cpu.heap_break = info.heap_start;
        cpu.write(2, args.mem_size as u32); // restore SP after ELF load
    } else if is_falc(&file_bytes) {
        load_falc(&file_bytes, &mut cpu, &mut mem, args.mem_size)?;
    } else if looks_like_text(&file_bytes) {
        load_asm_text(&file_bytes, &mut cpu, &mut mem)?;
    } else {
        // Flat binary: load at 0x0
        load_bytes(&mut mem.ram, 0, &file_bytes)
            .map_err(|e| format!("Binary load error: {e}"))?;
        cpu.pc = 0;
        let bss_end = file_bytes.len() as u32;
        cpu.heap_break = (bss_end.wrapping_add(15)) & !15;
    }

    mem.invalidate_all();
    mem.reset_stats();

    // ── 4. Run until halt ────────────────────────────────────────────────────
    let mut cycles: u64 = 0;
    let mut faulted = false;

    loop {
        if cycles >= args.max_cycles {
            eprintln!(
                "raven: warning: --max-cycles limit ({}) reached — stopping",
                args.max_cycles
            );
            break;
        }

        match falcon::exec::step(&mut cpu, &mut mem, &mut console) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                eprintln!("raven: fault at PC=0x{:08X}: {e}", cpu.pc);
                faulted = true;
                break;
            }
        }
        cycles += 1;
    }

    // Print program stdout
    if !cpu.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&cpu.stdout));
    }
    // Print any console errors from the simulation
    for line in &console.lines {
        if line.is_error() {
            eprintln!("raven: {}", line.text);
        }
    }

    // ── 5. Serialize and output ──────────────────────────────────────────────
    if !args.nout {
        let exit_code = if faulted { None } else { cpu.exit_code };
        let text = match args.format {
            OutputFormat::Json   => format_json(&mem, &file_name, exit_code),
            OutputFormat::Fstats => format_fstats(&mem, &file_name, exit_code),
            OutputFormat::Csv    => format_csv(&mem, &file_name),
        };
        match &args.output {
            Some(path) => std::fs::write(path, &text)
                .map_err(|e| format!("Cannot write '{}': {e}", path))?,
            None => print!("{text}"),
        }
    }

    Ok(())
}

/// Serialize the default cache config to a `.fcache` file (for `--export-config`).
pub fn export_default_config(output: Option<&str>) -> Result<(), String> {
    let text = serialize_cache_configs(
        &CacheConfig::default(),
        &CacheConfig::default(),
        &[],
    );
    match output {
        Some(path) => std::fs::write(path, &text)
            .map_err(|e| format!("Cannot write '{}': {e}", path)),
        None => { print!("{text}"); Ok(()) }
    }
}

// ── Build helpers ─────────────────────────────────────────────────────────────

/// Write a `Program` as a FALC binary (native Raven format).
fn write_falc(prog: &crate::falcon::asm::Program, path: &str) -> Result<(), String> {
    let text_bytes: Vec<u8> = prog.text.iter().flat_map(|w| w.to_le_bytes()).collect();
    let text_size = text_bytes.len() as u32;
    let data_size = prog.data.len() as u32;
    let bss_size  = prog.bss_size;

    let mut bytes: Vec<u8> = Vec::with_capacity(16 + text_bytes.len() + prog.data.len());
    bytes.extend_from_slice(b"FALC");
    bytes.extend_from_slice(&text_size.to_le_bytes());
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.extend_from_slice(&bss_size.to_le_bytes());
    bytes.extend_from_slice(&text_bytes);
    bytes.extend_from_slice(&prog.data);

    std::fs::write(path, bytes)
        .map_err(|e| format!("cannot write '{}': {e}", path))
}

fn replace_ext(path: &str, new_ext: &str) -> String {
    std::path::Path::new(path)
        .with_extension(new_ext)
        .to_string_lossy()
        .to_string()
}

// ── Loaders ───────────────────────────────────────────────────────────────────

fn is_elf(b: &[u8]) -> bool {
    b.len() >= 4 && &b[0..4] == b"\x7fELF"
}

fn is_falc(b: &[u8]) -> bool {
    b.len() >= 16 && &b[0..4] == b"FALC"
}

fn looks_like_text(b: &[u8]) -> bool {
    // Heuristic: if >85% of bytes are printable ASCII or common control chars → text
    if b.is_empty() { return false; }
    let printable = b.iter().filter(|&&c| c >= 32 || c == b'\n' || c == b'\r' || c == b'\t').count();
    printable * 100 / b.len() >= 85
}

fn load_asm_text(bytes: &[u8], cpu: &mut Cpu, mem: &mut CacheController) -> Result<(), String> {
    let text = String::from_utf8_lossy(bytes).to_string();
    let prog = falcon::asm::assemble(&text, 0x0)
        .map_err(|e| format!("Assembly error at line {}: {}", e.line + 1, e.msg))?;

    load_words(&mut mem.ram, 0x0, &prog.text)
        .map_err(|e| format!("Load error: {e}"))?;

    if !prog.data.is_empty() {
        load_bytes(&mut mem.ram, prog.data_base, &prog.data)
            .map_err(|e| format!("Data load error: {e}"))?;
    }

    let bss_base = prog.data_base.wrapping_add(prog.data.len() as u32);
    if prog.bss_size > 0 {
        zero_bytes(&mut mem.ram, bss_base, prog.bss_size)
            .map_err(|e| format!("BSS error: {e}"))?;
    }

    cpu.pc = 0x0;
    let bss_end = bss_base.wrapping_add(prog.bss_size);
    cpu.heap_break = (bss_end.wrapping_add(15)) & !15;
    Ok(())
}

fn load_falc(bytes: &[u8], cpu: &mut Cpu, mem: &mut CacheController, mem_size: usize) -> Result<(), String> {
    let text_sz = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let data_sz = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let bss_sz  = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let body    = &bytes[16..];

    if body.len() < text_sz + data_sz {
        return Err("FALC binary truncated or corrupt".to_string());
    }

    let text_bytes = &body[..text_sz];
    let data_bytes = &body[text_sz..text_sz + data_sz];

    // Text at 0x0, data right after (4-byte aligned)
    let data_base: u32 = ((text_sz as u32).wrapping_add(3)) & !3;

    load_bytes(&mut mem.ram, 0, text_bytes)
        .map_err(|e| format!("Load error: {e}"))?;

    if !data_bytes.is_empty() {
        load_bytes(&mut mem.ram, data_base, data_bytes)
            .map_err(|e| format!("Data load error: {e}"))?;
    }

    if bss_sz > 0 {
        let bss_base = data_base.wrapping_add(data_bytes.len() as u32);
        zero_bytes(&mut mem.ram, bss_base, bss_sz)
            .map_err(|e| format!("BSS error: {e}"))?;
    }

    cpu.pc = 0;
    cpu.write(2, mem_size as u32);

    let bss_end = data_base
        .wrapping_add(data_bytes.len() as u32)
        .wrapping_add(bss_sz);
    cpu.heap_break = (bss_end.wrapping_add(15)) & !15;

    Ok(())
}

// ── JSON output ───────────────────────────────────────────────────────────────

fn format_json(mem: &CacheController, file: &str, exit_code: Option<u32>) -> String {
    let instr = mem.instruction_count;
    let total_cyc = mem.total_program_cycles();
    let cpi = mem.overall_cpi();
    let ipc = mem.ipc();

    let mut s = String::from("{\n");
    s.push_str(&format!("  \"file\": \"{}\",\n", json_escape(file)));
    match exit_code {
        Some(c) => s.push_str(&format!("  \"exit_code\": {c},\n")),
        None    => s.push_str("  \"exit_code\": null,\n"),
    }
    s.push_str(&format!("  \"instructions\": {instr},\n"));
    s.push_str(&format!("  \"total_cycles\": {total_cyc},\n"));
    s.push_str(&format!("  \"base_cycles\": {},\n", mem.extra_cycles));
    s.push_str(&format!("  \"cache_cycles\": {},\n", total_cyc.saturating_sub(mem.extra_cycles)));
    s.push_str(&format!("  \"cpi\": {cpi:.4},\n"));
    s.push_str(&format!("  \"ipc\": {ipc:.4},\n"));

    s.push_str("  \"icache\": ");
    append_cache_json(&mut s, &mem.icache, "I-Cache L1", instr, mem.icache_amat());
    s.push_str(",\n");

    s.push_str("  \"dcache\": ");
    append_cache_json(&mut s, &mem.dcache, "D-Cache L1", instr, mem.dcache_amat());

    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let name = format!("L{} (Unified)", i + 2);
        let key = format!("l{}", i + 2);
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 { lvl.config.hit_latency as f64 } else { lvl.stats.total_cycles as f64 / total as f64 };
        s.push_str(",\n");
        s.push_str(&format!("  \"{key}\": "));
        append_cache_json(&mut s, lvl, &name, instr, amat);
    }

    s.push_str("\n}\n");
    s
}

fn append_cache_json(s: &mut String, cache: &Cache, name: &str, instructions: u64, amat: f64) {
    let stats = &cache.stats;
    let cfg   = &cache.config;
    let total = stats.total_accesses();
    let hit_rate  = stats.hit_rate();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let mpki      = stats.mpki(instructions);

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
    s.push_str(&format!("    \"ram_write_bytes\": {},\n", stats.ram_write_bytes));
    s.push_str(&format!("    \"bytes_stored\": {},\n", stats.bytes_stored));

    // Top miss PCs
    s.push_str("    \"top_miss_pcs\": [");
    for (j, (pc, count)) in hotspots.iter().enumerate() {
        let pct = if stats.misses == 0 { 0.0 } else { *count as f64 / stats.misses as f64 * 100.0 };
        if j > 0 { s.push_str(", "); }
        s.push_str(&format!("{{ \"pc\": \"0x{pc:08x}\", \"count\": {count}, \"pct\": {pct:.1} }}"));
    }
    s.push_str("]\n  }");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ── fstats output (compatible with TUI import) ────────────────────────────────

fn format_fstats(mem: &CacheController, file: &str, exit_code: Option<u32>) -> String {
    let instr     = mem.instruction_count;
    let total_cyc = mem.total_program_cycles();
    let cpi       = mem.overall_cpi();
    let ipc       = mem.ipc();

    let mut s = String::from("# FALCON-ASM Simulation Results v1\n");
    s.push_str(&format!("label={file}\n"));
    if let Some(code) = exit_code {
        s.push_str(&format!("prog.exit_code={code}\n"));
    }
    s.push_str(&format!("prog.instructions={instr}\n"));
    s.push_str(&format!("prog.total_cycles={total_cyc}\n"));
    s.push_str(&format!("prog.base_cycles={}\n", mem.extra_cycles));
    s.push_str(&format!("prog.cache_cycles={}\n", total_cyc.saturating_sub(mem.extra_cycles)));
    s.push_str(&format!("prog.cpi={cpi:.4}\n"));
    s.push_str(&format!("prog.ipc={ipc:.4}\n"));
    s.push_str(&format!("extra_levels={}\n", mem.extra_levels.len()));

    write_cache_level_fstats(&mut s, "icache", "I-Cache L1", &mem.icache, instr, mem.icache_amat());
    write_cache_level_fstats(&mut s, "dcache", "D-Cache L1", &mem.dcache, instr, mem.dcache_amat());
    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let key  = format!("l{}", i + 2);
        let name = format!("{} Unified", CacheController::extra_level_name(i));
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 { lvl.config.hit_latency as f64 } else { lvl.stats.total_cycles as f64 / total as f64 };
        write_cache_level_fstats(&mut s, &key, &name, lvl, instr, amat);
    }

    // Miss hotspots (I-cache)
    let mut hotspots: Vec<(u32, u64)> = mem.icache.stats.miss_pcs.iter().map(|(&k, &v)| (k, v)).collect();
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
    for (i, (x, y)) in hist_i.iter().enumerate() { s.push_str(&format!("history_i.{i}={x}:{y}\n")); }
    s.push_str(&format!("history_d_count={}\n", hist_d.len()));
    for (i, (x, y)) in hist_d.iter().enumerate() { s.push_str(&format!("history_d.{i}={x}:{y}\n")); }

    // CPI config (defaults)
    s.push_str("cpi.alu=1\ncpi.mul=3\ncpi.div=20\ncpi.load=0\ncpi.store=0\n");
    s.push_str("cpi.branch_taken=3\ncpi.branch_not_taken=1\ncpi.jump=2\ncpi.system=10\n");
    s
}

fn write_cache_level_fstats(s: &mut String, key: &str, name: &str, cache: &Cache, _instr: u64, amat: f64) {
    let st  = &cache.stats;
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

fn format_csv(mem: &CacheController, file: &str) -> String {
    let instr     = mem.instruction_count;
    let total_cyc = mem.total_program_cycles();

    let mut s = String::new();
    s.push_str(&format!("# {file}\n"));
    s.push_str("PROGRAM SUMMARY\n");
    s.push_str("Instructions,Total Cycles,Base Cycles,Cache Cycles,CPI,IPC\n");
    s.push_str(&format!(
        "{},{},{},{},{:.4},{:.4}\n",
        instr, total_cyc, mem.extra_cycles,
        total_cyc.saturating_sub(mem.extra_cycles),
        mem.overall_cpi(), mem.ipc()
    ));
    s.push('\n');

    s.push_str("CACHE LEVELS\n");
    s.push_str("Level,Hits,Misses,Total Accesses,Hit Rate (%),Miss Rate (%),MPKI,AMAT (cycles),Evictions,Writebacks,RAM Reads (B),RAM Writes (B),Total Cycles\n");

    let write_level = |s: &mut String, label: &str, cache: &Cache, instructions: u64, amat: f64| {
        let st = &cache.stats;
        let total = st.total_accesses();
        let hit_rate  = st.hit_rate();
        let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
        let mpki      = st.mpki(instructions);
        s.push_str(&format!(
            "{label},{},{},{},{:.1},{:.1},{:.2},{:.4},{},{},{},{},{}\n",
            st.hits, st.misses, total, hit_rate, miss_rate, mpki, amat,
            st.evictions, st.writebacks, st.bytes_loaded, st.ram_write_bytes, st.total_cycles
        ));
    };

    write_level(&mut s, "I-Cache L1", &mem.icache, instr, mem.icache_amat());
    write_level(&mut s, "D-Cache L1", &mem.dcache, instr, mem.dcache_amat());
    for (i, lvl) in mem.extra_levels.iter().enumerate() {
        let label = format!("L{} Unified", i + 2);
        let total = lvl.stats.total_accesses();
        let amat = if total == 0 { lvl.config.hit_latency as f64 } else { lvl.stats.total_cycles as f64 / total as f64 };
        write_level(&mut s, &label, lvl, instr, amat);
    }

    // Miss hotspots
    let mut hotspots: Vec<(u32, u64)> = mem.icache.stats.miss_pcs.iter().map(|(&k, &v)| (k, v)).collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(5);
    if !hotspots.is_empty() {
        s.push('\n');
        s.push_str("TOP I-CACHE MISS PCs\n");
        s.push_str("PC,Miss Count,% of Misses\n");
        let total_misses = mem.icache.stats.misses;
        for (pc, count) in &hotspots {
            let pct = if total_misses == 0 { 0.0 } else { *count as f64 / total_misses as f64 * 100.0 };
            s.push_str(&format!("0x{pc:08x},{count},{pct:.1}\n"));
        }
    }

    s
}

// ── Cache config serialization / parsing ─────────────────────────────────────

pub fn serialize_cache_configs(icfg: &CacheConfig, dcfg: &CacheConfig, extra: &[CacheConfig]) -> String {
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

pub fn parse_cache_configs(text: &str) -> Result<(CacheConfig, CacheConfig, Vec<CacheConfig>), String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
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
        get(key)?.parse::<usize>().map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };
    let get_u64 = |key: &str| -> Result<u64, String> {
        get(key)?.parse::<u64>().map_err(|_| format!("Invalid {prefix}.{key}: expected integer"))
    };

    let replacement = match get("replacement")? {
        "lru"    => ReplacementPolicy::Lru,
        "mru"    => ReplacementPolicy::Mru,
        "fifo"   => ReplacementPolicy::Fifo,
        "random" => ReplacementPolicy::Random,
        "lfu"    => ReplacementPolicy::Lfu,
        "clock"  => ReplacementPolicy::Clock,
        other    => return Err(format!("Unknown replacement policy: {other}")),
    };
    let write_policy = match get("write_policy")? {
        "writethrough" => WritePolicy::WriteThrough,
        "writeback"    => WritePolicy::WriteBack,
        other          => return Err(format!("Unknown write_policy: {other}")),
    };
    let write_alloc = match get("write_alloc")? {
        "writeallocate"   => WriteAllocPolicy::WriteAllocate,
        "nowriteallocate" => WriteAllocPolicy::NoWriteAllocate,
        other             => return Err(format!("Unknown write_alloc: {other}")),
    };
    let inclusion = match map.get(&format!("{prefix}.inclusion")).map(String::as_str).unwrap_or("noninclusive") {
        "inclusive"  => InclusionPolicy::Inclusive,
        "exclusive"  => InclusionPolicy::Exclusive,
        _            => InclusionPolicy::NonInclusive,
    };
    let assoc_penalty = map.get(&format!("{prefix}.assoc_penalty"))
        .and_then(|v| v.parse::<u64>().ok()).unwrap_or(1);
    let transfer_width = map.get(&format!("{prefix}.transfer_width"))
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(8).max(1);

    Ok(CacheConfig {
        size: get_usize("size")?,
        line_size: get_usize("line_size")?,
        associativity: get_usize("associativity")?,
        replacement, write_policy, write_alloc, inclusion,
        hit_latency: get_u64("hit_latency")?,
        miss_penalty: get_u64("miss_penalty")?,
        assoc_penalty,
        transfer_width,
    })
}
