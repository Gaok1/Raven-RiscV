# Thread scheduling

> [Leia em Português](../pt-BR/threads.md)

Without a scheduler, every hart a program starts takes a free core and keeps it until it exits, and `hart_start` fails once every core is taken. That is the model [Hardware threads](hart.md) describes, and it is still the default (`sched=off`).

Turn a scheduler on and the harts become **threads** that share the cores. A program can start more threads than there are cores: a thread that finds no free core waits in the **ready queue**, and the scheduler decides which thread each core runs. With one core and three threads you can watch FIFO, round robin and priority scheduling play out instruction by instruction, and read off the waiting, response and turnaround times a scheduling course asks for.

---

## 1. Policies

| `sched` | Policy | A running thread leaves its core when… |
|---|---|---|
| `off` | none (default) | it exits. One hart per core. |
| `fifo` | first come, first served | it exits, blocks in `thread_join` or calls `thread_yield`. Never preempted. |
| `rr` | round robin | as FIFO, or it has run a whole **quantum** while another thread waits. It goes to the back of the queue. |
| `priority` | preemptive priority | as round robin among threads of equal priority, or a **more urgent** thread becomes ready: that one takes the core at once. A lower number is more urgent. |

Every thread starts with priority `10`, or its parent's if the parent changed its own. Under `priority`, when several cores could be taken, the least urgent running thread loses its core first.

## 2. Ticks, quantum and switch cost

The scheduler counts time in **ticks**. A tick is one round of the simulator: every core with a thread executes one instruction (sequential mode) or one clock cycle (pipeline). At the end of every tick the scheduler:

1. counts where every thread spent the tick: running, waiting in the ready queue, or blocked;
2. takes off its core every thread that exited or blocked;
3. gives every idle core the thread that waited longest (under `priority`: the most urgent);
4. hands the core of a thread that yielded, or used up its quantum, to the next thread waiting, if any;
5. under `priority`, lets a more urgent waiting thread preempt a less urgent running one.

| Setting | Default | Meaning |
|---|---|---|
| `sched_quantum` | `50` | Ticks a thread runs before round robin hands its core on. Only `rr` and `priority` use it. |
| `sched_switch_cost` | `0` | Ticks a core spends on each context switch, running no thread: saving one thread's registers and loading the next one's. |

A thread started during a tick can take a core at the end of that tick at the earliest. A thread alone on its core is never preempted for nothing: when nobody waits, its quantum just starts over.

With the pipeline, a context switch empties the core's pipeline and refetches from the new thread's PC. Nothing is lost: an instruction changes the machine only when it commits, so the old thread fetches again what was in flight when it runs next.

## 3. Syscalls

| `a7` | Name | Arguments | Returns |
|---|---|---|---|
| `1100` | `hart_start` | `a0` entry PC, `a1` stack top, `a2` argument | thread id, or `-2` (bad PC) / `-3` (bad stack). With a scheduler it never fails for lack of a core. |
| `1101` | `hart_exit` | — | does not return; threads joined on this one wake up |
| `1103` | `thread_yield` | — | `0`. Gives the core to the next thread waiting; keeps running if none waits. |
| `1104` | `thread_join` | `a0` thread id | `0` once that thread has exited; `-1` for an unknown id or the caller's own |
| `1105` | `thread_set_priority` | `a0` thread id, `a1` priority | `0`, or `-1` for an unknown id |

The Linux `sched_yield` (124) is the same request as `thread_yield`. A thread's id is the hart id it reads from `mhartid`; the program's first thread is `0`.

Without a scheduler the three new calls still work: `thread_yield` has no one to hand the core to, `thread_set_priority` only checks its argument, and `thread_join` keeps the hart on its own core, not stepping, until the target exits.

If every thread left is blocked in `thread_join` — A joins B while B joins A — no thread can run again: the run stops with a **deadlock** error naming who waits for whom.

## 4. Example

[`Program Examples/harts/thread_scheduling.s`](../../Program%20Examples/harts/thread_scheduling.s) starts three workers, A, B and C, and joins each. A worker does four rounds of busy work and prints its letter after each round:

```
raven run thread_scheduling.s --cores 1 --sched fifo             # AAAABBBBCCCC
raven run thread_scheduling.s --cores 1 --sched rr --quantum 20  # ABCABCABCABC
raven run thread_scheduling.s --cores 1 --sched rr --quantum 200 # AABBCCAABBCC
raven run thread_scheduling.s --cores 2 --sched rr --quantum 20
raven run thread_scheduling.s --cores 1 --sched rr --quantum 20 --switch-cost 10 --format csv
```

Things to try:

- Shrink the quantum and watch the context switches and the waiting time go up while the response time goes down.
- Raise `--switch-cost`: utilization drops, and with a tiny quantum the cores spend more time switching than running.
- Under `priority`, give one worker priority `1` with `thread_set_priority`: it runs to completion first, then the others share by round robin.
- Replace a `thread_join` with a spin on a flag: with one core, the spinning thread burns its whole quantum every turn. `thread_yield` inside the loop fixes it.

## 5. In the TUI

**Settings → THREAD SCHEDULING** has three rows: `scheduler` (off / FIFO / round robin / priority; changing it restarts the simulation), `quantum` and `switch cost` (these take effect at once). With a scheduler, a step advances every core by one tick, whatever the run scope says.

The **Threads** tab shows the scheduler at work:

- **the averages**: policy, tick, context switches, preemptions, how busy the cores were, and the average waiting, response and turnaround times;
- **CPU**: one card per core with the thread it runs, a bar for how much of its quantum is gone, its PC, and which thread had the core before (and why it left). A core switching shows the switch cost being paid;
- **READY QUEUE**: one card per waiting thread, next first, with the PC and stack pointer it saved, how long it has waited and why it left its core (`quantum`, `yield`, `forced`, `new`…). Blocked and finished threads are listed under it;
- **timeline**: one row per core, every tick so far squeezed into the width, each column showing the thread that ran (`0`-`9`, `a`-`z`), `·` while switching and `─` while idle. The selected thread is highlighted;
- **CONTEXT**: the selected thread's control block (TCB): PC with the instruction there, and all 32 registers — live while it runs on a core, frozen as saved while it waits — with its times and counters, and the four steps of a context switch;
- **threads** and **events**: every thread with its state, wait, response and turnaround, and the latest saves and loads of contexts.

You can drive it by hand:

| Key | Mouse | Action |
|---|---|---|
| `↑` `↓` `←` `→` | click a card or a row | select a thread; its context shows in CONTEXT |
| `x` | `force context switch` | take the selected core from its thread now, as a timer interrupt would: its context is saved and it goes to the back of the queue, and the next thread is loaded |
| `Enter` | `put Tn on core` | put the selected (ready) thread on the selected core now, whoever is first in line |
| `[` `]` | click a core card | select a core |
| `s` / `p` | `step` / `run` | one tick / run and pause |

A forced switch does not ask the policy: under `priority`, a less urgent thread put on a core this way loses it again at the end of the next tick.

The core selector in the Run and Pipeline tabs still picks a **core**. The registers and pipeline it shows belong to whichever thread that core runs now.

## 6. In the CLI and in `.rcfg`

```
raven run program.s --sched rr --quantum 20 --switch-cost 2 --cores 1
```

`--sched`, `--quantum` and `--switch-cost` override the `[sim]` keys:

```ini
[sim]
max_cores=1
sched=rr
sched_quantum=20
sched_switch_cost=2
```

Headless scheduling runs in sequential mode; `--pipeline` together with a scheduler is rejected. `--max-cycles` counts ticks.

Every results format gets the scheduler's books: a `"scheduler"` object in JSON (policy, averages, per-core busy/switching/idle ticks and one entry per thread), a `[sched]` section in rstats (`sched.*`, `coreN.*_ticks` and `threadN.*` keys), and `SCHEDULER`, `CORES` and `THREADS` tables in CSV.

## 7. Metrics

| Metric | Definition |
|---|---|
| arrival | tick the thread was started |
| first run | tick it first ran on a core |
| finish | tick it exited |
| response | first run − arrival |
| turnaround | finish − arrival |
| waiting | ticks spent in the ready queue |
| blocked | ticks spent in `thread_join` |
| dispatches | times it was put on a core |
| preemptions | times the scheduler took its core away (quantum, or a more urgent thread) |
| utilization | share of core-ticks spent running a thread, rather than idle or switching |

The averages run over every thread; average turnaround runs over the threads that finished. When `exit` ends the program, the threads still alive keep the state they had.

---

## See also

- [Hardware threads (harts)](hart.md): starting harts, stacks, `max_cores`
- [Syscall reference](syscalls.md): the full table
- [Cache coherence](coherence.md): what threads on different cores see of each other's writes
