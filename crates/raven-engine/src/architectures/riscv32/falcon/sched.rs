// falcon/sched.rs — which software thread each core runs
//
// With `SchedPolicy::Off` there is no scheduler: every hart a program starts
// takes a free core and keeps it until it exits, and `hart_start` fails when
// no core is free. That is what RAVEN always did, and nothing here runs.
//
// Any other policy turns the harts a program starts into threads that share
// the cores. A thread that finds no free core waits in the ready queue, and
// the scheduler decides, at the end of every tick, which thread each core runs
// next:
//
//   fifo      first come, first served. A thread keeps its core until it
//             exits, blocks in `thread_join` or gives it up with `thread_yield`.
//   rr        round robin: FIFO, plus a thread that ran a whole quantum goes
//             to the back of the queue when another thread is waiting.
//   priority  the most urgent ready thread always runs, taking the core from a
//             less urgent one at once; threads of equal priority share by
//             round robin. A lower number is more urgent.
//
// A tick is one round of the simulator: every core executes one instruction
// in sequential mode, or one clock cycle with the pipeline. The quantum and
// the cost of a context switch are counted in ticks.
//
// The scheduler only keeps the books. The host owns the threads' contexts
// while they run — registers, PC, whatever else it shows per thread — and
// parks them here while they wait. `end_tick` says which thread leaves and
// which enters each core, and the host moves the contexts accordingly.

use std::collections::VecDeque;

/// Thread identifier. It is the hart id a program reads from `mhartid`.
pub type Tid = u32;

/// Priority a thread starts with. Lower is more urgent.
pub const DEFAULT_PRIORITY: u32 = 10;

/// Segments kept per core for the timeline; older ones are dropped.
const TIMELINE_CAP: usize = 4096;

/// Events kept for the log; older ones are dropped.
const EVENT_CAP: usize = 512;

/// How the threads share the cores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SchedPolicy {
    /// No scheduler: one hart per core, as RAVEN always did.
    #[default]
    Off,
    /// First come, first served, without preemption.
    Fifo,
    /// Round robin with a quantum.
    RoundRobin,
    /// Preemptive priority, round robin among equal priorities.
    Priority,
}

impl SchedPolicy {
    pub const ALL: [Self; 4] = [Self::Off, Self::Fifo, Self::RoundRobin, Self::Priority];

    /// Stable lowercase tag used in `.rcfg` files and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fifo => "fifo",
            Self::RoundRobin => "rr",
            Self::Priority => "priority",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "pinned" => Some(Self::Off),
            "fifo" | "fcfs" => Some(Self::Fifo),
            "rr" | "round-robin" | "roundrobin" | "round_robin" => Some(Self::RoundRobin),
            "priority" | "prio" => Some(Self::Priority),
            _ => None,
        }
    }

    /// The name a screen shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fifo => "FIFO",
            Self::RoundRobin => "Round robin",
            Self::Priority => "Priority",
        }
    }

    /// Whether threads share the cores at all.
    pub fn is_enabled(self) -> bool {
        self != Self::Off
    }

    /// Whether a running thread can lose its core without asking.
    pub fn is_preemptive(self) -> bool {
        matches!(self, Self::RoundRobin | Self::Priority)
    }
}

/// The scheduler's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedConfig {
    pub policy: SchedPolicy,
    /// Ticks a thread runs before round robin hands its core to the next.
    pub quantum: u32,
    /// Ticks a core spends switching from one thread to another, running
    /// neither: saving one context and loading the next.
    pub switch_cost: u32,
}

impl SchedConfig {
    pub const DEFAULT_QUANTUM: u32 = 50;
    pub const DEFAULT_SWITCH_COST: u32 = 0;
    pub const MAX_QUANTUM: u32 = 1_000_000;
    pub const MAX_SWITCH_COST: u32 = 10_000;

    /// The `[sim]` keys this config is saved as.
    pub fn to_rcfg(&self) -> String {
        format!(
            "sched={}\nsched_quantum={}\nsched_switch_cost={}\n",
            self.policy.as_str(),
            self.quantum,
            self.switch_cost
        )
    }

    /// Read the `[sim]` keys back. A key that is absent keeps its default.
    pub fn from_rcfg_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut cfg = Self::default();
        if let Some(v) = get("sched") {
            cfg.policy = SchedPolicy::from_str(&v)
                .ok_or_else(|| format!("Invalid sched: {v} (use off|fifo|rr|priority)"))?;
        }
        if let Some(v) = get("sched_quantum") {
            cfg.quantum = v
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|q| (1..=Self::MAX_QUANTUM).contains(q))
                .ok_or_else(|| {
                    format!("Invalid sched_quantum: {v} (use 1..={})", Self::MAX_QUANTUM)
                })?;
        }
        if let Some(v) = get("sched_switch_cost") {
            cfg.switch_cost = v
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|c| *c <= Self::MAX_SWITCH_COST)
                .ok_or_else(|| {
                    format!(
                        "Invalid sched_switch_cost: {v} (use 0..={})",
                        Self::MAX_SWITCH_COST
                    )
                })?;
        }
        Ok(cfg)
    }
}

impl Default for SchedConfig {
    fn default() -> Self {
        Self {
            policy: SchedPolicy::Off,
            quantum: Self::DEFAULT_QUANTUM,
            switch_cost: Self::DEFAULT_SWITCH_COST,
        }
    }
}

/// Where a thread is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// Waiting in the ready queue for a core.
    Ready,
    /// On this core: running, or being switched in.
    Running(usize),
    /// In `thread_join`, waiting for this thread to exit.
    Blocked(Tid),
    /// Finished.
    Exited,
}

impl ThreadState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Running(_) => "RUN",
            Self::Blocked(_) => "BLOCK",
            Self::Exited => "EXIT",
        }
    }
}

/// What a thread did with its time. Response time is `first_run - arrival`,
/// turnaround `finish - arrival`, waiting time `ready_ticks`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadStats {
    /// Tick it was created.
    pub arrival: u64,
    /// Tick it first ran on a core.
    pub first_run: Option<u64>,
    /// Tick it exited.
    pub finish: Option<u64>,
    /// Ticks spent running on a core.
    pub run_ticks: u64,
    /// Ticks spent in the ready queue.
    pub ready_ticks: u64,
    /// Ticks spent blocked in `thread_join`.
    pub blocked_ticks: u64,
    /// Times it was put on a core.
    pub dispatches: u64,
    /// Times the scheduler took its core away: quantum over, or a more
    /// urgent thread arrived.
    pub preemptions: u64,
    /// Times it gave its core up with `thread_yield`.
    pub yields: u64,
}

impl ThreadStats {
    pub fn response(&self) -> Option<u64> {
        self.first_run.map(|t| t.saturating_sub(self.arrival))
    }

    pub fn turnaround(&self) -> Option<u64> {
        self.finish.map(|t| t.saturating_sub(self.arrival))
    }
}

/// One thread as the scheduler sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadInfo {
    pub tid: Tid,
    /// The thread that started it; `None` for the program's first thread.
    pub parent: Option<Tid>,
    /// Where it started.
    pub entry_pc: u32,
    pub priority: u32,
    pub state: ThreadState,
    /// The core it ran on last, if any.
    pub last_core: Option<usize>,
    pub stats: ThreadStats,
}

/// What a core is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreState {
    /// No thread to run.
    Idle,
    /// Switching to `tid`; it starts running after `left` more ticks.
    Switching { tid: Tid, left: u32 },
    /// Running `tid`, which has used `slice` ticks of its quantum.
    Running { tid: Tid, slice: u32 },
}

impl CoreState {
    /// The thread that holds the core, running or switching in.
    pub fn tid(self) -> Option<Tid> {
        match self {
            Self::Idle => None,
            Self::Switching { tid, .. } | Self::Running { tid, .. } => Some(tid),
        }
    }
}

/// What filled a stretch of a core's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Idle,
    /// Switching to this thread.
    Switch(Tid),
    Run(Tid),
}

/// A stretch of one core's timeline, ticks `start..end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub lane: Lane,
    pub start: u64,
    pub end: u64,
}

/// Why a thread left its core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaveReason {
    Exit,
    Block,
    Yield,
    /// Used its whole quantum while another thread was waiting.
    Quantum,
    /// A more urgent thread took the core.
    Preempt,
}

impl LeaveReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Block => "join",
            Self::Yield => "yield",
            Self::Quantum => "quantum",
            Self::Preempt => "preempt",
        }
    }
}

/// A change of thread on one core. The host parks the context of `out`, if
/// any, and loads the context of `inn`, if any — every `out` of a tick before
/// any `inn`, since a thread can leave one core and enter another in the same
/// tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switch {
    pub core: usize,
    pub out: Option<(Tid, LeaveReason)>,
    pub inn: Option<Tid>,
}

/// Something worth a line in the scheduler's log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedEventKind {
    Spawn { parent: Option<Tid> },
    Dispatch { core: usize },
    Leave { core: usize, reason: LeaveReason },
    /// Woken because the thread it joined exited.
    Wake,
    Priority { priority: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedEvent {
    pub tick: u64,
    pub tid: Tid,
    pub kind: SchedEventKind,
}

/// How a `thread_join` call turned out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// The caller blocks until the target exits.
    Wait,
    /// The target had already exited: the call returns at once.
    Done,
    /// No such thread, or the caller itself.
    Invalid,
}

/// Averages over the threads, in ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SchedSummary {
    pub threads: usize,
    pub finished: usize,
    pub ticks: u64,
    pub context_switches: u64,
    pub preemptions: u64,
    pub avg_waiting: f64,
    pub avg_response: f64,
    /// Over the threads that finished.
    pub avg_turnaround: f64,
    /// Share of core-ticks spent running a thread, 0..=1.
    pub utilization: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CoreFlags {
    exit: bool,
    block: bool,
    yielded: bool,
}

/// Per-core counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoreTicks {
    pub busy: u64,
    pub switching: u64,
    pub idle: u64,
}

/// The ready queue, the cores and the threads' books. `C` is the context the
/// host saves for a thread that is not on a core.
#[derive(Clone, Debug)]
pub struct Scheduler<C> {
    cfg: SchedConfig,
    threads: Vec<ThreadInfo>,
    parked: Vec<Option<C>>,
    ready: VecDeque<Tid>,
    cores: Vec<CoreState>,
    flags: Vec<CoreFlags>,
    core_ticks: Vec<CoreTicks>,
    timeline: Vec<VecDeque<Segment>>,
    events: VecDeque<SchedEvent>,
    now: u64,
    context_switches: u64,
}

impl<C> Scheduler<C> {
    /// A scheduler for `cores` cores with the program's first thread, tid 0,
    /// already running on core 0 from `entry_pc`. Its context stays with the
    /// host.
    pub fn new(cfg: SchedConfig, cores: usize, entry_pc: u32) -> Self {
        let cores = cores.max(1);
        let mut sched = Self {
            cfg,
            threads: Vec::new(),
            parked: Vec::new(),
            ready: VecDeque::new(),
            cores: vec![CoreState::Idle; cores],
            flags: vec![CoreFlags::default(); cores],
            core_ticks: vec![CoreTicks::default(); cores],
            timeline: vec![VecDeque::new(); cores],
            events: VecDeque::new(),
            now: 0,
            context_switches: 0,
        };
        sched.threads.push(ThreadInfo {
            tid: 0,
            parent: None,
            entry_pc,
            priority: DEFAULT_PRIORITY,
            state: ThreadState::Running(0),
            last_core: Some(0),
            stats: ThreadStats {
                first_run: Some(0),
                dispatches: 1,
                ..ThreadStats::default()
            },
        });
        sched.parked.push(None);
        sched.cores[0] = CoreState::Running { tid: 0, slice: 0 };
        sched.log(0, SchedEventKind::Spawn { parent: None });
        sched.log(0, SchedEventKind::Dispatch { core: 0 });
        sched
    }

    pub fn config(&self) -> SchedConfig {
        self.cfg
    }

    /// Change the quantum or the switch cost of a running scheduler. The
    /// policy is fixed for its lifetime.
    pub fn set_timing(&mut self, quantum: u32, switch_cost: u32) {
        self.cfg.quantum = quantum.max(1);
        self.cfg.switch_cost = switch_cost;
    }

    /// Ticks elapsed.
    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn threads(&self) -> &[ThreadInfo] {
        &self.threads
    }

    pub fn thread(&self, tid: Tid) -> Option<&ThreadInfo> {
        self.threads.get(tid as usize)
    }

    /// The next tid `spawn` hands out.
    pub fn next_tid(&self) -> Tid {
        self.threads.len() as Tid
    }

    /// The ready queue, front first.
    pub fn ready_queue(&self) -> impl Iterator<Item = Tid> + '_ {
        self.ready.iter().copied()
    }

    pub fn cores(&self) -> &[CoreState] {
        &self.cores
    }

    pub fn core_ticks(&self) -> &[CoreTicks] {
        &self.core_ticks
    }

    /// The thread `core` is executing this tick, if any. A core switching to
    /// a thread holds it but does not execute it yet.
    pub fn running_on(&self, core: usize) -> Option<Tid> {
        match self.cores.get(core)? {
            CoreState::Running { tid, .. } => Some(*tid),
            _ => None,
        }
    }

    /// The thread on `core`, running or switching in.
    pub fn thread_on(&self, core: usize) -> Option<Tid> {
        self.cores.get(core).and_then(|c| c.tid())
    }

    /// The core `tid` is on, running or switching in.
    pub fn core_of(&self, tid: Tid) -> Option<usize> {
        self.cores.iter().position(|c| c.tid() == Some(tid))
    }

    pub fn timeline(&self, core: usize) -> impl Iterator<Item = &Segment> + '_ {
        self.timeline.get(core).into_iter().flatten()
    }

    /// The log, oldest first.
    pub fn events(&self) -> impl DoubleEndedIterator<Item = &SchedEvent> + '_ {
        self.events.iter()
    }

    pub fn context_switches(&self) -> u64 {
        self.context_switches
    }

    /// The saved context of a thread that is not on a core.
    pub fn parked(&self, tid: Tid) -> Option<&C> {
        self.parked.get(tid as usize).and_then(Option::as_ref)
    }

    pub fn parked_mut(&mut self, tid: Tid) -> Option<&mut C> {
        self.parked.get_mut(tid as usize).and_then(Option::as_mut)
    }

    /// Every saved context, with its thread.
    pub fn parked_iter_mut(&mut self) -> impl Iterator<Item = (Tid, &mut C)> + '_ {
        self.parked
            .iter_mut()
            .enumerate()
            .filter_map(|(tid, ctx)| ctx.as_mut().map(|ctx| (tid as Tid, ctx)))
    }

    /// Save the context of a thread that left its core.
    pub fn park(&mut self, tid: Tid, ctx: C) {
        if let Some(slot) = self.parked.get_mut(tid as usize) {
            *slot = Some(ctx);
        }
    }

    /// Hand back the context of a thread entering a core.
    pub fn unpark(&mut self, tid: Tid) -> Option<C> {
        self.parked.get_mut(tid as usize).and_then(Option::take)
    }

    /// A thread that has not exited: running, ready or blocked.
    pub fn has_live_threads(&self) -> bool {
        self.threads
            .iter()
            .any(|t| t.state != ThreadState::Exited)
    }

    /// Whether some thread can still make progress: one holds a core or waits
    /// for one.
    pub fn has_runnable_threads(&self) -> bool {
        self.threads
            .iter()
            .any(|t| matches!(t.state, ThreadState::Ready | ThreadState::Running(_)))
    }

    /// Every thread that has not exited is blocked in `thread_join`, so none
    /// will ever run again.
    pub fn is_deadlocked(&self) -> bool {
        self.has_live_threads() && !self.has_runnable_threads()
    }

    /// Start a thread created by `parent`. It waits in the ready queue for a
    /// core, which it can get at the end of this tick at the earliest.
    pub fn spawn(&mut self, parent: Option<Tid>, entry_pc: u32, ctx: C) -> Tid {
        let tid = self.next_tid();
        // A child starts at its parent's priority, as a forked process does.
        let priority = parent
            .and_then(|p| self.thread(p))
            .map_or(DEFAULT_PRIORITY, |p| p.priority);
        self.threads.push(ThreadInfo {
            tid,
            parent,
            entry_pc,
            priority,
            state: ThreadState::Ready,
            last_core: None,
            stats: ThreadStats {
                arrival: self.now,
                ..ThreadStats::default()
            },
        });
        self.parked.push(Some(ctx));
        self.ready.push_back(tid);
        self.log(tid, SchedEventKind::Spawn { parent });
        tid
    }

    /// The thread on `core` exited. Threads joined on it wake up.
    pub fn exit(&mut self, core: usize) {
        let Some(tid) = self.thread_on(core) else {
            return;
        };
        let Some(thread) = self.threads.get_mut(tid as usize) else {
            return;
        };
        if thread.state == ThreadState::Exited {
            return;
        }
        thread.state = ThreadState::Exited;
        // It ran during this tick, so it is done when the tick ends.
        thread.stats.finish = Some(self.now + 1);
        self.flags[core].exit = true;
        for waiter in 0..self.threads.len() {
            if self.threads[waiter].state == ThreadState::Blocked(tid) {
                self.threads[waiter].state = ThreadState::Ready;
                self.ready.push_back(waiter as Tid);
                self.log(waiter as Tid, SchedEventKind::Wake);
            }
        }
    }

    /// The thread on `core` called `thread_join(target)`.
    pub fn join(&mut self, core: usize, target: Tid) -> JoinOutcome {
        let Some(tid) = self.running_on(core) else {
            return JoinOutcome::Invalid;
        };
        if target == tid {
            return JoinOutcome::Invalid;
        }
        match self.thread(target).map(|t| t.state) {
            None => JoinOutcome::Invalid,
            Some(ThreadState::Exited) => JoinOutcome::Done,
            Some(_) => {
                self.threads[tid as usize].state = ThreadState::Blocked(target);
                self.flags[core].block = true;
                JoinOutcome::Wait
            }
        }
    }

    /// The thread on `core` called `thread_yield`.
    pub fn yield_now(&mut self, core: usize) {
        if let Some(tid) = self.running_on(core) {
            self.threads[tid as usize].stats.yields += 1;
            self.flags[core].yielded = true;
        }
    }

    /// Give `tid` a new priority. Under the priority policy the change takes
    /// effect at the end of this tick.
    pub fn set_priority(&mut self, tid: Tid, priority: u32) -> bool {
        let Some(thread) = self.threads.get_mut(tid as usize) else {
            return false;
        };
        thread.priority = priority;
        self.log(tid, SchedEventKind::Priority { priority });
        true
    }

    /// The program ended during this tick (`exit` ends every thread at once):
    /// count the tick and decide nothing more. The thread on `core` is the
    /// one that ended it.
    pub fn close(&mut self, core: usize) {
        self.exit(core);
        self.account_tick();
        self.now += 1;
        self.flags.iter_mut().for_each(|f| *f = CoreFlags::default());
    }

    /// Close the tick: count where every thread spent it, then decide which
    /// thread each core runs next. The host applies the switches returned.
    pub fn end_tick(&mut self) -> Vec<Switch> {
        self.account_tick();
        self.now += 1;

        // A core that finished switching starts running its thread.
        for core in 0..self.cores.len() {
            if let CoreState::Switching { tid, left } = self.cores[core] {
                if left <= 1 {
                    self.cores[core] = CoreState::Running { tid, slice: 0 };
                    self.threads[tid as usize]
                        .stats
                        .first_run
                        .get_or_insert(self.now);
                } else {
                    self.cores[core] = CoreState::Switching { tid, left: left - 1 };
                }
            }
        }

        let mut switches: Vec<Switch> = Vec::new();

        // Threads that exited or blocked leave their core.
        for core in 0..self.cores.len() {
            let flags = std::mem::take(&mut self.flags[core]);
            let Some(tid) = self.thread_on(core) else {
                continue;
            };
            let reason = if flags.exit {
                LeaveReason::Exit
            } else if flags.block {
                LeaveReason::Block
            } else {
                if flags.yielded {
                    // Kept for after the idle cores are filled: a yield with
                    // no one waiting goes on running.
                    self.flags[core].yielded = true;
                }
                continue;
            };
            self.vacate(core, tid, reason, &mut switches);
        }

        // Idle cores take the threads waiting longest (or most urgent).
        for core in 0..self.cores.len() {
            if self.cores[core] == CoreState::Idle {
                if let Some(next) = self.pick_next(None) {
                    self.dispatch(core, next, &mut switches);
                }
            }
        }

        // A thread that yielded, or used its quantum, hands its core to the
        // next one waiting — if any is waiting.
        for core in 0..self.cores.len() {
            let yielded = std::mem::take(&mut self.flags[core].yielded);
            let CoreState::Running { tid, slice } = self.cores[core] else {
                continue;
            };
            let priority = self.threads[tid as usize].priority;
            let quantum_over = self.cfg.policy.is_preemptive() && slice >= self.cfg.quantum;
            if !yielded && !quantum_over {
                continue;
            }
            // Under priority, only an equally or more urgent thread is next.
            let ceiling = (self.cfg.policy == SchedPolicy::Priority).then_some(priority);
            if self.pick_next(ceiling).is_none() {
                // Nobody to hand over to: start a fresh quantum.
                self.cores[core] = CoreState::Running { tid, slice: 0 };
                continue;
            }
            let reason = if yielded {
                LeaveReason::Yield
            } else {
                LeaveReason::Quantum
            };
            self.vacate(core, tid, reason, &mut switches);
            if let Some(next) = self.pick_next(ceiling) {
                self.dispatch(core, next, &mut switches);
            }
        }

        // Priority: a waiting thread more urgent than a running one takes
        // that one's core, least urgent first.
        if self.cfg.policy == SchedPolicy::Priority {
            while let Some(best) = self.pick_next(None) {
                let best_priority = self.threads[best as usize].priority;
                let victim = (0..self.cores.len())
                    .filter_map(|core| match self.cores[core] {
                        CoreState::Running { tid, .. } => {
                            Some((self.threads[tid as usize].priority, core, tid))
                        }
                        _ => None,
                    })
                    .max_by_key(|&(priority, core, _)| (priority, core));
                let Some((victim_priority, core, victim)) = victim else {
                    break;
                };
                if victim_priority <= best_priority {
                    break;
                }
                self.vacate(core, victim, LeaveReason::Preempt, &mut switches);
                self.dispatch(core, best, &mut switches);
            }
        }

        switches
    }

    /// Where the next thread comes from: the front of the queue, or under
    /// priority the most urgent thread, first come first among equals.
    /// `ceiling` limits the choice to priorities at least that urgent.
    fn pick_next(&self, ceiling: Option<u32>) -> Option<Tid> {
        let eligible = |tid: &Tid| {
            ceiling.is_none_or(|c| self.threads[*tid as usize].priority <= c)
        };
        if self.cfg.policy == SchedPolicy::Priority {
            self.ready
                .iter()
                .copied()
                .filter(eligible)
                .min_by_key(|tid| self.threads[*tid as usize].priority)
        } else {
            self.ready.iter().copied().find(eligible)
        }
    }

    /// Take `tid` off `core`. A thread that can run again goes to the back of
    /// the ready queue.
    fn vacate(&mut self, core: usize, tid: Tid, reason: LeaveReason, switches: &mut Vec<Switch>) {
        self.cores[core] = CoreState::Idle;
        let thread = &mut self.threads[tid as usize];
        match reason {
            LeaveReason::Exit | LeaveReason::Block => {}
            LeaveReason::Yield => {
                thread.state = ThreadState::Ready;
                self.ready.push_back(tid);
            }
            LeaveReason::Quantum | LeaveReason::Preempt => {
                thread.state = ThreadState::Ready;
                thread.stats.preemptions += 1;
                self.ready.push_back(tid);
            }
        }
        self.log(tid, SchedEventKind::Leave { core, reason });
        Self::switch_slot(switches, core).out = Some((tid, reason));
    }

    /// Put `tid` on `core`, through a context switch when it costs ticks.
    fn dispatch(&mut self, core: usize, tid: Tid, switches: &mut Vec<Switch>) {
        self.ready.retain(|&t| t != tid);
        let cost = self.cfg.switch_cost;
        let thread = &mut self.threads[tid as usize];
        thread.state = ThreadState::Running(core);
        thread.last_core = Some(core);
        thread.stats.dispatches += 1;
        if cost == 0 {
            thread.stats.first_run.get_or_insert(self.now);
            self.cores[core] = CoreState::Running { tid, slice: 0 };
        } else {
            self.cores[core] = CoreState::Switching { tid, left: cost };
        }
        self.context_switches += 1;
        self.log(tid, SchedEventKind::Dispatch { core });
        Self::switch_slot(switches, core).inn = Some(tid);
    }

    /// The entry for `core` in this tick's switches. A core can lose a thread
    /// and gain another in one tick, so the two halves share an entry.
    fn switch_slot(switches: &mut Vec<Switch>, core: usize) -> &mut Switch {
        match switches.iter().position(|s| s.core == core) {
            Some(at) => &mut switches[at],
            None => {
                switches.push(Switch {
                    core,
                    out: None,
                    inn: None,
                });
                switches.last_mut().expect("just pushed")
            }
        }
    }

    /// Charge the tick that just ran to every core and thread.
    fn account_tick(&mut self) {
        let now = self.now;
        for core in 0..self.cores.len() {
            let lane = match &mut self.cores[core] {
                CoreState::Idle => {
                    self.core_ticks[core].idle += 1;
                    Lane::Idle
                }
                CoreState::Switching { tid, .. } => {
                    self.core_ticks[core].switching += 1;
                    Lane::Switch(*tid)
                }
                CoreState::Running { tid, slice } => {
                    *slice = slice.saturating_add(1);
                    self.core_ticks[core].busy += 1;
                    let tid = *tid;
                    self.threads[tid as usize].stats.run_ticks += 1;
                    Lane::Run(tid)
                }
            };
            let line = &mut self.timeline[core];
            match line.back_mut() {
                Some(last) if last.lane == lane && last.end == now => last.end = now + 1,
                _ => {
                    line.push_back(Segment {
                        lane,
                        start: now,
                        end: now + 1,
                    });
                    if line.len() > TIMELINE_CAP {
                        line.pop_front();
                    }
                }
            }
        }
        for thread in &mut self.threads {
            // A thread that blocked during the tick spent it running.
            if self.cores.iter().any(|c| c.tid() == Some(thread.tid)) {
                continue;
            }
            match thread.state {
                ThreadState::Ready => thread.stats.ready_ticks += 1,
                ThreadState::Blocked(_) => thread.stats.blocked_ticks += 1,
                ThreadState::Running(_) | ThreadState::Exited => {}
            }
        }
    }

    fn log(&mut self, tid: Tid, kind: SchedEventKind) {
        self.events.push_back(SchedEvent {
            tick: self.now,
            tid,
            kind,
        });
        if self.events.len() > EVENT_CAP {
            self.events.pop_front();
        }
    }

    pub fn summary(&self) -> SchedSummary {
        let n = self.threads.len();
        let mean = |values: Vec<u64>| {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<u64>() as f64 / values.len() as f64
            }
        };
        let finished: Vec<u64> = self
            .threads
            .iter()
            .filter_map(|t| t.stats.turnaround())
            .collect();
        let core_ticks: u64 = self
            .core_ticks
            .iter()
            .map(|c| c.busy + c.switching + c.idle)
            .sum();
        let busy: u64 = self.core_ticks.iter().map(|c| c.busy).sum();
        SchedSummary {
            threads: n,
            finished: finished.len(),
            ticks: self.now,
            context_switches: self.context_switches,
            preemptions: self.threads.iter().map(|t| t.stats.preemptions).sum(),
            avg_waiting: mean(self.threads.iter().map(|t| t.stats.ready_ticks).collect()),
            avg_response: mean(
                self.threads
                    .iter()
                    .filter_map(|t| t.stats.response())
                    .collect(),
            ),
            avg_turnaround: mean(finished),
            utilization: if core_ticks == 0 {
                0.0
            } else {
                busy as f64 / core_ticks as f64
            },
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/falcon_sched.rs"]
mod tests;
