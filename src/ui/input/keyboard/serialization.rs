use crate::falcon::cache::{
    Cache, CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy, extra_level_presets,
};
use crate::ui::app::{
    App, CacheResultsSnapshot, CpiConfig, LevelSnapshot, PathInput, PathInputAction,
    PipelineResultsSnapshot, RunScope,
};
use rfd::FileDialog as OSFileDialog;
use std::collections::HashMap;

// ── Cache config serialization ────────────────────────────────────────────────

pub(super) fn serialize_one_config(s: &mut String, prefix: &str, cfg: &CacheConfig) {
    s.push_str(&format!("{prefix}.size={}\n", cfg.size));
    s.push_str(&format!("{prefix}.line_size={}\n", cfg.line_size));
    s.push_str(&format!("{prefix}.associativity={}\n", cfg.associativity));
    s.push_str(&format!("{prefix}.replacement={:?}\n", cfg.replacement));
    s.push_str(&format!("{prefix}.write_policy={:?}\n", cfg.write_policy));
    s.push_str(&format!("{prefix}.write_alloc={:?}\n", cfg.write_alloc));
    s.push_str(&format!("{prefix}.hit_latency={}\n", cfg.hit_latency));
    s.push_str(&format!("{prefix}.miss_penalty={}\n", cfg.miss_penalty));
    s.push_str(&format!("{prefix}.assoc_penalty={}\n", cfg.assoc_penalty));
    s.push_str(&format!("{prefix}.transfer_width={}\n", cfg.transfer_width));
    s.push_str(&format!("{prefix}.inclusion={:?}\n", cfg.inclusion));
}

pub(super) fn serialize_cache_configs(
    icfg: &CacheConfig,
    dcfg: &CacheConfig,
    extra: &[CacheConfig],
) -> String {
    let mut s = String::from("# Raven Cache Config v2\n");
    s.push_str(&format!("levels={}\n", extra.len()));
    serialize_one_config(&mut s, "icache", icfg);
    serialize_one_config(&mut s, "dcache", dcfg);
    for (i, cfg) in extra.iter().enumerate() {
        let prefix = level_prefix(i);
        serialize_one_config(&mut s, &prefix, cfg);
    }
    s
}

pub(super) fn serialize_rcfg(
    cpi: &CpiConfig,
    cache_enabled: bool,
    run_scope: RunScope,
    mem_kb: usize,
) -> String {
    let mut s = String::from("# Raven Sim Config v2\n");
    s.push_str(&format!("cache_enabled={}\n", cache_enabled));
    s.push_str(&format!("mem_kb={}\n", mem_kb));
    s.push_str(&format!(
        "run_scope={}\n",
        match run_scope {
            RunScope::AllHarts => "all",
            RunScope::FocusedHart => "focus",
        }
    ));
    s.push_str("\n# CPI (cycles per instruction)\n");
    s.push_str(&format!("cpi.alu={}\n", cpi.alu));
    s.push_str(&format!("cpi.mul={}\n", cpi.mul));
    s.push_str(&format!("cpi.div={}\n", cpi.div));
    s.push_str(&format!("cpi.load={}\n", cpi.load));
    s.push_str(&format!("cpi.store={}\n", cpi.store));
    s.push_str(&format!("cpi.branch_taken={}\n", cpi.branch_taken));
    s.push_str(&format!("cpi.branch_not_taken={}\n", cpi.branch_not_taken));
    s.push_str(&format!("cpi.jump={}\n", cpi.jump));
    s.push_str(&format!("cpi.system={}\n", cpi.system));
    s.push_str(&format!("cpi.fp={}\n", cpi.fp));
    s
}

pub(super) fn serialize_pcfg(pipeline: &crate::ui::pipeline::PipelineSimState) -> String {
    crate::ui::pipeline::serialize_pipeline_config(
        &crate::ui::pipeline::PipelineConfig::from_state(pipeline),
    )
}

pub(super) fn parse_pcfg(text: &str) -> Result<crate::ui::pipeline::PipelineConfig, String> {
    crate::ui::pipeline::parse_pipeline_config(text)
}

pub(super) fn parse_rcfg(text: &str) -> Result<(CpiConfig, bool, RunScope, Option<usize>), String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
        }
    }
    let cpi = CpiConfig {
        alu: map.get("cpi.alu").and_then(|v| v.parse().ok()).unwrap_or(1),
        mul: map.get("cpi.mul").and_then(|v| v.parse().ok()).unwrap_or(3),
        div: map
            .get("cpi.div")
            .and_then(|v| v.parse().ok())
            .unwrap_or(20),
        load: map
            .get("cpi.load")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        store: map
            .get("cpi.store")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        branch_taken: map
            .get("cpi.branch_taken")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3),
        branch_not_taken: map
            .get("cpi.branch_not_taken")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1),
        jump: map
            .get("cpi.jump")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2),
        system: map
            .get("cpi.system")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10),
        fp: map.get("cpi.fp").and_then(|v| v.parse().ok()).unwrap_or(5),
    };
    let cache_enabled = map
        .get("cache_enabled")
        .map(|v| v != "false")
        .unwrap_or(true);
    let run_scope = match map.get("run_scope").map(String::as_str).unwrap_or("all") {
        "all" => RunScope::AllHarts,
        "focus" | "focused" => RunScope::FocusedHart,
        other => return Err(format!("invalid run_scope: {other}")),
    };
    let mem_bytes = if let Some(kb) = map.get("mem_kb").and_then(|v| v.parse::<usize>().ok()) {
        // Current format: value in KB
        let snapped = crate::ui::app::nearest_pow2_clamp(kb.max(4), 4, 4 * 1024 * 1024);
        Some(snapped * 1024)
    } else if let Some(mb) = map.get("mem_mb").and_then(|v| v.parse::<usize>().ok()) {
        // Legacy format: value in MB (backward compat)
        let snapped = crate::ui::app::nearest_pow2_clamp(mb.max(1), 1, 4096);
        Some(snapped * 1024 * 1024)
    } else {
        None
    };
    Ok((cpi, cache_enabled, run_scope, mem_bytes))
}

/// Returns prefix like "l2", "l3", etc. for extra_level index i (0-based → L2, L3, …)
pub(super) fn level_prefix(i: usize) -> String {
    format!("l{}", i + 2)
}

pub(super) fn parse_cache_configs(
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
    let presets = extra_level_presets();
    for i in 0..n_extra {
        let prefix = level_prefix(i);
        if map.contains_key(&format!("{prefix}.size")) {
            extra.push(parse_single_config(&map, &prefix)?);
        } else {
            extra.push(presets[1].clone());
        }
    }

    Ok((icfg, dcfg, extra))
}

pub(super) fn parse_single_config(
    map: &HashMap<String, String>,
    prefix: &str,
) -> Result<CacheConfig, String> {
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

    let assoc_penalty = map
        .get(&format!("{prefix}.assoc_penalty"))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);
    let transfer_width = map
        .get(&format!("{prefix}.transfer_width"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8)
        .max(1);

    use crate::falcon::cache::InclusionPolicy;
    let inclusion = match map
        .get(&format!("{prefix}.inclusion"))
        .map(String::as_str)
        .unwrap_or("noninclusive")
    {
        "inclusive" => InclusionPolicy::Inclusive,
        "exclusive" => InclusionPolicy::Exclusive,
        _ => InclusionPolicy::NonInclusive,
    };

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

// ── Simulation results export/import ─────────────────────────────────────────

pub(super) fn make_level_snapshot(
    name: &str,
    cache: &Cache,
    _instructions: u64,
    amat: f64,
) -> LevelSnapshot {
    let cfg = &cache.config;
    LevelSnapshot {
        name: name.to_string(),
        size: cfg.size,
        line_size: cfg.line_size,
        associativity: cfg.associativity,
        replacement: format!("{:?}", cfg.replacement),
        write_policy: format!("{:?}", cfg.write_policy),
        hit_latency: cfg.hit_latency,
        miss_penalty: cfg.miss_penalty,
        hits: cache.stats.hits,
        misses: cache.stats.misses,
        evictions: cache.stats.evictions,
        writebacks: cache.stats.writebacks,
        bytes_loaded: cache.stats.bytes_loaded,
        bytes_stored: cache.stats.bytes_stored,
        total_cycles: cache.stats.total_cycles,
        ram_write_bytes: cache.stats.ram_write_bytes,
        amat,
    }
}

pub(super) fn capture_snapshot(app: &App) -> CacheResultsSnapshot {
    let mem = &app.run.mem;
    let pipeline = capture_pipeline_snapshot(app);
    let i_amat = mem.icache_amat();
    let d_amat = mem.dcache_amat();
    let icache_snap = make_level_snapshot("I-Cache L1", &mem.icache, mem.instruction_count, i_amat);
    let dcache_snap = make_level_snapshot("D-Cache L1", &mem.dcache, mem.instruction_count, d_amat);
    let extra_snaps: Vec<LevelSnapshot> = mem
        .extra_levels
        .iter()
        .enumerate()
        .map(|(i, lvl)| {
            let name = format!(
                "{} Unified",
                crate::falcon::cache::CacheController::extra_level_name(i)
            );
            let total = lvl.stats.total_accesses();
            let amat = if total == 0 {
                lvl.config.hit_latency as f64
            } else {
                lvl.stats.total_cycles as f64 / total as f64
            };
            make_level_snapshot(&name, lvl, mem.instruction_count, amat)
        })
        .collect();

    let mut hotspots: Vec<(u32, u64)> = mem
        .icache
        .stats
        .miss_pcs
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    hotspots.sort_by(|a, b| b.1.cmp(&a.1));
    hotspots.truncate(10);

    let instr_start = app.cache.window_start_instr;
    let instr_end = pipeline
        .as_ref()
        .map_or(mem.instruction_count, |p| p.committed);
    let start_f = instr_start as f64;

    let history_i: Vec<(f64, f64)> = mem
        .icache
        .stats
        .history
        .iter()
        .filter(|(x, _)| *x >= start_f)
        .cloned()
        .collect();
    let history_d: Vec<(f64, f64)> = mem
        .dcache
        .stats
        .history
        .iter()
        .filter(|(x, _)| *x >= start_f)
        .cloned()
        .collect();

    let (instruction_count, total_cycles, base_cycles, cpi, ipc) = if let Some(p) = &pipeline {
        let ipc = if p.cycles > 0 {
            p.committed as f64 / p.cycles as f64
        } else {
            0.0
        };
        (p.committed, p.cycles, 0, p.cpi, ipc)
    } else {
        (
            mem.instruction_count,
            mem.total_program_cycles(),
            mem.extra_cycles,
            mem.overall_cpi(),
            mem.ipc(),
        )
    };

    CacheResultsSnapshot {
        label: format!("[{}\u{2013}{}]", instr_start, instr_end),
        instr_start,
        instr_end,
        instruction_count,
        total_cycles,
        base_cycles,
        cpi,
        ipc,
        icache: icache_snap,
        dcache: dcache_snap,
        extra_levels: extra_snaps,
        cpi_config: app.run.cpi_config.clone(),
        miss_hotspots: hotspots,
        hit_rate_history_i: history_i,
        hit_rate_history_d: history_d,
        pipeline,
    }
}

fn capture_pipeline_snapshot(app: &App) -> Option<PipelineResultsSnapshot> {
    app.aggregate_pipeline_snapshot()
}

fn capture_selected_pipeline_export_snapshot(app: &App) -> CacheResultsSnapshot {
    let mut snap = capture_snapshot(app);
    if let Some(p) = app.selected_pipeline_snapshot() {
        let ipc = if p.cycles > 0 {
            p.committed as f64 / p.cycles as f64
        } else {
            0.0
        };
        snap.instruction_count = p.committed;
        snap.total_cycles = p.cycles;
        snap.base_cycles = 0;
        snap.cpi = p.cpi;
        snap.ipc = ipc;
        snap.instr_end = p.committed;
        snap.label = format!("[{}–{}]", snap.instr_start, snap.instr_end);
        snap.pipeline = Some(p);
    }
    snap
}

pub(crate) fn do_export_cfg(app: &mut App) {
    let text = serialize_cache_configs(
        &app.cache.pending_icache,
        &app.cache.pending_dcache,
        &app.cache.extra_pending,
    );
    if let Some(path) = OSFileDialog::new()
        .add_filter("Cache Config", &["fcache"])
        .set_file_name("cache.fcache")
        .save_file()
    {
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.cache.config_error = None;
                app.cache.config_status = Some(format!(
                    "Exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Export failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::SaveFcache);
    }
}

pub(crate) fn do_import_cfg(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("Cache Config", &["fcache"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_cache_configs(&text) {
                Ok((icfg, dcfg, extra)) => {
                    app.cache.pending_icache = icfg;
                    app.cache.pending_dcache = dcfg;
                    let n_extra = extra.len();
                    app.cache.extra_pending = extra;
                    app.run.mem.extra_levels.clear();
                    for cfg in &app.cache.extra_pending {
                        app.run
                            .mem
                            .extra_levels
                            .push(crate::falcon::cache::Cache::new(cfg.clone()));
                    }
                    if app.cache.selected_level > n_extra {
                        app.cache.selected_level = n_extra;
                    }
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Import failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::OpenFcache);
    }
}

pub(super) fn do_export_rcfg(app: &mut App) {
    let text = serialize_rcfg(
        &app.run.cpi_config,
        app.run.cache_enabled,
        app.run_scope,
        app.run.mem_size / 1024,
    );
    if let Some(path) = OSFileDialog::new()
        .add_filter("Raven Sim Config", &["rcfg"])
        .set_file_name("settings.rcfg")
        .save_file()
    {
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.cache.config_error = None;
                app.cache.config_status = Some(format!(
                    "Settings exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Export failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::SaveRcfg);
    }
}

pub(super) fn do_import_rcfg(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("Raven Sim Config", &["rcfg"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_rcfg(&text) {
                Ok((cpi, cache_enabled, run_scope, mem_bytes)) => {
                    app.run.cpi_config = cpi;
                    app.set_cache_enabled(cache_enabled);
                    app.run_scope = run_scope;
                    if let Some(bytes) = mem_bytes {
                        if bytes != app.run.mem_size {
                            app.ram_override = Some(bytes);
                            app.restart_simulation();
                        }
                    }
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Settings imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Import failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::OpenRcfg);
    }
}

pub(crate) fn do_export_pcfg(app: &mut App) {
    let text = serialize_pcfg(&app.pipeline);
    if let Some(path) = OSFileDialog::new()
        .add_filter("Raven Pipeline Config", &["pcfg"])
        .set_file_name("pipeline.pcfg")
        .save_file()
    {
        let path = ensure_extension(path, "pcfg");
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.pipeline.status_error = None;
                app.pipeline.status_msg = Some(format!(
                    "Pipeline config exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                app.pipeline.status_msg = None;
                app.pipeline.status_error = Some(format!("Export failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::SavePcfg);
    }
}

pub(crate) fn do_import_pcfg(app: &mut App) {
    if let Some(path) = OSFileDialog::new()
        .add_filter("Raven Pipeline Config", &["pcfg"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_pcfg(&text) {
                Ok(cfg) => {
                    cfg.apply_to_state(&mut app.pipeline);
                    app.pipeline.status_error = None;
                    app.pipeline.status_msg = Some(format!(
                        "Pipeline config imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.pipeline.status_msg = None;
                    app.pipeline.status_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.pipeline.status_msg = None;
                app.pipeline.status_error = Some(format!("Import failed: {e}"));
            }
        }
    } else {
        open_path_input(app, PathInputAction::OpenPcfg);
    }
}

pub(crate) fn do_export_results(app: &mut App) {
    let mut snap = capture_snapshot(app);
    if let Some(path) = OSFileDialog::new()
        .add_filter("FALCON Stats", &["fstats"])
        .add_filter("CSV Spreadsheet", &["csv"])
        .set_file_name("results.fstats")
        .save_file()
    {
        let path = ensure_extension(path, "fstats");
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("fstats");
        snap.label = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let windows = &app.cache.session_history;
        let text = if ext == "csv" {
            serialize_results_csv(&snap, windows)
        } else {
            serialize_results_fstats(&snap, windows)
        };
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.cache.config_status = Some(format!(
                    "Results exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                app.cache.config_error = None;
            }
            Err(e) => {
                app.cache.config_error = Some(format!("Export failed: {e}"));
                app.cache.config_status = None;
            }
        }
    } else {
        open_path_input(app, PathInputAction::SaveResults);
    }
}

pub(crate) fn do_export_pipeline_results(app: &mut App) {
    let mut snap = capture_selected_pipeline_export_snapshot(app);
    if let Some(path) = OSFileDialog::new()
        .add_filter("Pipeline Stats", &["pstats"])
        .add_filter("CSV Spreadsheet", &["csv"])
        .set_file_name("pipeline.pstats")
        .save_file()
    {
        let path = ensure_extension(path, "pstats");
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("pstats");
        snap.label = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let text = if ext == "csv" {
            serialize_pipeline_results_csv(&snap)
        } else {
            serialize_pipeline_results_pstats(&snap)
        };
        match std::fs::write(&path, &text) {
            Ok(()) => {
                app.pipeline.status_msg = Some(format!(
                    "Pipeline results exported to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                app.pipeline.status_error = None;
            }
            Err(e) => {
                app.pipeline.status_error = Some(format!("Export failed: {e}"));
                app.pipeline.status_msg = None;
            }
        }
    } else {
        open_path_input(app, PathInputAction::SavePipelineResults);
    }
}

pub(super) fn write_level_snap(s: &mut String, prefix: &str, l: &LevelSnapshot) {
    s.push_str(&format!("{prefix}.name={}\n", l.name));
    s.push_str(&format!("{prefix}.size={}\n", l.size));
    s.push_str(&format!("{prefix}.line_size={}\n", l.line_size));
    s.push_str(&format!("{prefix}.associativity={}\n", l.associativity));
    s.push_str(&format!("{prefix}.replacement={}\n", l.replacement));
    s.push_str(&format!("{prefix}.write_policy={}\n", l.write_policy));
    s.push_str(&format!("{prefix}.hit_latency={}\n", l.hit_latency));
    s.push_str(&format!("{prefix}.miss_penalty={}\n", l.miss_penalty));
    s.push_str(&format!("{prefix}.hits={}\n", l.hits));
    s.push_str(&format!("{prefix}.misses={}\n", l.misses));
    s.push_str(&format!("{prefix}.evictions={}\n", l.evictions));
    s.push_str(&format!("{prefix}.writebacks={}\n", l.writebacks));
    s.push_str(&format!("{prefix}.bytes_loaded={}\n", l.bytes_loaded));
    s.push_str(&format!("{prefix}.bytes_stored={}\n", l.bytes_stored));
    s.push_str(&format!("{prefix}.total_cycles={}\n", l.total_cycles));
    s.push_str(&format!("{prefix}.ram_write_bytes={}\n", l.ram_write_bytes));
    s.push_str(&format!("{prefix}.amat={:.4}\n", l.amat));
}

pub(super) fn serialize_results_fstats(
    snap: &CacheResultsSnapshot,
    windows: &[CacheResultsSnapshot],
) -> String {
    let pipeline_mode = snap.pipeline.is_some();
    let mut s = if pipeline_mode {
        String::from("# FALCON-ASM Simulation Results v2\n")
    } else {
        String::from("# FALCON-ASM Simulation Results v1\n")
    };
    s.push_str(&format!("label={}\n", snap.label));
    s.push_str(&format!(
        "prog.clock_model={}\n",
        if pipeline_mode { "pipeline" } else { "serial" }
    ));
    s.push_str(&format!("prog.instructions={}\n", snap.instruction_count));
    s.push_str(&format!("prog.instr_start={}\n", snap.instr_start));
    s.push_str(&format!("prog.instr_end={}\n", snap.instr_end));
    s.push_str(&format!("prog.total_cycles={}\n", snap.total_cycles));
    if !pipeline_mode {
        s.push_str(&format!("prog.base_cycles={}\n", snap.base_cycles));
        s.push_str(&format!(
            "prog.cache_cycles={}\n",
            snap.total_cycles.saturating_sub(snap.base_cycles)
        ));
    }
    s.push_str(&format!("prog.cpi={:.4}\n", snap.cpi));
    s.push_str(&format!("prog.ipc={:.4}\n", snap.ipc));
    if let Some(p) = &snap.pipeline {
        s.push_str("pipeline.enabled=true\n");
        s.push_str(&format!("pipeline.scope={}\n", p.scope));
        s.push_str(&format!("pipeline.committed={}\n", p.committed));
        s.push_str(&format!("pipeline.cycles={}\n", p.cycles));
        s.push_str(&format!("pipeline.stalls={}\n", p.stalls));
        s.push_str(&format!("pipeline.flushes={}\n", p.flushes));
        s.push_str(&format!("pipeline.cpi={:.4}\n", p.cpi));
        s.push_str(&format!("pipeline.branches={}\n", p.branches));
        s.push_str(&format!("pipeline.raw_stalls={}\n", p.raw_stalls));
        s.push_str(&format!("pipeline.load_use_stalls={}\n", p.load_use_stalls));
        s.push_str(&format!("pipeline.branch_stalls={}\n", p.branch_stalls));
        s.push_str(&format!("pipeline.fu_stalls={}\n", p.fu_stalls));
        s.push_str(&format!("pipeline.mem_stalls={}\n", p.mem_stalls));
        s.push_str(&format!("pipeline.bypass={}\n", p.bypass));
        s.push_str(&format!("pipeline.mode={}\n", p.mode));
        s.push_str(&format!("pipeline.branch_resolve={}\n", p.branch_resolve));
        s.push_str(&format!("pipeline.branch_predict={}\n", p.branch_predict));
    }
    s.push_str(&format!("extra_levels={}\n", snap.extra_levels.len()));
    write_level_snap(&mut s, "icache", &snap.icache);
    write_level_snap(&mut s, "dcache", &snap.dcache);
    for (i, lvl) in snap.extra_levels.iter().enumerate() {
        write_level_snap(&mut s, &format!("l{}", i + 2), lvl);
    }
    let cpi = &snap.cpi_config;
    s.push_str(&format!(
        "cpi.alu={}\ncpi.mul={}\ncpi.div={}\n",
        cpi.alu, cpi.mul, cpi.div
    ));
    s.push_str(&format!("cpi.load={}\ncpi.store={}\n", cpi.load, cpi.store));
    s.push_str(&format!(
        "cpi.branch_taken={}\ncpi.branch_not_taken={}\n",
        cpi.branch_taken, cpi.branch_not_taken
    ));
    s.push_str(&format!(
        "cpi.jump={}\ncpi.system={}\n",
        cpi.jump, cpi.system
    ));
    s.push_str(&format!(
        "miss_hotspot_count={}\n",
        snap.miss_hotspots.len()
    ));
    for (i, (pc, count)) in snap.miss_hotspots.iter().enumerate() {
        s.push_str(&format!("miss_hotspot.{i}.pc=0x{pc:08x}\n"));
        s.push_str(&format!("miss_hotspot.{i}.count={count}\n"));
    }
    s.push_str(&format!(
        "history_i_count={}\n",
        snap.hit_rate_history_i.len()
    ));
    for (i, (x, y)) in snap.hit_rate_history_i.iter().enumerate() {
        s.push_str(&format!("history_i.{i}={x}:{y}\n"));
    }
    s.push_str(&format!(
        "history_d_count={}\n",
        snap.hit_rate_history_d.len()
    ));
    for (i, (x, y)) in snap.hit_rate_history_d.iter().enumerate() {
        s.push_str(&format!("history_d.{i}={x}:{y}\n"));
    }
    // Window snapshots
    s.push_str("\n# --- Window Snapshots ---\n");
    s.push_str(&format!("window_count={}\n", windows.len()));
    for (n, w) in windows.iter().enumerate() {
        let i_total = w.icache.hits + w.icache.misses;
        let d_total = w.dcache.hits + w.dcache.misses;
        let i_miss_rate = if i_total == 0 {
            0.0
        } else {
            w.icache.misses as f64 / i_total as f64 * 100.0
        };
        let d_miss_rate = if d_total == 0 {
            0.0
        } else {
            w.dcache.misses as f64 / d_total as f64 * 100.0
        };
        s.push_str(&format!("window.{n}.label={}\n", w.label));
        s.push_str(&format!("window.{n}.instr_start={}\n", w.instr_start));
        s.push_str(&format!("window.{n}.instr_end={}\n", w.instr_end));
        s.push_str(&format!("window.{n}.total_cycles={}\n", w.total_cycles));
        s.push_str(&format!("window.{n}.cpi={:.4}\n", w.cpi));
        s.push_str(&format!("window.{n}.icache.hits={}\n", w.icache.hits));
        s.push_str(&format!("window.{n}.icache.misses={}\n", w.icache.misses));
        s.push_str(&format!("window.{n}.icache.miss_rate={:.4}\n", i_miss_rate));
        s.push_str(&format!("window.{n}.icache.amat={:.4}\n", w.icache.amat));
        s.push_str(&format!("window.{n}.dcache.hits={}\n", w.dcache.hits));
        s.push_str(&format!("window.{n}.dcache.misses={}\n", w.dcache.misses));
        s.push_str(&format!("window.{n}.dcache.miss_rate={:.4}\n", d_miss_rate));
        s.push_str(&format!("window.{n}.dcache.amat={:.4}\n", w.dcache.amat));
        let n_extra = w.extra_levels.len();
        if n_extra > 0 {
            s.push_str(&format!("window.{n}.extra_count={n_extra}\n"));
            for (k, lvl) in w.extra_levels.iter().enumerate() {
                s.push_str(&format!("window.{n}.extra.{k}.name={}\n", lvl.name));
                s.push_str(&format!("window.{n}.extra.{k}.hits={}\n", lvl.hits));
                s.push_str(&format!("window.{n}.extra.{k}.misses={}\n", lvl.misses));
                s.push_str(&format!("window.{n}.extra.{k}.amat={:.4}\n", lvl.amat));
            }
        }
    }
    s
}

pub(super) fn csv_level_row(s: &mut String, label: &str, l: &LevelSnapshot, instructions: u64) {
    let total = l.hits + l.misses;
    let hit_rate = if total == 0 {
        0.0
    } else {
        l.hits as f64 / total as f64 * 100.0
    };
    let miss_rate = 100.0 - hit_rate;
    let mpki = if instructions == 0 {
        0.0
    } else {
        l.misses as f64 / instructions as f64 * 1000.0
    };
    s.push_str(&format!(
        "{label},{},{},{},{:.1},{:.1},{:.2},{:.2},{},{},{},{},{}\n",
        l.hits,
        l.misses,
        total,
        hit_rate,
        miss_rate,
        mpki,
        l.amat,
        l.evictions,
        l.writebacks,
        l.bytes_loaded,
        l.ram_write_bytes,
        l.total_cycles
    ));
}

pub(super) fn serialize_results_csv(
    snap: &CacheResultsSnapshot,
    windows: &[CacheResultsSnapshot],
) -> String {
    let mut s = String::new();
    let pipeline_mode = snap.pipeline.is_some();
    s.push_str("PROGRAM SUMMARY\n");
    if pipeline_mode {
        s.push_str("Clock Model,Instructions,Total Cycles,CPI,IPC\n");
        s.push_str(&format!(
            "pipeline,{},{},{:.4},{:.4}\n",
            snap.instruction_count, snap.total_cycles, snap.cpi, snap.ipc
        ));
    } else {
        s.push_str("Clock Model,Instructions,Total Cycles,Base Cycles,Cache Cycles,CPI,IPC\n");
        s.push_str(&format!(
            "serial,{},{},{},{},{:.4},{:.4}\n",
            snap.instruction_count,
            snap.total_cycles,
            snap.base_cycles,
            snap.total_cycles.saturating_sub(snap.base_cycles),
            snap.cpi,
            snap.ipc
        ));
    }
    s.push('\n');
    if let Some(p) = &snap.pipeline {
        s.push_str("PIPELINE SUMMARY\n");
        s.push_str("Committed,Cycles,Stalls,Flushes,CPI,Branches,RAW Stalls,Load-Use Stalls,Branch Stalls,FU Stalls,Mem Stalls,Bypass,Mode,Branch Resolve,Branch Predict\n");
        s.push_str(&format!(
            "{},{},{},{},{:.4},{},{},{},{},{},{},{},{},{},{}\n\n",
            p.committed,
            p.cycles,
            p.stalls,
            p.flushes,
            p.cpi,
            p.branches,
            p.raw_stalls,
            p.load_use_stalls,
            p.branch_stalls,
            p.fu_stalls,
            p.mem_stalls,
            p.bypass,
            p.mode,
            p.branch_resolve,
            p.branch_predict
        ));
    }
    s.push_str("CACHE LEVELS\n");
    s.push_str("Level,Hits,Misses,Total Accesses,Hit Rate (%),Miss Rate (%),MPKI,AMAT (cycles),Evictions,Writebacks,RAM Reads (B),RAM Writes (B),Total Cycles\n");
    csv_level_row(&mut s, "I-Cache L1", &snap.icache, snap.instruction_count);
    csv_level_row(&mut s, "D-Cache L1", &snap.dcache, snap.instruction_count);
    for lvl in &snap.extra_levels {
        csv_level_row(&mut s, &lvl.name, lvl, snap.instruction_count);
    }
    s.push('\n');
    s.push_str("MISS HOTSPOTS (I-Cache)\n");
    s.push_str("PC,Miss Count\n");
    for (pc, count) in &snap.miss_hotspots {
        s.push_str(&format!("0x{pc:08x},{count}\n"));
    }
    if !windows.is_empty() {
        s.push('\n');
        s.push_str("WINDOW SNAPSHOTS\n");
        s.push_str("Window,Instructions,I-Cache Hits,I-Cache Misses,I-Cache Miss Rate (%),I-Cache Access Time,D-Cache Hits,D-Cache Misses,D-Cache Miss Rate (%),D-Cache Access Time,Total Cycles,CPI\n");
        for w in windows {
            let instr = w.instr_end.saturating_sub(w.instr_start);
            let i_total = w.icache.hits + w.icache.misses;
            let d_total = w.dcache.hits + w.dcache.misses;
            let i_miss_rate = if i_total == 0 {
                0.0
            } else {
                w.icache.misses as f64 / i_total as f64 * 100.0
            };
            let d_miss_rate = if d_total == 0 {
                0.0
            } else {
                w.dcache.misses as f64 / d_total as f64 * 100.0
            };
            s.push_str(&format!(
                "{},{},{},{},{:.1},{:.2},{},{},{:.1},{:.2},{},{:.4}\n",
                w.label,
                instr,
                w.icache.hits,
                w.icache.misses,
                i_miss_rate,
                w.icache.amat,
                w.dcache.hits,
                w.dcache.misses,
                d_miss_rate,
                w.dcache.amat,
                w.total_cycles,
                w.cpi,
            ));
        }
    }
    s
}

pub(super) fn serialize_pipeline_results_pstats(snap: &CacheResultsSnapshot) -> String {
    let mut s = String::from("# Raven Pipeline Results v2\n");
    s.push_str(&format!("label={}\n", snap.label));
    s.push_str("prog.clock_model=pipeline\n");
    s.push_str(&format!("prog.instructions={}\n", snap.instruction_count));
    s.push_str(&format!("prog.total_cycles={}\n", snap.total_cycles));
    s.push_str(&format!("prog.cpi={:.4}\n", snap.cpi));
    s.push_str(&format!("prog.ipc={:.4}\n", snap.ipc));

    if let Some(p) = &snap.pipeline {
        s.push_str("pipeline.enabled=true\n");
        s.push_str(&format!("pipeline.scope={}\n", p.scope));
        s.push_str(&format!("pipeline.committed={}\n", p.committed));
        s.push_str(&format!("pipeline.cycles={}\n", p.cycles));
        s.push_str(&format!("pipeline.stalls={}\n", p.stalls));
        s.push_str(&format!("pipeline.flushes={}\n", p.flushes));
        s.push_str(&format!("pipeline.cpi={:.4}\n", p.cpi));
        s.push_str(&format!("pipeline.branches={}\n", p.branches));
        s.push_str(&format!("pipeline.raw_stalls={}\n", p.raw_stalls));
        s.push_str(&format!("pipeline.load_use_stalls={}\n", p.load_use_stalls));
        s.push_str(&format!("pipeline.branch_stalls={}\n", p.branch_stalls));
        s.push_str(&format!("pipeline.fu_stalls={}\n", p.fu_stalls));
        s.push_str(&format!("pipeline.mem_stalls={}\n", p.mem_stalls));
        s.push_str(&format!("pipeline.bypass={}\n", p.bypass));
        s.push_str(&format!("pipeline.mode={}\n", p.mode));
        s.push_str(&format!("pipeline.branch_resolve={}\n", p.branch_resolve));
        s.push_str(&format!("pipeline.branch_predict={}\n", p.branch_predict));
    } else {
        s.push_str("pipeline.enabled=false\n");
    }

    s
}

pub(super) fn serialize_pipeline_results_csv(snap: &CacheResultsSnapshot) -> String {
    let mut s = String::new();
    s.push_str("PROGRAM SUMMARY\n");
    s.push_str("Instructions,Total Cycles,CPI,IPC\n");
    s.push_str(&format!(
        "{},{},{:.4},{:.4}\n\n",
        snap.instruction_count, snap.total_cycles, snap.cpi, snap.ipc
    ));

    s.push_str("PIPELINE SUMMARY\n");
    s.push_str("Scope,Committed,Cycles,Stalls,Flushes,CPI,Control Ops,RAW Stalls,Load-Use Stalls,Branch Stalls,FU Stalls,Mem Stalls,Bypass,Mode,Branch Resolve,Branch Predict\n");
    if let Some(p) = &snap.pipeline {
        s.push_str(&format!(
            "{},{},{},{},{},{:.4},{},{},{},{},{},{},{},{},{},{}\n",
            p.scope,
            p.committed,
            p.cycles,
            p.stalls,
            p.flushes,
            p.cpi,
            p.branches,
            p.raw_stalls,
            p.load_use_stalls,
            p.branch_stalls,
            p.fu_stalls,
            p.mem_stalls,
            p.bypass,
            p.mode,
            p.branch_resolve,
            p.branch_predict
        ));
    }

    s
}

pub(super) fn apply_imem_search(app: &mut App) {
    let q = app.run.imem_search_query.trim().to_lowercase();
    if q.is_empty() {
        app.run.imem_search_matches.clear();
        app.run.imem_search_cursor = 0;
        app.run.imem_search_match_count = 0;
        return;
    }

    let matches: Vec<u32> = if q.starts_with("0x") {
        // Address lookup: parse hex, check if it's within the loaded program
        let hex = q.trim_start_matches("0x");
        if let Ok(addr) = u32::from_str_radix(hex, 16) {
            // Align to 4 bytes (instructions are word-aligned)
            let addr = addr & !3;
            if app.run.imem_vrow_cache.contains_key(&addr) {
                vec![addr]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    } else {
        let vrow_cache = &app.run.imem_vrow_cache;
        let mut v: Vec<u32> = app
            .run
            .labels_lower
            .iter()
            .filter(|(addr, labels_lc)| {
                vrow_cache.contains_key(*addr) && labels_lc.iter().any(|l| l.contains(&q))
            })
            .map(|(&addr, _)| addr)
            .collect();
        v.sort();
        v
    };

    app.run.imem_search_match_count = matches.len();
    // Scroll to first match; cursor is reset to 0 on every query change
    if let Some(&addr) = matches.first() {
        app.scroll_imem_to_addr(addr);
    }
    app.run.imem_search_cursor = 0;
    app.run.imem_search_matches = matches;
}

pub(super) fn apply_mem_search(app: &mut App) {
    let q = app
        .run
        .mem_search_query
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if let Ok(addr) = u32::from_str_radix(q, 16) {
        let aligned = addr & !(app.run.mem_view_bytes - 1);
        let max = app
            .run
            .mem_size
            .saturating_sub(app.run.mem_view_bytes as usize) as u32;
        app.run.mem_view_addr = aligned.min(max);
        app.run.mem_region = crate::ui::app::MemRegion::Custom;
    }
}

/// Fuzzy score for how well `name` matches `prefix` (lower = better match).
/// Returns None if the match is too poor to include.
pub(super) fn fuzzy_score(name: &str, prefix: &str) -> Option<i32> {
    if prefix.is_empty() {
        return Some(0);
    }
    let name_lc = name.to_lowercase();
    let pfx_lc = prefix.to_lowercase();

    // Tier 0: exact case-insensitive prefix
    if name_lc.starts_with(&pfx_lc) {
        return Some(0);
    }
    // Tier 1: case-insensitive substring anywhere
    if let Some(pos) = name_lc.find(&pfx_lc) {
        return Some(100 + pos as i32);
    }
    // Tier 2: all prefix chars appear as a subsequence (in order)
    let pfx_chars: Vec<char> = pfx_lc.chars().collect();
    let mut pi = 0usize;
    for nc in name_lc.chars() {
        if pi < pfx_chars.len() && nc == pfx_chars[pi] {
            pi += 1;
        }
    }
    if pi == pfx_chars.len() {
        // Score by name length — shorter name = tighter match
        return Some(200 + name.len() as i32);
    }
    // Tier 3: Levenshtein on the first N chars of name vs prefix
    // Allow 1 edit per 3 chars of prefix, minimum 1
    let max_dist = (pfx_lc.chars().count() / 3).max(1);
    let name_head: String = name_lc.chars().take(pfx_lc.chars().count() + 1).collect();
    let dist = levenshtein(&name_head, &pfx_lc);
    if dist <= max_dist {
        return Some(400 + dist as i32 * 50);
    }
    None
}

pub(super) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    // Rolling two-row DP — O(n) space
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

pub(super) fn refresh_path_completions(input: &mut PathInput) {
    let query = &input.query;
    let path = std::path::Path::new(query);
    let (dir, prefix) = if query.ends_with('/') || query.ends_with(std::path::MAIN_SEPARATOR) {
        (path.to_path_buf(), String::new())
    } else {
        let parent = path
            .parent()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    std::path::Path::new(".").to_path_buf()
                } else {
                    p.to_path_buf()
                }
            })
            .unwrap_or_else(|| std::path::Path::new(".").to_path_buf());
        let pfx = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (parent, pfx)
    };

    let mut scored: Vec<(i32, String)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let fname = e.file_name();
            let name = fname.to_str()?;
            let score = fuzzy_score(name, &prefix)?;
            let p = e.path();
            let mut s = p.to_string_lossy().to_string();
            let is_dir = p.is_dir();
            if is_dir {
                s.push('/');
            }
            // Within the same tier, directories sort before files
            let dir_penalty = if is_dir { 0 } else { 1 };
            Some((score * 10 + dir_penalty, s))
        })
        .collect();

    scored.sort_by(|(sa, na), (sb, nb)| sa.cmp(sb).then(na.cmp(nb)));
    input.completions = scored.into_iter().map(|(_, s)| s).collect();
    input.completion_sel = 0;
}

pub(crate) fn open_path_input(app: &mut App, action: PathInputAction) {
    app.path_input.action = action;
    app.path_input.open = true;
    app.path_input.query = std::env::current_dir()
        .map(|p| {
            let mut s = p.to_string_lossy().to_string();
            s.push('/');
            s
        })
        .unwrap_or_default();
    refresh_path_completions(&mut app.path_input);
}

fn ensure_extension(path: std::path::PathBuf, ext: &str) -> std::path::PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some(existing) if !existing.is_empty() => path,
        _ => {
            let mut path = path;
            path.set_extension(ext);
            path
        }
    }
}

/// Auto-detect whether `path` is a binary (ELF / FALC / non-UTF-8) or assembly
/// source and load it accordingly. Used by Ctrl+O and the path-input overlay.
pub(super) fn open_file_autodetect(app: &mut App, path: &std::path::Path) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let is_binary = bytes.starts_with(b"\x7fELF")
        || bytes.starts_with(b"FALC")
        || std::str::from_utf8(&bytes).is_err();
    if is_binary {
        app.load_binary(&bytes);
        use crate::ui::view::disasm::disasm_word;
        let lines: Vec<String> = if let Some(ref words) = app.editor.last_ok_text {
            words.iter().map(|&w| disasm_word(w)).collect()
        } else {
            bytes
                .chunks(4)
                .map(|chunk| {
                    let mut b = [0u8; 4];
                    for (i, &v) in chunk.iter().enumerate() {
                        b[i] = v;
                    }
                    disasm_word(u32::from_le_bytes(b))
                })
                .collect()
        };
        app.editor.buf.lines = lines;
        app.editor.buf.cursor_row = 0;
        app.editor.buf.cursor_col = 0;
    } else {
        // SAFETY: from_utf8 succeeded above.
        let content = unsafe { String::from_utf8_unchecked(bytes) };
        app.editor.buf.lines = content.lines().map(|s| s.to_string()).collect();
        app.editor.buf.cursor_row = 0;
        app.editor.buf.cursor_col = 0;
        app.assemble_and_load();
    }
}

pub(super) fn dispatch_path_input(
    app: &mut App,
    action: PathInputAction,
    path: std::path::PathBuf,
) {
    match action {
        PathInputAction::OpenFas => {
            open_file_autodetect(app, &path);
        }
        PathInputAction::SaveFas => {
            let _ = std::fs::write(&path, app.editor.buf.text());
        }
        PathInputAction::OpenBin => {
            if let Ok(bytes) = std::fs::read(&path) {
                app.load_binary(&bytes);
                use crate::ui::view::disasm::disasm_word;
                let lines: Vec<String> = if let Some(ref words) = app.editor.last_ok_text {
                    words.iter().map(|&w| disasm_word(w)).collect()
                } else {
                    bytes
                        .chunks(4)
                        .map(|chunk| {
                            let mut b = [0u8; 4];
                            for (i, &v) in chunk.iter().enumerate() {
                                b[i] = v;
                            }
                            disasm_word(u32::from_le_bytes(b))
                        })
                        .collect()
                };
                app.editor.buf.lines = lines;
                app.editor.buf.cursor_row = 0;
                app.editor.buf.cursor_col = 0;
            }
        }
        PathInputAction::SaveBin => {
            let (words, data, bss_size) = match (
                app.editor.last_ok_text.as_ref(),
                app.editor.last_ok_data.as_ref(),
                app.editor.last_ok_bss_size,
            ) {
                (Some(t), Some(d), bss) => (t.clone(), d.clone(), bss.unwrap_or(0)),
                _ => match crate::falcon::asm::assemble(&app.editor.buf.text(), app.run.base_pc) {
                    Ok(p) => (p.text, p.data, p.bss_size),
                    Err(e) => {
                        app.console.push_error(format!(
                            "Cannot export: assemble error at line {}: {}",
                            e.line + 1,
                            e.msg
                        ));
                        return;
                    }
                },
            };
            let text_bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            let text_size = text_bytes.len() as u32;
            let data_size = data.len() as u32;
            let mut bytes: Vec<u8> = Vec::with_capacity(16 + text_bytes.len() + data.len());
            bytes.extend_from_slice(b"FALC");
            bytes.extend_from_slice(&text_size.to_le_bytes());
            bytes.extend_from_slice(&data_size.to_le_bytes());
            bytes.extend_from_slice(&bss_size.to_le_bytes());
            bytes.extend_from_slice(&text_bytes);
            bytes.extend_from_slice(&data);
            let _ = std::fs::write(&path, bytes);
        }
        PathInputAction::OpenFcache => match std::fs::read_to_string(&path) {
            Ok(text) => match parse_cache_configs(&text) {
                Ok((icfg, dcfg, extra)) => {
                    let n_extra = extra.len();
                    app.cache.pending_icache = icfg;
                    app.cache.pending_dcache = dcfg;
                    app.cache.extra_pending = extra;
                    app.run.mem.extra_levels.clear();
                    for cfg in &app.cache.extra_pending {
                        app.run
                            .mem
                            .extra_levels
                            .push(crate::falcon::cache::Cache::new(cfg.clone()));
                    }
                    if app.cache.selected_level > n_extra {
                        app.cache.selected_level = n_extra;
                    }
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Import failed: {e}"));
            }
        },
        PathInputAction::SaveFcache => {
            let text = serialize_cache_configs(
                &app.cache.pending_icache,
                &app.cache.pending_dcache,
                &app.cache.extra_pending,
            );
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Exported to {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(e) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Export failed: {e}"));
                }
            }
        }
        PathInputAction::OpenRcfg => match std::fs::read_to_string(&path) {
            Ok(text) => match parse_rcfg(&text) {
                Ok((cpi, cache_enabled, run_scope, mem_bytes)) => {
                    app.run.cpi_config = cpi;
                    app.set_cache_enabled(cache_enabled);
                    app.run_scope = run_scope;
                    if let Some(bytes) = mem_bytes {
                        if bytes != app.run.mem_size {
                            app.ram_override = Some(bytes);
                            app.restart_simulation();
                        }
                    }
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Settings imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Import failed: {e}"));
            }
        },
        PathInputAction::SaveRcfg => {
            let text = serialize_rcfg(
                &app.run.cpi_config,
                app.run.cache_enabled,
                app.run_scope,
                app.run.mem_size / 1024,
            );
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Settings exported to {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(e) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Export failed: {e}"));
                }
            }
        }
        PathInputAction::OpenPcfg => match std::fs::read_to_string(&path) {
            Ok(text) => match parse_pcfg(&text) {
                Ok(cfg) => {
                    cfg.apply_to_state(&mut app.pipeline);
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Pipeline config imported from {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(msg) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Import failed: {msg}"));
                }
            },
            Err(e) => {
                app.cache.config_status = None;
                app.cache.config_error = Some(format!("Import failed: {e}"));
            }
        },
        PathInputAction::SavePcfg => {
            let path = ensure_extension(path, "pcfg");
            let text = serialize_pcfg(&app.pipeline);
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    app.cache.config_error = None;
                    app.cache.config_status = Some(format!(
                        "Pipeline config exported to {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(e) => {
                    app.cache.config_status = None;
                    app.cache.config_error = Some(format!("Export failed: {e}"));
                }
            }
        }
        PathInputAction::SaveResults => {
            let path = ensure_extension(path, "fstats");
            let mut snap = capture_snapshot(app);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("fstats");
            snap.label = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let windows = app.cache.session_history.clone();
            let text = if ext == "csv" {
                serialize_results_csv(&snap, &windows)
            } else {
                serialize_results_fstats(&snap, &windows)
            };
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    app.cache.config_status = Some(format!(
                        "Results exported to {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    app.cache.config_error = None;
                }
                Err(e) => {
                    app.cache.config_error = Some(format!("Export failed: {e}"));
                    app.cache.config_status = None;
                }
            }
        }
        PathInputAction::SavePipelineResults => {
            let path = ensure_extension(path, "pstats");
            let mut snap = capture_selected_pipeline_export_snapshot(app);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("pstats");
            snap.label = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let text = if ext == "csv" {
                serialize_pipeline_results_csv(&snap)
            } else {
                serialize_pipeline_results_pstats(&snap)
            };
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    app.pipeline.status_msg = Some(format!(
                        "Pipeline results exported to {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    app.pipeline.status_error = None;
                }
                Err(e) => {
                    app.pipeline.status_error = Some(format!("Export failed: {e}"));
                    app.pipeline.status_msg = None;
                }
            }
        }
    }
}
