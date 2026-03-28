use super::Tab;

/// Pages in the Docs tab. Tab key cycles through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsPage {
    InstrRef,
    Syscalls,
    MemoryMap,
    FcacheRef,
}

impl DocsPage {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::InstrRef => Self::Syscalls,
            Self::Syscalls => Self::MemoryMap,
            Self::MemoryMap => Self::FcacheRef,
            Self::FcacheRef => Self::InstrRef,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::InstrRef => "Instr Ref",
            Self::Syscalls => "Syscalls",
            Self::MemoryMap => "Memory Map",
            Self::FcacheRef => "Config Ref",
        }
    }
}

/// UI language for Syscalls and MemoryMap pages. L key toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsLang {
    En,
    PtBr,
}

impl DocsLang {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::En => Self::PtBr,
            Self::PtBr => Self::En,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::En => "EN",
            Self::PtBr => "PT-BR",
        }
    }
}

pub(crate) struct DocsState {
    pub(crate) page: DocsPage,
    pub(crate) lang: DocsLang,
    pub(crate) scroll: usize,
    pub(crate) search_open: bool,
    pub(crate) search_query: String,
    pub(crate) hover_page: Option<DocsPage>,
    /// Bitmask of visible type categories (see docs::ALL_MASK / TY_* constants).
    pub(crate) type_filter: u16,
    /// Cursor position in the filter bar: 0 = "All", 1–12 = individual types.
    pub(crate) filter_cursor: usize,
    // ── Render-side position tracking (set by render, read by mouse handler) ──
    /// Y row of the page tab bar (relative to terminal origin).
    pub(crate) tab_bar_y: std::cell::Cell<u16>,
    /// (x_start, x_end) for each of the 4 page tabs, relative to terminal origin.
    pub(crate) tab_bar_xs: std::cell::Cell<[(u16, u16); 4]>,
    /// Y row of the filter bar (InstrRef page only).
    pub(crate) filter_bar_y: std::cell::Cell<u16>,
}

// ── Tutorial state ─────────────────────────────────────────────────────────────

/// State for the interactive guided tutorial ([?] button).
pub struct TutorialState {
    pub active: bool,
    pub(crate) tab: Tab,
    pub step_idx: usize,
    pub lang: DocsLang,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            active: false,
            tab: Tab::Editor,
            step_idx: 0,
            lang: DocsLang::PtBr,
        }
    }
}

// ── Path input bar ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Default)]
pub(crate) enum PathInputAction {
    #[default]
    OpenFas,
    SaveFas,
    OpenBin,
    SaveBin,
    OpenFcache,
    SaveFcache,
    OpenRcfg,
    SaveRcfg,
    OpenPcfg,
    SavePcfg,
    SaveResults,
}

pub(crate) struct PathInput {
    pub(crate) open: bool,
    pub(crate) query: String,
    pub(crate) completions: Vec<String>,
    pub(crate) completion_sel: usize,
    pub(crate) action: PathInputAction,
}

impl PathInput {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            completions: Vec::new(),
            completion_sel: 0,
            action: PathInputAction::default(),
        }
    }
}
