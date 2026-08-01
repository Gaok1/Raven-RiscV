use super::*;

impl App {
    fn copy_pipeline_config_to_hart(
        src: &raven_riscv_engine::falcon::pipeline::PipelineSimState,
        dst: &mut raven_riscv_engine::falcon::pipeline::PipelineSimState,
    ) {
        dst.enabled = src.enabled;
        dst.sequential_mode = src.sequential_mode;
        dst.bypass = src.bypass;
        dst.branch_resolve = src.branch_resolve;
        dst.mode = src.mode;
        dst.set_predict(src.predict);
        dst.exec_regions = src.exec_regions.clone();
        dst.fu_capacity = src.fu_capacity;
    }

    /// Timing summed across every hart, or `None` when the active backend is
    /// not clocking a pipeline at all — which is how a backend with no pipeline
    /// capability answers, so callers fall back to cache timing without asking
    /// which architecture is running.
    pub(crate) fn aggregate_pipeline_snapshot(&self) -> Option<PipelineResultsSnapshot> {
        if !self.pipeline().is_some_and(|pipeline| pipeline.status().enabled) {
            return None;
        }

        let mut saw_pipeline = false;
        let mut cycles = 0u64;
        let mut committed = 0u64;
        let mut stalls = 0u64;
        let mut flushes = 0u64;
        let mut branches = 0u64;
        let mut stall_by_type = [0u64; crate::ui::pipeline::HazardType::STALL_TYPE_COUNT];

        let mut accumulate = |pipe: &raven_riscv_engine::falcon::pipeline::PipelineSimState| {
            saw_pipeline = true;
            cycles = cycles.max(pipe.cycle_count);
            committed = committed.saturating_add(pipe.instr_committed);
            stalls = stalls.saturating_add(pipe.stall_count);
            flushes = flushes.saturating_add(pipe.flush_count);
            branches = branches.saturating_add(pipe.branches_executed);
            for (dst, src) in stall_by_type.iter_mut().zip(pipe.stall_by_type.iter()) {
                *dst = dst.saturating_add(*src);
            }
        };

        if self.core_hart_id(self.selected_core).is_some()
            || !matches!(self.core_status(self.selected_core), HartLifecycle::Free)
        {
            if let Some(model) = self.rv32() {
                accumulate(model.pipeline());
            }
        }

        for (idx, hart) in self.harts.iter().enumerate() {
            if idx == self.selected_core || hart.hart_id.is_none() {
                continue;
            }
            if let Some(pipe) = hart.pipeline.as_ref() {
                accumulate(pipe);
            }
        }

        if !saw_pipeline {
            return None;
        }

        let [
            raw_stalls,
            load_use_stalls,
            branch_stalls,
            fu_stalls,
            mem_stalls,
        ] = stall_by_type;
        let cpi = if committed > 0 {
            cycles as f64 / committed as f64
        } else {
            0.0
        };

        Some(PipelineResultsSnapshot {
            scope: "aggregate".to_string(),
            committed,
            cycles,
            stalls,
            flushes,
            cpi,
            branches,
            raw_stalls,
            load_use_stalls,
            branch_stalls,
            fu_stalls,
            mem_stalls,
            bypass: self.native().pipeline().bypass.summary(),
            mode: format!("{:?}", self.native().pipeline().mode),
            branch_resolve: format!("{:?}", self.native().pipeline().branch_resolve),
            branch_predict: format!("{:?}", self.native().pipeline().predict),
        })
    }

    pub(crate) fn selected_pipeline_snapshot(&self) -> Option<PipelineResultsSnapshot> {
        if !self.pipeline().is_some_and(|pipeline| pipeline.status().enabled) {
            return None;
        }

        let pipe = &self.native().pipeline();
        let [
            raw_stalls,
            load_use_stalls,
            branch_stalls,
            fu_stalls,
            mem_stalls,
        ] = pipe.stall_by_type;
        let cpi = if pipe.instr_committed > 0 {
            pipe.cycle_count as f64 / pipe.instr_committed as f64
        } else {
            0.0
        };

        Some(PipelineResultsSnapshot {
            scope: format!(
                "selected-core:{}:hart:{}",
                self.selected_core,
                self.core_hart_id(self.selected_core)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            committed: pipe.instr_committed,
            cycles: pipe.cycle_count,
            stalls: pipe.stall_count,
            flushes: pipe.flush_count,
            cpi,
            branches: pipe.branches_executed,
            raw_stalls,
            load_use_stalls,
            branch_stalls,
            fu_stalls,
            mem_stalls,
            bypass: pipe.bypass.summary(),
            mode: format!("{:?}", pipe.mode),
            branch_resolve: format!("{:?}", pipe.branch_resolve),
            branch_predict: format!("{:?}", pipe.predict),
        })
    }

    pub(in crate::ui) fn tab_visible(&self, tab: Tab) -> bool {
        let capabilities = self.architecture.descriptor().capabilities;
        match tab {
            Tab::Cache => capabilities.cache && self.session.cache_enabled,
            Tab::Tlb => capabilities.virtual_memory,
            Tab::Pipeline => capabilities.pipeline,
            Tab::Activity => capabilities.guided_learning,
            _ => true,
        }
    }

    pub(in crate::ui) fn visible_tabs(&self) -> Vec<Tab> {
        Tab::all()
            .iter()
            .copied()
            .filter(|&tab| self.tab_visible(tab))
            .collect()
    }

    pub(in crate::ui) fn ensure_visible_tab(&mut self) {
        if !self.tab_visible(self.tab) {
            self.tab = Tab::Run;
        }
    }

    pub(in crate::ui) fn set_cache_enabled(&mut self, enabled: bool) {
        self.session.cache_enabled = enabled;
        if self.rv32().is_some() {
            self.native_mut().mem_mut_unjournaled().bypass = !enabled;
            self.native_mut().mem_mut_unjournaled().flush_all();
        }
        self.ensure_visible_tab();
    }

    /// Empty the pipeline and refetch from `pc`, dropping the visual state that
    /// described the old program. The physical stages live in the runtime and
    /// the presentation beside it, which is why both move together here.
    pub(in crate::ui) fn reset_pipeline_stages(&mut self, pc: u32) {
        if let Some(rv32) = self.rv32_mut() {
            rv32.pipeline_mut().reset_stages(pc);
        }
        self.run.pipeline_view.reset_for_program();
    }

    /// Point the fetch stage at `pc` without disturbing what has already been
    /// clocked in, and clear the status line the last redirect left behind.
    pub(in crate::ui) fn redirect_pipeline_pc(&mut self, pc: u32) {
        if let Some(rv32) = self.rv32_mut() {
            rv32.pipeline_mut().redirect_pc(pc);
        }
        self.run.pipeline_view.status_msg = None;
        self.run.pipeline_view.status_error = None;
    }

    pub(in crate::ui) fn set_pipeline_enabled(&mut self, enabled: bool) {
        if let Some(pipeline) = self.pipeline_config_mut() {
            pipeline.enabled = enabled;
            pipeline.sequential_mode = !enabled;
            self.reconfigure_pipeline_model();
        }
        self.ensure_visible_tab();
    }

    /// The user-facing VM mode.
    pub(in crate::ui) fn vm_mode(&self) -> crate::falcon::mmu::VmMode {
        self.session.vm_mode
    }

    /// The paging scheme implied by the current VM mode: the user-configured
    /// scheme in `Custom` (when valid), the standard Sv32 preset otherwise. A
    /// mid-edit invalid Custom scheme falls back to Sv32 so the live MMU is
    /// never driven with a malformed geometry (apply re-validates explicitly).
    pub(in crate::ui) fn active_scheme(&self) -> crate::falcon::mmu::PagingScheme {
        if matches!(self.session.vm_mode, crate::falcon::mmu::VmMode::Custom)
            && self.tlb.pending_scheme.is_valid()
        {
            self.tlb.pending_scheme.clone()
        } else {
            crate::falcon::mmu::PagingScheme::sv32()
        }
    }

    /// Push the current `vm_mode` (flags + active scheme) into the live MMU.
    /// Used after rebuilding the memory subsystem (assemble / load).
    pub(in crate::ui) fn push_vm_mode_to_mmu(&mut self) {
        let (enabled, force_translate) = self.session.vm_mode.flags();
        let scheme = self.active_scheme();
        let tlb_enabled = self.session.tlb_enabled;
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        let mmu = runtime.mem_mut_unjournaled().mmu_mut();
        mmu.set_scheme(scheme);
        mmu.enabled = enabled;
        mmu.force_translate = force_translate;
        mmu.tlb_enabled = tlb_enabled;
    }

    /// Backward-compatible on/off entry point. `true` selects Sv32 (the
    /// classic "VM on" flavor); `false` selects Off.
    pub(in crate::ui) fn set_vm_enabled(&mut self, enabled: bool) {
        use crate::falcon::mmu::VmMode;
        self.set_vm_mode(if enabled { VmMode::Sv32 } else { VmMode::Off });
    }

    /// Select the VM mode (Off / Sv32 / Custom / Manual) and push the derived
    /// engine flags + active paging scheme into the MMU.
    pub(in crate::ui) fn set_vm_mode(&mut self, mode: crate::falcon::mmu::VmMode) {
        let (enabled, _) = mode.flags();
        self.session.vm_mode = mode;
        self.push_vm_mode_to_mmu();
        if !enabled {
            // Drop all cached translations so re-enabling starts from a clean
            // slate (no stale PA mappings).
            if let Some(runtime) = self.rv32_mut() {
                runtime.mem_mut_unjournaled().mmu.flush();
            }
        } else if self.session.jit_kind != crate::falcon::jit::BackendKind::None {
            // The JIT does not yet invalidate translations on satp/sfence.vma,
            // so keeping it on with VM would silently run stale code. Demote to
            // the interpreter and rebuild the backend.
            self.session.jit_kind = crate::falcon::jit::BackendKind::None;
            self.rebuild_backend();
        }
    }

    /// Enable/disable the TLB cache. When off, every translation walks the
    /// page table (miss + penalty, no hits). Mirrors the flag into the engine.
    pub(in crate::ui) fn set_tlb_enabled(&mut self, enabled: bool) {
        self.session.tlb_enabled = enabled;
        let Some(runtime) = self.rv32_mut() else {
            return;
        };
        runtime.mem_mut_unjournaled().mmu.tlb_enabled = enabled;
        if !enabled {
            // Drop cached translations so re-enabling starts cold.
            runtime.mem_mut_unjournaled().mmu.flush();
        }
    }

    pub(in crate::ui) fn set_trace_syscalls(&mut self, enabled: bool) {
        self.session.trace_syscalls = enabled;
        self.console.trace_syscalls = enabled;
    }

    pub(in crate::ui) fn set_jit_mode(&mut self, kind: crate::falcon::jit::BackendKind) {
        // Refuse to enable JIT while VM is on — the JIT does not invalidate
        // its translation cache on satp/sfence.vma. The user can disable VM
        // first to flip the JIT on.
        if self.session.vm_enabled() && kind != crate::falcon::jit::BackendKind::None {
            self.session.jit_kind = crate::falcon::jit::BackendKind::None;
        } else {
            self.session.jit_kind = kind;
        }
        self.rebuild_backend();
    }

    /// Reconstrói o backend de execução com base em `run.jit_kind` e o
    /// estado atual de cpu/mem. Chamado ao trocar o modo JIT ou ao carregar
    /// um programa (para que FullBackend faça o scan eager no estado correto).
    pub(in crate::ui) fn rebuild_backend(&mut self) {
        use crate::falcon::jit::{BackendKind, make_backend};
        self.session.backend = match self.session.jit_kind {
            BackendKind::None | BackendKind::Hot => make_backend(self.session.jit_kind)
                .unwrap_or_else(|_| make_backend(BackendKind::None).unwrap()),
            BackendKind::Full => {
                #[cfg(feature = "jit")]
                {
                    match self.rv32() {
                        Some(runtime) => crate::falcon::jit::make_full_backend(
                            runtime.cpu(),
                            runtime.mem(),
                        ),
                        None => make_backend(BackendKind::None).unwrap(),
                    }
                }
                #[cfg(not(feature = "jit"))]
                {
                    make_backend(BackendKind::None).unwrap()
                }
            }
        };
    }

    pub(crate) fn reconfigure_pipeline_model(&mut self) {
        self.session.is_running = false;
        let __rpc = self.program_counter() as u32;
        self.reset_pipeline_stages(__rpc);

        for (idx, hart) in self.harts.iter_mut().enumerate() {
            if idx == self.selected_core {
                continue;
            }
            // By field, not through `native`: the runtime and `harts` are
            // disjoint places, which a whole-`self` borrow would hide.
            if let Some(p) = hart.pipeline.as_mut()
                && let Some(model) = rv32_runtime(&*self.machine)
            {
                Self::copy_pipeline_config_to_hart(model.pipeline(), p);
                p.reset_stages(hart.cpu.pc);
            }
        }
    }

    pub(super) fn selected_runtime_mut(&mut self) -> Option<&mut HartCoreRuntime> {
        self.harts.get_mut(self.selected_core)
    }

    pub(super) fn selected_runtime(&self) -> Option<&HartCoreRuntime> {
        self.harts.get(self.selected_core)
    }

    pub(crate) fn peer_hart_ids_at(&self, addr: u32) -> Vec<u32> {
        self.harts
            .iter()
            .enumerate()
            .filter(|(idx, hart)| {
                *idx != self.selected_core
                    && hart.hart_id.is_some()
                    && matches!(
                        hart.lifecycle,
                        HartLifecycle::Running | HartLifecycle::Paused | HartLifecycle::Exited
                    )
                    && hart.cpu.pc == addr
            })
            .map(|(_, hart)| hart.hart_id.unwrap())
            .collect()
    }

    pub(super) fn rebuild_harts(&mut self) {
        // Harts mirror the runtime the host steps; a backend without one still
        // gets the cores, so the toolbar has something to name.
        let selected = self
            .rv32()
            .map_or_else(crate::falcon::Cpu::default, |rv32| rv32.cpu().clone());
        self.selected_core = 0;
        self.next_hart_id = 1;
        self.harts.clear();
        for core in 0..self.max_cores {
            let mut runtime = HartCoreRuntime::free(self.session.base_pc, self.session.mem_size);
            runtime.cpu.heap_break = selected.heap_break;
            if core == 0 {
                runtime.hart_id = Some(0);
                runtime.lifecycle = HartLifecycle::Running;
                runtime.cpu = selected.clone();
                runtime.cpu.hart_id = 0;
                runtime.prev_x = self.session.prev_x;
                runtime.prev_f = self.session.prev_f;
                runtime.prev_pc = self.session.prev_pc;
                runtime.faulted = self.session.faulted;
                runtime.reg_age = self.session.reg_age;
                runtime.f_age = self.session.f_age;
                runtime.reg_last_write_pc = self.session.reg_last_write_pc;
                runtime.f_last_write_pc = self.session.f_last_write_pc;
                runtime.exec_counts = self.session.exec_counts.clone();
                runtime.exec_trace = self.session.exec_trace.clone();
                runtime.dyn_mem_access = self.session.dyn_mem_access;
                runtime.mem_access_log = self.session.mem_access_log.clone();
                runtime.pipeline = None;
            } else if let Some(p) = runtime.pipeline.as_mut()
                && let Some(model) = self.rv32()
            {
                Self::copy_pipeline_config_to_hart(model.pipeline(), p);
                p.reset_stages(runtime.cpu.pc);
            }
            self.harts.push(runtime);
        }
    }

    pub(crate) fn rebuild_harts_for_debug(&mut self) {
        self.rebuild_harts();
    }

    pub(super) fn sync_selected_core_to_runtime(&mut self) {
        let selected = self.selected_core;
        let replacement = raven_riscv_engine::falcon::pipeline::PipelineSimState::new();
        if let Some(runtime) = self.harts.get_mut(selected) {
            runtime.cpu = rv32_runtime(&*self.machine).expect(NOT_RV32).cpu().clone();
            runtime.prev_x = self.session.prev_x;
            runtime.prev_f = self.session.prev_f;
            runtime.prev_pc = self.session.prev_pc;
            runtime.faulted = self.session.faulted;
            runtime.reg_age = self.session.reg_age;
            runtime.f_age = self.session.f_age;
            runtime.reg_last_write_pc = self.session.reg_last_write_pc;
            runtime.f_last_write_pc = self.session.f_last_write_pc;
            runtime.exec_counts = self.session.exec_counts.clone();
            runtime.exec_trace = self.session.exec_trace.clone();
            runtime.dyn_mem_access = self.session.dyn_mem_access;
            runtime.mem_access_log = self.session.mem_access_log.clone();
            runtime.pipeline = Some(std::mem::replace(
                rv32_runtime_mut(&mut *self.machine)
                    .expect(NOT_RV32)
                    .pipeline_mut(),
                replacement,
            ));
        }
    }

    pub(crate) fn sync_runtime_for_debug(&mut self) {
        self.sync_runtime_to_selected_core();
    }

    pub(super) fn sync_runtime_to_selected_core(&mut self) {
        let selected = self.selected_core;
        if let Some(runtime) = self.harts.get_mut(selected) {
            *rv32_runtime_mut(&mut *self.machine)
                .expect(NOT_RV32)
                .cpu_mut_unjournaled() = runtime.cpu.clone();
            self.session.prev_x = runtime.prev_x;
            self.session.prev_f = runtime.prev_f;
            self.session.prev_pc = runtime.prev_pc;
            self.session.faulted = runtime.faulted;
            self.session.reg_age = runtime.reg_age;
            self.session.f_age = runtime.f_age;
            self.session.reg_last_write_pc = runtime.reg_last_write_pc;
            self.session.f_last_write_pc = runtime.f_last_write_pc;
            self.session.exec_counts = runtime.exec_counts.clone();
            self.session.exec_trace = runtime.exec_trace.clone();
            self.session.dyn_mem_access = runtime.dyn_mem_access;
            self.session.mem_access_log = runtime.mem_access_log.clone();
            let mut pipeline = runtime
                .pipeline
                .take()
                .unwrap_or_else(raven_riscv_engine::falcon::pipeline::PipelineSimState::new);
            if let Some(model) = self.rv32() {
                Self::copy_pipeline_config_to_hart(model.pipeline(), &mut pipeline);
            }
            if pipeline.fetch_pc == 0 && pipeline.cycle_count == 0 {
                pipeline.reset_stages(self.program_counter() as u32);
            }
            *self.native_mut().pipeline_mut() = pipeline;
        }
    }

    /// Park the live runtime in the current core's slot and load the new one.
    /// Only a backend the host steps by hand has harts to swap between, so on
    /// any other the selector stays put rather than pretending to move.
    pub(crate) fn switch_selected_core(&mut self, new_core: usize) {
        if new_core >= self.max_cores || new_core == self.selected_core || self.rv32().is_none() {
            return;
        }
        self.sync_selected_core_to_runtime();
        self.selected_core = new_core;
        self.sync_runtime_to_selected_core();
    }

    pub(crate) fn cycle_selected_core(&mut self, delta: isize) {
        if self.max_cores <= 1 {
            return;
        }
        let count = self.max_cores as isize;
        let next = (self.selected_core as isize + delta).rem_euclid(count) as usize;
        self.switch_selected_core(next);
        self.ensure_pc_visible_in_imem();
    }

    pub(crate) fn core_status(&self, core: usize) -> HartLifecycle {
        self.harts
            .get(core)
            .map(|h| h.lifecycle)
            .unwrap_or(HartLifecycle::Free)
    }

    pub(crate) fn core_hart_id(&self, core: usize) -> Option<u32> {
        self.harts.get(core).and_then(|h| h.hart_id)
    }

    pub(super) fn stack_slot_size(&self) -> u32 {
        let denom = (self.max_cores as u32).saturating_add(1).max(2);
        let mem = self.session.mem_size as u32;
        (mem / denom).clamp(4096, 64 * 1024)
    }

    pub(super) fn stack_slot_bounds(&self, core: usize) -> (u32, u32) {
        let size = self.stack_slot_size();
        let top = (self.session.mem_size as u32).saturating_sub(size.saturating_mul(core as u32));
        let bottom = top.saturating_sub(size);
        (bottom, top)
    }

    pub(super) fn is_pc_in_program(&self, pc: u32) -> bool {
        self.pc_in_executable_region(pc)
    }

    pub(super) fn process_pending_hart_start_for_selected(&mut self) {
        let Some(request) = self
            .native_mut()
            .cpu_mut_unjournaled()
            .pending_hart_start
            .take()
        else {
            return;
        };

        let free_core = (0..self.max_cores).find(|&idx| {
            idx != self.selected_core
                && matches!(
                    self.core_status(idx),
                    HartLifecycle::Free | HartLifecycle::Exited | HartLifecycle::Faulted
                )
        });
        let Some(free_core) = free_core else {
            self.native_mut()
                .cpu_mut_unjournaled()
                .write(10, (-1i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: no free core available (max_cores={})",
                    self.selected_core,
                    self.core_hart_id(self.selected_core).unwrap_or(0),
                    self.max_cores
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        };
        if !self.is_pc_in_program(request.entry_pc) {
            self.native_mut()
                .cpu_mut_unjournaled()
                .write(10, (-2i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: entry PC 0x{:08X} is outside any executable region",
                    self.selected_core,
                    self.core_hart_id(self.selected_core).unwrap_or(0),
                    request.entry_pc
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        if request.stack_ptr == 0
            || request.stack_ptr > self.session.mem_size as u32
            || request.stack_ptr & 0xF != 0
        {
            self.native_mut()
                .cpu_mut_unjournaled()
                .write(10, (-3i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: stack 0x{:08X} invalid (must be non-zero, 16-byte aligned, within memory [0..0x{:08X}])",
                    self.selected_core,
                    self.core_hart_id(self.selected_core).unwrap_or(0),
                    request.stack_ptr,
                    self.session.mem_size,
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        let hart_id = self.next_hart_id;
        self.next_hart_id = self.next_hart_id.saturating_add(1);

        let mut child = HartCoreRuntime::free(self.session.base_pc, self.session.mem_size);
        child.hart_id = Some(hart_id);
        child.cpu.hart_id = hart_id;
        child.lifecycle = HartLifecycle::Running;
        child.cpu.pc = request.entry_pc;
        child.cpu.write(2, request.stack_ptr);
        child.cpu.write(10, request.arg);
        child.cpu.heap_break = self.rv32().map_or(0, |rv32| rv32.cpu().heap_break);
        child.prev_pc = child.cpu.pc;
        if let Some(p) = child.pipeline.as_mut() {
            Self::copy_pipeline_config_to_hart(&self.native().pipeline(), p);
            p.reset_stages(child.cpu.pc);
        }

        self.harts[free_core] = child;
        self.native_mut().cpu_mut_unjournaled().write(10, hart_id);
        self.console.push_colored(
            format!(
                "[C{}:H{}] hart start -> core {} pc=0x{:08X}",
                self.selected_core,
                self.core_hart_id(self.selected_core).unwrap_or(0),
                free_core,
                request.entry_pc
            ),
            crate::ui::console::ConsoleColor::Info,
        );
    }

    /// Handle a hart-spawn request issued by a non-selected (background) hart.
    /// Equivalent to `process_pending_hart_start_for_selected` but reads from
    /// and writes to `self.harts[core_idx].cpu` instead of `self.native().cpu()`.
    pub(super) fn process_pending_hart_start_for_bg(&mut self, core_idx: usize) {
        let Some(request) = self.harts[core_idx].cpu.pending_hart_start.take() else {
            return;
        };

        let free_core = (0..self.max_cores).find(|&idx| {
            idx != core_idx
                && matches!(
                    self.core_status(idx),
                    HartLifecycle::Free | HartLifecycle::Exited | HartLifecycle::Faulted
                )
        });
        let Some(free_core) = free_core else {
            self.harts[core_idx].cpu.write(10, (-1i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: no free core available (max_cores={})",
                    core_idx,
                    self.harts[core_idx].hart_id.unwrap_or(0),
                    self.max_cores
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        };
        if !self.is_pc_in_program(request.entry_pc) {
            self.harts[core_idx].cpu.write(10, (-2i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: entry PC 0x{:08X} is outside any executable region",
                    core_idx,
                    self.harts[core_idx].hart_id.unwrap_or(0),
                    request.entry_pc
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }
        if request.stack_ptr == 0
            || request.stack_ptr > self.session.mem_size as u32
            || request.stack_ptr & 0xF != 0
        {
            self.harts[core_idx].cpu.write(10, (-3i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: stack 0x{:08X} invalid \
                     (must be non-zero, 16-byte aligned, within memory [0..0x{:08X}])",
                    core_idx,
                    self.harts[core_idx].hart_id.unwrap_or(0),
                    request.stack_ptr,
                    self.session.mem_size,
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        let hart_id = self.next_hart_id;
        self.next_hart_id = self.next_hart_id.saturating_add(1);

        let mut child = HartCoreRuntime::free(self.session.base_pc, self.session.mem_size);
        child.hart_id = Some(hart_id);
        child.cpu.hart_id = hart_id;
        child.lifecycle = HartLifecycle::Running;
        child.cpu.pc = request.entry_pc;
        child.cpu.write(2, request.stack_ptr);
        child.cpu.write(10, request.arg);
        child.cpu.heap_break = self.harts[core_idx].cpu.heap_break;
        child.prev_pc = child.cpu.pc;
        if let Some(p) = child.pipeline.as_mut() {
            Self::copy_pipeline_config_to_hart(&self.native().pipeline(), p);
            p.reset_stages(child.cpu.pc);
        }

        self.harts[free_core] = child;
        self.harts[core_idx].cpu.write(10, hart_id);
        self.console.push_colored(
            format!(
                "[C{}:H{}] hart start -> core {} pc=0x{:08X}",
                core_idx,
                self.harts[core_idx].hart_id.unwrap_or(0),
                free_core,
                request.entry_pc
            ),
            crate::ui::console::ConsoleColor::Info,
        );
    }

    pub(super) fn propagate_heap_break(&mut self, heap_break: u32) {
        self.native_mut().cpu_mut_unjournaled().heap_break = heap_break;
        for (idx, hart) in self.harts.iter_mut().enumerate() {
            if idx != self.selected_core {
                hart.cpu.heap_break = heap_break;
            }
        }
    }
}
