//! Guided-learning activity presets.
//!
//! This module is intentionally self-contained so it can be removed without
//! touching the rest of the codebase — just delete the directory and the
//! `pub mod guided_learning;` line in `src/lib.rs`.

mod configs;
mod programs;
pub mod keys;
pub mod view;

use crate::ui::{App, Tab, apply_fcache_text, apply_pcfg_text, apply_rcfg_text};

// ── Preset enum ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuidedPreset {
    // D1 — Pipeline Hazards
    D1_01, // independent instructions   (R100 + P100 + D101)
    D1_02, // RAW dependency chain        (R100 + P100 + D102)
    // D2 — Load-use / Flush
    D2_01, // load-use stall              (R100 + P100 + D201)
    D2_02, // control flush               (R100 + P100 + D202)
    // D3 — Cache & AMAT
    D3_01, // AMAT config A 16KB lru      (R300 + P101 + C311 + D301)
    D3_02, // AMAT config B 64KB lru      (R300 + P101 + C312 + D301)
    D3_03, // streaming LRU               (R300 + P101 + C321 + D301)
    D3_04, // streaming FIFO              (R300 + P101 + C322 + D301)
    D3_05, // thrashing 2-way             (R300 + P101 + C331 + D302)
    D3_06, // thrashing 8-way             (R300 + P101 + C332 + D302)
    // D4 — Encoding
    D4_01, // R-type vs S-type            (R100 + P101 + D401)
    // D5 — Multi-core
    D5_01, // 2 cores independent regs    (R500 + P101 + D501)
    // D6 — Pipeline Speedup
    D6_01, // without pipeline            (R100 + P101 + D102)
    D6_02, // with pipeline               (R100 + P100 + D102)
}

impl GuidedPreset {
    /// Short label shown in the preset list (e.g. "D1-01").
    pub fn label(self) -> &'static str {
        match self {
            Self::D1_01 => "D1-01",
            Self::D1_02 => "D1-02",
            Self::D2_01 => "D2-01",
            Self::D2_02 => "D2-02",
            Self::D3_01 => "D3-01",
            Self::D3_02 => "D3-02",
            Self::D3_03 => "D3-03",
            Self::D3_04 => "D3-04",
            Self::D3_05 => "D3-05",
            Self::D3_06 => "D3-06",
            Self::D4_01 => "D4-01",
            Self::D5_01 => "D5-01",
            Self::D6_01 => "D6-01",
            Self::D6_02 => "D6-02",
        }
    }

    /// One-line description shown next to the label.
    pub fn description(self) -> &'static str {
        match self {
            Self::D1_01 => "instrucoes independentes",
            Self::D1_02 => "cadeia de dependencias RAW",
            Self::D2_01 => "load-use stall",
            Self::D2_02 => "flush por desvio de controle",
            Self::D3_01 => "AMAT config A  (16 KB, hit=1)",
            Self::D3_02 => "AMAT config B  (64 KB, hit=4)",
            Self::D3_03 => "streaming com LRU",
            Self::D3_04 => "streaming com FIFO",
            Self::D3_05 => "thrashing 2-way (conflito)",
            Self::D3_06 => "thrashing 8-way (resolvido)",
            Self::D4_01 => "R-type vs S-type",
            Self::D5_01 => "2 cores com registradores independentes",
            Self::D6_01 => "sem pipeline — referencia",
            Self::D6_02 => "com pipeline — speedup",
        }
    }

    /// After applying this preset, which tab should be active?
    pub(crate) fn suggested_tab(self) -> Tab {
        match self {
            Self::D3_01
            | Self::D3_02
            | Self::D3_03
            | Self::D3_04
            | Self::D3_05
            | Self::D3_06 => Tab::Cache,
            Self::D5_01 => Tab::Run,
            _ => Tab::Pipeline,
        }
    }

    /// All presets in display order.
    pub fn all() -> &'static [GuidedPreset] {
        &[
            Self::D1_01,
            Self::D1_02,
            Self::D2_01,
            Self::D2_02,
            Self::D3_01,
            Self::D3_02,
            Self::D3_03,
            Self::D3_04,
            Self::D3_05,
            Self::D3_06,
            Self::D4_01,
            Self::D5_01,
            Self::D6_01,
            Self::D6_02,
        ]
    }

    /// Section header for this preset (returned when it is the first item in
    /// its domain group so the view can insert a separator).
    pub fn section_header(self) -> Option<&'static str> {
        match self {
            Self::D1_01 => Some("D1 · Hazards de Pipeline"),
            Self::D2_01 => Some("D2 · Load-use / Flush"),
            Self::D3_01 => Some("D3 · Cache e AMAT"),
            Self::D4_01 => Some("D4 · Codificacao de Instrucoes"),
            Self::D5_01 => Some("D5 · Multi-core"),
            Self::D6_01 => Some("D6 · Speedup de Pipeline"),
            _ => None,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct GuidedLearningState {
    /// Index into `GuidedPreset::all()`.
    pub cursor: usize,
    /// The last preset that was successfully applied.
    pub last_applied: Option<GuidedPreset>,
    /// One-line status message shown at the bottom of the Activity view.
    pub status_msg: Option<String>,
    /// One-line error message shown at the bottom of the Activity view.
    pub status_err: Option<String>,
}

// ── Apply ─────────────────────────────────────────────────────────────────────

/// Apply a preset to the app: loads configs, sets editor text, assembles,
/// then switches to the suggested tab.
///
/// Returns `Ok(())` on success, `Err(msg)` if any step fails.
pub fn apply_preset(app: &mut App, preset: GuidedPreset) -> Result<(), String> {
    use configs::*;
    use programs::*;

    // 1. Apply global simulator config (.rcfg)
    let rcfg_text = match preset {
        GuidedPreset::D3_01
        | GuidedPreset::D3_02
        | GuidedPreset::D3_03
        | GuidedPreset::D3_04
        | GuidedPreset::D3_05
        | GuidedPreset::D3_06 => R300,
        GuidedPreset::D5_01 => R500,
        _ => R100,
    };
    apply_rcfg_text(app, rcfg_text)?;

    // 2. Apply pipeline config (.pcfg)
    let pcfg_text = match preset {
        GuidedPreset::D1_01
        | GuidedPreset::D1_02
        | GuidedPreset::D2_01
        | GuidedPreset::D2_02
        | GuidedPreset::D6_02 => P100,
        _ => P101,
    };
    apply_pcfg_text(app, pcfg_text)?;

    // 3. Apply cache config (.fcache) — only needed for D3
    match preset {
        GuidedPreset::D3_01 => apply_fcache_text(app, C311)?,
        GuidedPreset::D3_02 => apply_fcache_text(app, C312)?,
        GuidedPreset::D3_03 => apply_fcache_text(app, C321)?,
        GuidedPreset::D3_04 => apply_fcache_text(app, C322)?,
        GuidedPreset::D3_05 => apply_fcache_text(app, C331)?,
        GuidedPreset::D3_06 => apply_fcache_text(app, C332)?,
        _ => {}
    }

    // 4. Load the program into the editor and assemble
    let prog_text = match preset {
        GuidedPreset::D1_01 => D101,
        GuidedPreset::D1_02 | GuidedPreset::D6_01 | GuidedPreset::D6_02 => D102,
        GuidedPreset::D2_01 => D201,
        GuidedPreset::D2_02 => D202,
        GuidedPreset::D3_01
        | GuidedPreset::D3_02
        | GuidedPreset::D3_03
        | GuidedPreset::D3_04 => D301,
        GuidedPreset::D3_05 | GuidedPreset::D3_06 => D302,
        GuidedPreset::D4_01 => D401,
        GuidedPreset::D5_01 => D501,
    };
    // 4b. Load program text into editor and assemble
    app.load_editor_text(prog_text);

    // 5. Reset pipeline state for a clean start
    app.pipeline_reset_to_current_pc();

    // 6. Navigate to the suggested tab
    app.navigate_to_tab(preset.suggested_tab());

    Ok(())
}
