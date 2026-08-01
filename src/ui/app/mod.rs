mod cache_state;
mod cpi;
mod docs_state;
mod formatting;
mod hart;
mod instr_edit;
mod program;
mod run_loop;
mod run_state;
mod runtime;
mod settings_state;
mod tlb_state;

use self::cpi::classify_cpi_cycles;
use self::program::Diagnostics;
pub(crate) use self::cpi::{classify_cpi_for_display, cpi_class_label};
use self::formatting::{classify_mem_access, word_at};

// Re-export pub(crate) items from submodules so they are accessible as
// `crate::ui::app::X` from other modules in the crate.
pub(crate) use self::cache_state::{
    CacheAddrMode, CacheDataFmt, CacheDataGroup, CacheHoverTarget, CacheResultsSnapshot,
    CacheScope, CacheState, CacheSubtab, CacheViewFocus, ConfigField, LevelSnapshot,
    PipelineResultsSnapshot, TlbSnapshot,
};
pub(crate) use self::docs_state::{
    DocsLang, DocsPage, DocsState, PathInput, PathInputAction, SbDrag, TutorialState,
};
pub(crate) use self::hart::{
    HartCoreRuntime, HartLifecycle, is_transparent_single_step_word, step_hart_bg_inner,
};
pub(crate) use self::instr_edit::{EncFormat, InstrFieldKind, detect_format};
pub(crate) use self::run_state::{
    BuildStats, EditorFile, EditorMode, EditorState, FileTabId, FormatMode, MemRegion, RunButton,
    RunEditTarget, RunSpeed, RunState,
};
pub(crate) use self::settings_state::{
    RunScope, SETTINGS_ROW_CACHE_ENABLED, SETTINGS_ROW_CPI_START, SETTINGS_ROW_JIT_MODE,
    SETTINGS_ROW_MAX_CORES, SETTINGS_ROW_MEM_SIZE, SETTINGS_ROW_PIPELINE_ENABLED,
    SETTINGS_ROW_RUN_SCOPE, SETTINGS_ROW_SCREEN_TARGET, SETTINGS_ROW_TLB_ENABLED,
    SETTINGS_ROW_TRACE_SYSCALLS, SETTINGS_ROW_VM_ENABLED, SETTINGS_ROWS, SettingsState,
    nearest_pow2_clamp,
};
pub(crate) use self::tlb_state::{TlbHoverTarget, TlbState, VmSettingsField, VmSubtab};

use super::{
    console::Console,
    editor::Editor,
    input::{handle_key, handle_mouse},
    view::ui,
};
use raven_riscv_engine::capability::{
    AddressTranslation, CacheHierarchy, InstructionCodec, MemoryInspect, PipelineInspect,
    RegisterBank, RegisterEntry, RegisterFile, RegisterId,
};

/// Register age meaning "not changed recently", so the sidebar draws it plain.
pub(crate) const NO_REG_AGE: u8 = 255;

use crate::falcon::cache::CacheConfig;
use crate::falcon::machine::parse::{CellFormat, parse_cell};
use crate::falcon::machine::types::MemWidth;
use crate::falcon;
use crate::ui::platform::Clipboard;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use ratatui::{DefaultTerminal, layout::Rect};
#[cfg(unix)]
use std::sync::atomic::Ordering;
use std::sync::{Arc, atomic::AtomicBool};
use std::{
    io,
    time::{Duration, Instant},
};

pub use run_loop::run;

/// RAM handed to the RV32 runtime when `--mem` says nothing.
const DEFAULT_MEM_SIZE: usize = 16 * 1024 * 1024;

// ── CPI (Cycles Per Instruction) configuration ───────────────────────────────

pub use raven_riscv_engine::falcon::pipeline::PipelineTiming as CpiConfig;

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum Tab {
    Editor,
    Run,
    Cache,
    Tlb,
    Pipeline,
    Docs,
    Settings,
    Activity,
}

impl Tab {
    pub(super) fn all() -> &'static [Tab] {
        &[
            Tab::Editor,
            Tab::Run,
            Tab::Cache,
            Tab::Tlb,
            Tab::Pipeline,
            Tab::Docs,
            Tab::Settings,
            Tab::Activity,
        ]
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Editor => "Editor",
            Tab::Run => "Run",
            Tab::Cache => "Cache",
            Tab::Tlb => "Virtual Memory",
            Tab::Pipeline => "Pipeline",
            Tab::Docs => "Docs",
            Tab::Settings => "Settings",
            Tab::Activity => "Activity",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// RV32's runtime: CPU, cache hierarchy, MMU and pipeline behind one
/// journaling gateway, which is what makes step-back a property of the whole
/// machine rather than of any one part.
pub(crate) type FalconRuntime =
    falcon::machine::Machine<raven_riscv_engine::falcon::pipeline::PipelineSimState>;

const NOT_RV32: &str = "the native execution paths run only while RV32 is loaded";

/// The RV32 runtime inside `machine`, or `None` for any other backend.
///
/// Free functions rather than methods only: taking the field lets a caller
/// borrow the runtime and another `App` field at once — the pipeline hands its
/// tick the console, a background hart its cache — which a `&mut self` method
/// would collapse into one borrow of everything.
fn rv32_runtime(machine: &dyn raven_riscv_engine::Machine) -> Option<&FalconRuntime> {
    (machine as &dyn std::any::Any)
        .downcast_ref::<raven_riscv_engine::architectures::riscv32::RiscV32Machine>()
        .map(|rv32| rv32.falcon())
}

fn rv32_runtime_mut(
    machine: &mut dyn raven_riscv_engine::Machine,
) -> Option<&mut FalconRuntime> {
    (machine as &mut dyn std::any::Any)
        .downcast_mut::<raven_riscv_engine::architectures::riscv32::RiscV32Machine>()
        .map(|rv32| rv32.falcon_mut())
}

/// Cycles and retired instructions as the active backend counts them.
///
/// Built by [`App::execution_totals`]; `cpi` is derived rather than stored so
/// no pane can show a ratio that disagrees with the two numbers beside it.
#[derive(Clone, Copy)]
pub(crate) struct ExecutionTotals {
    pub(crate) cycles: u64,
    pub(crate) instructions: u64,
    pub(crate) scope: ExecutionScope,
}

/// How much of the program the totals cover: a pipeline counts cycles for the
/// hart it is clocking, a cache hierarchy for everything that ran.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionScope {
    Selected,
    Program,
}

impl ExecutionTotals {
    pub(crate) fn cpi(self) -> f64 {
        if self.instructions == 0 {
            0.0
        } else {
            self.cycles as f64 / self.instructions as f64
        }
    }
}

impl ExecutionScope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Program => "program",
        }
    }
}

// ── Top-level app ──────────────────────────────────────────────────────────────

pub struct App {
    pub(crate) architecture: Arc<dyn raven_riscv_engine::Architecture>,
    /// The active backend, whichever architecture is loaded. There is exactly
    /// one, and every pane reads it through the capability accessors below.
    ///
    /// RV32 is here too. Its runtime — pipeline, cache hierarchy, MMU, harts,
    /// JIT and the step-back journal — is reached with [`App::rv32`], because
    /// stepping *that* is the one thing the trait does not describe; see
    /// [`App::rv32`] for where the host still drives a backend by hand.
    pub(crate) machine: Box<dyn raven_riscv_engine::Machine>,
    pub(super) tab: Tab,
    pub(super) mode: EditorMode,

    pub(super) editor: EditorState,
    pub(super) run: RunState,
    pub(super) docs: DocsState,
    pub(super) cache: CacheState,
    pub(super) tlb: TlbState,
    pub(super) settings: SettingsState,
    pub(crate) max_cores: usize,
    pub(crate) selected_core: usize,
    pub(crate) run_scope: RunScope,
    pub(crate) next_hart_id: u32,
    harts: Vec<HartCoreRuntime>,

    pub(super) show_exit_popup: bool,
    pub(super) should_quit: bool,

    // Help popup
    pub(super) help_open: bool,
    pub(super) help_page: usize,
    pub(super) hover_help: bool,

    // Mouse tracking (shared across tabs)
    pub(super) mouse_x: u16,
    pub(super) mouse_y: u16,
    pub(super) hover_tab: Option<Tab>,
    pub(super) hover_run_button: Option<RunButton>,

    // Program I/O console (shared across tabs)
    pub(super) console: Console,

    // Persistent clipboard — must stay alive on Linux/X11 to retain ownership
    pub(super) clipboard: Option<Clipboard>,

    // Timestamp of the last bracketed-paste event (Event::Paste). Used to
    // suppress the arboard Ctrl+V handler if a bracketed-paste already fired
    // within the same keypress cycle, preventing double-paste in terminals
    // that emit both Event::Paste and a Ctrl+V key event simultaneously.
    pub(super) last_bracketed_paste: Option<Instant>,

    // Splash screen — set to Some(start_instant) on launch, cleared after 4s
    pub(super) splash_start: Option<Instant>,

    // RAM size override from --mem CLI flag. None = use per-mode defaults.
    pub(super) ram_override: Option<usize>,

    // TUI path input bar (fallback when OS file dialog returns None)
    pub(super) path_input: PathInput,

    // Interactive guided tutorial ([?] button)
    pub tutorial: TutorialState,

    // Guided learning activity presets (Activity tab)
    pub(crate) activity: crate::guided_learning::GuidedLearningState,
}

pub(super) fn compute_find_matches(query: &str, lines: &[String]) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return vec![];
    }
    let q = query.to_lowercase();
    let q_len = q.len();
    let mut matches = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        let mut byte_from = 0;
        while byte_from < line_lower.len() {
            if let Some(rel) = line_lower[byte_from..].find(&q) {
                let byte_pos = byte_from + rel;
                let col = line[..byte_pos].chars().count();
                matches.push((row, col));
                byte_from = byte_pos + q_len.max(1);
            } else {
                break;
            }
        }
    }
    matches
}

impl App {
    pub fn new(ram_override: Option<usize>) -> Self {
        Self::new_with_jit(ram_override, crate::falcon::jit::BackendKind::None)
    }

    pub fn new_with_jit(
        ram_override: Option<usize>,
        initial_jit_kind: crate::falcon::jit::BackendKind,
    ) -> Self {
        let base_pc = 0x0000_0000;
        let architecture = crate::riscv32::architecture();
        let mem_size = architecture
            .descriptor()
            .clamp_memory_size(ram_override.unwrap_or(DEFAULT_MEM_SIZE));
        let data_base = base_pc + 0x1000;
        let machine = architecture
            .create_machine(mem_size)
            .expect("RV32 accepts a size its own descriptor clamped");
        let mut app = Self {
            architecture,
            machine,
            tab: Tab::Editor,
            mode: EditorMode::Insert,
            editor: EditorState {
                buf: Editor::with_sample(),
                files: vec![EditorFile {
                    name: "main.s".to_string(),
                    buf: Editor::empty(),
                }],
                active_file: 0,
                file_line_offsets: vec![0],
                hover_file_tab: None,
                file_delete_armed: None,
                last_file_tab_click: None,
                sb: std::cell::Cell::new(None),
                sb_drag: None,
                dirty: true,
                last_edit_at: Some(Instant::now()),
                auto_check_delay: Duration::from_millis(400),
                last_assemble_msg: None,
                last_build_stats: None,
                last_compile_ok: None,
                last_ok_image: None,
                last_ok_text: None,
                last_ok_data: None,
                last_ok_data_base: None,
                last_ok_bss_size: None,
                last_ok_elf_bytes: None,
                last_ok_comments: std::collections::HashMap::new(),
                last_ok_block_comments: std::collections::HashMap::new(),
                last_ok_labels: std::collections::HashMap::new(),
                last_ok_halt_pcs: std::collections::HashSet::new(),
                diag_line: None,
                diag_msg: None,
                diag_line_text: None,
                label_to_line: std::collections::HashMap::new(),
                line_to_addr: std::collections::HashMap::new(),
                show_addr_hints: false,
                elf_prompt_open: false,
                find_open: false,
                find_query: String::new(),
                replace_open: false,
                replace_query: String::new(),
                find_in_replace: false,
                find_matches: Vec::new(),
                find_current: 0,
                goto_open: false,
                goto_query: String::new(),
                show_encoding: false,
            },
            run: RunState {
                pipeline_view: crate::ui::pipeline::PipelineViewState::new(),
                prev_x: [0; 32],
                prev_pc: base_pc,
                mem_size,
                breakpoints: std::collections::HashSet::new(),
                base_pc,
                data_base,
                heap_start: data_base,
                exec_regions: Vec::new(),
                mem_view_addr: data_base,
                mem_view_bytes: 4,
                mem_region: MemRegion::Data,
                mem_search_open: false,
                mem_search_query: String::new(),
                show_registers: true,
                fmt_mode: FormatMode::Hex,
                show_signed: false,
                sidebar_width: 38,
                hover_sidebar_bar: false,
                sidebar_drag: false,
                sidebar_drag_start_x: 0,
                sidebar_width_start: 38,
                sidebar_collapsed: false,
                imem_width: 34,
                hover_imem_bar: false,
                imem_drag: false,
                imem_drag_start_x: 0,
                imem_width_start: 34,
                imem_scroll: 0,
                hover_imem_addr: None,
                last_imem_click: None,
                imem_inner_height: std::cell::Cell::new(16),
                imem_collapsed: false,
                imem_search_open: false,
                imem_search_query: String::new(),
                imem_vrow_cache: std::collections::HashMap::new(),
                labels_lower: std::collections::HashMap::new(),
                imem_search_matches: Vec::new(),
                imem_search_cursor: 0,
                imem_search_match_count: 0,
                details_collapsed: false,
                details_addr: None,
                last_details_click: None,
                details_field_hitboxes: std::cell::RefCell::new(Vec::new()),
                details_rendered_addr: std::cell::Cell::new(0),
                console_height: 5,
                hover_console_bar: false,
                hover_console_clear: false,
                console_drag: false,
                console_drag_start_y: 0,
                console_height_start: 5,
                regs_scroll: 0,
                regs_sb: std::cell::Cell::new(None),
                regs_sb_drag: None,
                is_running: false,
                last_step_time: Instant::now(),
                step_interval: Duration::from_millis(80),
                faulted: false,
                speed: RunSpeed::X1,
                go_checkpointed: false,
                run_edit: None,
                run_edit_buf: String::new(),
                run_edit_error: None,
                comments: std::collections::HashMap::new(),
                labels: std::collections::HashMap::new(),
                halt_pcs: std::collections::HashSet::new(),
                elf_sections: Vec::new(),
                exec_counts: std::collections::HashMap::new(),
                exec_trace: std::collections::VecDeque::new(),
                reg_age: [255u8; 32],
                show_trace: false,
                pinned_regs: Vec::new(),
                reg_cursor: 0,
                block_comments: std::collections::HashMap::new(),
                reg_last_write_pc: [None; 32],
                show_dyn: false,
                dyn_mem_access: None,
                hover_reg_row: None,
                reg_bank: 0,
                prev_f: [0u32; 32],
                f_age: [255u8; 32],
                f_last_write_pc: [None; 32],
                cpi_config: CpiConfig::default(),
                show_exec_count: true,
                show_instr_type: true,
                show_screen: false,
                screen_seen: false,
                mem_access_log: Vec::new(),
                cache_enabled: false,
                vm_mode: crate::falcon::mmu::VmMode::Off,
                tlb_enabled: true,
                trace_syscalls: false,
                jit_kind: crate::falcon::jit::BackendKind::None,
                backend: crate::falcon::jit::make_backend(crate::falcon::jit::BackendKind::None)
                    .expect("interpreter backend is always available"),
            },
            docs: DocsState {
                page: DocsPage::InstrRef,
                lang: DocsLang::En,
                scroll: 0,
                h_scroll: 0,
                search_open: false,
                search_query: String::new(),
                hover_page: None,
                type_filter: 0x0FFF,
                filter_cursor: 0,
                tab_bar_y: std::cell::Cell::new(0),
                tab_bar_xs: std::cell::Cell::new([(0, 0); 4]),
                filter_bar_y: std::cell::Cell::new(0),
                sb_v: std::cell::Cell::new(None),
                sb_h: std::cell::Cell::new(None),
                sb_drag: SbDrag::None,
            },
            cache: CacheState {
                subtab: CacheSubtab::Stats,
                scope: CacheScope::Both,
                stats_scroll: 0,
                selected_level: 0,
                hover: None,
                view_focus: CacheViewFocus::ICache,
                view_scroll: 0,
                view_scroll_d: 0,
                view_h_scroll: 0,
                view_h_scroll_d: 0,
                data_fmt: CacheDataFmt::Hex,
                data_group: CacheDataGroup::B1,
                addr_mode: CacheAddrMode::Base,
                subtab_header_origin: std::cell::Cell::new((0, 0)),
                level_origin: std::cell::Cell::new((0, 0)),
                ctrl_origin: std::cell::Cell::new((0, 0)),
                ctrl_scope_origin: std::cell::Cell::new((0, 0)),
                exec_origin: std::cell::Cell::new((0, 0)),
                view_fmt_btn: std::cell::Cell::new((0, 0, 0)),
                view_group_btn: std::cell::Cell::new((0, 0, 0)),
                view_tag_btn: std::cell::Cell::new((0, 0, 0)),
                config_hitboxes_i: std::cell::Cell::new([(0, 0, 0); 11]),
                config_hitboxes_d: std::cell::Cell::new([(0, 0, 0); 11]),
                config_hitboxes_u: std::cell::Cell::new([(0, 0, 0); 11]),
                config_preset_origin_i: std::cell::Cell::new((0, 0)),
                config_preset_origin_d: std::cell::Cell::new((0, 0)),
                config_preset_origin_u: std::cell::Cell::new((0, 0)),
                config_apply_origin: std::cell::Cell::new((0, 0)),
                pending_icache: CacheConfig::default(),
                pending_dcache: CacheConfig::default(),
                extra_pending: vec![],
                config_error: None,
                config_status: None,
                edit_field: None,
                edit_buf: String::new(),
                cpi_selected: 0,
                cpi_editing: false,
                cpi_edit_buf: String::new(),
                session_history: Vec::new(),
                history_scroll: 0,
                history_sb: std::cell::Cell::new(None),
                history_sb_drag: None,
                viewing_snapshot: None,
                window_start_instr: 0,
                hscroll_bars: std::cell::Cell::new([None; 2]),
                hscroll_drag: None,
                hscroll_drag_is_dcache: false,
                view_num_sets: std::cell::Cell::new(0),
                view_num_sets_d: std::cell::Cell::new(0),
                view_visible_sets: std::cell::Cell::new(0),
                view_visible_sets_d: std::cell::Cell::new(0),
                view_scroll_max: std::cell::Cell::new(0),
                view_scroll_max_d: std::cell::Cell::new(0),
            },
            tlb: TlbState::default(),
            show_exit_popup: false,
            should_quit: false,
            help_open: false,
            help_page: 0,
            hover_help: false,
            mouse_x: 0,
            mouse_y: 0,
            hover_tab: None,
            hover_run_button: None,
            console: Console::default(),
            clipboard: Clipboard::new().ok(),
            last_bracketed_paste: None,
            ram_override,
            splash_start: Some(Instant::now()),
            path_input: PathInput::new(),
            tutorial: TutorialState::default(),
            activity: crate::guided_learning::GuidedLearningState::default(),
            settings: SettingsState::default(),
            max_cores: 4,
            selected_core: 0,
            run_scope: RunScope::AllHarts,
            next_hart_id: 1,
            harts: Vec::new(),
        };
        app.console.trace_syscalls = app.run.trace_syscalls;
        app.assemble_and_load();
        app.rebuild_harts();
        if initial_jit_kind != crate::falcon::jit::BackendKind::None {
            app.set_jit_mode(initial_jit_kind);
        }
        app
    }

    pub fn new_with_architecture(
        ram_override: Option<usize>,
        initial_jit_kind: crate::falcon::jit::BackendKind,
        architecture_id: &str,
    ) -> Result<Self, String> {
        let mut app = Self::new_with_jit(ram_override, initial_jit_kind);
        app.activate_architecture(architecture_id, true)?;
        Ok(app)
    }

    pub(crate) fn architecture_id(&self) -> &'static str {
        self.architecture.descriptor().id
    }

    /// RV32's runtime, when RV32 is the backend that is loaded.
    ///
    /// This says nothing about what the UI can *draw*: every pane reads backend
    /// state through the capability accessors above, so all architectures get
    /// the same tabs. What is behind here is execution control the trait does
    /// not cover — multi-hart scheduling, JIT backend selection, breakpoints
    /// and step-back. RV32 is the only backend with any of it, so it is the
    /// only one the host steps by hand; every other backend answers `None` and
    /// is driven through [`raven_riscv_engine::Machine::step`].
    ///
    /// Reading state through here rather than through a capability is a bug:
    /// it is how an architecture's name gets back into the view layer.
    pub(crate) fn rv32(&self) -> Option<&FalconRuntime> {
        rv32_runtime(&*self.machine)
    }

    pub(crate) fn rv32_mut(&mut self) -> Option<&mut FalconRuntime> {
        rv32_runtime_mut(&mut *self.machine)
    }

    /// The RV32 runtime inside code only RV32 reaches: the native step loop,
    /// the hart scheduler, the JIT, the load path. Every caller sits under a
    /// fork that already asked [`App::rv32`], which is why this may assert —
    /// and why it is scoped to this module, where those forks live. The view
    /// layer cannot call it, and must degrade through `rv32` instead.
    pub(in crate::ui::app) fn native(&self) -> &FalconRuntime {
        self.rv32().expect(NOT_RV32)
    }

    pub(in crate::ui::app) fn native_mut(&mut self) -> &mut FalconRuntime {
        self.rv32_mut().expect(NOT_RV32)
    }

    /// Move to the next registered architecture, wrapping around.
    pub(crate) fn cycle_architecture(&mut self) {
        let Some(next) = crate::arch::registry().next_after(self.architecture_id()) else {
            return;
        };
        if let Err(error) = self.activate_architecture(next.descriptor().id, false) {
            self.console.push_error(error);
        }
    }

    pub(crate) fn activate_architecture(
        &mut self,
        architecture_id: &str,
        replace_source: bool,
    ) -> Result<(), String> {
        let architecture = crate::arch::lookup(architecture_id)?;
        let descriptor = architecture.descriptor();
        let memory_size = self
            .ram_override
            .map_or(descriptor.default_memory_size, |requested| {
                descriptor.clamp_memory_size(requested)
            });
        let machine = architecture
            .create_machine(memory_size)
            .map_err(|e| e.to_string())?;
        self.run.is_running = false;
        self.architecture = architecture;
        self.machine = machine;
        if self.rv32().is_none() && self.architecture.descriptor().capabilities.cache {
            self.run.cache_enabled = true;
        }
        self.editor.last_ok_image = None;
        if replace_source {
            self.editor.buf.lines = self
                .architecture
                .default_source()
                .lines()
                .map(str::to_string)
                .collect();
            self.editor.buf.cursor_row = 0;
            self.editor.buf.cursor_col = 0;
        }
        self.ensure_visible_tab();
        self.assemble_and_load();
        Ok(())
    }

    pub(crate) fn machine_snapshot(&self) -> raven_riscv_engine::MachineSnapshot {
        self.machine.snapshot()
    }

    // ── Capability accessors ─────────────────────────────────────────────
    //
    // The one way the view layer reaches backend state. Each answers for the
    // active architecture, so a view never asks *which* backend it is drawing —
    // it asks what the backend can do, and draws that. `None` means "this
    // architecture does not offer it"; the caller shows one pane fewer.

    pub(crate) fn registers(&self) -> Option<&dyn RegisterFile> {
        self.machine.registers()
    }

    pub(crate) fn registers_mut(&mut self) -> Option<&mut dyn RegisterFile> {
        self.machine.registers_mut()
    }

    pub(crate) fn memory(&self) -> Option<&dyn MemoryInspect> {
        self.machine.memory()
    }

    pub(crate) fn memory_mut(&mut self) -> Option<&mut dyn MemoryInspect> {
        self.machine.memory_mut()
    }

    pub(crate) fn code(&self) -> Option<&dyn InstructionCodec> {
        self.machine.code()
    }

    pub(crate) fn cache_hierarchy(&self) -> Option<&dyn CacheHierarchy> {
        self.machine.caches()
    }

    pub(crate) fn pipeline(&self) -> Option<&dyn PipelineInspect> {
        self.machine.pipeline()
    }

    /// The pipeline *model* rather than a view of it, for the Pipeline tab's
    /// configuration controls: bypass paths, branch resolution, how many
    /// functional units of each kind. Retuning a datapath is not something the
    /// `Machine` trait describes, and a backend that has no such model has no
    /// Pipeline tab either — hence the `Option` and not a panic.
    pub(crate) fn pipeline_config(
        &self,
    ) -> Option<&raven_riscv_engine::falcon::pipeline::PipelineSimState> {
        self.rv32().map(FalconRuntime::pipeline)
    }

    pub(crate) fn pipeline_config_mut(
        &mut self,
    ) -> Option<&mut raven_riscv_engine::falcon::pipeline::PipelineSimState> {
        self.rv32_mut().map(FalconRuntime::pipeline_mut)
    }

    /// What the pipeline is doing right now — enabled, sequential, halted,
    /// faulted — for the keys and panes that only need to know that much.
    pub(crate) fn pipeline_status(
        &self,
    ) -> Option<raven_riscv_engine::capability::PipelineStatus> {
        self.pipeline().map(|pipeline| pipeline.status())
    }

    /// Cycles the active backend's cache hierarchy has charged the program.
    pub(crate) fn cache_total_cycles(&self) -> u64 {
        self.cache_hierarchy()
            .map_or(0, |caches| caches.total_cycles())
    }

    /// What the run toolbar, the Run status line and the Cache tab all mean by
    /// "Cycles / CPI / Instrs".
    ///
    /// Timing comes from whichever model the active backend actually charges
    /// cycles to: a running pipeline counts them per stage, everything else
    /// pays for them through the cache hierarchy. Deciding that here is what
    /// keeps three panes from disagreeing about the same program.
    pub(crate) fn execution_totals(&self) -> ExecutionTotals {
        match self.aggregate_pipeline_snapshot() {
            Some(pipeline) => ExecutionTotals {
                cycles: pipeline.cycles,
                instructions: pipeline.committed,
                scope: ExecutionScope::Selected,
            },
            None => ExecutionTotals {
                cycles: self.cache_total_cycles(),
                instructions: self.instructions_retired(),
                scope: ExecutionScope::Program,
            },
        }
    }

    /// Instructions the active backend has retired.
    pub(crate) fn instructions_retired(&self) -> u64 {
        self.machine.snapshot().instructions
    }

    /// Whether the active backend's cache can be retuned, which is what gates
    /// the Cache tab's add/remove/edit controls.
    pub(crate) fn cache_is_configurable(&self) -> bool {
        self.cache_hierarchy()
            .is_some_and(|caches| caches.is_configurable())
    }

    pub(crate) fn translation(&self) -> Option<&dyn AddressTranslation> {
        self.machine.translation()
    }

    /// The register bank the sidebar is showing, clamped to what this backend
    /// actually has — so the "next bank" key is a no-op on a single-bank ISA
    /// rather than a way to scroll into nothing.
    pub(crate) fn visible_register_bank(&self) -> usize {
        let banks = self.registers().map_or(0, |file| file.banks().len());
        self.run.reg_bank.min(banks.saturating_sub(1))
    }

    /// Show the next register bank, wrapping. On a single-bank ISA this stays
    /// put, which is why the key needs no per-architecture guard.
    pub(crate) fn cycle_register_bank(&mut self) {
        let banks = self.registers().map_or(0, |file| file.banks().len());
        if banks > 1 {
            self.run.reg_bank = (self.visible_register_bank() + 1) % banks;
            self.run.regs_scroll = 0;
        }
    }

    /// The active backend's program counter.
    pub(crate) fn program_counter(&self) -> u64 {
        self.registers().map_or(0, |file| file.program_counter())
    }

    /// The instruction at `address` as assembly text, through the backend's own
    /// disassembler. `None` when the bytes do not decode.
    pub(crate) fn disassemble_at(&self, address: u64) -> Option<String> {
        let (code, memory) = (self.code()?, self.memory()?);
        code.disassemble(address, &memory.peek(address, 8))
    }

    /// How many bytes the instruction at `address` occupies, so a listing walks
    /// a variable-width ISA correctly without knowing it is variable-width.
    pub(crate) fn instruction_width_at(&self, address: u64) -> usize {
        let Some((code, memory)) = self.code().zip(self.memory()) else {
            return 4;
        };
        code.instruction_width(address, &memory.peek(address, 8))
            .max(1)
    }

    /// The banks this backend declares, empty when it has no register file.
    pub(crate) fn register_banks(&self) -> Vec<RegisterBank> {
        self.registers().map_or_else(Vec::new, |file| file.banks().to_vec())
    }

    /// Whether the open editor is on a register its bank declares as a float,
    /// which is what decides the characters the editor accepts: a decimal
    /// point and a sign belong there and nowhere else.
    pub(crate) fn editing_a_float_register(&self) -> bool {
        let Some(RunEditTarget::Register(id)) = self.run.run_edit else {
            return false;
        };
        self.registers()
            .and_then(|file| file.banks().get(id.bank))
            .is_some_and(|bank| {
                bank.format == raven_riscv_engine::capability::RegisterFormat::Float
            })
    }

    /// What a click on register row `row` edits: row 0 is the PC, the rest are
    /// the visible bank's registers in the order the sidebar draws them. Both
    /// the renderer and the hit-test ask here, so a row cannot edit one cell
    /// and display another.
    pub(crate) fn register_edit_target(&self, row: usize) -> Option<RunEditTarget> {
        match row {
            0 => Some(RunEditTarget::ProgramCounter),
            _ => self
                .visible_register_entries()
                .get(row - 1)
                .map(|entry| RunEditTarget::Register(entry.id)),
        }
    }

    /// Every register in the visible bank, in display order.
    pub(crate) fn visible_register_entries(&self) -> Vec<RegisterEntry> {
        let bank = self.visible_register_bank();
        self.registers().map_or_else(Vec::new, |file| {
            file.entries()
                .into_iter()
                .filter(|entry| entry.id.bank == bank)
                .collect()
        })
    }

    /// How many steps ago `id` last changed; `NO_REG_AGE` means "not recently",
    /// which the sidebar draws unhighlighted.
    ///
    /// Only the RV32 runtime records this today, so other backends get a steady
    /// pane rather than a wrong one.
    pub(crate) fn register_age(&self, id: RegisterId) -> u8 {
        if self.rv32().is_none() {
            return NO_REG_AGE;
        }
        let ages: &[u8; 32] = match id.bank {
            0 => &self.run.reg_age,
            1 => &self.run.f_age,
            _ => return NO_REG_AGE,
        };
        ages.get(id.index).copied().unwrap_or(NO_REG_AGE)
    }

    /// The PC of the instruction that last wrote `id`, when the runtime tracks
    /// it.
    pub(crate) fn register_last_write(&self, id: RegisterId) -> Option<u64> {
        if self.rv32().is_none() || id.bank != 0 {
            return None;
        }
        self.run
            .reg_last_write_pc
            .get(id.index)
            .copied()
            .flatten()
            .map(u64::from)
    }

    /// Assemble the workspace for export ("save binary"), at the same base
    /// address the running program was built with.
    pub(crate) fn export_program_image(&self) -> Result<raven_riscv_engine::ProgramImage, String> {
        self.architecture
            .assembler()
            .assemble(&self.combined_source().0, u64::from(self.run.base_pc))
            .map_err(|error| error.to_string())
    }

    /// Run/pause for a trait-driven backend. A program that already finished is
    /// reset first, so the key restarts it rather than doing nothing.
    /// Run/pause, whichever runtime drives the active backend.
    ///
    /// The single place that fork lives. Every caller — the Run tab key, the
    /// Cache tab key, the toolbar button — goes through here, so the two paths
    /// cannot drift apart in what a spacebar means.
    pub(crate) fn toggle_run(&mut self) {
        if self.rv32().is_none() {
            self.machine_toggle_run();
            return;
        }
        if self.run.is_running {
            self.run.is_running = false;
            return;
        }
        if self.core_status(self.selected_core) == HartLifecycle::Exited {
            self.restart_simulation();
        } else if self.core_status(self.selected_core) == HartLifecycle::Paused
            || !self.run.faulted
        {
            self.resume_selected_hart();
        }
        if self.can_start_run() {
            self.run.is_running = true;
        }
    }

    pub(crate) fn machine_toggle_run(&mut self) {
        if self.run.is_running {
            self.run.is_running = false;
            return;
        }
        if matches!(
            self.machine.snapshot().state,
            raven_riscv_engine::MachineState::Halted
                | raven_riscv_engine::MachineState::Exited(_)
                | raven_riscv_engine::MachineState::Faulted
        ) {
            self.machine.reset();
        }
        self.run.is_running = true;
    }

    fn machine_step(&mut self) {
        let machine = &mut self.machine;
        let pc = machine.snapshot().pc;
        match machine.step() {
            Ok(raven_riscv_engine::StepOutcome::Stepped) => {
                *self.run.exec_counts.entry(pc as u32).or_insert(0) += 1;
            }
            Ok(_) => {
                *self.run.exec_counts.entry(pc as u32).or_insert(0) += 1;
                self.run.is_running = false;
            }
            Err(error) => {
                self.run.is_running = false;
                self.console.push_error(error.to_string());
            }
        }
    }

    /// Load `text` into the editor and assemble + load the program.
    /// Used by guided-learning presets (outside `ui/`).
    pub(crate) fn load_editor_text(&mut self, text: &str) {
        self.editor.buf.lines = text.lines().map(|s| s.to_string()).collect();
        if self.editor.buf.lines.is_empty() {
            self.editor.buf.lines.push(String::new());
        }
        self.editor.buf.cursor_row = 0;
        self.editor.buf.cursor_col = 0;
        self.assemble_and_load();
    }

    /// Switch to the given tab (used by guided-learning presets).
    pub(crate) fn navigate_to_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    /// Reset the pipeline to the current CPU PC (used after loading a preset).
    pub(crate) fn pipeline_reset_to_current_pc(&mut self) {
        let __rpc = self.program_counter() as u32;
        self.reset_pipeline_stages(__rpc);
    }

    // ── Multi-file workspace ───────────────────────────────────────────────

    /// All files' source concatenated in tab order, plus each file's starting
    /// line offset in the combined text. Labels resolve across files, so a
    /// program can be split into modules; the first file's `.text` runs first.
    pub(crate) fn combined_source(&self) -> (String, Vec<usize>) {
        let mut parts: Vec<String> = Vec::with_capacity(self.editor.files.len());
        let mut offsets = Vec::with_capacity(self.editor.files.len());
        let mut offset = 0usize;
        for (i, file) in self.editor.files.iter().enumerate() {
            let buf = if i == self.editor.active_file {
                &self.editor.buf
            } else {
                &file.buf
            };
            offsets.push(offset);
            offset += buf.lines.len().max(1);
            parts.push(buf.text());
        }
        (parts.join("\n"), offsets)
    }

    /// Map a combined-source line to `(file index, line local to that file)`.
    pub(crate) fn combined_to_local(&self, line: usize) -> (usize, usize) {
        let offs = &self.editor.file_line_offsets;
        let idx = offs.iter().rposition(|&o| o <= line).unwrap_or(0);
        (idx, line - offs.get(idx).copied().unwrap_or(0))
    }

    /// Store post-assemble source metadata: `label_to_line` stays in combined
    /// space (cross-file goto), `line_to_addr` is translated to the active file.
    fn store_source_meta(
        &mut self,
        label_to_line: std::collections::HashMap<String, usize>,
        line_addrs: std::collections::HashMap<usize, u32>,
        offsets: Vec<usize>,
    ) {
        let lo = offsets.get(self.editor.active_file).copied().unwrap_or(0);
        let hi = lo + self.editor.buf.lines.len();
        self.editor.line_to_addr = line_addrs
            .into_iter()
            .filter(|&(l, _)| l >= lo && l < hi)
            .map(|(l, a)| (l - lo, a))
            .collect();
        self.editor.label_to_line = label_to_line;
        self.editor.file_line_offsets = offsets;
    }

    /// Diagnostic fields for an assemble error at combined line `line`. When the
    /// error is in another file the message is prefixed with its name and no
    /// line is underlined in the active buffer.
    fn set_diag(&mut self, line: usize, msg: &str, offsets: &[usize]) {
        self.editor.file_line_offsets = offsets.to_vec();
        let (fidx, local) = self.combined_to_local(line);
        if fidx == self.editor.active_file {
            self.editor.diag_line = Some(local);
            self.editor.diag_line_text = self.editor.buf.lines.get(local).cloned();
            self.editor.last_assemble_msg =
                Some(format!("Assemble error at line {}: {}", local + 1, msg));
        } else {
            let name = self
                .editor
                .files
                .get(fidx)
                .map(|f| f.name.as_str())
                .unwrap_or("?");
            self.editor.diag_line = None;
            self.editor.diag_line_text = None;
            self.editor.last_assemble_msg = Some(format!(
                "Assemble error in {} line {}: {}",
                name,
                local + 1,
                msg
            ));
        }
        self.editor.diag_msg = Some(msg.to_string());
        self.editor.last_build_stats = None;
        self.editor.last_compile_ok = Some(false);
    }

    /// Make file `idx` the active buffer. Per-buffer state (cursor, selection,
    /// scroll, undo) travels with the `Editor`; view state that indexes into the
    /// buffer (find/goto, line→addr, diagnostics) is reset and rebuilt by the
    /// next auto-check.
    pub(crate) fn switch_file(&mut self, idx: usize) {
        if idx >= self.editor.files.len() || idx == self.editor.active_file {
            return;
        }
        let cur = self.editor.active_file;
        std::mem::swap(&mut self.editor.files[cur].buf, &mut self.editor.buf);
        std::mem::swap(&mut self.editor.files[idx].buf, &mut self.editor.buf);
        self.editor.active_file = idx;
        self.editor.find_open = false;
        self.editor.replace_open = false;
        self.editor.goto_open = false;
        self.editor.find_in_replace = false;
        self.editor.find_matches.clear();
        self.editor.find_current = 0;
        self.editor.line_to_addr.clear();
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;
        self.editor.file_delete_armed = None;
        self.editor.dirty = true;
        self.editor.last_edit_at = Some(Instant::now());
    }

    /// Create an empty file tab and switch to it.
    pub(crate) fn new_file(&mut self) {
        let mut n = self.editor.files.len() + 1;
        let name = loop {
            let candidate = format!("file{n}.s");
            if !self.editor.files.iter().any(|f| f.name == candidate) {
                break candidate;
            }
            n += 1;
        };
        self.add_file_with_lines(name, vec![String::new()]);
    }

    /// Add a file tab with the given content and switch to it (Ctrl+O / import).
    pub(crate) fn add_file_with_lines(&mut self, name: String, lines: Vec<String>) {
        let mut name = name;
        while self.editor.files.iter().any(|f| f.name == name) {
            name.insert_str(0, "_");
        }
        self.editor.files.push(EditorFile {
            name,
            buf: Editor::empty(),
        });
        self.switch_file(self.editor.files.len() - 1);
        self.editor.buf.lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        self.editor.buf.cursor_row = 0;
        self.editor.buf.cursor_col = 0;
        self.editor.buf.scroll_offset.set(0);
        self.editor.buf.clear_selection();
    }

    /// Delete the active file tab (needs a second tab to fall back to).
    pub(crate) fn delete_active_file(&mut self) {
        if self.editor.files.len() <= 1 {
            return;
        }
        let idx = self.editor.active_file;
        let next = if idx + 1 < self.editor.files.len() {
            idx + 1
        } else {
            idx - 1
        };
        self.switch_file(next);
        self.editor.files.remove(idx);
        if self.editor.active_file > idx {
            self.editor.active_file -= 1;
        }
    }

    /// Build the workspace and load it into the active backend.
    ///
    /// Every architecture takes this path: assemble with the backend's own
    /// assembler, install the resulting image, refresh the derived views. See
    /// [`program`] for what "install" means on each of the two runtimes.
    pub(crate) fn assemble_and_load(&mut self) {
        let Some((image, offsets)) = self.assemble_workspace(Diagnostics::Follow) else {
            return;
        };
        if !self.install_program(&image) {
            return;
        }
        self.record_build(&image, "Assembled");
        self.cache_last_ok(&image);
        self.adopt_program(&image, offsets);
        self.editor.dirty = false;
    }

    /// The background syntax check: build, remember the result, touch no
    /// machine state.
    fn check_assemble(&mut self) {
        if let Some((image, offsets)) = self.assemble_workspace(Diagnostics::Report) {
            self.record_build(&image, "OK:");
            self.cache_last_ok(&image);
            self.store_image_source_meta(&image, offsets);
        }
        self.editor.dirty = false;
    }

    fn sync_pipeline_program_range(&mut self) {
        let regions = self.run.exec_regions.clone();
        self.native_mut().pipeline_mut().set_exec_regions(&regions);
        for hart in &mut self.harts {
            if let Some(p) = hart.pipeline.as_mut() {
                p.set_exec_regions(&regions);
            }
        }
    }

    /// Reload the last program that built, without rebuilding it.
    ///
    /// A loaded ELF re-parses its original bytes so every section comes back;
    /// anything else replays the image the last successful build produced.
    fn load_last_ok_program(&mut self) {
        if let Some(elf_bytes) = self.editor.last_ok_elf_bytes.clone() {
            self.load_binary(&elf_bytes);
            return;
        }
        let Some(image) = self.editor.last_ok_image.clone() else {
            return;
        };
        if self.rv32().is_some() {
            self.reset_screen_device();
        }
        if !self.install_program(&image) {
            return;
        }
        self.record_build(&image, "Loaded last successful build:");
        let offsets = self.editor.file_line_offsets.clone();
        self.adopt_program(&image, offsets);
    }

    pub(super) fn restart_simulation(&mut self) {
        self.run.is_running = false;
        if self.rv32().is_none() {
            self.machine.reset();
            self.run.exec_counts.clear();
            return;
        }
        self.run.faulted = false;
        // Drop step-back history: the timeline (and any GO checkpoint) belongs to
        // the program being replaced.
        self.native_mut().clear_journal();
        self.run.go_checkpointed = false;
        self.native_mut().cpu_mut_unjournaled().ebreak_hit = false;
        self.cache.window_start_instr = 0;
        self.load_last_ok_program();
        // Rebuild JIT backend AFTER load so FullBackend can scan the loaded program.
        self.rebuild_backend();
    }

    /// Drop the screen device from the previous run (closes any OS window) and
    /// re-arm the Screen sub-view auto-open for the next `screen_init`.
    fn reset_screen_device(&mut self) {
        self.console.screen = None;
        self.console.screen_uninit_warned = false;
        self.run.show_screen = false;
        self.run.screen_seen = false;
    }

    /// Open a compiled program: an ELF, a FALC container, or a flat block of
    /// machine code.
    ///
    /// ELF keeps a path of its own — it carries a symbol table and a section
    /// list a [`ProgramImage`](raven_riscv_engine::ProgramImage) cannot express
    /// — and is offered only by backends that declare ELF support. Everything
    /// else is decoded into an image and takes the shared load path, so a
    /// container opens the same way here as it does in the CLI.
    pub(super) fn load_binary(&mut self, bytes: &[u8]) {
        if self.architecture.descriptor().capabilities.elf && bytes.starts_with(b"\x7fELF") {
            self.load_elf_binary(bytes);
            return;
        }
        let image = match self.decode_binary(bytes) {
            Ok(image) => image,
            Err(error) => {
                self.console.push_error(error);
                self.run.faulted = self.rv32().is_some();
                return;
            }
        };
        if self.rv32().is_some() {
            self.reset_screen_device();
            // The container names where it runs; the panes key off `base_pc`.
            self.run.base_pc = u32::try_from(image.entry).unwrap_or(self.run.base_pc);
        }
        if !self.install_program(&image) {
            return;
        }
        self.record_build(&image, "Loaded binary:");
        self.cache_last_ok(&image);
        let offsets = self.editor.file_line_offsets.clone();
        self.adopt_program(&image, offsets);
        self.lock_editor_on_binary();
    }

    /// Load an ELF32 RISC-V image into the native runtime, taking the entry
    /// point, the section list and the symbol table from the file.
    fn load_elf_binary(&mut self, bytes: &[u8]) {
        self.reset_screen_device();
        self.reset_native_runtime();
        let info =
            match falcon::program::load_elf(bytes, &mut self.native_mut().mem_mut_unjournaled().ram)
            {
                Ok(info) => info,
                Err(e) => {
                    self.console.push_error(e.to_string());
                    self.run.faulted = true;
                    return;
                }
            };

        self.native_mut().cpu_mut_unjournaled().pc = info.entry;
        self.run.prev_pc = info.entry;
        self.run.base_pc = info.text_base;
        self.run.data_base = info.data_base;
        self.run.mem_view_addr = info.data_base;
        self.run.mem_region = MemRegion::Data;
        self.native_mut().mem_mut_unjournaled().invalidate_all();
        self.native_mut().mem_mut_unjournaled().reset_stats();
        self.run.heap_start = info.heap_start;
        self.native_mut().cpu_mut_unjournaled().heap_break = info.heap_start;

        // Labels and the sections viewer come from the ELF symbol table.
        self.run.labels = info.symbols;
        self.run.halt_pcs.clear();
        self.run.elf_sections = info.sections;
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.run.mem_access_log.clear();
        self.run.reg_age = [255u8; 32];
        self.run.f_age = [255u8; 32];
        self.run.reg_last_write_pc = [None; 32];
        self.run.f_last_write_pc = [None; 32];

        let words: Vec<u32> = info
            .text_bytes
            .chunks(4)
            .map(|chunk| {
                let mut word = [0u8; 4];
                word[..chunk.len()].copy_from_slice(chunk);
                u32::from_le_bytes(word)
            })
            .collect();
        let instruction_count = words.len();
        let data_bytes = self
            .run
            .elf_sections
            .iter()
            .map(|section| section.bytes.len())
            .sum();
        self.editor.last_ok_image = None;
        self.editor.last_ok_text = Some(words);
        self.editor.last_ok_data = Some(Vec::new());
        self.editor.last_ok_data_base = Some(info.data_base);
        self.editor.last_ok_bss_size = Some(0);
        self.editor.last_ok_elf_bytes = Some(bytes.to_vec());
        self.editor.last_build_stats = Some(BuildStats {
            instruction_count,
            data_bytes,
        });
        self.editor.last_assemble_msg = Some(format!(
            "Loaded ELF: {} bytes, entry 0x{:08X} ({instruction_count} instructions)",
            info.total_bytes, info.entry,
        ));
        self.editor.last_compile_ok = Some(true);
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;

        self.run.imem_scroll = 0;
        self.run.hover_imem_addr = None;
        self.rebuild_imem_vrow_cache();
        self.clear_details_selection();
        self.reset_exec_regions_to_loaded_text();
        self.sync_pipeline_program_range();
        let pc = self.native().cpu().pc;
        self.reset_pipeline_stages(pc);
        self.rebuild_harts();
        self.lock_editor_on_binary();
    }

    /// A loaded binary is not editable text: leave command mode in charge and
    /// close any stale "edit opcodes?" prompt.
    fn lock_editor_on_binary(&mut self) {
        self.mode = EditorMode::Command;
        self.editor.elf_prompt_open = false;
    }

    /// Convert the currently-loaded ELF into an editable assembly source and load
    /// it into the editor buffer.  Called when the user chooses "Edit opcodes".
    pub(super) fn load_elf_as_asm(&mut self) {
        let Some(text_words) = self.editor.last_ok_text.as_ref() else {
            return;
        };
        let source = crate::elf_listing::elf_to_asm_source(
            text_words,
            self.run.base_pc,
            &self.run.labels,
            &self.run.elf_sections,
        );
        // Load source into editor
        self.editor.buf.lines = source.lines().map(|l| l.to_string()).collect();
        if self.editor.buf.lines.is_empty() {
            self.editor.buf.lines.push(String::new());
        }
        self.editor.buf.cursor_row = 0;
        self.editor.buf.cursor_col = 0;
        // Discard ELF lock; editor now holds assembly source
        self.editor.last_ok_elf_bytes = None;
        self.editor.dirty = true;
        self.editor.last_edit_at = Some(std::time::Instant::now());
        self.editor.last_build_stats = None;
        self.editor.last_assemble_msg =
            Some("ELF disassembled — edit and press Run to re-assemble.".to_string());
        self.mode = crate::ui::app::EditorMode::Insert;
        // Trigger live syntax check so labels and diagnostics appear immediately
        self.check_assemble();
    }

    /// Commit the current numeric edit_buf into pending config for the selected level.
    pub(super) fn commit_cache_edit(&mut self) {
        if let Some((is_icache, field)) = self.cache.edit_field {
            self.cache.config_error = None;
            self.cache.config_status = None;
            if field.is_numeric() {
                let s = self.cache.edit_buf.trim().to_string();
                let cfg = self.selected_level_pending_cfg_mut(is_icache);
                match field {
                    ConfigField::Size => {
                        if let Ok(v) = s.parse::<usize>() {
                            // Size must yield a power-of-2 number of sets.
                            // Snap: compute the unit (line_size * assoc), then snap
                            // num_sets = v/unit to the nearest power of two.
                            let unit = cfg.line_size.max(1) * cfg.associativity.max(1);
                            let sets = (v / unit).max(1);
                            let snapped = nearest_pow2_clamp(sets, 1, 1 << 20);
                            cfg.size = snapped * unit;
                        }
                    }
                    ConfigField::LineSize => {
                        if let Ok(v) = s.parse::<usize>() {
                            cfg.line_size = nearest_pow2_clamp(v, 4, 4096);
                        }
                    }
                    ConfigField::Associativity => {
                        if let Ok(v) = s.parse::<usize>() {
                            cfg.associativity = v.max(1);
                        }
                    }
                    ConfigField::HitLatency => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.hit_latency = v.max(1);
                        }
                    }
                    ConfigField::MissPenalty => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.miss_penalty = v;
                        }
                    }
                    ConfigField::AssocPenalty => {
                        if let Ok(v) = s.parse::<u64>() {
                            cfg.assoc_penalty = v;
                        }
                    }
                    ConfigField::TransferWidth => {
                        if let Ok(v) = s.parse::<u32>() {
                            cfg.transfer_width = v.max(1);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Cycle an enum-typed config field (forward=true → next option).
    pub(super) fn cycle_cache_field(&mut self, is_icache: bool, field: ConfigField, forward: bool) {
        use crate::falcon::cache::{ReplacementPolicy, WriteAllocPolicy, WritePolicy};
        self.cache.config_error = None;
        self.cache.config_status = None;
        let cfg = self.selected_level_pending_cfg_mut(is_icache);
        match field {
            ConfigField::Replacement => {
                cfg.replacement = if forward {
                    match cfg.replacement {
                        ReplacementPolicy::Lru => ReplacementPolicy::Mru,
                        ReplacementPolicy::Mru => ReplacementPolicy::Fifo,
                        ReplacementPolicy::Fifo => ReplacementPolicy::Random,
                        ReplacementPolicy::Random => ReplacementPolicy::Lfu,
                        ReplacementPolicy::Lfu => ReplacementPolicy::Clock,
                        ReplacementPolicy::Clock => ReplacementPolicy::Lru,
                    }
                } else {
                    match cfg.replacement {
                        ReplacementPolicy::Lru => ReplacementPolicy::Clock,
                        ReplacementPolicy::Mru => ReplacementPolicy::Lru,
                        ReplacementPolicy::Fifo => ReplacementPolicy::Mru,
                        ReplacementPolicy::Random => ReplacementPolicy::Fifo,
                        ReplacementPolicy::Lfu => ReplacementPolicy::Random,
                        ReplacementPolicy::Clock => ReplacementPolicy::Lfu,
                    }
                };
            }
            ConfigField::WritePolicy => {
                cfg.write_policy = match cfg.write_policy {
                    WritePolicy::WriteThrough => WritePolicy::WriteBack,
                    WritePolicy::WriteBack => WritePolicy::WriteThrough,
                };
            }
            ConfigField::WriteAlloc => {
                cfg.write_alloc = match cfg.write_alloc {
                    WriteAllocPolicy::WriteAllocate => WriteAllocPolicy::NoWriteAllocate,
                    WriteAllocPolicy::NoWriteAllocate => WriteAllocPolicy::WriteAllocate,
                };
            }
            ConfigField::Inclusion => {
                use crate::falcon::cache::InclusionPolicy;
                cfg.inclusion = if forward {
                    match cfg.inclusion {
                        InclusionPolicy::NonInclusive => InclusionPolicy::Inclusive,
                        InclusionPolicy::Inclusive => InclusionPolicy::Exclusive,
                        InclusionPolicy::Exclusive => InclusionPolicy::NonInclusive,
                    }
                } else {
                    match cfg.inclusion {
                        InclusionPolicy::NonInclusive => InclusionPolicy::Exclusive,
                        InclusionPolicy::Inclusive => InclusionPolicy::NonInclusive,
                        InclusionPolicy::Exclusive => InclusionPolicy::Inclusive,
                    }
                };
            }
            _ => {}
        }
    }

    /// Current pending config field value as string (for populating edit_buf).
    pub(super) fn cache_field_value_str(&self, is_icache: bool, field: ConfigField) -> String {
        let cfg = self.selected_level_pending_cfg(is_icache);
        match field {
            ConfigField::Size => cfg.size.to_string(),
            ConfigField::LineSize => cfg.line_size.to_string(),
            ConfigField::Associativity => cfg.associativity.to_string(),
            ConfigField::HitLatency => cfg.hit_latency.to_string(),
            ConfigField::MissPenalty => cfg.miss_penalty.to_string(),
            ConfigField::AssocPenalty => cfg.assoc_penalty.to_string(),
            ConfigField::TransferWidth => cfg.transfer_width.to_string(),
            _ => String::new(),
        }
    }

    /// Get the pending config for the selected cache level (immutable).
    /// Level 0 = L1 (is_icache selects I or D); Level 1+ = L2+ (is_icache ignored).
    pub(super) fn selected_level_pending_cfg(&self, is_icache: bool) -> &CacheConfig {
        let level = self.cache.selected_level;
        if level == 0 {
            if is_icache {
                &self.cache.pending_icache
            } else {
                &self.cache.pending_dcache
            }
        } else if level - 1 < self.cache.extra_pending.len() {
            &self.cache.extra_pending[level - 1]
        } else {
            &self.cache.pending_dcache // fallback
        }
    }

    /// Get the pending config for the selected cache level (mutable).
    pub(super) fn selected_level_pending_cfg_mut(&mut self, is_icache: bool) -> &mut CacheConfig {
        let level = self.cache.selected_level;
        if level == 0 {
            if is_icache {
                &mut self.cache.pending_icache
            } else {
                &mut self.cache.pending_dcache
            }
        } else if level - 1 < self.cache.extra_pending.len() {
            &mut self.cache.extra_pending[level - 1]
        } else {
            &mut self.cache.pending_dcache // fallback
        }
    }

    /// Add a new extra cache level (L2, L3, …).
    pub(super) fn add_cache_level(&mut self) {
        use crate::falcon::cache::extra_level_presets;
        let cfg = extra_level_presets()[0].clone(); // Small L2 default
        self.cache.extra_pending.push(cfg.clone());
        if let Some(runtime) = self.rv32_mut() {
            runtime.mem_mut_unjournaled().add_extra_level(cfg);
        }
        // Select the newly added level
        self.cache.selected_level = self.cache.extra_pending.len(); // 1-based (L1=0)
    }

    // ── TLB helpers ─────────────────────────────────────────────────────────

    pub(crate) fn flush_tlb(&mut self) {
        if let Some(runtime) = self.rv32_mut() {
            runtime.mem_mut_unjournaled().mmu_mut().tlb.flush();
        }
        self.tlb.config_status = Some("TLB flushed".into());
        self.tlb.config_error = None;
    }

    // ── VM Settings panel helpers ──────────────────────────────────────────

    /// The string shown when a numeric VM-settings field enters edit mode.
    pub(crate) fn vm_field_value_str(&self, field: VmSettingsField) -> String {
        use crate::falcon::mmu::MapKind;
        match field {
            VmSettingsField::Offset => match self.tlb.pending_map.kind {
                MapKind::Offset(v) => v.to_string(),
                _ => "0".into(),
            },
            VmSettingsField::OffsetBits => self.tlb.pending_scheme.offset_bits.to_string(),
            VmSettingsField::LevelBits(i) => self
                .tlb
                .pending_scheme
                .level_bits
                .get(i)
                .copied()
                .unwrap_or(0)
                .to_string(),
            VmSettingsField::Asid => self.tlb.pending_map.asid.to_string(),
            VmSettingsField::TlbEntries => self.tlb.pending.entry_count.to_string(),
            VmSettingsField::TlbAssoc => self.tlb.pending.associativity.to_string(),
            VmSettingsField::TlbHitLat => self.tlb.pending.hit_latency.to_string(),
            VmSettingsField::TlbMissLat => self.tlb.pending.miss_penalty.to_string(),
            _ => String::new(),
        }
    }

    /// Commit the in-progress numeric VM-settings edit into the pending state.
    pub(crate) fn commit_vm_edit(&mut self) {
        use crate::falcon::mmu::MapKind;
        let Some(field) = self.tlb.vm_edit_field else {
            return;
        };
        let buf = self.tlb.vm_edit_buf.trim().to_string();
        match field {
            VmSettingsField::Offset => match buf.parse::<i32>() {
                Ok(v) => {
                    self.tlb.pending_map.kind = MapKind::Offset(v);
                    self.tlb.map_status = None;
                }
                Err(_) if buf.is_empty() || buf == "-" => {
                    self.tlb.pending_map.kind = MapKind::Offset(0);
                }
                Err(_) => {
                    self.tlb.map_status = Some("offset must be an integer (MiB)".into());
                }
            },
            VmSettingsField::OffsetBits => {
                if let Ok(v) = buf.parse::<u8>() {
                    self.tlb.pending_scheme.offset_bits = v.clamp(12, 30);
                    self.tlb.map_status = None;
                }
            }
            VmSettingsField::LevelBits(i) => {
                if let Ok(v) = buf.parse::<u8>() {
                    if let Some(b) = self.tlb.pending_scheme.level_bits.get_mut(i) {
                        *b = v.clamp(1, 12);
                    }
                    self.tlb.map_status = None;
                }
            }
            VmSettingsField::Asid => {
                if let Ok(v) = buf.parse::<u16>() {
                    self.tlb.pending_map.asid = v.min(511);
                    self.tlb.map_status = None;
                }
            }
            VmSettingsField::TlbEntries => {
                if let Ok(v) = buf.parse::<u16>() {
                    self.tlb.pending.entry_count = v.clamp(1, 4096);
                }
            }
            VmSettingsField::TlbAssoc => {
                if let Ok(v) = buf.parse::<u8>() {
                    self.tlb.pending.associativity = v.clamp(1, 64);
                }
            }
            VmSettingsField::TlbHitLat => {
                if let Ok(v) = buf.parse::<u8>() {
                    self.tlb.pending.hit_latency = v;
                }
            }
            VmSettingsField::TlbMissLat => {
                if let Ok(v) = buf.parse::<u8>() {
                    self.tlb.pending.miss_penalty = v;
                }
            }
            _ => {}
        }
    }

    /// Toggle / cycle a non-numeric VM-settings control (click action).
    pub(crate) fn toggle_vm_field(&mut self, field: VmSettingsField) {
        use crate::falcon::cache::ReplacementPolicy;
        use crate::falcon::mmu::MapKind;
        self.tlb.map_status = None;
        match field {
            VmSettingsField::Mode => {
                let next = self.vm_mode().cycle();
                self.set_vm_mode(next);
            }
            VmSettingsField::TlbEnabled => {
                self.set_tlb_enabled(!self.run.tlb_enabled);
            }
            VmSettingsField::Kind => {
                self.tlb.pending_map.kind = match self.tlb.pending_map.kind {
                    MapKind::Identity => MapKind::Offset(0),
                    MapKind::Offset(_) => MapKind::Identity,
                };
            }
            VmSettingsField::AddLevel => {
                if self.tlb.pending_scheme.level_bits.len() < 4 {
                    self.tlb.pending_scheme.level_bits.push(10);
                }
            }
            VmSettingsField::RemoveLevel => {
                if self.tlb.pending_scheme.level_bits.len() > 1 {
                    self.tlb.pending_scheme.level_bits.pop();
                }
            }
            VmSettingsField::PermR => self.tlb.pending_map.perms.r ^= true,
            VmSettingsField::PermW => self.tlb.pending_map.perms.w ^= true,
            VmSettingsField::PermX => self.tlb.pending_map.perms.x ^= true,
            VmSettingsField::PermU => self.tlb.pending_map.perms.u ^= true,
            VmSettingsField::Global => self.tlb.pending_map.global ^= true,
            VmSettingsField::TlbReplacement => {
                self.tlb.pending.replacement = match self.tlb.pending.replacement {
                    ReplacementPolicy::Lru => ReplacementPolicy::Mru,
                    ReplacementPolicy::Mru => ReplacementPolicy::Fifo,
                    ReplacementPolicy::Fifo => ReplacementPolicy::Random,
                    ReplacementPolicy::Random => ReplacementPolicy::Lfu,
                    ReplacementPolicy::Lfu => ReplacementPolicy::Clock,
                    ReplacementPolicy::Clock => ReplacementPolicy::Lru,
                };
            }
            // Numeric fields enter edit mode via the mouse/keyboard handlers.
            _ => {}
        }
    }

    /// Apply the whole VM Settings panel: TLB geometry, then the page map +
    /// paging scheme (in the didactic auto modes).
    pub(crate) fn apply_vm_settings(&mut self) {
        let cfg = self.tlb.pending.clone();
        if cfg.associativity < 1 || cfg.entry_count < cfg.associativity as u16 {
            self.tlb.map_status = Some("TLB: entry count must be ≥ associativity ≥ 1".into());
            return;
        }
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        runtime.mem_mut_unjournaled().mmu_mut().tlb.reconfigure(cfg);
        if self.run.vm_mode.is_auto() {
            self.apply_page_map();
        } else {
            // Manual / Off: the map is program-driven; only the TLB applies.
            self.tlb.map_status = Some("TLB applied (map is program-driven in this mode)".into());
        }
    }

    /// Apply the pending paging scheme + map: rewrite the root page table in
    /// RAM and re-point satp. Only valid in the didactic auto modes (Sv32 /
    /// Custom), where the simulator owns the page tables.
    pub(crate) fn apply_page_map(&mut self) {
        use crate::falcon::mmu::{Mmu, Satp, VmMode};
        if !self.run.vm_mode.is_auto() {
            self.tlb.map_status = Some("set VM mode to Sv32 or Custom first".into());
            return;
        }
        // In Custom mode validate the user scheme before touching RAM.
        let scheme = if matches!(self.run.vm_mode, VmMode::Custom) {
            if !self.tlb.pending_scheme.is_valid() {
                self.tlb.map_status =
                    Some("invalid scheme: index+offset bits must total 32".into());
                return;
            }
            self.tlb.pending_scheme.clone()
        } else {
            crate::falcon::mmu::PagingScheme::sv32()
        };
        let root_pa = scheme.root_pa(self.run.mem_size as u32);
        let window = (
            self.run.base_pc.min(self.run.data_base),
            self.run.heap_start,
        );
        let spec = self.tlb.pending_map;
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        Mmu::install_map_scheme(
            &mut runtime.mem_mut_unjournaled().ram,
            root_pa,
            &scheme,
            spec,
            window,
        );
        let satp_val = Mmu::make_satp(root_pa, spec.asid);
        runtime.cpu_mut_unjournaled().satp = satp_val;
        let mmu = runtime.mem_mut_unjournaled().mmu_mut();
        mmu.set_scheme(scheme);
        mmu.satp = Satp::new(satp_val);
        mmu.force_translate = true;
        // Stale cached translations would mask the new map.
        mmu.tlb.flush();
        self.tlb.page_map = spec;
        self.tlb.map_status = Some("Map applied (TLB flushed)".into());
    }

    pub(crate) fn apply_tlb_preset(&mut self, idx: usize) {
        use crate::falcon::cache::ReplacementPolicy;
        let (entries, assoc) = match idx {
            0 => (16u16, 4u8),
            1 => (32u16, 4u8),
            _ => (64u16, 8u8),
        };
        self.tlb.pending.entry_count = entries;
        self.tlb.pending.associativity = assoc;
        self.tlb.pending.replacement = ReplacementPolicy::Lru;
    }

    /// Delete the currently selected session snapshot, fixing up the scroll
    /// position and any open popup. Shared by the Cache and VM Stats subtabs.
    pub(crate) fn delete_selected_snapshot(&mut self) {
        if self.cache.session_history.is_empty() {
            return;
        }
        let idx = self
            .cache
            .history_scroll
            .min(self.cache.session_history.len() - 1);
        self.cache.session_history.remove(idx);
        if !self.cache.session_history.is_empty() {
            self.cache.history_scroll = idx.min(self.cache.session_history.len() - 1);
        } else {
            self.cache.history_scroll = 0;
        }
        if let Some(v) = self.cache.viewing_snapshot {
            if v == idx {
                self.cache.viewing_snapshot = None;
            } else if v > idx {
                self.cache.viewing_snapshot = Some(v - 1);
            }
        }
        if self.cache.session_history.is_empty() {
            self.cache.viewing_snapshot = None;
        }
    }

    /// Remove the last extra cache level.
    pub(super) fn remove_last_cache_level(&mut self) {
        if !self.cache.extra_pending.is_empty() {
            self.cache.extra_pending.pop();
            if let Some(runtime) = self.rv32_mut() {
                runtime.mem_mut_unjournaled().remove_extra_level();
            }
            let max_level = self.cache.extra_pending.len();
            if self.cache.selected_level > max_level {
                self.cache.selected_level = max_level;
            }
        }
    }

    // ── Instruction-memory scroll helpers (visual-row units) ─────────────────

    pub(crate) fn text_exec_region(&self) -> Option<crate::falcon::registers::ExecRegion> {
        if let Some(text) = &self.editor.last_ok_text {
            let start = self.run.base_pc;
            let end = start.saturating_add((text.len() as u32).saturating_mul(4));
            Some(crate::falcon::registers::ExecRegion::new(start, end))
        } else {
            None
        }
    }

    fn imem_in_range(&self, addr: u32) -> bool {
        self.text_exec_region()
            .is_some_and(|region| region.contains(addr))
    }

    pub(crate) fn executable_region_containing(
        &self,
        addr: u32,
    ) -> Option<crate::falcon::registers::ExecRegion> {
        self.run
            .exec_regions
            .iter()
            .copied()
            .find(|region| region.contains(addr))
    }

    pub(crate) fn pc_in_executable_region(&self, addr: u32) -> bool {
        self.executable_region_containing(addr).is_some()
    }

    pub(crate) fn active_imem_exec_region(&self) -> Option<crate::falcon::registers::ExecRegion> {
        let pc = self.program_counter() as u32;
        let region = self.executable_region_containing(pc)?;
        if self.imem_in_range(pc) {
            None
        } else {
            Some(region)
        }
    }

    fn reset_exec_regions_to_loaded_text(&mut self) {
        self.run.exec_regions.clear();
        if let Some(region) = self.text_exec_region() {
            self.run.exec_regions.push(region);
        }
    }

    fn register_exec_region(&mut self, region: crate::falcon::registers::ExecRegion) {
        if region.start >= region.end {
            return;
        }
        self.run.exec_regions.push(region);
        self.run.exec_regions.sort_by_key(|r| r.start);

        let mut merged: Vec<crate::falcon::registers::ExecRegion> =
            Vec::with_capacity(self.run.exec_regions.len());
        for current in self.run.exec_regions.iter().copied() {
            if let Some(last) = merged.last_mut() {
                if current.start <= last.end {
                    last.end = last.end.max(current.end);
                    continue;
                }
            }
            merged.push(current);
        }
        self.run.exec_regions = merged;
        self.sync_pipeline_program_range();
    }

    fn process_pending_exec_map_for_selected(&mut self) {
        if let Some(region) = self
            .native_mut()
            .cpu_mut_unjournaled()
            .pending_exec_map
            .take()
        {
            self.register_exec_region(region);
        }
    }

    fn process_pending_exec_map_for_bg(&mut self, core_idx: usize) {
        if let Some(region) = self.harts[core_idx].cpu.pending_exec_map.take() {
            self.register_exec_region(region);
        }
    }

    /// Total visual rows in the instruction list (block_comment + labels + instruction per addr).
    pub(super) fn imem_total_visual_rows(&self) -> usize {
        if let Some(region) = self.active_imem_exec_region() {
            return ((region.end.saturating_sub(region.start)) / 4) as usize;
        }
        let mut count = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) {
                break;
            }
            if self.run.block_comments.contains_key(&addr) {
                count += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                count += names.len();
            }
            count += 1;
            addr = addr.wrapping_add(4);
        }
        count
    }

    /// Returns (start_addr, header_skip) for the current imem_scroll (visual row offset).
    /// header_skip = how many block_comment/label rows to skip at the top of start_addr's block.
    pub(super) fn imem_addr_skip_for_scroll(&self) -> (u32, usize) {
        if let Some(region) = self.active_imem_exec_region() {
            let start = region
                .start
                .saturating_add((self.run.imem_scroll as u32).saturating_mul(4));
            return (start.min(region.end.saturating_sub(4)), 0);
        }
        let scroll = self.run.imem_scroll;
        let base = self.run.base_pc;
        let mut vrow = 0usize;
        let mut addr = base;
        loop {
            if !self.imem_in_range(addr) {
                return (base, 0);
            }
            let bc = if self.run.block_comments.contains_key(&addr) {
                1
            } else {
                0
            };
            let lbls = self.run.labels.get(&addr).map_or(0, |v| v.len());
            let block = bc + lbls + 1;
            if vrow + block > scroll {
                return (addr, scroll - vrow);
            }
            vrow += block;
            addr = addr.wrapping_add(4);
        }
    }

    /// Visual row of the current PC within the full instruction list.
    pub(super) fn imem_visual_row_of_pc(&self) -> Option<usize> {
        let pc = self.program_counter() as u32;
        if let Some(region) = self.active_imem_exec_region() {
            return Some(((pc.saturating_sub(region.start)) / 4) as usize);
        }
        if pc < self.run.base_pc {
            return None;
        }
        let mut vrow = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) {
                return None;
            }
            if self.run.block_comments.contains_key(&addr) {
                vrow += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                vrow += names.len();
            }
            if addr == pc {
                return Some(vrow);
            }
            vrow += 1;
            addr = addr.wrapping_add(4);
        }
    }

    /// Ensure PC is visible in the imem panel, updating imem_scroll if needed.
    pub(super) fn ensure_pc_visible_in_imem(&mut self) {
        let visible = self.run.imem_inner_height.get();
        if visible == 0 {
            return;
        }
        if let Some(region) = self.active_imem_exec_region() {
            let pc_row =
                (((self.program_counter() as u32).saturating_sub(region.start)) / 4) as usize;
            let max_scroll = ((region.end.saturating_sub(region.start)) / 4) as usize;
            let max_scroll = max_scroll.saturating_sub(visible);
            let scroll = self.run.imem_scroll.min(max_scroll);
            if pc_row < scroll {
                self.run.imem_scroll = pc_row.saturating_sub(2);
            } else if pc_row + 1 >= scroll + visible {
                self.run.imem_scroll = pc_row
                    .saturating_sub(visible.saturating_sub(3))
                    .min(max_scroll);
            }
            return;
        }
        let Some(pc_vrow) = self.imem_visual_row_of_pc() else {
            return;
        };
        let scroll = self.run.imem_scroll;
        if pc_vrow < scroll {
            // PC above view
            self.run.imem_scroll = pc_vrow.saturating_sub(2);
        } else if pc_vrow + 1 >= scroll + visible {
            // PC at or below bottom edge
            self.run.imem_scroll = pc_vrow.saturating_sub(visible.saturating_sub(3));
        }
    }

    /// Visual row of an arbitrary address within the full instruction list.
    /// O(1) — reads the pre-computed cache built by `rebuild_imem_vrow_cache`.
    pub(super) fn imem_visual_row_of_addr(&self, target: u32) -> Option<usize> {
        self.run.imem_vrow_cache.get(&target).copied()
    }

    /// Rebuild the addr→visual-row cache and the pre-lowercased label index.
    /// Must be called whenever `run.labels` or `run.block_comments` change
    /// (i.e. after every program load).
    pub(super) fn rebuild_imem_vrow_cache(&mut self) {
        let mut cache =
            std::collections::HashMap::with_capacity((self.run.mem_size / 4).min(1 << 20));
        let mut vrow = 0usize;
        let mut addr = self.run.base_pc;
        loop {
            if !self.imem_in_range(addr) {
                break;
            }
            if self.run.block_comments.contains_key(&addr) {
                vrow += 1;
            }
            if let Some(names) = self.run.labels.get(&addr) {
                vrow += names.len();
            }
            cache.insert(addr, vrow);
            vrow += 1;
            addr = addr.wrapping_add(4);
        }
        self.run.imem_vrow_cache = cache;
        self.run.labels_lower = self
            .run
            .labels
            .iter()
            .map(|(&a, names)| (a, names.iter().map(|n| n.to_lowercase()).collect()))
            .collect();
    }

    /// Scroll the instruction memory panel to bring `addr` near the top.
    pub(super) fn scroll_imem_to_addr(&mut self, addr: u32) {
        if let Some(vrow) = self.imem_visual_row_of_addr(addr) {
            self.run.imem_scroll = vrow.saturating_sub(2);
        }
    }

    fn tick(&mut self) {
        // A trait-driven backend has one gear: step while running. The rest of
        // this method is the native runtime's — speed limiting, GO checkpoints,
        // pipeline cycles, background harts — none of which the trait models.
        if self.rv32().is_none() {
            if self.run.is_running {
                match self.run.speed {
                    RunSpeed::X1 | RunSpeed::X2 => {
                        let divisor = if matches!(self.run.speed, RunSpeed::X2) { 2 } else { 1 };
                        if self.run.last_step_time.elapsed() >= self.run.step_interval / divisor {
                            self.machine_step();
                            self.run.last_step_time = Instant::now();
                        }
                    }
                    RunSpeed::X4 | RunSpeed::X8 => {
                        let steps = if matches!(self.run.speed, RunSpeed::X4) { 4 } else { 8 };
                        for _ in 0..steps {
                            if !self.run.is_running {
                                break;
                            }
                            self.machine_step();
                        }
                    }
                    RunSpeed::Instant => {
                        let start = Instant::now();
                        while self.run.is_running && start.elapsed() < Duration::from_millis(8) {
                            self.machine_step();
                        }
                    }
                }
            }
            if matches!(self.tab, Tab::Editor)
                && self.editor.dirty
                && self
                    .editor
                    .last_edit_at
                    .is_some_and(|at| at.elapsed() >= self.editor.auto_check_delay)
            {
                self.check_assemble();
            }
            return;
        }
        if self.native().cpu().exit_code.is_some() || self.native().cpu().local_exit {
            self.run.is_running = false;
        }
        if self.run.is_running {
            // A live run owns the state; close any inline editor left open so its
            // keystrokes don't fight the running program.
            self.cancel_run_edit();
            // A GO/Instant burst writes RAM directly (un-journaled), so take one
            // full checkpoint at the burst's first tick — step-back can then
            // rewind to just before it. Rate-limited modes single-step through
            // the journaling path and need no checkpoint; pipeline modes journal
            // per-cycle separately (Phase 4b).
            let go_burst = matches!(self.run.speed, RunSpeed::Instant)
                && !self.native().pipeline().enabled
                && !self.native().pipeline().sequential_mode;
            if go_burst && !self.run.go_checkpointed {
                self.native_mut().checkpoint();
                self.run.go_checkpointed = true;
            }
            // When pipeline is enabled and we're viewing the Pipeline tab,
            // use pipeline speed for rate-limiting (educational slow stepping).
            // Otherwise use run speed.
            use crate::ui::pipeline::PipelineSpeed;
            let use_pipeline_speed = (self.native().pipeline().enabled
                || self.native().pipeline().sequential_mode)
                && matches!(self.tab, Tab::Pipeline);

            if use_pipeline_speed {
                match self.run.pipeline_view().speed {
                    PipelineSpeed::Slow => {
                        if self.run.pipeline_view().last_tick.elapsed() >= Duration::from_millis(600) {
                            self.single_step();
                            self.run.pipeline_view_mut().last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Normal => {
                        if self.run.pipeline_view().last_tick.elapsed() >= Duration::from_millis(300) {
                            self.single_step();
                            self.run.pipeline_view_mut().last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Fast => {
                        if self.run.pipeline_view().last_tick.elapsed() >= Duration::from_millis(80) {
                            self.single_step();
                            self.run.pipeline_view_mut().last_tick = Instant::now();
                        }
                    }
                    PipelineSpeed::Instant => {
                        let budget = Duration::from_millis(8);
                        let start = Instant::now();
                        while self.run.is_running && start.elapsed() < budget {
                            self.single_step();
                        }
                    }
                }
            } else {
                match self.run.speed {
                    RunSpeed::X1 => {
                        if self.run.last_step_time.elapsed() >= self.run.step_interval {
                            self.single_step();
                            self.run.last_step_time = Instant::now();
                        }
                    }
                    RunSpeed::X2 => {
                        if self.run.last_step_time.elapsed() >= Duration::from_millis(20) {
                            self.single_step();
                            self.run.last_step_time = Instant::now();
                        }
                    }
                    RunSpeed::X4 => {
                        for _ in 0..4 {
                            if !self.run.is_running {
                                break;
                            }
                            self.single_step();
                        }
                    }
                    RunSpeed::X8 => {
                        for _ in 0..8 {
                            if !self.run.is_running {
                                break;
                            }
                            self.single_step();
                        }
                    }
                    RunSpeed::Instant => {
                        let budget = Duration::from_millis(14);
                        let start = Instant::now();
                        while self.run.is_running && start.elapsed() < budget {
                            self.single_step();
                        }
                    }
                }
            }
        }
        // Auto-open the Screen sub-view the first time this program calls
        // screen_init (2000). One-shot so Esc can close it afterwards. When
        // the screen went to an OS window, don't mirror it in the TUI — the
        // toolbar toggle still opens the sub-view manually if wanted.
        if let Some(screen) = &self.console.screen {
            if !self.run.screen_seen {
                self.run.screen_seen = true;
                self.run.show_screen = !screen.has_window();
            }
        }
        // Arm the next GO burst's one-shot checkpoint once the run has stopped.
        if !self.run.is_running {
            self.run.go_checkpointed = false;
        }
        // Scroll instruction list to follow PC (skipped in Instant to avoid pointless churn)
        if self.run.is_running && !matches!(self.run.speed, RunSpeed::Instant) {
            self.ensure_pc_visible_in_imem();
        }
        // Auto-follow SP/HB in Stack and Heap views — runs every tick so it works
        // regardless of execution path (sequential or pipeline).
        match self.run.mem_region {
            MemRegion::Stack => {
                let sp = self.native().cpu().x[2];
                self.run.mem_view_addr = sp & !(self.run.mem_view_bytes - 1);
            }
            MemRegion::Heap => {
                let hb = self.native().cpu().heap_break;
                self.run.mem_view_addr = hb & !(self.run.mem_view_bytes - 1);
            }
            _ => {}
        }
        if matches!(self.tab, Tab::Editor) && self.editor.dirty {
            if let Some(t) = self.editor.last_edit_at {
                if t.elapsed() >= self.editor.auto_check_delay {
                    self.check_assemble();
                    if self.editor.last_compile_ok == Some(true) {
                        self.load_last_ok_program();
                    }
                }
            }
        }
    }

    fn finalize_selected_core_after_step(&mut self) {
        self.process_pending_hart_start_for_selected();
        self.process_pending_exec_map_for_selected();
        let heap_break = self.native().cpu().heap_break;
        self.propagate_heap_break(heap_break);
        let program_exit = self.native().cpu().exit_code;

        let lifecycle = if self.native().cpu().local_exit {
            // FALCON_HART_EXIT: exit only this hart, leave others running.
            HartLifecycle::Exited
        } else if self.native().cpu().ebreak_hit {
            if self.run.halt_pcs.contains(&self.run.prev_pc) {
                HartLifecycle::Exited
            } else {
                HartLifecycle::Paused
            }
        } else if self.run.faulted || self.native().pipeline().faulted {
            HartLifecycle::Faulted
        } else if self.native().cpu().exit_code.is_some() || self.native().pipeline().halted {
            HartLifecycle::Exited
        } else {
            HartLifecycle::Running
        };

        if let Some(runtime) = self.selected_runtime_mut() {
            runtime.lifecycle = lifecycle;
            runtime.hart_id.get_or_insert(0);
            runtime.faulted = matches!(lifecycle, HartLifecycle::Faulted);
        }

        if let Some(code) = program_exit {
            // Global exit — kill all harts immediately.
            for hart in &mut self.harts {
                if hart.hart_id.is_some() {
                    hart.lifecycle = HartLifecycle::Exited;
                    hart.cpu.exit_code = Some(code);
                }
            }
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Faulted) {
            // A fault in any hart stops the whole run.
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Paused) {
            // In AllHarts scope: only stop the run when no other harts are still running.
            // The paused hart is skipped by step_all_cores_once; others keep going.
            // In FocusedHart scope or single-core: stop everything so the user can inspect.
            let stop_all = self.max_cores <= 1
                || matches!(self.run_scope, RunScope::FocusedHart)
                || !self.any_running_harts();
            if stop_all {
                self.run.is_running = false;
            }
        } else if !matches!(lifecycle, HartLifecycle::Running) && !self.any_running_harts() {
            // Last hart finished (halt/local-exit) — stop.
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
        }
    }

    /// Whether a step-back is currently allowed: not mid auto-run, and with at
    /// least one journaled change to undo.
    ///
    /// The journal is the ground truth for what is reversible. The sequential
    /// interpreter (`step_interpreted`), each pipeline clock cycle
    /// (`step_pipeline`), and GO checkpoints fill it; background harts and
    /// program exit/fault mutate state through the un-journaled hatches, which
    /// clear it. So a non-empty journal already implies the last activity was a
    /// reversible step, pipeline cycle, or GO burst — no separate mode check is
    /// needed here.
    pub(crate) fn can_stepback_now(&self) -> bool {
        !self.run.is_running
            && self
                .rv32()
                .is_some_and(FalconRuntime::can_stepback)
    }

    /// Undo the most recent journaled change — one instruction, one edit, or the
    /// whole of the last GO burst (back to its checkpoint) — then refresh the
    /// derived run-tab bookkeeping so the view matches the rewound state.
    pub(crate) fn stepback_one(&mut self) {
        if !self.can_stepback_now() {
            return;
        }
        let before_x = self.native().cpu().x;
        let before_f = self.native().cpu().f;
        let Some(kind) = self.native_mut().stepback() else {
            return;
        };

        let now_x = self.native().cpu().x;
        let now_f = self.native().cpu().f;
        let pc = self.native().cpu().pc;

        // Highlight the registers/floats the undo reverted and age the rest,
        // mirroring the forward single-step bookkeeping.
        for i in 0..32 {
            if now_x[i] != before_x[i] {
                self.run.reg_age[i] = 0;
                self.run.reg_last_write_pc[i] = Some(pc);
            } else {
                self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
            }
            if now_f[i] != before_f[i] {
                self.run.f_age[i] = 0;
                self.run.f_last_write_pc[i] = Some(pc);
            } else {
                self.run.f_age[i] = self.run.f_age[i].saturating_add(1).min(8);
            }
        }
        self.run.prev_x = now_x;
        self.run.prev_f = now_f;
        self.run.prev_pc = pc;
        self.run.dyn_mem_access = None;

        // A committed instruction (sequential step, or a pipeline cycle that
        // retired one) owns one exec-trace row and one run-count tick; a
        // stall/bubble cycle (`Cycle`), an edit, or a GO checkpoint owns
        // neither. Decrement the run count for the *retired* PC — taken from the
        // popped trace row, not the rewound `cpu.pc`, since in the pipeline the
        // instruction that committed is several stages behind the fetch PC.
        if kind == crate::falcon::machine::StepbackKind::Step {
            if let Some((trace_pc, _)) = self.run.exec_trace.pop_back() {
                if let Some(count) = self.run.exec_counts.get_mut(&trace_pc) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.run.exec_counts.remove(&trace_pc);
                    }
                }
            }
        }

        // Rewinding out of a fault/halt returns to a runnable state.
        self.run.faulted = false;
        if let Some(runtime) = self.selected_runtime_mut() {
            runtime.lifecycle = HartLifecycle::Running;
            runtime.faulted = false;
        }
        self.ensure_pc_visible_in_imem();
    }

    // ── Inline editing of live state (registers / PC / floats / RAM) ────────

    /// Open the inline editor on `target`, seeding the buffer with the cell's
    /// current value. No-op while a run is active — running state is the
    /// program's to own, not the user's to hand-edit.
    pub(crate) fn begin_run_edit(&mut self, target: RunEditTarget) {
        if self.run.is_running {
            return;
        }
        self.run.run_edit_buf = self.run_edit_seed(target);
        self.run.run_edit = Some(target);
        self.run.run_edit_error = None;
    }

    /// Close the inline editor, discarding any in-progress text.
    pub(crate) fn cancel_run_edit(&mut self) {
        self.run.run_edit = None;
        self.run.run_edit_buf.clear();
        self.run.run_edit_error = None;
    }

    /// Drop the details panel's pinned instruction (it follows the PC again)
    /// and close any instruction editor that referenced the old program.
    /// Called when a (re)load replaces the instruction memory.
    pub(crate) fn clear_details_selection(&mut self) {
        self.run.details_addr = None;
        self.run.last_details_click = None;
        self.run.details_field_hitboxes.borrow_mut().clear();
        if matches!(
            self.run.run_edit,
            Some(RunEditTarget::Instr { .. } | RunEditTarget::InstrField { .. })
        ) {
            self.cancel_run_edit();
        }
    }

    /// Commit the open inline edit: parse the buffer against the target's width
    /// and current display format, then write it through the journaling
    /// `Machine` mutator so step-back can undo it. On rejection the editor stays
    /// open and `run_edit_error` carries the reason, so the input isn't lost.
    pub(crate) fn commit_run_edit(&mut self) {
        let Some(target) = self.run.run_edit else {
            return;
        };
        let buf = self.run.run_edit_buf.clone();
        // The highlight bookkeeping below diffs RV32's two banks; a backend
        // without them gets no highlight rather than a wrong one.
        let before = self.rv32().map(|rv32| (rv32.cpu().x, rv32.cpu().f));

        let result: Result<(), String> = match target {
            RunEditTarget::Register(id) => self
                .parse_register_value(id, &buf)
                .and_then(|value| self.write_register_value(id, value)),
            RunEditTarget::ProgramCounter => {
                parse_cell(&buf, MemWidth::B4, self.cell_format(), self.run.show_signed)
                    .map_err(|e| e.message())
                    .and_then(|value| {
                        self.registers_mut()
                            .ok_or_else(|| "this backend has no program counter".to_string())?
                            .set_program_counter(value)
                            .map_err(|e| e.to_string())
                    })
            }
            RunEditTarget::Mem { addr, width } => {
                parse_cell(&buf, width, self.cell_format(), self.run.show_signed)
                    .map_err(|e| e.message())
                    .and_then(|value| self.poke_cell(addr, width.bytes() as usize, value))
            }
            RunEditTarget::Instr { addr } => {
                let width = self.instruction_width_at(u64::from(addr));
                parse_cell(&buf, MemWidth::B4, self.cell_format(), self.run.show_signed)
                    .map_err(|e| e.message())
                    .and_then(|value| self.write_instr_word(addr, value, width))
            }
            RunEditTarget::InstrField { addr, field } => self.commit_instruction_field(addr, field, &buf),
        };

        match result {
            Ok(()) => {
                // A PC edit must also steer the pipeline: with the pipeline
                // enabled (the default) execution fetches from `fetch_pc`, not
                // `cpu.pc`, so without this the next step keeps fetching the old
                // address. Mirrors the imem PC-redirect click. The redirect is
                // reversible state captured by the change-set's pipeline
                // snapshot, so step-back undoes it together with the PC write.
                if target == RunEditTarget::ProgramCounter
                    && self.pipeline_status().is_some_and(|status| status.enabled)
                {
                    let pc = self.program_counter() as u32;
                    self.redirect_pipeline_pc(pc);
                }
                self.cancel_run_edit();
                self.refresh_after_edit(before, target);
            }
            Err(message) => self.run.run_edit_error = Some(message),
        }
    }

    /// Read the text of a register edit as the value that register holds: a
    /// float bank takes a decimal, every other bank the Run tab's own format.
    fn parse_register_value(&self, id: RegisterId, text: &str) -> Result<u64, String> {
        let is_float = self.registers().and_then(|file| file.banks().get(id.bank)).is_some_and(
            |bank| bank.format == raven_riscv_engine::capability::RegisterFormat::Float,
        );
        if is_float {
            return text
                .trim()
                .parse::<f32>()
                .map(|value| u64::from(value.to_bits()))
                .map_err(|_| format!("cannot parse \"{}\" as a float", text.trim()));
        }
        parse_cell(text, MemWidth::B4, self.cell_format(), self.run.show_signed)
            .map_err(|e| e.message())
    }

    fn write_register_value(&mut self, id: RegisterId, value: u64) -> Result<(), String> {
        self.registers_mut()
            .ok_or_else(|| "this backend has no editable registers".to_string())?
            .write(id, value)
            .map_err(|e| e.to_string())
    }

    /// Write `value` into the `bytes`-wide cell at `addr` through the backend's
    /// own memory, little-endian like every other cell the pane shows.
    fn poke_cell(&mut self, addr: u32, bytes: usize, value: u64) -> Result<(), String> {
        let payload = value.to_le_bytes();
        self.memory_mut()
            .ok_or_else(|| "this backend exposes no writable memory".to_string())?
            .poke(u64::from(addr), &payload[..bytes.min(8)])
            .map_err(|e| e.to_string())
    }

    /// Commit one field of the instruction at `addr`.
    ///
    /// The mnemonic line goes through the backend's own assembler, so any
    /// architecture can be edited by retyping the instruction. The named
    /// encoding slots — `rd`, `funct7`, the scattered immediate — are RV32's
    /// own bit layout, and say so rather than splicing another ISA's word.
    fn commit_instruction_field(
        &mut self,
        addr: u32,
        field: InstrFieldKind,
        buf: &str,
    ) -> Result<(), String> {
        let width = self.instruction_width_at(u64::from(addr));
        if field == InstrFieldKind::Asm {
            let codec = self
                .code()
                .ok_or_else(|| "this backend cannot assemble a single instruction".to_string())?;
            let bytes = codec
                .assemble(u64::from(addr), buf)
                .map_err(|diagnostic| diagnostic.message)?;
            return self.write_instr_bytes(addr, &bytes);
        }

        // The binary view is the same word in another base — no encoding
        // knowledge, so every backend can be retyped bit by bit.
        if field == InstrFieldKind::Bin {
            let value = instr_edit::parse_field_value(field, buf)?;
            return self.write_instr_word(addr, value as u64, width);
        }

        let current = self
            .memory()
            .map_or(0, |memory| memory.peek_word(u64::from(addr), 4)) as u32;
        if self.rv32().is_none() {
            return Err(format!(
                "{field:?} is a field of RV32's encoding; edit the instruction line instead"
            ));
        }
        if !instr_edit::field_available(current, field) {
            return Err(format!("{field:?} is not a field of this instruction format"));
        }
        let value = instr_edit::parse_field_value(field, buf)?;
        let word = instr_edit::splice_field(current, detect_format(current), field, value)?;
        self.write_instr_word(addr, u64::from(word), width)
    }

    /// Write an instruction word of `width` bytes, then drop everything that
    /// may still hold the old one: the JIT's translation and the pipeline's
    /// latches. Shared by the word editor and the per-field editors.
    fn write_instr_word(&mut self, addr: u32, word: u64, width: usize) -> Result<(), String> {
        let bytes = word.to_le_bytes();
        self.write_instr_bytes(addr, &bytes[..width.clamp(1, 8)])
    }

    fn write_instr_bytes(&mut self, addr: u32, bytes: &[u8]) -> Result<(), String> {
        self.memory_mut()
            .ok_or_else(|| "this backend exposes no writable memory".to_string())?
            .poke(u64::from(addr), bytes)
            .map_err(|e| e.to_string())?;
        let end = addr.wrapping_add(bytes.len() as u32);
        self.run.backend.invalidate(addr, end);
        if self.pipeline_status().is_some_and(|status| status.enabled) {
            let pc = self.program_counter() as u32;
            self.redirect_pipeline_pc(pc);
        }
        Ok(())
    }

    /// Map the Run tab's display format to the parser's [`CellFormat`].
    pub(crate) fn cell_format(&self) -> CellFormat {
        match self.run.fmt_mode {
            FormatMode::Hex => CellFormat::Hex,
            FormatMode::Dec => CellFormat::Dec,
            FormatMode::Bin => CellFormat::Bin,
            FormatMode::Str => CellFormat::Str,
        }
    }

    /// The text an editor starts from: the cell's current value, formatted the
    /// way it reads on screen (plain digits, no `0x`). `Str` mode seeds empty —
    /// rendering non-printable bytes back as `.` would round-trip lossily.
    fn run_edit_seed(&self, target: RunEditTarget) -> String {
        let plain_word = |value: u32| match self.run.fmt_mode {
            FormatMode::Hex => format!("{value:x}"),
            FormatMode::Dec if self.run.show_signed => format!("{}", value as i32),
            FormatMode::Dec => format!("{value}"),
            FormatMode::Bin => format!("{value:b}"),
            FormatMode::Str => String::new(),
        };
        let peek = |addr: u32, bytes: usize| {
            self.memory()
                .map_or(0, |memory| memory.peek_word(u64::from(addr), bytes))
        };
        match target {
            RunEditTarget::ProgramCounter => plain_word(self.program_counter() as u32),
            RunEditTarget::Register(id) => {
                let value = self.registers().and_then(|file| file.read(id)).unwrap_or(0);
                let is_float = self
                    .registers()
                    .and_then(|file| file.banks().get(id.bank))
                    .is_some_and(|bank| {
                        bank.format == raven_riscv_engine::capability::RegisterFormat::Float
                    });
                if is_float {
                    format!("{}", f32::from_bits(value as u32))
                } else {
                    plain_word(value as u32)
                }
            }
            RunEditTarget::Mem { addr, width } => plain_word(peek(addr, width.bytes() as usize) as u32),
            // Seeded from the same read the details panel renders, at whatever
            // width this backend's instructions occupy.
            RunEditTarget::Instr { addr } => {
                plain_word(peek(addr, self.instruction_width_at(u64::from(addr))) as u32)
            }
            RunEditTarget::InstrField { addr, field } => match field {
                InstrFieldKind::Asm => self.disassemble_at(u64::from(addr)).unwrap_or_default(),
                InstrFieldKind::Bin => {
                    let width = self.instruction_width_at(u64::from(addr));
                    let bits = width * 8;
                    format!("{:0bits$b}", peek(addr, width))
                }
                _ => instr_edit::seed_field(peek(addr, 4) as u32, field),
            },
        }
    }

    /// Refresh the Run tab's highlight bookkeeping after a committed edit:
    /// light up the register/float it changed and flag the touched memory cell.
    fn refresh_after_edit(
        &mut self,
        before: Option<([u32; 32], [u32; 32])>,
        target: RunEditTarget,
    ) {
        let pc = self.program_counter() as u32;
        if let Some(((before_x, before_f), runtime)) = before.zip(self.rv32()) {
            let (now_x, now_f) = (runtime.cpu().x, runtime.cpu().f);
            for i in 0..32 {
                if now_x[i] != before_x[i] {
                    self.run.reg_age[i] = 0;
                    self.run.reg_last_write_pc[i] = Some(pc);
                }
                if now_f[i] != before_f[i] {
                    self.run.f_age[i] = 0;
                    self.run.f_last_write_pc[i] = Some(pc);
                }
            }
        }
        match target {
            RunEditTarget::Mem { addr, width } => {
                self.run.mem_access_log.push((addr, width.bytes(), 0));
            }
            RunEditTarget::Instr { addr } | RunEditTarget::InstrField { addr, .. } => {
                self.run
                    .mem_access_log
                    .push((addr, MemWidth::B4.bytes(), 0));
            }
            _ => {}
        }
        self.ensure_pc_visible_in_imem();
    }

    fn any_running_harts(&self) -> bool {
        self.harts
            .iter()
            .any(|hart| matches!(hart.lifecycle, HartLifecycle::Running))
    }

    /// Finalise the lifecycle of a non-selected hart after it has been stepped
    /// by `step_hart_bg_inner`.  Mirrors `finalize_selected_core_after_step`
    /// but operates directly on `self.harts[core_idx]` instead of `self.run`.
    fn finalize_bg_hart(&mut self, core_idx: usize, breakpoint_hit: bool) {
        self.process_pending_hart_start_for_bg(core_idx);
        self.process_pending_exec_map_for_bg(core_idx);

        let heap_break = self.harts[core_idx].cpu.heap_break;
        self.propagate_heap_break(heap_break);

        let program_exit = self.harts[core_idx].cpu.exit_code;

        let pipe_halted = self.harts[core_idx]
            .pipeline
            .as_ref()
            .map_or(false, |p| p.halted);
        let pipe_faulted = self.harts[core_idx]
            .pipeline
            .as_ref()
            .map_or(false, |p| p.faulted);

        let lifecycle = if self.harts[core_idx].cpu.local_exit {
            HartLifecycle::Exited
        } else if breakpoint_hit {
            HartLifecycle::Paused
        } else if self.harts[core_idx].cpu.ebreak_hit {
            if self.run.halt_pcs.contains(&self.harts[core_idx].prev_pc) {
                HartLifecycle::Exited
            } else {
                HartLifecycle::Paused
            }
        } else if self.harts[core_idx].faulted || pipe_faulted {
            HartLifecycle::Faulted
        } else if program_exit.is_some() || pipe_halted {
            HartLifecycle::Exited
        } else {
            HartLifecycle::Running
        };

        if let Some(code) = program_exit {
            // Global exit — mark every hart (including the one currently in
            // self.run) as exited and stop the run loop.
            for h in &mut self.harts {
                if h.hart_id.is_some() {
                    h.lifecycle = HartLifecycle::Exited;
                    h.cpu.exit_code = Some(code);
                }
            }
            self.native_mut().cpu_mut_unjournaled().exit_code = Some(code);
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
            return;
        }

        self.harts[core_idx].lifecycle = lifecycle;
        self.harts[core_idx].faulted = matches!(lifecycle, HartLifecycle::Faulted);

        if matches!(lifecycle, HartLifecycle::Faulted) {
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
        } else if matches!(lifecycle, HartLifecycle::Paused) {
            // step_all_cores_once is only called in AllHarts scope; keep running
            // as long as at least one hart is still active.
            if !self.any_running_harts() {
                self.run.is_running = false;
            }
        } else if !matches!(lifecycle, HartLifecycle::Running) && !self.any_running_harts() {
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.run.is_running = false;
        }

        // If this hart blocked on keyboard input, pause the entire run loop.
        // The keyboard handler resumes is_running when Enter is pressed.
        if self.console.reading {
            self.run.is_running = false;
        }
    }

    pub(crate) fn can_start_run(&self) -> bool {
        if self.max_cores <= 1 {
            let status = self.core_status(self.selected_core);
            return status == HartLifecycle::Paused
                || (!self.run.faulted && status == HartLifecycle::Running);
        }

        if matches!(self.run_scope, RunScope::FocusedHart) && !matches!(self.tab, Tab::Pipeline) {
            matches!(
                self.core_status(self.selected_core),
                HartLifecycle::Running | HartLifecycle::Paused
            )
        } else {
            self.any_running_harts()
                || self.core_status(self.selected_core) == HartLifecycle::Paused
        }
    }

    pub(crate) fn resume_selected_hart(&mut self) {
        if self.core_status(self.selected_core) != HartLifecycle::Paused {
            return;
        }
        self.native_mut().cpu_mut_unjournaled().ebreak_hit = false;
        self.run.faulted = false;
        self.native_mut().pipeline_mut().halted = false;
        self.native_mut().pipeline_mut().faulted = false;
        if let Some(runtime) = self.selected_runtime_mut() {
            if runtime.hart_id.is_some() {
                runtime.lifecycle = HartLifecycle::Running;
            }
        }
    }

    /// Execute one pipeline tick using shared cpu/mem state.
    /// Execute one pipeline cycle. Returns true if an instruction was committed.
    fn pipeline_step(&mut self) -> bool {
        // screen_sleep_ms parking: don't burn pipeline cycles refetching the
        // parked ecall; the tick loop retries once the deadline passes.
        if self
            .native()
            .cpu()
            .sleep_until
            .is_some_and(|t| Instant::now() < t)
        {
            return false;
        }
        // Sequential mode: if the CPU advanced outside the pipeline (e.g. the
        // user stepped in the Run tab), auto-reset so the visualization starts
        // fresh from the current PC.
        if self.native().pipeline().sequential_mode {
            let all_clear = self.native().pipeline().stages.iter().all(|s| s.is_none());
            if all_clear
                && self.native().pipeline().fetch_pc != self.native().cpu().pc
                && !self.native().pipeline().halted
                && !self.native().pipeline().faulted
            {
                let __rpc = self.native().cpu().pc;
                self.reset_pipeline_stages(__rpc);
            }
        }

        if self.native().pipeline().halted || self.native().pipeline().faulted {
            return false;
        }

        self.run.prev_x = self.native().cpu().x;
        self.run.prev_f = self.native().cpu().f;
        self.run.prev_pc = self.native().cpu().pc;

        // Clone CpiConfig to avoid borrow conflict (80 bytes, cheap)
        let cpi = self.run.cpi_config.clone();

        // One journaled clock cycle. `step_pipeline` re-syncs the MMU to the
        // selected hart (journal-preserving), snapshots cpu+mem+pipeline, runs
        // the tick on the machine-owned pipeline, and records the change-set —
        // so a single step-back rewinds exactly this cycle. The closure borrows
        // the console, which is a field disjoint from the runtime — so this
        // reaches the runtime by field rather than through `native_mut`, which
        // would borrow the whole `App`.
        let console = &mut self.console;
        let commit = rv32_runtime_mut(&mut *self.machine)
            .expect(NOT_RV32)
            .step_pipeline(
            |pipe, cpu, mem| raven_riscv_engine::falcon::pipeline::sim::pipeline_tick(pipe, cpu, mem, &cpi, console),
            |commit| commit.is_some(),
        );

        let committed = if let Some(info) = commit {
            *self.run.exec_counts.entry(info.pc).or_insert(0) += 1;
            let word = self.native().mem().peek32(info.pc).unwrap_or(0);
            let disasm = {
                match falcon::decoder::decode(word) {
                    Ok(instr) => format!("{instr:?}"),
                    Err(_) => format!("0x{word:08x}"),
                }
            };
            self.run.exec_trace.push_back((info.pc, disasm));
            if self.run.exec_trace.len() > 200 {
                self.run.exec_trace.pop_front();
            }

            for i in 0..32usize {
                if self.native().cpu().x[i] != self.run.prev_x[i] {
                    self.run.reg_age[i] = 0;
                    self.run.reg_last_write_pc[i] = Some(info.pc);
                } else {
                    self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
                }
            }
            for i in 0..32usize {
                if self.native().cpu().f[i] != self.run.prev_f[i] {
                    self.run.f_age[i] = 0;
                    self.run.f_last_write_pc[i] = Some(info.pc);
                } else {
                    self.run.f_age[i] = self.run.f_age[i].saturating_add(1).min(8);
                }
            }
            self.run.prev_x = self.native().cpu().x;
            self.run.prev_f = self.native().cpu().f;
            self.run.prev_pc = info.pc;

            self.native_mut().account_pipeline_commit();
            !is_transparent_single_step_word(word)
        } else {
            false
        };

        if self.native().pipeline().faulted {
            self.run.faulted = true;
        }
        if self.run.breakpoints.contains(&self.native().cpu().pc) {
            self.run.is_running = false;
        }
        self.finalize_selected_core_after_step();
        committed
    }

    fn step_all_cores_once(&mut self) -> bool {
        let original = self.selected_core;
        let mut selected_committed = false;

        // Pre-compute values needed by step_hart_bg_inner.  These are read
        // here — before any mutable borrow of self.harts — to satisfy the
        // borrow checker's disjoint-field rules.
        let exec_regions = self.run.exec_regions.clone();
        let mem_size = self.run.mem_size;
        let pipeline_enabled = self.native().pipeline().enabled || self.native().pipeline().sequential_mode;
        // CpiConfig is ~80 bytes; cheap to clone once per round.
        let cpi = self.run.cpi_config.clone();

        // In run mode is_running starts true; in single-step mode it starts false.
        // We only want to abort the round early when a hart *causes* a stop during
        // this round — not because is_running was already false before we began.
        let was_running = self.run.is_running;

        for core_idx in 0..self.max_cores {
            if core_idx == original {
                // ── Selected core: already live in self.run — no sync needed ─
                if self.core_status(core_idx) != HartLifecycle::Running {
                    continue;
                }
                if pipeline_enabled {
                    let committed = self.pipeline_step();
                    selected_committed = committed;
                } else {
                    self.single_step_selected_sequential();
                    selected_committed = true;
                }
            } else {
                // ── Non-selected core: step directly, zero HashMap/VecDeque clones ─
                if self.harts[core_idx].lifecycle != HartLifecycle::Running {
                    continue;
                }
                // Four disjoint places at once — the hart, the runtime's cache,
                // the console and the JIT backend — which is why the runtime is
                // borrowed from the field rather than through `native_mut`.
                let faulted = {
                    let hart = &mut self.harts[core_idx];
                    let mem = rv32_runtime_mut(&mut *self.machine)
                        .expect(NOT_RV32)
                        .mem_mut_unjournaled();
                    let console = &mut self.console;
                    let backend = self.run.backend.as_mut();
                    step_hart_bg_inner(
                        hart,
                        mem,
                        console,
                        &cpi,
                        &exec_regions,
                        mem_size,
                        pipeline_enabled,
                        backend,
                    )
                };
                let bp_hit = self.run.breakpoints.contains(&self.harts[core_idx].cpu.pc);
                let _ = faulted; // lifecycle determined inside finalize_bg_hart via hart.faulted
                self.finalize_bg_hart(core_idx, bp_hit);
            }
            // Only abort early if a hart *caused* a stop during this round.
            // When single-stepping, is_running is false from the start — that
            // must not be treated as a mid-round exit signal.
            if was_running && !self.run.is_running {
                break;
            }
        }

        // If multiple non-selected harts called sbrk in the same round, each
        // finalize_bg_hart propagated its own heap_break, overwriting the previous.
        // Propagate the maximum heap_break across all harts so none is lost.
        let max_break = self
            .harts
            .iter()
            .filter(|h| h.hart_id.is_some())
            .map(|h| h.cpu.heap_break)
            .chain(std::iter::once(self.native().cpu().heap_break))
            .max()
            .unwrap_or(self.native().cpu().heap_break);
        if max_break != self.native().cpu().heap_break {
            self.propagate_heap_break(max_break);
        }

        // Sync selected core's CPU snapshot to harts[original] (cheap — skips
        // exec_counts/exec_trace).  Keeps harts[selected].cpu current so that
        // UI code and tests that read it directly get a consistent view.
        let cpu = self.native().cpu().clone();
        if let Some(runtime) = self.harts.get_mut(original) {
            runtime.cpu = cpu;
            runtime.prev_pc = self.run.prev_pc;
            runtime.prev_x = self.run.prev_x;
            runtime.prev_f = self.run.prev_f;
            runtime.faulted = self.run.faulted;
        }

        selected_committed
    }

    fn step_selected_core_once(&mut self) -> bool {
        let status = self.core_status(self.selected_core);
        if !matches!(status, HartLifecycle::Running | HartLifecycle::Paused) {
            return false;
        }
        if status == HartLifecycle::Paused {
            self.resume_selected_hart();
        }
        if self.native().pipeline().enabled || self.native().pipeline().sequential_mode {
            self.pipeline_step()
        } else {
            self.single_step_selected_sequential();
            true
        }
    }

    fn pipeline_tab_step_once(&mut self) -> bool {
        // Sequential mode always drives the selected hart through the pipeline
        // visualizer; other harts stay paused.
        if self.native().pipeline().sequential_mode {
            return self.pipeline_step();
        }
        if self.max_cores > 1 {
            if matches!(self.run_scope, RunScope::AllHarts) {
                self.step_all_cores_once()
            } else {
                self.step_selected_core_once()
            }
        } else {
            self.pipeline_step()
        }
    }

    pub(super) fn single_step(&mut self) {
        // One instruction is the whole of "step" for a trait-driven backend;
        // below, a step may be a pipeline cycle or a round of every hart.
        if self.rv32().is_none() {
            self.machine_step();
            return;
        }
        if self.core_status(self.selected_core) == HartLifecycle::Paused {
            self.resume_selected_hart();
        }

        if matches!(self.tab, Tab::Pipeline)
            && (self.native().pipeline().enabled || self.native().pipeline().sequential_mode)
        {
            // Pipeline tab (pipelined or sequential): advance one cycle, then
            // skip only consecutive cache-only hold cycles. If a cycle advanced
            // stages or committed, stop immediately so EX/MEM/WB remain visible.
            let committed = self.pipeline_tab_step_once();
            if committed || !self.native().pipeline().last_cycle_cache_only {
                if !self.run.is_running {
                    self.ensure_pc_visible_in_imem();
                }
                return;
            }
            for _ in 0..1_000_000 {
                if self.native().pipeline().halted || self.native().pipeline().faulted {
                    break;
                }
                let committed = self.pipeline_tab_step_once();
                if committed || !self.native().pipeline().last_cycle_cache_only {
                    break;
                }
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            return;
        }

        if self.max_cores > 1 {
            let all_scope = matches!(self.run_scope, RunScope::AllHarts);

            if (self.native().pipeline().enabled || self.native().pipeline().sequential_mode)
                && !matches!(self.tab, Tab::Pipeline)
            {
                for _ in 0..200 {
                    let selected_running =
                        self.core_status(self.selected_core) == HartLifecycle::Running;
                    let can_progress = if all_scope {
                        self.any_running_harts()
                    } else if selected_running {
                        true
                    } else {
                        false
                    };
                    if !can_progress {
                        break;
                    }
                    let committed = if all_scope {
                        self.step_all_cores_once()
                    } else if selected_running {
                        self.step_selected_core_once()
                    } else {
                        false
                    };
                    let should_stop = if all_scope {
                        committed || !self.any_running_harts()
                    } else if selected_running {
                        committed || self.core_status(self.selected_core) != HartLifecycle::Running
                    } else {
                        true
                    };
                    if should_stop {
                        break;
                    }
                }
                if !self.run.is_running {
                    self.ensure_pc_visible_in_imem();
                }
            } else if all_scope {
                self.step_all_cores_once();
            } else {
                self.step_selected_core_once();
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            return;
        }

        if self.native().pipeline().enabled || self.native().pipeline().sequential_mode {
            // Run/Cache/other tabs: advance until one instruction commits
            // Safety limit to prevent infinite loop on stall/halt/fault
            for _ in 0..200 {
                let committed = self.pipeline_step();
                if committed || self.native().pipeline().halted || self.native().pipeline().faulted {
                    break;
                }
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            return;
        }

        self.single_step_selected_sequential();
    }

    fn single_step_selected_sequential(&mut self) {
        // Restore the selected hart's satp/priv_mode into the shared MMU — a
        // background hart may have just run with different page tables. Uses the
        // journal-preserving sync (it only touches MMU metadata) so the step
        // history survives across single-steps.
        self.native_mut().sync_mmu();
        let go_mode = matches!(self.run.speed, RunSpeed::Instant);
        for _ in 0..16 {
            if self.native().cpu().exit_code.is_some() || self.native().cpu().local_exit {
                self.run.is_running = false;
                return;
            }
            // screen_sleep_ms parking: leave the hart on its ecall until the
            // wall-clock deadline passes (the tick loop retries every ~10ms).
            if self
                .native()
                .cpu()
                .sleep_until
                .is_some_and(|t| Instant::now() < t)
            {
                return;
            }
            // In GO mode skip the 256-byte register snapshot — reg_age not updated mid-run.
            if !go_mode {
                self.run.prev_x = self.native().cpu().x;
                self.run.prev_f = self.native().cpu().f;
            }
            self.run.prev_pc = self.native().cpu().pc;
            let step_pc = self.native().cpu().pc;

            if !self.pc_in_executable_region(step_pc) {
                self.console.push_error(format!(
                    "Execution reached 0x{step_pc:08X}, outside any executable region. \
                     Add `li a7, 93; ecall` to terminate cleanly."
                ));
                self.run.faulted = true;
                return;
            }

            let word = self.native().mem().peek32(step_pc).unwrap_or(0);
            let cpi_cycles = classify_cpi_cycles(word, self.native().cpu(), &self.run.cpi_config);
            let mem_access = if go_mode {
                None
            } else {
                classify_mem_access(word, self.native().cpu())
            };

            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if go_mode {
                    // Run mode: usa o backend JIT completo (blocos compilados).
                    // A rajada GO escreve direto na RAM, então passa pelo escape
                    // hatch não-journalizado.
                    let (cpu, mem) = rv32_runtime_mut(&mut *self.machine)
                        .expect(NOT_RV32)
                        .cpu_mem_mut_unjournaled();
                    let mut ctx = crate::falcon::jit::ExecCtx::new(cpu, mem, &mut self.console);
                    self.run.backend.run_until_yield(&mut ctx)
                } else {
                    // Step mode: 1 instrução via interpretador, journalizada pelo
                    // `Machine` para que trace/highlights/exec_counts sejam
                    // por-instrução e o stepback possa revertê-la.
                    rv32_runtime_mut(&mut *self.machine)
                        .expect(NOT_RV32)
                        .step_interpreted(&mut self.console)
                }
            }));
            let (alive, jit_instr_count) = match res {
                Ok(Ok(crate::falcon::jit::ExecOutcome::Stepped { instructions })) => {
                    (true, instructions)
                }
                Ok(Ok(
                    crate::falcon::jit::ExecOutcome::Halted
                    | crate::falcon::jit::ExecOutcome::AwaitingInput,
                )) => (false, 1),
                Ok(Err(e)) => {
                    use crate::falcon::errors::FalconError;
                    let msg = if matches!(&e, FalconError::Bus(_)) {
                        let ram_kb = self.run.mem_size / 1024;
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
                    self.console.push_error(msg);
                    self.run.faulted = true;
                    (false, 1)
                }
                Err(_) => {
                    self.run.faulted = true;
                    (false, 1)
                }
            };
            if go_mode {
                self.native_mut()
                    .mem_mut_unjournaled()
                    .add_instruction_cycles(cpi_cycles);
                self.native_mut().mem_mut_unjournaled().snapshot_stats();
            } else {
                // Dobra o accounting de ciclos/stats dentro do passo journalizado,
                // sem zerar o journal — o stepback (Fase 4) o desfaz junto.
                self.native_mut().account_step_cycles(cpi_cycles);
            }

            // Track every instruction the JIT block executed (not just block entry).
            // For the interpreter jit_instr_count == 1, so this is equivalent.
            for i in 0..jit_instr_count {
                let instr_pc = step_pc.wrapping_add(i * 4);
                *self.run.exec_counts.entry(instr_pc).or_insert(0) += 1;
            }

            if !go_mode {
                let disasm = match falcon::decoder::decode(word) {
                    Ok(instr) => format!("{instr:?}"),
                    Err(_) => format!("0x{word:08x}"),
                };
                self.run.exec_trace.push_back((step_pc, disasm));
                if self.run.exec_trace.len() > 200 {
                    self.run.exec_trace.pop_front();
                }

                for entry in &mut self.run.mem_access_log {
                    entry.2 = entry.2.saturating_add(1);
                }
                self.run.mem_access_log.retain(|e| e.2 < 3);
                if let Some((addr, size, _)) = mem_access {
                    self.run.mem_access_log.push((addr, size, 0));
                }

                for i in 0..32usize {
                    if self.native().cpu().x[i] != self.run.prev_x[i] {
                        self.run.reg_age[i] = 0;
                        self.run.reg_last_write_pc[i] = Some(step_pc);
                    } else {
                        self.run.reg_age[i] = self.run.reg_age[i].saturating_add(1).min(8);
                    }
                }
                for i in 0..32usize {
                    if self.native().cpu().f[i] != self.run.prev_f[i] {
                        self.run.f_age[i] = 0;
                        self.run.f_last_write_pc[i] = Some(step_pc);
                    } else {
                        self.run.f_age[i] = self.run.f_age[i].saturating_add(1).min(8);
                    }
                }
            }

            if self.run.mem_region == crate::ui::app::MemRegion::Access {
                if let Some((addr, _, _)) = mem_access {
                    self.run.mem_view_addr = addr & !(self.run.mem_view_bytes - 1);
                }
            }
            self.run.dyn_mem_access = mem_access;
            if self.run.show_dyn {
                if let Some((addr, _, is_store)) = mem_access {
                    if is_store {
                        self.run.mem_view_addr = addr & !(self.run.mem_view_bytes - 1);
                    }
                }
            }

            if alive && self.run.breakpoints.contains(&self.native().cpu().pc) {
                self.run.is_running = false;
            }
            if !alive {
                if !self.console.reading {
                    self.run.faulted = self.native().cpu().exit_code.is_none()
                        && !self.native().cpu().ebreak_hit
                        && !self.native().cpu().local_exit;
                } else {
                    self.run.is_running = false;
                }
            }
            if !self.run.is_running {
                self.ensure_pc_visible_in_imem();
            }
            self.finalize_selected_core_after_step();

            if self.run.faulted
                || !alive
                || !matches!(self.core_status(self.selected_core), HartLifecycle::Running)
                || !is_transparent_single_step_word(word)
            {
                break;
            }
        }
    }

    pub(crate) fn align_mem_view_addr_to_last_access(&mut self) {
        let Some((addr, _, _)) = self.run.dyn_mem_access else {
            return;
        };
        let align = self.run.mem_view_bytes.max(1);
        self.run.mem_view_addr = addr & !(align - 1);
    }

    pub(crate) fn run_sidebar_shows_memory(&self) -> bool {
        !self.run.show_registers
            && (!self.run.show_dyn
                || self
                    .run
                    .dyn_mem_access
                    .is_some_and(|(_, _, is_store)| is_store))
    }

    pub(crate) fn run_sidebar_shows_registers(&self) -> bool {
        !self.run_sidebar_shows_memory()
    }

    pub(crate) fn sync_mem_focus_for_active_sidebar_mode(&mut self) {
        if self.run.mem_region == crate::ui::app::MemRegion::Access {
            self.align_mem_view_addr_to_last_access();
        }
        if self.run.show_dyn
            && self
                .run
                .dyn_mem_access
                .is_some_and(|(_, _, is_store)| is_store)
        {
            self.align_mem_view_addr_to_last_access();
        }
    }

    /// Jump editor cursor to the definition of the label under the cursor.
    pub(super) fn goto_label_definition(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() {
            return;
        }
        let line = &self.editor.buf.lines[row];
        let word = word_at(line, col);
        if word.is_empty() {
            return;
        }
        if let Some(&combined) = self.editor.label_to_line.get(&word) {
            let (fidx, local) = self.combined_to_local(combined);
            if fidx != self.editor.active_file {
                self.switch_file(fidx);
            }
            self.editor.buf.cursor_row = local.min(self.editor.buf.lines.len().saturating_sub(1));
            self.editor.buf.cursor_col = 0;
        }
    }

    /// Select next occurrence of the word currently under the cursor.
    pub(super) fn select_next_occurrence(&mut self) {
        let row = self.editor.buf.cursor_row;
        let col = self.editor.buf.cursor_col;
        if row >= self.editor.buf.lines.len() {
            return;
        }
        let word = word_at(&self.editor.buf.lines[row], col);
        if word.is_empty() {
            return;
        }
        let lines = &self.editor.buf.lines;
        // Find next occurrence after (row, col+word.len())
        let start_col = col + 1;
        let positions: Vec<(usize, usize)> = lines
            .iter()
            .enumerate()
            .flat_map(|(r, l)| {
                let mut found = Vec::new();
                let mut search = l.as_str();
                let mut byte_off = 0;
                while let Some(idx) = search.find(&word) {
                    let char_col = Editor::char_count(&l[..byte_off + idx]);
                    found.push((r, char_col));
                    byte_off += idx + word.len();
                    search = &l[byte_off..];
                }
                found
            })
            .collect();
        if positions.is_empty() {
            return;
        }
        // Find the next position after current cursor
        let next = positions
            .iter()
            .find(|&&(r, c)| r > row || (r == row && c >= start_col))
            .or_else(|| positions.first());
        if let Some(&(r, c)) = next {
            self.editor.buf.cursor_row = r;
            self.editor.buf.cursor_col = c;
            // Select the word via the selection_anchor API
            self.editor.buf.selection_anchor = Some((r, c));
            self.editor.buf.cursor_col = c + Editor::char_count(&word);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/support/ui_app_internal.rs"]
mod tests;
