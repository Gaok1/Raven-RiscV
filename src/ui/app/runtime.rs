use super::*;

impl App {
    pub(in crate::ui) fn tab_visible(&self, tab: Tab) -> bool {
        match tab {
            Tab::Cache => self.run.cache_enabled,
            Tab::Pipeline => self.pipeline.enabled,
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
        self.run.cache_enabled = enabled;
        self.run.mem.bypass = !enabled;
        self.run.mem.flush_all();
        self.ensure_visible_tab();
    }

    pub(in crate::ui) fn set_pipeline_enabled(&mut self, enabled: bool) {
        self.pipeline.enabled = enabled;
        self.ensure_visible_tab();
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
        self.selected_core = 0;
        self.next_hart_id = 1;
        self.harts.clear();
        for core in 0..self.max_cores {
            let mut runtime = HartCoreRuntime::free(self.run.base_pc, self.run.mem_size);
            runtime.cpu.heap_break = self.run.cpu.heap_break;
            if core == 0 {
                runtime.hart_id = Some(0);
                runtime.lifecycle = HartLifecycle::Running;
                runtime.cpu = self.run.cpu.clone();
                runtime.prev_x = self.run.prev_x;
                runtime.prev_f = self.run.prev_f;
                runtime.prev_pc = self.run.prev_pc;
                runtime.faulted = self.run.faulted;
                runtime.reg_age = self.run.reg_age;
                runtime.f_age = self.run.f_age;
                runtime.reg_last_write_pc = self.run.reg_last_write_pc;
                runtime.f_last_write_pc = self.run.f_last_write_pc;
                runtime.exec_counts = self.run.exec_counts.clone();
                runtime.exec_trace = self.run.exec_trace.clone();
                runtime.dyn_mem_access = self.run.dyn_mem_access;
                runtime.mem_access_log = self.run.mem_access_log.clone();
                runtime.pipeline = None;
            } else if let Some(p) = runtime.pipeline.as_mut() {
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
        let replacement = crate::ui::pipeline::PipelineSimState::new();
        if let Some(runtime) = self.harts.get_mut(selected) {
            runtime.cpu = self.run.cpu.clone();
            runtime.prev_x = self.run.prev_x;
            runtime.prev_f = self.run.prev_f;
            runtime.prev_pc = self.run.prev_pc;
            runtime.faulted = self.run.faulted;
            runtime.reg_age = self.run.reg_age;
            runtime.f_age = self.run.f_age;
            runtime.reg_last_write_pc = self.run.reg_last_write_pc;
            runtime.f_last_write_pc = self.run.f_last_write_pc;
            runtime.exec_counts = self.run.exec_counts.clone();
            runtime.exec_trace = self.run.exec_trace.clone();
            runtime.dyn_mem_access = self.run.dyn_mem_access;
            runtime.mem_access_log = self.run.mem_access_log.clone();
            runtime.pipeline = Some(std::mem::replace(&mut self.pipeline, replacement));
        }
    }

    pub(crate) fn sync_runtime_for_debug(&mut self) {
        self.sync_runtime_to_selected_core();
    }

    pub(super) fn sync_runtime_to_selected_core(&mut self) {
        let selected = self.selected_core;
        if let Some(runtime) = self.harts.get_mut(selected) {
            self.run.cpu = runtime.cpu.clone();
            self.run.prev_x = runtime.prev_x;
            self.run.prev_f = runtime.prev_f;
            self.run.prev_pc = runtime.prev_pc;
            self.run.faulted = runtime.faulted;
            self.run.reg_age = runtime.reg_age;
            self.run.f_age = runtime.f_age;
            self.run.reg_last_write_pc = runtime.reg_last_write_pc;
            self.run.f_last_write_pc = runtime.f_last_write_pc;
            self.run.exec_counts = runtime.exec_counts.clone();
            self.run.exec_trace = runtime.exec_trace.clone();
            self.run.dyn_mem_access = runtime.dyn_mem_access;
            self.run.mem_access_log = runtime.mem_access_log.clone();
            let mut pipeline = runtime
                .pipeline
                .take()
                .unwrap_or_else(crate::ui::pipeline::PipelineSimState::new);
            if pipeline.fetch_pc == 0 && pipeline.cycle_count == 0 {
                pipeline.reset_stages(self.run.cpu.pc);
            }
            self.pipeline = pipeline;
        }
    }

    pub(crate) fn switch_selected_core(&mut self, new_core: usize) {
        if new_core >= self.max_cores || new_core == self.selected_core {
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
        let mem = self.run.mem_size as u32;
        (mem / denom).clamp(4096, 64 * 1024)
    }

    pub(super) fn stack_slot_bounds(&self, core: usize) -> (u32, u32) {
        let size = self.stack_slot_size();
        let top = (self.run.mem_size as u32).saturating_sub(size.saturating_mul(core as u32));
        let bottom = top.saturating_sub(size);
        (bottom, top)
    }

    pub(super) fn is_pc_in_program(&self, pc: u32) -> bool {
        self.imem_in_range(pc)
    }

    pub(super) fn process_pending_hart_start_for_selected(&mut self) {
        let Some(request) = self.run.cpu.pending_hart_start.take() else {
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
            self.run.cpu.write(10, (-1i32) as u32);
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
            self.run.cpu.write(10, (-2i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: entry PC 0x{:08X} is outside the loaded program",
                    self.selected_core,
                    self.core_hart_id(self.selected_core).unwrap_or(0),
                    request.entry_pc
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        if request.stack_ptr == 0
            || request.stack_ptr > self.run.mem_size as u32
            || request.stack_ptr & 0xF != 0
        {
            self.run.cpu.write(10, (-3i32) as u32);
            self.console.push_colored(
                format!(
                    "[C{}:H{}] hart start failed: stack 0x{:08X} invalid (must be non-zero, 16-byte aligned, within memory [0..0x{:08X}])",
                    self.selected_core,
                    self.core_hart_id(self.selected_core).unwrap_or(0),
                    request.stack_ptr,
                    self.run.mem_size,
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        let hart_id = self.next_hart_id;
        self.next_hart_id = self.next_hart_id.saturating_add(1);

        let mut child = HartCoreRuntime::free(self.run.base_pc, self.run.mem_size);
        child.hart_id = Some(hart_id);
        child.lifecycle = HartLifecycle::Running;
        child.cpu.pc = request.entry_pc;
        child.cpu.write(2, request.stack_ptr);
        child.cpu.write(10, request.arg);
        child.cpu.heap_break = self.run.cpu.heap_break;
        child.prev_pc = child.cpu.pc;
        if let Some(p) = child.pipeline.as_mut() {
            p.enabled = self.pipeline.enabled;
            p.forwarding = self.pipeline.forwarding;
            p.branch_resolve = self.pipeline.branch_resolve;
            p.mode = self.pipeline.mode;
            p.predict = self.pipeline.predict;
            p.speed = self.pipeline.speed;
            p.reset_stages(child.cpu.pc);
        }

        self.harts[free_core] = child;
        self.run.cpu.write(10, hart_id);
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
    /// and writes to `self.harts[core_idx].cpu` instead of `self.run.cpu`.
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
                    "[C{}:H{}] hart start failed: entry PC 0x{:08X} is outside the loaded program",
                    core_idx,
                    self.harts[core_idx].hart_id.unwrap_or(0),
                    request.entry_pc
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }
        if request.stack_ptr == 0
            || request.stack_ptr > self.run.mem_size as u32
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
                    self.run.mem_size,
                ),
                crate::ui::console::ConsoleColor::Warning,
            );
            return;
        }

        let hart_id = self.next_hart_id;
        self.next_hart_id = self.next_hart_id.saturating_add(1);

        let mut child = HartCoreRuntime::free(self.run.base_pc, self.run.mem_size);
        child.hart_id = Some(hart_id);
        child.lifecycle = HartLifecycle::Running;
        child.cpu.pc = request.entry_pc;
        child.cpu.write(2, request.stack_ptr);
        child.cpu.write(10, request.arg);
        child.cpu.heap_break = self.harts[core_idx].cpu.heap_break;
        child.prev_pc = child.cpu.pc;
        if let Some(p) = child.pipeline.as_mut() {
            p.enabled = self.pipeline.enabled;
            p.forwarding = self.pipeline.forwarding;
            p.branch_resolve = self.pipeline.branch_resolve;
            p.mode = self.pipeline.mode;
            p.predict = self.pipeline.predict;
            p.speed = self.pipeline.speed;
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
        self.run.cpu.heap_break = heap_break;
        for (idx, hart) in self.harts.iter_mut().enumerate() {
            if idx != self.selected_core {
                hart.cpu.heap_break = heap_break;
            }
        }
    }
}
