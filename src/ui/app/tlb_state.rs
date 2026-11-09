// ui/app/tlb_state.rs — UI state for the top-level Virtual Memory tab.
//
// Five subtabs, each answering one question:
//   walk   — what happened to this access (the TLB probe, every PTE the walk
//            read, the physical address, what the TLB kept and dropped)
//   tlb    — what the TLB holds and why (sets × ways, recency)
//   map    — how the page table is built (one column per level)
//   stats  — what it cost
//   config — how it is set up
//
// The three live views follow the same "current access" — the newest
// translation of the followed kind, unless the reader pinned an older one —
// so moving between subtabs keeps looking at the same thing.

use std::cell::RefCell;

use ratatui::layout::Rect;
use raven_engine::capability::{TranslationAccess, TranslationOutcome, TranslationRecordView};

use crate::falcon::cache::ReplacementPolicy;
use crate::falcon::mmu::{PageMapSpec, TlbConfig, VmMode};
use crate::ui::app::RunButton;

/// Virtual Memory subtab.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum VmSubtab {
    Walk,
    Tlb,
    Map,
    Stats,
    Config,
}

impl VmSubtab {
    pub(crate) const ALL: [VmSubtab; 5] = [
        Self::Walk,
        Self::Tlb,
        Self::Map,
        Self::Stats,
        Self::Config,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Tlb => "tlb",
            Self::Map => "map",
            Self::Stats => "stats",
            Self::Config => "config",
        }
    }
}

/// Which translations the views follow while the program runs.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum WalkFollow {
    /// Loads and stores — where a program's data lives.
    Data,
    /// Instruction fetches.
    Fetch,
    /// Whatever translated last.
    Any,
}

impl WalkFollow {
    pub(crate) const ALL: [WalkFollow; 3] = [Self::Data, Self::Fetch, Self::Any];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Fetch => "fetch",
            Self::Any => "any",
        }
    }

    pub(crate) fn admits(self, access: TranslationAccess) -> bool {
        match self {
            Self::Data => access != TranslationAccess::Fetch,
            Self::Fetch => access == TranslationAccess::Fetch,
            Self::Any => true,
        }
    }
}

/// Which records the history list shows.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum HistoryFilter {
    All,
    Misses,
    Faults,
}

impl HistoryFilter {
    pub(crate) const ALL: [HistoryFilter; 3] = [Self::All, Self::Misses, Self::Faults];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Misses => "misses",
            Self::Faults => "faults",
        }
    }

    pub(crate) fn admits(self, record: &TranslationRecordView) -> bool {
        match self {
            Self::All => true,
            Self::Misses => record.outcome != TranslationOutcome::TlbHit,
            Self::Faults => record.outcome == TranslationOutcome::Fault,
        }
    }
}

/// A number typed into the config subtab.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum VmField {
    Asid,
    HitCycles,
    WalkCycles,
}

impl VmField {
    pub(crate) const ALL: [VmField; 3] = [Self::Asid, Self::HitCycles, Self::WalkCycles];
}

/// Everything clickable in the tab. The renderer records each one with the
/// rectangle it drew it in; the mouse handler looks the pointer up in that
/// list, so a control is clickable exactly where it is drawn.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum VmHit {
    Subtab(VmSubtab),
    Run(RunButton),
    Results,
    ImportCfg,
    ExportCfg,
    FlushTlb,
    /// Write the simulator's map now (auto mode, satp still Bare).
    InstallMap,
    /// Turn virtual memory on in auto mode.
    TurnOn,

    // walk
    Older,
    Newer,
    Follow(WalkFollow),
    Filter(HistoryFilter),
    /// A history row, by translation number.
    Record(u64),
    /// The history list, for the wheel.
    History,

    // tlb
    Slot(usize),
    SetTab(usize),
    /// The TLB matrix, for the wheel.
    TlbArea,

    // map
    MapRow { column: usize, row: usize },
    /// A map column, for the wheel.
    MapColumn(usize),
    /// Back to following the current access.
    MapFollow,
    /// Start typing an address to look up.
    MapQuery,

    // stats
    Snapshot(usize),

    // config
    Mode(VmMode),
    Perm(usize),
    Global,
    Edit(VmField),
    TlbOn(bool),
    Entries(u16),
    Ways(u8),
    Policy(ReplacementPolicy),
}

/// Which map column holds the keyboard.
pub(crate) const MAP_COLUMNS: usize = 2;

pub(crate) struct TlbState {
    pub(crate) vm_subtab: VmSubtab,
    pub(crate) hover: Option<VmHit>,
    /// Every clickable region drawn this frame, last on top.
    pub(crate) hits: RefCell<Vec<(VmHit, Rect)>>,

    // ── walk ─────────────────────────────────────────────────────────────────
    pub(crate) follow: WalkFollow,
    /// The record the reader pinned; `None` follows the newest.
    pub(crate) pinned: Option<TranslationRecordView>,
    pub(crate) filter: HistoryFilter,
    /// Rows scrolled off the top of the history (newest first).
    pub(crate) history_scroll: usize,

    // ── tlb ──────────────────────────────────────────────────────────────────
    /// The slot under the cursor; `None` puts it on the slot the current
    /// access touched.
    pub(crate) tlb_cursor: Option<usize>,
    /// Sets scrolled off the top of the matrix.
    pub(crate) tlb_scroll: usize,

    // ── map ──────────────────────────────────────────────────────────────────
    pub(crate) map_column: usize,
    /// The entry index selected in each column; `None` follows the path of the
    /// current access.
    pub(crate) map_cursor: [Option<u64>; MAP_COLUMNS],
    pub(crate) map_scroll: [usize; MAP_COLUMNS],
    /// Groups of alike entries the reader opened: `(table, first index)`.
    pub(crate) map_open: Vec<(u64, u64)>,
    /// The address being typed into the lookup, while it is open.
    pub(crate) map_query: Option<String>,
    /// An address the map shows the path of, instead of the current access.
    pub(crate) map_target: Option<u64>,

    // ── config ───────────────────────────────────────────────────────────────
    /// What the simulator's map grants, applied whenever it is (re)written.
    pub(crate) page_map: PageMapSpec,
    /// The TLB shape the next program load builds.
    pub(crate) pending: TlbConfig,
    /// The number field being typed into, and its text so far.
    pub(crate) edit: Option<(VmField, String)>,

    /// Footer status line (✓ / ✗ at the bottom of the screen).
    pub(crate) config_error: Option<String>,
    pub(crate) config_status: Option<String>,
}

impl Default for TlbState {
    fn default() -> Self {
        Self {
            vm_subtab: VmSubtab::Walk,
            hover: None,
            hits: RefCell::new(Vec::new()),
            follow: WalkFollow::Data,
            pinned: None,
            filter: HistoryFilter::All,
            history_scroll: 0,
            tlb_cursor: None,
            tlb_scroll: 0,
            map_column: 0,
            map_cursor: [None; MAP_COLUMNS],
            map_scroll: [0; MAP_COLUMNS],
            map_open: Vec::new(),
            map_query: None,
            map_target: None,
            page_map: PageMapSpec::default(),
            pending: TlbConfig::default(),
            edit: None,
            config_error: None,
            config_status: None,
        }
    }
}

impl TlbState {
    /// Record a clickable region for this frame.
    pub(crate) fn hit(&self, target: VmHit, rect: Rect) {
        if rect.width > 0 && rect.height > 0 {
            self.hits.borrow_mut().push((target, rect));
        }
    }

    /// The topmost region under `(column, row)`.
    pub(crate) fn hit_at(&self, column: u16, row: u16) -> Option<VmHit> {
        self.hits
            .borrow()
            .iter()
            .rev()
            .find(|(_, r)| {
                column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height
            })
            .map(|(hit, _)| *hit)
    }

    pub(crate) fn is_hovered(&self, target: VmHit) -> bool {
        self.hover == Some(target)
    }

    /// Stop pinning anything: every view goes back to the current access.
    pub(crate) fn follow_current(&mut self) {
        self.pinned = None;
        self.tlb_cursor = None;
        self.map_cursor = [None; MAP_COLUMNS];
        self.map_target = None;
        self.history_scroll = 0;
    }
}
