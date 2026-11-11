// ui/app/threads.rs — the thread scheduler as the TUI drives it
//
// With `sched=off` the harts own their cores, as they always did, and only the
// thread syscalls' bookkeeping below runs: a yield has no one to hand the core
// to, and a join keeps the hart off its core until the hart it waits for stops.
//
// With a scheduler, `harts[core]` holds the thread that core runs — or a free
// runtime when it runs none — and every other thread's runtime is parked in
// the scheduler. The pipeline belongs to the core, not to the thread: it stays
// in `harts[core]` across a switch and is redirected to the incoming thread's
// PC. Nothing in flight had changed the machine yet (an instruction changes it
// at commit), so the outgoing thread loses nothing it will not fetch again.

use super::*;
use crate::falcon::registers::SchedRequest;
use crate::falcon::sched::{JoinOutcome, SchedPolicy, Scheduler, ThreadState, Tid};

/// Something on the Threads tab the mouse can point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadsHit {
    /// A thread: a queue card, a core card's thread, a row of the list.
    Thread(Tid),
    /// A core card.
    Core(usize),
    /// Take the selected core from its thread now.
    ForceSwitch,
    /// Put the selected thread on the selected core now.
    Dispatch,
    Step,
    RunPause,
}

/// What the Threads tab keeps between frames.
#[derive(Default)]
pub(crate) struct ThreadsViewState {
    /// The thread whose context the tab shows. `None` follows the thread on
    /// the selected core.
    pub(crate) selected: Option<Tid>,
    /// What the last action did, and whether it failed.
    pub(crate) status: Option<(String, bool)>,
    /// Where each clickable thing was drawn — written by render, read by mouse.
    pub(crate) hits: std::cell::RefCell<Vec<(Rect, ThreadsHit)>>,
    pub(crate) hover: Option<ThreadsHit>,
}

impl ThreadsViewState {
    pub(crate) fn hit_at(&self, x: u16, y: u16) -> Option<ThreadsHit> {
        self.hits
            .borrow()
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            .map(|(_, hit)| *hit)
    }
}

impl App {
    /// The scheduler, when one shares the cores.
    pub(crate) fn scheduler(&self) -> Option<&Scheduler<HartCoreRuntime>> {
        self.sched.as_ref()
    }

    /// Whether a step goes round every core rather than the selected one:
    /// several cores, or several threads on one.
    pub(super) fn multi_context(&self) -> bool {
        self.max_cores > 1 || self.sched.is_some()
    }

    /// Whether a run advances every hart. A scheduler needs every tick, so it
    /// ignores the FOCUS scope.
    pub(super) fn steps_all_harts(&self) -> bool {
        self.sched.is_some() || matches!(self.run_scope, RunScope::AllHarts)
    }

    /// A fresh scheduler for the program just loaded, or `None` with
    /// `sched=off`. Its first thread is the program's, on core 0.
    pub(super) fn new_scheduler(&self) -> Option<Scheduler<HartCoreRuntime>> {
        if !self.sched_config.policy.is_enabled() || self.rv32().is_none() {
            return None;
        }
        let entry = self.rv32().map_or(0, |rv32| rv32.cpu().pc);
        Some(Scheduler::new(self.sched_config, self.max_cores, entry))
    }

    /// Change the policy. The threads of the running program were made under
    /// the old one, so this restarts the simulation, as the coherence
    /// protocol does.
    pub(crate) fn set_sched_policy(&mut self, policy: SchedPolicy) {
        if self.sched_config.policy == policy {
            return;
        }
        self.sched_config.policy = policy;
        self.restart_simulation();
    }

    /// The next policy in the Settings row's cycle.
    pub(crate) fn cycle_sched_policy(&mut self, forward: bool) {
        let all = SchedPolicy::ALL;
        let at = all
            .iter()
            .position(|&p| p == self.sched_config.policy)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % all.len()
        } else {
            (at + all.len() - 1) % all.len()
        };
        self.set_sched_policy(all[next]);
    }

    /// Change the quantum or the switch cost. They only shape decisions still
    /// to come, so the running scheduler takes them at once.
    pub(crate) fn set_sched_timing(&mut self, quantum: u32, switch_cost: u32) {
        self.sched_config.quantum = quantum.max(1);
        self.sched_config.switch_cost = switch_cost;
        if let Some(sched) = self.sched.as_mut() {
            sched.set_timing(quantum, switch_cost);
        }
    }

    fn core_cpu(&self, core: usize) -> &crate::falcon::Cpu {
        if core == self.selected_core {
            self.native().cpu()
        } else {
            &self.harts[core].cpu
        }
    }

    /// The CPU of `core`'s hart. Only reached for when something changes:
    /// on the selected core this goes around the step-back journal.
    fn core_cpu_mut(&mut self, core: usize) -> &mut crate::falcon::Cpu {
        if core == self.selected_core {
            self.native_mut().cpu_mut_unjournaled()
        } else {
            &mut self.harts[core].cpu
        }
    }

    /// The thread `core` is running, if its hart is one the program started.
    fn core_tid(&self, core: usize) -> Option<Tid> {
        match &self.sched {
            Some(sched) => sched.thread_on(core),
            None => self.harts.get(core).and_then(|h| h.hart_id),
        }
    }

    /// Whether `core`'s hart sits in `thread_join` and must not be stepped.
    pub(super) fn core_join_waiting(&self, core: usize) -> bool {
        self.harts.get(core).is_some() && self.core_cpu(core).join_wait.is_some()
    }

    /// Carry out what `core`'s hart asked of the scheduler during its step —
    /// a yield, a priority, a join — or, with `sched=off`, what stands in for
    /// it. Also settles a join that was waiting, once its target is done.
    pub(super) fn service_thread_requests(&mut self, core: usize) {
        if core >= self.harts.len() {
            return;
        }
        let (request, join, me) = {
            let cpu = self.core_cpu(core);
            (cpu.pending_sched, cpu.join_wait, cpu.hart_id)
        };
        if request.is_none() && join.is_none() {
            return;
        }
        let mut invalid = false;
        let mut join_done = false;
        if let Some(sched) = self.sched.as_mut() {
            match request {
                Some(SchedRequest::Yield) => sched.yield_now(core),
                Some(SchedRequest::SetPriority { tid, priority }) => {
                    invalid |= !sched.set_priority(tid, priority);
                }
                None => {}
            }
            if let Some(target) = join {
                match sched.join(core, target) {
                    // Blocked: the end of the tick takes it off the core, and
                    // dispatching it again clears the wait.
                    JoinOutcome::Wait => {}
                    JoinOutcome::Done => join_done = true,
                    JoinOutcome::Invalid => {
                        join_done = true;
                        invalid = true;
                    }
                }
            }
        } else {
            // No scheduler: harts are known by the ids handed out so far, and
            // alive while they run or wait at a breakpoint.
            let known = |id: u32| id < self.next_hart_id;
            let alive = |id: u32| {
                self.harts.iter().any(|h| {
                    h.hart_id == Some(id)
                        && matches!(h.lifecycle, HartLifecycle::Running | HartLifecycle::Paused)
                })
            };
            if let Some(SchedRequest::SetPriority { tid, .. }) = request {
                invalid |= !known(tid);
            }
            if let Some(target) = join {
                if target == me || !known(target) {
                    join_done = true;
                    invalid = true;
                } else if !alive(target) {
                    join_done = true;
                }
            }
        }
        if request.is_none() && !join_done && !invalid {
            // A join still waiting: nothing to write back.
            return;
        }
        let cpu = self.core_cpu_mut(core);
        cpu.pending_sched = None;
        if join_done {
            cpu.join_wait = None;
        }
        if invalid {
            cpu.write(10, (-1i32) as u32);
        }
    }

    /// `core`'s thread retired an instruction this tick: its quantum may
    /// now run out.
    pub(super) fn note_thread_retired(&mut self, core: usize) {
        if let Some(sched) = self.sched.as_mut() {
            sched.retired(core);
        }
    }

    /// Tell the scheduler how `core`'s step ended.
    pub(super) fn note_thread_lifecycle(&mut self, core: usize, lifecycle: HartLifecycle, program_exit: bool) {
        let Some(sched) = self.sched.as_mut() else {
            return;
        };
        if program_exit {
            sched.close(core);
        } else if lifecycle == HartLifecycle::Exited {
            sched.exit(core);
        }
    }

    /// Start the thread `child` for the hart on `parent_core`. Returns its id.
    pub(super) fn spawn_scheduled_thread(
        &mut self,
        parent_core: usize,
        mut child: HartCoreRuntime,
        entry_pc: u32,
    ) -> Option<Tid> {
        let parent = self.core_tid(parent_core);
        let sched = self.sched.as_mut()?;
        let tid = sched.next_tid();
        child.hart_id = Some(tid);
        child.cpu.hart_id = tid;
        // The pipeline is the core's; a waiting thread has none.
        child.pipeline = None;
        sched.spawn(parent, entry_pc, child);
        self.next_hart_id = tid.saturating_add(1);
        Some(tid)
    }

    /// Close the scheduler's tick and move the threads it switched.
    pub(super) fn end_sched_tick(&mut self) {
        let Some(sched) = self.sched.as_mut() else {
            return;
        };
        let switches = sched.end_tick();
        self.apply_switches(&switches);

        let Some(sched) = self.sched.as_ref() else {
            return;
        };
        if sched.is_deadlocked() {
            let waits: Vec<String> = sched
                .threads()
                .iter()
                .filter_map(|t| match t.state {
                    crate::falcon::sched::ThreadState::Blocked(on) => {
                        Some(format!("T{} joins T{on}", t.tid))
                    }
                    _ => None,
                })
                .collect();
            self.console.push_error(format!(
                "Deadlock at tick {}: every thread left is blocked in thread_join ({}).",
                sched.now(),
                waits.join(", ")
            ));
            self.session.is_running = false;
        } else if !sched.has_live_threads() {
            self.native_mut().mem_mut_unjournaled().sync_to_ram();
            self.session.is_running = false;
        }
    }

    /// Move the threads `switches` names: park every one that leaves a core,
    /// then load every one that enters.
    fn apply_switches(&mut self, switches: &[crate::falcon::sched::Switch]) {
        if switches.is_empty() {
            return;
        }
        let selected = self.selected_core;
        let touches_selected = switches.iter().any(|s| s.core == selected);
        // The selected core's hart lives in the machine; bring its state
        // to its slot so every core is switched the same way. Its
        // pipeline is the core's and stays in the machine.
        if touches_selected {
            self.save_selected_hart_state();
        }
        let (base_pc, mem_size) = (self.session.base_pc, self.session.mem_size);
        // Every thread that leaves is parked before any enters: one can
        // leave a core and enter another in the same tick.
        for switch in switches {
            let Some((tid, _)) = switch.out else {
                continue;
            };
            let slot = &mut self.harts[switch.core];
            let pipeline = slot.pipeline.take();
            let mut ctx = std::mem::replace(slot, HartCoreRuntime::free(base_pc, mem_size));
            slot.pipeline = pipeline;
            slot.cpu.heap_break = ctx.cpu.heap_break;
            ctx.pipeline = None;
            if let Some(sched) = self.sched.as_mut() {
                sched.park(tid, ctx);
            }
        }
        for switch in switches {
            let Some(tid) = switch.inn else {
                continue;
            };
            let Some(mut ctx) = self.sched.as_mut().and_then(|s| s.unpark(tid)) else {
                continue;
            };
            // Dispatched means nothing blocks it any more.
            ctx.cpu.join_wait = None;
            let mut pipeline = self.harts[switch.core].pipeline.take();
            if let Some(pipe) = pipeline.as_mut() {
                pipe.redirect_pc(ctx.cpu.pc);
            }
            ctx.pipeline = pipeline;
            self.harts[switch.core] = ctx;
        }
        if touches_selected {
            self.load_selected_hart_state();
            let pc = self.native().cpu().pc;
            self.native_mut().pipeline_mut().redirect_pc(pc);
        }
    }

    /// The thread the Threads tab shows: the one picked, or else the one on
    /// the selected core, or else the program's first.
    pub(crate) fn threads_selected_tid(&self) -> Option<Tid> {
        let sched = self.sched.as_ref()?;
        self.threads_view
            .selected
            .filter(|&tid| sched.thread(tid).is_some())
            .or_else(|| sched.thread_on(self.selected_core))
            .or(Some(0))
    }

    /// A thread's registers: live on the core it runs on (`true`), or as it
    /// saved them when it left (`false`).
    pub(crate) fn thread_cpu(&self, tid: Tid) -> Option<(&crate::falcon::Cpu, bool)> {
        let sched = self.sched.as_ref()?;
        match sched.core_of(tid) {
            Some(core) => Some((self.core_cpu(core), true)),
            None => sched.parked(tid).map(|ctx| (&ctx.cpu, false)),
        }
    }

    /// Show `tid` in the Threads tab.
    pub(crate) fn select_thread(&mut self, tid: Tid) {
        self.threads_view.selected = Some(tid);
    }

    /// Move the Threads tab's selection `delta` threads along.
    pub(crate) fn select_thread_offset(&mut self, delta: isize) {
        let Some(count) = self.sched.as_ref().map(|s| s.threads().len()) else {
            return;
        };
        let at = self.threads_selected_tid().unwrap_or(0) as isize;
        let next = (at + delta).rem_euclid(count.max(1) as isize) as Tid;
        self.threads_view.selected = Some(next);
    }

    /// Take the selected core from its thread now, as a timer interrupt
    /// would, and give it to the thread the policy picks.
    pub(crate) fn force_context_switch(&mut self) {
        self.forced_switch(None);
    }

    /// Put the thread the Threads tab shows on the selected core now. A
    /// thread already on a core just selects that core.
    pub(crate) fn dispatch_selected_thread(&mut self) {
        let Some(tid) = self.threads_selected_tid() else {
            return;
        };
        let state = self.sched.as_ref().and_then(|s| s.thread(tid)).map(|t| t.state);
        match state {
            Some(ThreadState::Ready) => self.forced_switch(Some(tid)),
            Some(ThreadState::Running(core)) => {
                self.switch_selected_core(core);
                self.threads_view.status = Some((format!("T{tid} is already on core {core}"), false));
            }
            Some(ThreadState::Blocked(on)) => {
                self.threads_view.status = Some((
                    format!("T{tid} is blocked joining T{on}: it cannot run until T{on} exits"),
                    true,
                ));
            }
            Some(ThreadState::Exited) => {
                self.threads_view.status = Some((format!("T{tid} has exited"), true));
            }
            None => {}
        }
    }

    fn forced_switch(&mut self, next: Option<Tid>) {
        let core = self.selected_core;
        let Some(sched) = self.sched.as_mut() else {
            self.threads_view.status =
                Some(("no scheduler: Settings → THREAD SCHEDULING".to_string(), true));
            return;
        };
        let tick = sched.now();
        match sched.force_switch(core, next) {
            Ok(switches) => {
                let mut parts = Vec::new();
                for s in &switches {
                    if let Some((out, _)) = s.out {
                        parts.push(format!("T{out} saved, back of the queue"));
                    }
                    if let Some(inn) = s.inn {
                        parts.push(format!("T{inn} loaded on core {core}"));
                        self.threads_view.selected = Some(inn);
                    }
                }
                self.apply_switches(&switches);
                self.threads_view.status =
                    Some((format!("tick {tick}: context switch — {}", parts.join(" · ")), false));
            }
            Err(message) => self.threads_view.status = Some((message, true)),
        }
    }

    /// Hand the heap break to the threads waiting off-core too.
    pub(super) fn propagate_heap_break_to_parked(&mut self, heap_break: u32) {
        if let Some(sched) = self.sched.as_mut() {
            for (_, ctx) in sched.parked_iter_mut() {
                ctx.cpu.heap_break = heap_break;
            }
        }
    }

    /// Where `tid` is now: its PC, read from the core it runs on or from its
    /// parked context.
    pub(crate) fn thread_pc(&self, tid: Tid) -> Option<u32> {
        let sched = self.sched.as_ref()?;
        match sched.core_of(tid) {
            Some(core) => Some(self.core_cpu(core).pc),
            None => sched.parked(tid).map(|ctx| ctx.cpu.pc),
        }
    }
}
