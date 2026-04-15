// Embedded guided-activity configuration files.
// Each constant is the verbatim content of the corresponding config file in
// guided-activity/ so the binary needs no external assets.

// ── Global simulator configs (.rcfg) ─────────────────────────────────────────

/// 1-hart focus, cache off, pipeline on, all CPI = 1.  Used by D1, D2, D4, D6.
pub(super) const R100: &str = include_str!("../../guided-activity/config-global/R100.rcfg");

/// 1-hart focus, cache on, pipeline off, all CPI = 1.  Used by D3.
pub(super) const R300: &str = include_str!("../../guided-activity/config-global/R300.rcfg");

/// All-harts, cache off, pipeline off, 2 cores, all CPI = 1.  Used by D5.
pub(super) const R500: &str = include_str!("../../guided-activity/config-global/R500.rcfg");

// ── Pipeline configs (.pcfg) ──────────────────────────────────────────────────

/// Pipeline enabled (forwarding, serialized mode).
pub(super) const P100: &str = include_str!("../../guided-activity/config-pipeline/P100.pcfg");

/// Pipeline disabled.
pub(super) const P101: &str = include_str!("../../guided-activity/config-pipeline/P101.pcfg");

// ── Cache configs (.fcache) ───────────────────────────────────────────────────

/// D3/Q1 config A — 16 KB D-cache, 4-way, hit latency 1.
pub(super) const C311: &str = include_str!("../../guided-activity/config-cache/C311.fcache");

/// D3/Q1 config B — 64 KB D-cache, 4-way, hit latency 4.
pub(super) const C312: &str = include_str!("../../guided-activity/config-cache/C312.fcache");

/// D3/Q2 streaming with LRU — 256 B D-cache, 2-way, miss penalty 100.
pub(super) const C321: &str = include_str!("../../guided-activity/config-cache/C321.fcache");

/// D3/Q2 streaming with FIFO — same geometry as C321 but FIFO replacement.
pub(super) const C322: &str = include_str!("../../guided-activity/config-cache/C322.fcache");

/// D3/Q3 conflict — 4 sets, 2 ways, 16-byte lines.
pub(super) const C331: &str = include_str!("../../guided-activity/config-cache/C331.fcache");

/// D3/Q3 resolved — 4 sets, 8 ways, 16-byte lines.
pub(super) const C332: &str = include_str!("../../guided-activity/config-cache/C332.fcache");
