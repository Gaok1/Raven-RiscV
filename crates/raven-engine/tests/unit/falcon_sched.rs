use super::*;

/// A host whose context for a thread is just the thread's id, so every
/// switch can be checked to move the right one.
struct Host {
    sched: Scheduler<Tid>,
    cores: Vec<Option<Tid>>,
}

impl Host {
    fn new(policy: SchedPolicy, quantum: u32, switch_cost: u32, cores: usize) -> Self {
        let cfg = SchedConfig {
            policy,
            quantum,
            switch_cost,
        };
        let mut on_core = vec![None; cores];
        on_core[0] = Some(0);
        Self {
            sched: Scheduler::new(cfg, cores, 0x100),
            cores: on_core,
        }
    }

    fn spawn(&mut self, parent: Tid) -> Tid {
        let tid = self.sched.next_tid();
        self.sched.spawn(Some(parent), 0x200, tid)
    }

    /// End the tick and move the contexts the way a host must: every `out`
    /// before any `inn`.
    fn tick(&mut self) -> Vec<Switch> {
        // Every running thread retires an instruction per tick, as in
        // sequential mode.
        for core in 0..self.cores.len() {
            if self.sched.running_on(core).is_some() {
                self.sched.retired(core);
            }
        }
        let switches = self.sched.end_tick();
        for s in &switches {
            if let Some((tid, _)) = s.out {
                let ctx = self.cores[s.core].take().expect("a context on the core");
                assert_eq!(ctx, tid, "core {} held another thread's context", s.core);
                self.sched.park(tid, ctx);
            }
        }
        for s in &switches {
            if let Some(tid) = s.inn {
                assert!(self.cores[s.core].is_none(), "core {} was not emptied", s.core);
                let ctx = self.sched.unpark(tid).expect("a parked context");
                assert_eq!(ctx, tid);
                self.cores[s.core] = Some(ctx);
            }
        }
        for core in 0..self.cores.len() {
            assert_eq!(self.cores[core], self.sched.thread_on(core), "core {core}");
        }
        switches
    }

    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn state(&self, tid: Tid) -> ThreadState {
        self.sched.thread(tid).unwrap().state
    }
}

#[test]
fn fifo_runs_each_thread_to_completion_in_arrival_order() {
    let mut h = Host::new(SchedPolicy::Fifo, 3, 0, 1);
    let a = h.spawn(0);
    let b = h.spawn(0);
    h.ticks(100);
    // No quantum under FIFO: the first thread keeps the only core.
    assert_eq!(h.sched.running_on(0), Some(0));
    assert_eq!(h.state(a), ThreadState::Ready);

    h.sched.exit(0);
    let switches = h.tick();
    assert_eq!(
        switches,
        vec![Switch {
            core: 0,
            out: Some((0, LeaveReason::Exit)),
            inn: Some(a),
        }]
    );
    h.ticks(10);
    assert_eq!(h.sched.running_on(0), Some(a));
    assert_eq!(h.state(b), ThreadState::Ready);
    assert_eq!(h.sched.thread(a).unwrap().stats.preemptions, 0);
}

#[test]
fn round_robin_rotates_on_the_quantum() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 3, 0, 1);
    let a = h.spawn(0);
    // Ticks 0, 1, 2 belong to thread 0; the quantum ends with tick 2.
    h.ticks(2);
    assert_eq!(h.sched.running_on(0), Some(0));
    let switches = h.tick();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Quantum)));
    assert_eq!(switches[0].inn, Some(a));
    h.ticks(3);
    assert_eq!(h.sched.running_on(0), Some(0));
    assert_eq!(h.sched.thread(0).unwrap().stats.preemptions, 1);
    assert_eq!(h.sched.thread(a).unwrap().stats.preemptions, 1);
    assert_eq!(h.sched.thread(a).unwrap().stats.first_run, Some(3));
    assert_eq!(h.sched.thread(a).unwrap().stats.response(), Some(3));
}

#[test]
fn round_robin_keeps_a_lone_thread_running() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 2, 0, 1);
    for _ in 0..20 {
        assert!(h.tick().is_empty());
    }
    assert_eq!(h.sched.context_switches(), 0);
}

#[test]
fn yield_hands_the_core_over_only_when_someone_waits() {
    let mut h = Host::new(SchedPolicy::Fifo, 50, 0, 1);
    h.sched.yield_now(0);
    assert!(h.tick().is_empty(), "nobody waiting: keep running");

    let a = h.spawn(0);
    h.sched.yield_now(0);
    let switches = h.tick();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Yield)));
    assert_eq!(switches[0].inn, Some(a));
    assert_eq!(h.state(0), ThreadState::Ready);
    assert_eq!(h.sched.thread(0).unwrap().stats.yields, 2);
    assert_eq!(h.sched.thread(0).unwrap().stats.preemptions, 0);
}

#[test]
fn join_blocks_until_the_target_exits() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 100, 0, 1);
    let a = h.spawn(0);
    assert_eq!(h.sched.join(0, 0), JoinOutcome::Invalid, "joining itself");
    assert_eq!(h.sched.join(0, 99), JoinOutcome::Invalid, "no such thread");
    assert_eq!(h.sched.join(0, a), JoinOutcome::Wait);
    let switches = h.tick();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Block)));
    assert_eq!(switches[0].inn, Some(a));
    assert_eq!(h.state(0), ThreadState::Blocked(a));

    h.ticks(4);
    assert_eq!(h.sched.thread(0).unwrap().stats.blocked_ticks, 4);
    h.sched.exit(0);
    assert_eq!(h.state(0), ThreadState::Ready, "woken by the exit");
    let switches = h.tick();
    assert_eq!(switches[0].out, Some((a, LeaveReason::Exit)));
    assert_eq!(switches[0].inn, Some(0));
    assert_eq!(h.sched.join(0, a), JoinOutcome::Done);
}

#[test]
fn threads_joined_on_each_other_deadlock() {
    let mut h = Host::new(SchedPolicy::Fifo, 10, 0, 2);
    let a = h.spawn(0);
    h.tick();
    assert_eq!(h.sched.running_on(1), Some(a));
    assert_eq!(h.sched.join(0, a), JoinOutcome::Wait);
    assert_eq!(h.sched.join(1, 0), JoinOutcome::Wait);
    h.tick();
    assert!(h.sched.is_deadlocked());
    assert!(h.sched.has_live_threads());
    assert!(!h.sched.has_runnable_threads());
}

#[test]
fn a_more_urgent_thread_preempts_at_once() {
    let mut h = Host::new(SchedPolicy::Priority, 1000, 0, 1);
    let low = h.spawn(0);
    h.sched.set_priority(low, 20);
    h.ticks(5);
    assert_eq!(h.sched.running_on(0), Some(0), "less urgent: waits");

    let high = h.spawn(0);
    h.sched.set_priority(high, 1);
    let switches = h.tick();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Preempt)));
    assert_eq!(switches[0].inn, Some(high));

    h.sched.exit(0);
    h.tick();
    // Thread 0 (priority 10) runs again before `low` (20).
    assert_eq!(h.sched.running_on(0), Some(0));
    h.sched.exit(0);
    h.tick();
    assert_eq!(h.sched.running_on(0), Some(low));
}

#[test]
fn equal_priorities_share_by_round_robin() {
    let mut h = Host::new(SchedPolicy::Priority, 2, 0, 1);
    let a = h.spawn(0);
    let low = h.spawn(0);
    h.sched.set_priority(low, 50);
    h.ticks(2);
    assert_eq!(h.sched.running_on(0), Some(a));
    h.ticks(2);
    assert_eq!(h.sched.running_on(0), Some(0));
    assert_eq!(h.sched.thread(low).unwrap().stats.dispatches, 0);
}

#[test]
fn a_child_inherits_its_parents_priority() {
    let mut h = Host::new(SchedPolicy::Priority, 10, 0, 1);
    h.sched.set_priority(0, 3);
    let a = h.spawn(0);
    assert_eq!(h.sched.thread(a).unwrap().priority, 3);
}

#[test]
fn idle_cores_take_new_threads_first() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 4, 0, 2);
    let a = h.spawn(0);
    let switches = h.tick();
    assert_eq!(
        switches,
        vec![Switch {
            core: 1,
            out: None,
            inn: Some(a),
        }]
    );
    let b = h.spawn(a);
    // Thread 0's quantum ends with tick 3: `b` takes core 0 and thread 0
    // waits, though `a` is still on core 1.
    h.ticks(3);
    assert_eq!(h.sched.running_on(0), Some(b));
    assert_eq!(h.sched.running_on(1), Some(a));
    assert_eq!(h.state(0), ThreadState::Ready);
    // `a`'s quantum ends one tick later, and thread 0 moves to core 1.
    h.tick();
    assert_eq!(h.sched.running_on(1), Some(0));
    assert_eq!(h.sched.thread(0).unwrap().last_core, Some(1));
}

#[test]
fn a_switch_costs_ticks_and_shows_on_the_timeline() {
    let mut h = Host::new(SchedPolicy::Fifo, 10, 2, 1);
    let a = h.spawn(0);
    h.sched.exit(0);
    h.tick(); // tick 0: thread 0 runs and exits.
    assert_eq!(h.sched.cores()[0], CoreState::Switching { tid: a, left: 2 });
    assert_eq!(h.sched.running_on(0), None);
    h.tick(); // tick 1: switching.
    assert_eq!(h.sched.running_on(0), None);
    h.tick(); // tick 2: switching; `a` runs from tick 3.
    assert_eq!(h.sched.running_on(0), Some(a));
    assert_eq!(h.sched.thread(a).unwrap().stats.first_run, Some(3));
    h.tick();
    let lanes: Vec<_> = h.sched.timeline(0).copied().collect();
    assert_eq!(
        lanes,
        vec![
            Segment {
                lane: Lane::Run(0),
                start: 0,
                end: 1,
            },
            Segment {
                lane: Lane::Switch(a),
                start: 1,
                end: 3,
            },
            Segment {
                lane: Lane::Run(a),
                start: 3,
                end: 4,
            },
        ]
    );
    assert_eq!(h.sched.core_ticks()[0].switching, 2);
}

#[test]
fn a_thread_can_move_cores_within_one_tick() {
    // Two cores. Priority preemption can push a thread off one core and onto
    // another in the same tick; the host must park before it loads.
    let mut h = Host::new(SchedPolicy::Priority, 1000, 0, 2);
    let mid = h.spawn(0);
    h.tick();
    h.sched.set_priority(0, 5);
    h.sched.set_priority(mid, 30);
    let worst = h.spawn(0);
    h.sched.set_priority(worst, 40);
    let urgent = h.spawn(0);
    h.sched.set_priority(urgent, 1);
    h.tick();
    assert_eq!(h.sched.running_on(1), Some(urgent));
    assert_eq!(h.sched.running_on(0), Some(0));
    assert_eq!(h.state(mid), ThreadState::Ready);
    assert_eq!(h.state(worst), ThreadState::Ready);
}

#[test]
fn stats_add_up() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 2, 0, 1);
    let a = h.spawn(0);
    h.ticks(6);
    h.sched.exit(0); // whoever holds the core
    h.tick();
    let summary = h.sched.summary();
    assert_eq!(summary.ticks, 7);
    assert_eq!(summary.threads, 2);
    let t0 = h.sched.thread(0).unwrap().stats;
    let ta = h.sched.thread(a).unwrap().stats;
    // One core, never idle: every tick ran exactly one thread.
    assert_eq!(t0.run_ticks + ta.run_ticks, 7);
    assert_eq!(t0.run_ticks + t0.ready_ticks, 7);
    assert!((summary.utilization - 1.0).abs() < 1e-9);
    assert_eq!(summary.finished, 1);
}

#[test]
fn config_round_trips_through_rcfg() {
    let cfg = SchedConfig {
        policy: SchedPolicy::Priority,
        quantum: 7,
        switch_cost: 3,
    };
    let text = cfg.to_rcfg();
    let map: std::collections::HashMap<String, String> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let back = SchedConfig::from_rcfg_lookup(|k| map.get(k).cloned()).unwrap();
    assert_eq!(back, cfg);

    assert!(SchedConfig::from_rcfg_lookup(|k| (k == "sched").then(|| "lottery".into())).is_err());
    assert!(
        SchedConfig::from_rcfg_lookup(|k| (k == "sched_quantum").then(|| "0".into())).is_err()
    );
    assert_eq!(
        SchedConfig::from_rcfg_lookup(|_| None).unwrap(),
        SchedConfig::default()
    );
    for policy in SchedPolicy::ALL {
        assert_eq!(SchedPolicy::from_str(policy.as_str()), Some(policy));
    }
}

#[test]
fn a_quantum_never_ends_before_the_thread_retired_anything() {
    // A pipeline needs several ticks before its first commit. A quantum that
    // ran out first would flush every thread forever.
    let mut h = Host::new(SchedPolicy::RoundRobin, 2, 0, 1);
    let a = h.spawn(0);
    for _ in 0..10 {
        assert!(h.sched.end_tick().is_empty(), "nothing retired yet");
    }
    h.sched.retired(0);
    let switches = h.sched.end_tick();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Quantum)));
    assert_eq!(switches[0].inn, Some(a));
}

#[test]
fn a_forced_switch_moves_the_running_thread_to_the_back_of_the_queue() {
    let mut h = Host::new(SchedPolicy::Fifo, 10, 0, 1);
    let a = h.spawn(0);
    let b = h.spawn(0);
    h.tick();
    // Choose `b`, though `a` is first in line.
    let switches = h.sched.force_switch(0, Some(b)).unwrap();
    assert_eq!(switches[0].out, Some((0, LeaveReason::Forced)));
    assert_eq!(switches[0].inn, Some(b));
    assert_eq!(h.sched.ready_queue().collect::<Vec<_>>(), vec![a, 0]);
    let t0 = h.sched.thread(0).unwrap();
    assert_eq!(t0.last_leave, Some((1, 0, LeaveReason::Forced)));
    assert_eq!(t0.stats.preemptions, 1);
    assert_eq!(h.sched.thread(b).unwrap().last_dispatch, Some(1));

    // Without a choice, the policy picks: the front of the queue.
    let switches = h.sched.force_switch(0, None).unwrap();
    assert_eq!(switches[0].inn, Some(a));

    assert!(h.sched.force_switch(0, Some(a)).is_err(), "a is running, not ready");
    assert!(h.sched.force_switch(7, None).is_err());
}

#[test]
fn a_forced_switch_needs_someone_waiting() {
    let mut h = Host::new(SchedPolicy::RoundRobin, 10, 0, 1);
    assert!(h.sched.force_switch(0, None).is_err());
}
