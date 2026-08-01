//! Building and loading programs — the one path every architecture takes.
//!
//! The TUI used to assemble RV32 by calling `falcon::asm::assemble` directly and
//! everything else through the engine's `Assembler`, then load RV32 by writing
//! segments into RAM by hand and everything else through `Machine::load`. Two
//! of everything, and the RV32 half hard-wired the app to one ISA.
//!
//! Here there is one path. [`App::assemble_workspace`] asks the *active*
//! architecture's assembler for a [`ProgramImage`]; [`App::install_program`]
//! puts that image in memory; [`App::adopt_program`] refreshes the views that
//! are derived from it. Only [`App::install_program`] still forks, and it says
//! exactly why.
//!
//! ## Why loading still forks
//!
//! Most backends are driven entirely through the engine's [`Machine`] trait:
//! load, step, reset, snapshot. RV32 is loaded by hand, because its memory
//! hierarchy is built from settings that live in the TUI — the cache geometry
//! the user is editing, the TLB, the VM mode — and `Machine::load` knows
//! nothing about any of them. So the app rebuilds that hierarchy
//! ([`App::reset_native_runtime`]) and fills it with the engine's own
//! installer ([`App::fill_native_memory`]).
//!
//! Both paths reach the *same* machine: [`App::rv32`] hands back the runtime
//! inside the loaded backend. What the fork is *not* is a fork on the
//! architecture's name: everything above the machine — assembling, statistics,
//! diagnostics, the source map, the FALC container — is backend-neutral and
//! shared.

use super::{App, BuildStats, MemRegion, NOT_RV32, rv32_runtime_mut};
use crate::falcon::{CacheController, Cpu};
use raven_riscv_engine::{ProgramImage, SourceMap};
use std::collections::{HashMap, HashSet};

/// Whether an assemble error should pull the editor to the offending file.
///
/// An explicit compile does ([`Follow`](Diagnostics::Follow)) — the underline
/// should land where the fix goes. The background syntax check does not
/// ([`Report`](Diagnostics::Report)): it must never yank the cursor out from
/// under someone who is still typing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Diagnostics {
    Follow,
    Report,
}

impl App {
    // ── Build ──────────────────────────────────────────────────────────────

    /// Assemble the whole workspace with the active backend's assembler.
    ///
    /// Returns the image and the per-file line offsets of the combined source,
    /// or `None` after recording the diagnostic.
    pub(super) fn assemble_workspace(
        &mut self,
        diagnostics: Diagnostics,
    ) -> Option<(ProgramImage, Vec<usize>)> {
        let (source, offsets) = self.combined_source();
        match self
            .architecture
            .assembler()
            .assemble(&source, u64::from(self.run.base_pc))
        {
            Ok(image) => Some((image, offsets)),
            Err(error) => {
                let line = error.line.unwrap_or(0);
                if diagnostics == Diagnostics::Follow {
                    self.editor.file_line_offsets = offsets.clone();
                    let (file, _) = self.combined_to_local(line);
                    if file != self.editor.active_file {
                        self.switch_file(file);
                    }
                }
                self.set_diag(line, &error.message, &offsets);
                None
            }
        }
    }

    /// Record the build statistics and the status line for a successful build.
    /// Every figure comes from the image, so the wording is the same whichever
    /// backend produced it.
    pub(super) fn record_build(&mut self, image: &ProgramImage, verb: &str) {
        let alignment = self.architecture.descriptor().instruction_alignment;
        let instruction_count = image.instruction_count(alignment);
        let data_bytes = image.data_bytes();
        self.editor.last_build_stats = Some(BuildStats {
            instruction_count,
            data_bytes,
        });
        self.editor.last_assemble_msg = Some(format!(
            "{verb} {instruction_count} instructions, {data_bytes} data bytes, {} bss bytes.",
            image.zero_filled_bytes()
        ));
        self.editor.last_compile_ok = Some(true);
        self.editor.diag_line = None;
        self.editor.diag_msg = None;
        self.editor.diag_line_text = None;
    }

    /// Point the editor's label and line-address maps at `image`, so "go to
    /// label" and the per-line address gutter follow the newest build.
    pub(super) fn store_image_source_meta(&mut self, image: &ProgramImage, offsets: Vec<usize>) {
        self.store_source_meta(
            image.source_map.label_to_line.clone(),
            narrow_line_addresses(image.source_map.line_addresses.clone()),
            offsets,
        );
    }

    /// Remember `image` as the last program that built, together with the
    /// flattened views the editor and Run panes read.
    pub(super) fn cache_last_ok(&mut self, image: &ProgramImage) {
        self.editor.last_ok_image = Some(image.clone());
        self.editor.last_ok_elf_bytes = None;
        self.editor.last_ok_text = Some(instruction_words(image));
        self.editor.last_ok_data = Some(
            image
                .data_segment()
                .map(|segment| segment.bytes.clone())
                .unwrap_or_default(),
        );
        self.editor.last_ok_data_base = Some(data_base(image, self.run.base_pc));
        self.editor.last_ok_bss_size = Some(image.zero_filled_bytes() as u32);
        self.editor.last_ok_comments = narrow(image.source_map.comments.clone());
        self.editor.last_ok_block_comments = narrow(image.source_map.block_comments.clone());
        self.editor.last_ok_labels = narrow(image.source_map.labels.clone());
        self.editor.last_ok_halt_pcs = halt_pcs(&image.source_map);
    }

    // ── Load ───────────────────────────────────────────────────────────────

    /// Put `image` in the active machine's memory. Returns `false` after
    /// reporting a failure.
    ///
    /// This is the only place that still tells the two runtimes apart: a
    /// trait-driven backend owns its memory and is handed the image, while the
    /// RV32 runtime's memory hierarchy is rebuilt from the TUI's cache and TLB
    /// settings — which `Machine::load` knows nothing about — and then filled by
    /// the engine's shared installer. Both end up running the same bytes at the
    /// same addresses.
    pub(super) fn install_program(&mut self, image: &ProgramImage) -> bool {
        let loaded = if self.rv32().is_some() {
            self.reset_native_runtime();
            self.fill_native_memory(image)
        } else {
            self.machine.load(image).map_err(|error| error.to_string())
        };
        if let Err(error) = loaded {
            // The program assembled but cannot run, so the editor's badge has to
            // stop saying it is fine.
            self.editor.last_compile_ok = Some(false);
            self.console.push_error(error);
            self.run.faulted = self.rv32().is_some();
            return false;
        }
        true
    }

    /// Rebuild the RV32 runtime's CPU and memory hierarchy from the current
    /// cache, TLB and VM settings, ready to receive a program.
    pub(super) fn reset_native_runtime(&mut self) {
        self.run.prev_x = self.native().cpu().x;
        self.run.mem_size = self.ram_override.unwrap_or(super::DEFAULT_MEM_SIZE);
        let (mem_size, base_pc, bypass) = (
            self.run.mem_size,
            self.run.base_pc,
            !self.run.cache_enabled,
        );
        let memory = CacheController::new(
            self.cache.pending_icache.clone(),
            self.cache.pending_dcache.clone(),
            self.cache.extra_pending.clone(),
            mem_size,
        );
        let tlb = self.tlb.pending.clone();
        self.run.prev_pc = base_pc;
        let runtime = self.native_mut();
        *runtime.cpu_mut_unjournaled() = Cpu::default();
        runtime.cpu_mut_unjournaled().pc = base_pc;
        runtime.cpu_mut_unjournaled().write(2, mem_size as u32);
        *runtime.mem_mut_unjournaled() = memory;
        runtime.mem_mut_unjournaled().bypass = bypass;
        runtime.mem_mut_unjournaled().mmu_mut().tlb.reconfigure(tlb);
        self.push_vm_mode_to_mmu();
        self.run.faulted = false;
    }

    /// Write `image` into the RV32 runtime and place the entry point, the stack
    /// pointer and the heap break exactly where the engine's own RV32 machine
    /// would put them.
    fn fill_native_memory(&mut self, image: &ProgramImage) -> Result<(), String> {
        let entry =
            u32::try_from(image.entry).map_err(|_| "entry point exceeds RV32".to_string())?;
        crate::riscv32::install_image(&mut self.native_mut().mem_mut_unjournaled().ram, image)
            .map_err(|error| error.to_string())?;
        self.native_mut().mem_mut_unjournaled().invalidate_all();
        self.native_mut().mem_mut_unjournaled().reset_stats();

        self.native_mut().cpu_mut_unjournaled().pc = entry;
        self.run.prev_pc = entry;
        self.run.heap_start = crate::riscv32::heap_break_after(image);
        self.native_mut().cpu_mut_unjournaled().heap_break = self.run.heap_start;
        self.run.data_base = data_base(image, self.run.base_pc);
        self.run.mem_view_addr = self.run.data_base;
        self.run.mem_region = MemRegion::Data;
        Ok(())
    }

    /// Refresh everything derived from a freshly loaded `image`: the source
    /// map the editor and Run panes read, the instruction pane, the execution
    /// regions, the didactic page map, the pipeline and the harts.
    ///
    /// A no-op beyond the source map for trait-driven backends, which have
    /// none of those panes.
    pub(super) fn adopt_program(&mut self, image: &ProgramImage, offsets: Vec<usize>) {
        self.store_image_source_meta(image, offsets);
        if self.rv32().is_none() {
            return;
        }
        self.run.comments = narrow(image.source_map.comments.clone());
        self.run.block_comments = narrow(image.source_map.block_comments.clone());
        self.run.labels = narrow(image.source_map.labels.clone());
        self.run.halt_pcs = halt_pcs(&image.source_map);
        self.run.exec_counts.clear();
        self.run.exec_trace.clear();
        self.run.mem_access_log.clear();
        self.run.reg_age = [255u8; 32];
        self.run.f_age = [255u8; 32];
        self.run.reg_last_write_pc = [None; 32];
        self.run.f_last_write_pc = [None; 32];
        self.run.imem_scroll = 0;
        self.run.hover_imem_addr = None;
        self.rebuild_imem_vrow_cache();
        self.clear_details_selection();
        self.reset_exec_regions_to_loaded_text();
        self.sync_pipeline_program_range();
        self.install_didactic_page_map();
        let pc = self.native().cpu().pc;
        self.reset_pipeline_stages(pc);
        self.rebuild_harts();
    }

    /// Didactic VM modes (Sv32 / Custom) auto-install the configured page map
    /// so any program shows TLB activity without hand-writing page tables.
    /// Manual mode leaves `satp` to the program.
    ///
    /// Runs on every load, not just the first: rebuilding the memory hierarchy
    /// wipes the tables out of RAM along with the program.
    fn install_didactic_page_map(&mut self) {
        if !self.run.vm_mode.is_auto() {
            return;
        }
        let scheme = self.active_scheme();
        let root_pa = scheme.root_pa(self.run.mem_size as u32);
        let window = (self.run.base_pc.min(self.run.data_base), self.run.heap_start);
        crate::falcon::mmu::Mmu::install_map_scheme(
            &mut rv32_runtime_mut(&mut *self.machine)
                .expect(NOT_RV32)
                .mem_mut_unjournaled()
                .ram,
            root_pa,
            &scheme,
            self.tlb.page_map,
            window,
        );
        let satp = crate::falcon::mmu::Mmu::make_satp(root_pa, self.tlb.page_map.asid);
        self.native_mut().cpu_mut_unjournaled().satp = satp;
        let mmu = self.native_mut().mem_mut_unjournaled().mmu_mut();
        mmu.satp = crate::falcon::mmu::Satp::new(satp);
        mmu.force_translate = true;
    }

    // ── Opening a binary ───────────────────────────────────────────────────

    /// Decode an opened file into an image for the active backend.
    ///
    /// Backends that declare no ELF support read FALC containers only; the RV32
    /// loader additionally accepts a flat block of machine code. ELF never
    /// reaches here — it carries a symbol table and a section list a
    /// [`ProgramImage`] cannot express, so it keeps its own path.
    pub(super) fn decode_binary(&self, bytes: &[u8]) -> Result<ProgramImage, String> {
        if self.rv32().is_none() {
            let image = ProgramImage::from_falc(bytes).map_err(|error| error.to_string())?;
            let id = self.architecture_id();
            return image
                .expect_architecture(id)
                .map(|()| image)
                .map_err(|error| error.to_string());
        }
        crate::riscv32::image_from_binary(bytes, u64::from(self.run.base_pc))
            .map_err(|error| error.to_string())
    }
}

// ── Narrowing the architecture-neutral source map ─────────────────────────────
//
// [`SourceMap`] addresses programs in 64 bits; the RV32 runtime and its panes
// are keyed by `u32`. Addresses that do not fit are dropped rather than wrapped
// — they cannot name an RV32 location anyway.

fn narrow<V>(map: HashMap<u64, V>) -> HashMap<u32, V> {
    map.into_iter()
        .filter_map(|(address, value)| u32::try_from(address).ok().map(|a| (a, value)))
        .collect()
}

fn narrow_line_addresses(map: HashMap<usize, u64>) -> HashMap<usize, u32> {
    map.into_iter()
        .filter_map(|(line, address)| u32::try_from(address).ok().map(|a| (line, a)))
        .collect()
}

fn halt_pcs(source_map: &SourceMap) -> HashSet<u32> {
    source_map
        .explicit_halts
        .iter()
        .filter_map(|pc| u32::try_from(*pc).ok())
        .collect()
}

/// Where the memory pane opens: the image's data segment, or the entry point
/// when it has none.
fn data_base(image: &ProgramImage, fallback: u32) -> u32 {
    image
        .data_segment()
        .and_then(|segment| u32::try_from(segment.address).ok())
        .unwrap_or(fallback)
}

/// The executable segment as 32-bit instruction words, zero-padding a trailing
/// partial word.
fn instruction_words(image: &ProgramImage) -> Vec<u32> {
    image
        .executable_bytes()
        .unwrap_or_default()
        .chunks(4)
        .map(|chunk| {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect()
}

