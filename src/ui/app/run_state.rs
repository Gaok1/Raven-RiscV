use super::CpiConfig;
use crate::falcon::jit::ExecutionBackend;
use crate::falcon::machine::Machine;
use crate::falcon::{CacheController, Cpu, registers::ExecRegion};
use crate::ui::editor::Editor;
use std::time::{Duration, Instant};

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum EditorMode {
    Insert,
    Command,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum MemRegion {
    Data,
    Stack,
    Access, // auto-follows last memory read/write
    Heap,   // auto-follows cpu.heap_break (sbrk pointer)
    Custom,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum FormatMode {
    Hex,
    Dec,
    Str,
}

/// Execution speed setting.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum RunSpeed {
    /// ~12 steps/sec — slow, instruction-by-instruction
    X1,
    /// ~50 steps/sec — faster but still watchable
    X2,
    /// ~400 steps/sec — fast, visual blur
    X4,
    /// ~800 steps/sec — very fast
    X8,
    /// Time-budgeted bulk — effectively instant
    Instant,
}

impl RunSpeed {
    /// Cycle to the next speed level (wraps around).
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X4,
            Self::X4 => Self::X8,
            Self::X8 => Self::Instant,
            Self::Instant => Self::X1,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
            Self::Instant => "GO",
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum RunButton {
    Core,
    View,
    Format,
    Sign,
    Bytes,
    Region,
    State,
    Speed,
    ExecCount,
    InstrType,
    Stepback,
    Reset,
}

// ── State per tab ──────────────────────────────────────────────────────────────

pub(crate) struct EditorState {
    pub(crate) buf: Editor,
    pub(crate) dirty: bool,
    pub(crate) last_edit_at: Option<Instant>,
    pub(crate) auto_check_delay: Duration,
    pub(crate) last_assemble_msg: Option<String>,
    pub(crate) last_build_stats: Option<BuildStats>,
    pub(crate) last_compile_ok: Option<bool>,

    // Last successfully assembled program (for restart without re-parsing)
    pub(crate) last_ok_text: Option<Vec<u32>>,
    pub(crate) last_ok_data: Option<Vec<u8>>,
    pub(crate) last_ok_data_base: Option<u32>,
    pub(crate) last_ok_bss_size: Option<u32>,
    /// Raw ELF bytes stored for re-loading on reset (None when loaded from source/FALC/flat).
    pub(crate) last_ok_elf_bytes: Option<Vec<u8>>,
    pub(crate) last_ok_comments: std::collections::HashMap<u32, String>,
    pub(crate) last_ok_block_comments: std::collections::HashMap<u32, String>,
    pub(crate) last_ok_labels: std::collections::HashMap<u32, Vec<String>>,
    pub(crate) last_ok_halt_pcs: std::collections::HashSet<u32>,

    // Compile diagnostics
    pub(crate) diag_line: Option<usize>,
    pub(crate) diag_msg: Option<String>,
    pub(crate) diag_line_text: Option<String>,

    // Source-level metadata from last successful assembly
    pub(crate) label_to_line: std::collections::HashMap<String, usize>,
    pub(crate) line_to_addr: std::collections::HashMap<usize, u32>,
    pub(crate) show_addr_hints: bool,

    /// Popup shown when user tries to edit while an ELF binary is loaded.
    pub(crate) elf_prompt_open: bool,
    // Find bar
    pub(crate) find_open: bool,
    pub(crate) find_query: String,
    pub(crate) replace_open: bool,
    pub(crate) replace_query: String,
    pub(crate) find_in_replace: bool,
    pub(crate) find_matches: Vec<(usize, usize)>,
    pub(crate) find_current: usize,
    // Goto bar
    pub(crate) goto_open: bool,
    pub(crate) goto_query: String,
    // Encoding overlay (Ctrl+E): show binary encoding of current line
    pub(crate) show_encoding: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct BuildStats {
    pub(crate) instruction_count: usize,
    pub(crate) data_bytes: usize,
}

impl RunState {
    /// Whether the MMU is engaged at all (any mode other than `Off`).
    pub(crate) fn vm_enabled(&self) -> bool {
        self.vm_mode != crate::falcon::mmu::VmMode::Off
    }

    /// Shared read access to the CPU. The `~117` `run.cpu` read sites borrow
    /// through here; mutation must go through a `Machine` method.
    pub(crate) fn cpu(&self) -> &Cpu {
        self.machine.cpu()
    }

    /// Shared read access to the memory hierarchy. See [`RunState::cpu`].
    pub(crate) fn mem(&self) -> &CacheController {
        self.machine.mem()
    }

    /// Shared read access to the pipeline simulator. The pipeline lives inside
    /// `Machine` so a clock cycle is journaled together with the CPU and memory
    /// (see [`crate::falcon::machine::Machine::step_pipeline`]); reads borrow
    /// through here.
    pub(crate) fn pipeline(&self) -> &crate::ui::pipeline::PipelineSimState {
        self.machine.pipeline()
    }

    /// Mutable pipeline access for UI/config changes (hover, scroll, subtab,
    /// forwarding/branch config, reset). Does **not** journal and does **not**
    /// clear history. Never use it to advance execution — that is
    /// [`crate::falcon::machine::Machine::step_pipeline`].
    pub(crate) fn pipeline_mut(&mut self) -> &mut crate::ui::pipeline::PipelineSimState {
        self.machine.pipeline_mut()
    }
}

pub(crate) struct RunState {
    /// The simulator's CPU + memory hierarchy, owned behind the journaling
    /// gateway. Reads go through [`RunState::cpu`] / [`RunState::mem`]; mutation
    /// is only expressible via `Machine`'s methods (see its module docs).
    pub(crate) machine: Machine<crate::ui::pipeline::PipelineSimState>,
    pub(crate) prev_x: [u32; 32],
    pub(crate) prev_pc: u32,
    pub(crate) breakpoints: std::collections::HashSet<u32>,
    pub(crate) mem_size: usize,
    pub(crate) base_pc: u32,
    pub(crate) data_base: u32,
    pub(crate) heap_start: u32,
    pub(crate) exec_regions: Vec<ExecRegion>,

    // Memory view
    pub(crate) mem_view_addr: u32,
    pub(crate) mem_view_bytes: u32,
    pub(crate) mem_region: MemRegion,
    pub(crate) mem_search_open: bool,
    pub(crate) mem_search_query: String,

    // Display options
    pub(crate) show_registers: bool,
    pub(crate) fmt_mode: FormatMode,
    pub(crate) show_signed: bool,

    // Sidebar panel (resizable + collapsible)
    pub(crate) sidebar_width: u16,
    pub(crate) hover_sidebar_bar: bool,
    pub(crate) sidebar_drag: bool,
    pub(crate) sidebar_drag_start_x: u16,
    pub(crate) sidebar_width_start: u16,
    pub(crate) sidebar_collapsed: bool,

    // Instruction memory panel (resizable + collapsible)
    pub(crate) imem_width: u16,
    pub(crate) hover_imem_bar: bool,
    pub(crate) imem_drag: bool,
    pub(crate) imem_drag_start_x: u16,
    pub(crate) imem_width_start: u16,
    // imem_scroll is now in VISUAL ROWS (not instruction count)
    pub(crate) imem_scroll: usize,
    pub(crate) hover_imem_addr: Option<u32>,
    // Set each frame by render so scroll handlers use the correct height
    pub(crate) imem_inner_height: std::cell::Cell<usize>,
    pub(crate) imem_collapsed: bool,
    pub(crate) imem_search_open: bool,
    pub(crate) imem_search_query: String,
    /// addr → visual row: pre-computed at load, replaces O(N) scan with O(1) lookup.
    pub(crate) imem_vrow_cache: std::collections::HashMap<u32, usize>,
    /// Pre-lowercased label names: avoids per-search String allocation.
    pub(crate) labels_lower: std::collections::HashMap<u32, Vec<String>>,
    /// Sorted list of matching addresses from the last apply_imem_search call.
    pub(crate) imem_search_matches: Vec<u32>,
    /// Index into imem_search_matches for the currently highlighted match.
    pub(crate) imem_search_cursor: usize,
    /// Match count from the last apply_imem_search call; read by the renderer.
    pub(crate) imem_search_match_count: usize,

    // Details panel (collapsible)
    pub(crate) details_collapsed: bool,

    // Console panel (resizable)
    pub(crate) console_height: u16,
    pub(crate) hover_console_bar: bool,
    pub(crate) hover_console_clear: bool,
    pub(crate) console_drag: bool,
    pub(crate) console_drag_start_y: u16,
    pub(crate) console_height_start: u16,

    // Execution
    pub(crate) regs_scroll: usize,
    pub(crate) is_running: bool,
    pub(crate) last_step_time: Instant,
    pub(crate) step_interval: Duration,
    pub(crate) faulted: bool,
    pub(crate) speed: RunSpeed,
    /// One-shot guard: a full step-back checkpoint has been taken for the
    /// current GO/Instant burst. Reset when the run stops. See `App::tick`.
    pub(crate) go_checkpointed: bool,

    // Visible comments from source (#! text), keyed by instruction address
    pub(crate) comments: std::collections::HashMap<u32, String>,

    // Source label metadata
    pub(crate) labels: std::collections::HashMap<u32, Vec<String>>,
    pub(crate) halt_pcs: std::collections::HashSet<u32>,

    // ELF sections for the sections viewer (empty when loaded from ASM)
    pub(crate) elf_sections: Vec<crate::falcon::program::ElfSection>,

    // Execution statistics
    pub(crate) exec_counts: std::collections::HashMap<u32, u64>,
    pub(crate) exec_trace: std::collections::VecDeque<(u32, String)>,

    // Register highlight age: 0 = just changed, 255 = unchanged for long
    pub(crate) reg_age: [u8; 32],

    // UI flags
    pub(crate) show_trace: bool,
    pub(crate) pinned_regs: Vec<u8>,
    pub(crate) reg_cursor: usize, // 0 = PC, 1-32 = x0-x31

    // Feature: block comments from source (Feature 4)
    pub(crate) block_comments: std::collections::HashMap<u32, String>,

    // Feature: register write trace (Feature 8)
    pub(crate) reg_last_write_pc: [Option<u32>; 32],

    // Feature: dynamic sidebar view (Dyn)
    pub(crate) show_dyn: bool,
    pub(crate) dyn_mem_access: Option<(u32, u32, bool)>, // last step's mem access (addr, size, is_store); None = non-mem instr

    // Mouse hover row in register sidebar (visual row index, 0-based within inner area)
    pub(crate) hover_reg_row: Option<usize>,

    // CPI configuration
    pub(crate) cpi_config: CpiConfig,

    // Instruction list display toggles
    pub(crate) show_exec_count: bool,
    pub(crate) show_instr_type: bool,

    // RV32F: float register sidebar
    pub(crate) show_float_regs: bool, // toggle between int / float register view
    pub(crate) prev_f: [u32; 32],     // previous float register values (for highlighting)
    pub(crate) f_age: [u8; 32],       // highlight age for float registers (0=just changed)
    pub(crate) f_last_write_pc: [Option<u32>; 32], // last instruction that wrote each f-reg

    // Memory access highlight: (base_addr, size_bytes, age); age 0=just accessed, disappears at 3
    pub(crate) mem_access_log: Vec<(u32, u32, u8)>,
    /// When false, cache simulation is fully bypassed (direct RAM access, no latency).
    pub(crate) cache_enabled: bool,
    /// How virtual memory behaves (Off / Sv32 / Custom / Manual). Drives the MMU
    /// `enabled`/`force_translate` flags and, in Custom mode, the active paging
    /// scheme. See [`crate::falcon::mmu::VmMode`].
    pub(crate) vm_mode: crate::falcon::mmu::VmMode,
    /// When false, the TLB cache is bypassed in the engine: every translation
    /// walks the page table (miss + penalty, no hits). Mirrors
    /// `Mmu::tlb_enabled`. Independent of `vm_enabled`.
    pub(crate) tlb_enabled: bool,
    /// When true, non-I/O syscalls are mirrored to the debug console.
    pub(crate) trace_syscalls: bool,
    /// Which JIT mode is currently active.
    pub(crate) jit_kind: crate::falcon::jit::BackendKind,
    /// Execution backend selected for the TUI session.
    pub(crate) backend: Box<dyn ExecutionBackend<CacheController>>,
}
