// ui/view/threads.rs — the Threads tab: the scheduler at work, drawn
//
// Top to bottom:
//   - the policy and its averages, then the actions: step, run, force a
//     context switch, put the selected thread on the selected core;
//   - the CPU — one card per core with the thread it runs and how much of its
//     quantum is gone — beside the ready queue, one card per waiting thread
//     with what it saved when it left its core;
//   - a timeline per core, each column a span of ticks coloured by the thread
//     that ran;
//   - the selected thread's context (its TCB: PC and the 32 registers, live
//     on a core or saved while it waits), next to the list of threads and the
//     latest scheduling events.
//
// Everything with a thread on it is clickable and selects that thread.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::falcon::sched::{CoreState, Lane, LeaveReason, SchedEventKind, ThreadState, Tid};
use crate::ui::app::App;
use crate::ui::app::threads::ThreadsHit;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

/// One colour per thread, cycled.
const THREAD_COLORS: [Color; 8] = [
    theme::CACHE_I,
    theme::CACHE_D,
    theme::LABEL_Y,
    theme::DIRTY,
    theme::VA_VPN0,
    theme::CPI_PANEL,
    theme::SPECULATIVE,
    theme::METRIC_CPI,
];

const ABI: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

const CORE_CARD_W: u16 = 30;
const QUEUE_CARD_W: u16 = 20;
const ARROW_W: u16 = 3;

pub(crate) fn thread_color(tid: Tid) -> Color {
    THREAD_COLORS[tid as usize % THREAD_COLORS.len()]
}

/// The glyph a thread takes on the timeline: 0-9, then a-z, then `#`.
fn thread_glyph(tid: Tid) -> char {
    match tid {
        0..=9 => char::from(b'0' + tid as u8),
        10..=35 => char::from(b'a' + (tid - 10) as u8),
        _ => '#',
    }
}

fn hit(app: &App, area: Rect, what: ThreadsHit) {
    app.threads_view.hits.borrow_mut().push((area, what));
}

fn hovered(app: &App, what: ThreadsHit) -> bool {
    app.threads_view.hover == Some(what)
}

/// A card: a rounded box whose border lights up when its thread is the one
/// selected, or when the mouse is on it.
fn card(title: Vec<Span<'static>>, lit: bool, hover: bool) -> Block<'static> {
    let border = if lit {
        theme::ACCENT
    } else if hover {
        theme::BORDER_HOV
    } else {
        theme::BORDER
    };
    let mut spans = vec![Span::styled(if lit { "━" } else { "─" }, Style::default().fg(border))];
    spans.extend(title);
    Block::default()
        .borders(Borders::ALL)
        .border_type(if lit {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(Style::default().fg(border))
        .title(Line::from(spans))
}

fn thread_span(tid: Tid) -> Span<'static> {
    Span::styled(
        format!("T{tid}"),
        Style::default().fg(thread_color(tid)).bold(),
    )
}

fn state_style(state: ThreadState, switching: bool) -> (String, Color) {
    match state {
        ThreadState::Ready => ("READY".into(), theme::PAUSED),
        ThreadState::Running(_) if switching => ("SWITCHING IN".into(), theme::SPECULATIVE),
        ThreadState::Running(_) => ("RUNNING".into(), theme::RUNNING),
        ThreadState::Blocked(on) => (format!("BLOCKED on T{on}"), theme::DANGER),
        ThreadState::Exited => ("EXITED".into(), theme::IDLE),
    }
}

fn reason_text(reason: LeaveReason) -> &'static str {
    match reason {
        LeaveReason::Exit => "it exited",
        LeaveReason::Block => "thread_join",
        LeaveReason::Yield => "thread_yield",
        LeaveReason::Quantum => "quantum over",
        LeaveReason::Preempt => "preempted",
        LeaveReason::Forced => "forced by you",
    }
}

/// A bar of `width` cells, `filled` of them lit.
fn bar(used: u64, total: u64, width: usize, color: Color) -> Vec<Span<'static>> {
    let total = total.max(1);
    let filled = ((used.min(total) * width as u64).div_ceil(total)) as usize;
    vec![
        Span::styled("▰".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "▱".repeat(width.saturating_sub(filled)),
            Style::default().fg(theme::BORDER),
        ),
    ]
}

pub(super) fn render_threads(f: &mut Frame, area: Rect, app: &App) {
    app.threads_view.hits.borrow_mut().clear();
    let inner = render_panel(f, area, panel::panel("Threads", PanelKind::Accent));
    if inner.height == 0 {
        return;
    }
    let Some(sched) = app.scheduler() else {
        render_scheduler_off(f, inner, app);
        return;
    };

    let cores = sched.cores().len() as u16;
    let timeline_h = (cores + 3).min(8);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(9),
            Constraint::Length(timeline_h),
            Constraint::Min(0),
        ])
        .split(inner);

    render_summary(f, rows[0], app);
    render_actions(f, rows[1], app);

    let cpu_w = (cores * CORE_CARD_W + 2)
        .min(rows[2].width / 2)
        .max(CORE_CARD_W + 2);
    let band = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(cpu_w), Constraint::Min(0)])
        .split(rows[2]);
    render_cpu(f, band[0], app);
    render_ready_queue(f, band[1], app);
    render_timeline(f, rows[3], app);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(rows[4]);
    render_context(f, bottom[0], app);
    let side_list_h = (sched.threads().len() as u16 + 3).min(bottom[1].height / 2).max(4);
    let side = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(side_list_h), Constraint::Min(0)])
        .split(bottom[1]);
    render_thread_list(f, side[0], app);
    render_events(f, side[1], app);
}

fn render_scheduler_off(f: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw(" "),
            Span::styled("No scheduler: ", style::title()),
            Span::styled(
                "every hart a program starts takes a free core and keeps it.",
                style::value(),
            ),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            format!(
                " With {} core{}, hart_start fails once they are all taken.",
                app.max_cores,
                if app.max_cores == 1 { "" } else { "s" }
            ),
            style::label(),
        )),
        Line::from(Span::styled(
            " Pick FIFO, round robin or priority in Settings → THREAD SCHEDULING to",
            style::label(),
        )),
        Line::from(Span::styled(
            " run more threads than cores and watch them share the cores here.",
            style::label(),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            " Try Program Examples/harts/thread_scheduling.s with 1 core and round robin.",
            style::label(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn render_summary(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let cfg = sched.config();
    let sum = sched.summary();
    let first = style::readout_row(vec![
        style::readout("policy", cfg.policy.label(), theme::ACCENT),
        style::readout(
            "quantum",
            if cfg.policy.is_preemptive() {
                format!("{} ticks", cfg.quantum)
            } else {
                "— (no preemption)".to_string()
            },
            theme::LABEL_Y,
        ),
        style::readout("switch cost", format!("{} ticks", cfg.switch_cost), theme::LABEL_Y),
        style::readout("tick", sum.ticks, theme::METRIC_CYC),
        style::readout("threads", format!("{}/{} done", sum.finished, sum.threads), theme::TEXT),
    ]);
    let second = style::readout_row(vec![
        style::readout("context switches", sum.context_switches, theme::METRIC_CPI),
        style::readout("preemptions", sum.preemptions, theme::METRIC_CPI),
        style::readout(
            "CPU busy",
            format!("{:.1}%", sum.utilization * 100.0),
            theme::RUNNING,
        ),
        style::readout("avg wait", format!("{:.1}", sum.avg_waiting), theme::METRIC_IPC),
        style::readout("avg response", format!("{:.1}", sum.avg_response), theme::METRIC_IPC),
        style::readout(
            "avg turnaround",
            format!("{:.1}", sum.avg_turnaround),
            theme::METRIC_IPC,
        ),
    ]);
    f.render_widget(Paragraph::new(vec![first, second]), area);
}

/// The buttons, then what the last one did.
fn render_actions(f: &mut Frame, area: Rect, app: &App) {
    let core = app.selected_core;
    let selected = app.threads_selected_tid();
    let ready_pick = selected.filter(|&tid| {
        app.scheduler()
            .and_then(|s| s.thread(tid))
            .is_some_and(|t| t.state == ThreadState::Ready)
    });
    let running = app.session.is_running;
    let mut buttons: Vec<(String, ThreadsHit)> = vec![
        ("s  step".to_string(), ThreadsHit::Step),
        (
            if running { "p  pause" } else { "p  run" }.to_string(),
            ThreadsHit::RunPause,
        ),
        (format!("x  force context switch on core {core}"), ThreadsHit::ForceSwitch),
    ];
    if let Some(tid) = ready_pick {
        buttons.push((format!("enter  put T{tid} on core {core}"), ThreadsHit::Dispatch));
    }
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    for (text, what) in buttons {
        let w = text.chars().count() as u16 + 2;
        hit(app, Rect::new(x, area.y, w, 1), what);
        spans.push(style::button_span(&text, false, hovered(app, what)));
        spans.push(Span::raw("  "));
        x += w + 2;
    }
    if let Some((message, error)) = &app.threads_view.status {
        spans.push(Span::styled(
            message.clone(),
            if *error { style::danger() } else { style::success() },
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// One card per core: who runs there, how much of its quantum is gone, and
/// who had the core before.
fn render_cpu(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let inner = render_panel(f, area, panel::panel("CPU — what each core runs", PanelKind::Plain));
    let cfg = sched.config();
    let fit = (inner.width / CORE_CARD_W).max(1) as usize;
    let selected = app.threads_selected_tid();
    for (core, state) in sched.cores().iter().enumerate().take(fit) {
        let rect = Rect::new(
            inner.x + core as u16 * CORE_CARD_W,
            inner.y,
            CORE_CARD_W.min(inner.width),
            inner.height.min(7),
        );
        let is_sel_core = core == app.selected_core;
        let mut title = vec![Span::styled(
            format!(" core {core} "),
            if is_sel_core {
                Style::default().fg(theme::ACCENT).bold()
            } else {
                style::label()
            },
        )];
        if is_sel_core {
            title.push(Span::styled("◆ selected ", Style::default().fg(theme::ACCENT)));
        }
        let lit = state.tid().is_some() && state.tid() == selected;
        let block = card(title, lit, hovered(app, ThreadsHit::Core(core)));
        let body = block.inner(rect);
        f.render_widget(block, rect);
        hit(app, rect, ThreadsHit::Core(core));

        // Who left this core last.
        let before = sched
            .threads()
            .iter()
            .filter_map(|t| t.last_leave.filter(|l| l.1 == core).map(|l| (l.0, t.tid, l.2)))
            .max_by_key(|l| l.0);
        let mut lines = Vec::new();
        match *state {
            CoreState::Idle => {
                lines.push(Line::from(Span::styled(" idle", style::idle())));
                lines.push(Line::from(Span::styled(
                    " no thread is waiting",
                    style::label(),
                )));
                lines.push(Line::raw(""));
            }
            CoreState::Switching { tid, left } => {
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("⇄ switching to ", Style::default().fg(theme::SPECULATIVE)),
                    thread_span(tid),
                ]));
                let mut spans = vec![Span::styled(" cost ", style::label())];
                spans.extend(bar(
                    u64::from(cfg.switch_cost - left.min(cfg.switch_cost)),
                    u64::from(cfg.switch_cost),
                    12,
                    theme::SPECULATIVE,
                ));
                spans.push(Span::styled(format!(" {left} left"), style::label()));
                lines.push(Line::from(spans));
                lines.push(Line::from(Span::styled(
                    " saving one, loading other",
                    style::label(),
                )));
            }
            CoreState::Running { tid, slice } => {
                let pc = app.thread_pc(tid).unwrap_or(0);
                lines.push(Line::from(vec![
                    Span::raw(" ▶ "),
                    thread_span(tid),
                    Span::styled("  running", Style::default().fg(theme::RUNNING)),
                ]));
                if cfg.policy.is_preemptive() {
                    let mut spans = vec![Span::styled(" quantum ", style::label())];
                    spans.extend(bar(u64::from(slice), u64::from(cfg.quantum), 10, thread_color(tid)));
                    spans.push(Span::styled(
                        format!(" {slice}/{}", cfg.quantum),
                        style::value(),
                    ));
                    lines.push(Line::from(spans));
                } else {
                    lines.push(Line::from(Span::styled(
                        format!(" ran {slice} ticks, no quantum"),
                        style::label(),
                    )));
                }
                lines.push(Line::from(vec![
                    Span::styled(" pc ", style::label()),
                    Span::styled(format!("0x{pc:08X}"), style::value()),
                ]));
            }
        }
        lines.push(match before {
            Some((tick, tid, reason)) => Line::from(vec![
                Span::styled(" before: ", style::label()),
                thread_span(tid),
                Span::styled(format!(" @{tick} ({})", reason.label()), style::label()),
            ]),
            None => Line::from(Span::styled(" before: —", style::label())),
        });
        f.render_widget(Paragraph::new(lines), body);
    }
    let hidden = sched.cores().len().saturating_sub(fit);
    if hidden > 0 && inner.height > 7 {
        f.render_widget(
            Paragraph::new(Span::styled(format!(" +{hidden} more cores"), style::label())),
            Rect::new(inner.x, inner.y + 7, inner.width, 1),
        );
    }
}

/// The ready queue, front first, each card holding what its thread saved
/// when it left a core.
fn render_ready_queue(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let title = if sched.config().policy == crate::falcon::sched::SchedPolicy::Priority {
        "READY QUEUE — the most urgent goes next, first come first among equals"
    } else {
        "READY QUEUE — front goes next"
    };
    let inner = render_panel(f, area, panel::panel(title, PanelKind::Plain));
    let queue: Vec<Tid> = sched.ready_queue().collect();
    let selected = app.threads_selected_tid();
    let now = sched.now();
    let mut x = inner.x;
    let fit = ((inner.width + ARROW_W) / (QUEUE_CARD_W + ARROW_W)).max(1) as usize;
    if queue.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled(
                    " (empty) — every live thread has a core or is blocked",
                    style::label(),
                )),
            ]),
            inner,
        );
    }
    for (pos, &tid) in queue.iter().enumerate().take(fit) {
        let rect = Rect::new(x, inner.y, QUEUE_CARD_W, inner.height.min(7));
        let Some(t) = sched.thread(tid) else {
            continue;
        };
        let ordinal = match pos {
            0 => "next".to_string(),
            n => format!("#{}", n + 1),
        };
        let block = card(
            vec![
                Span::raw(" "),
                thread_span(tid),
                Span::styled(format!(" {ordinal} "), style::label()),
            ],
            selected == Some(tid),
            hovered(app, ThreadsHit::Thread(tid)),
        );
        let body = block.inner(rect);
        f.render_widget(block, rect);
        hit(app, rect, ThreadsHit::Thread(tid));
        let (pc, sp) = app
            .thread_cpu(tid)
            .map_or((0, 0), |(cpu, _)| (cpu.pc, cpu.x[2]));
        let waited = t
            .last_leave
            .map(|l| l.0)
            .unwrap_or(t.stats.arrival);
        let why = match t.last_leave {
            Some((_, _, reason)) => reason.label().to_string(),
            None if t.stats.dispatches == 0 => "new".to_string(),
            None => "woken".to_string(),
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("pc ", style::label()),
                Span::styled(format!("{pc:08X}"), style::value()),
            ]),
            Line::from(vec![
                Span::styled("sp ", style::label()),
                Span::styled(format!("{sp:08X}"), style::value()),
            ]),
            Line::from(vec![
                Span::styled("waits ", style::label()),
                Span::styled(
                    format!("{}", now.saturating_sub(waited)),
                    Style::default().fg(theme::PAUSED),
                ),
                Span::styled(" ticks", style::label()),
            ]),
            Line::from(vec![
                Span::styled(format!("prio {} · ", t.priority), style::label()),
                Span::styled(why, style::label()),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), body);
        x += QUEUE_CARD_W;
        if pos + 1 < queue.len().min(fit) {
            f.render_widget(
                Paragraph::new(Span::styled(" → ", Style::default().fg(theme::LABEL))),
                Rect::new(x, inner.y + 3, ARROW_W, 1),
            );
        }
        x += ARROW_W;
    }
    if queue.len() > fit {
        f.render_widget(
            Paragraph::new(Span::styled(
                format!("+{}", queue.len() - fit),
                style::label(),
            )),
            Rect::new(
                (x.saturating_sub(ARROW_W)).min(inner.x + inner.width.saturating_sub(4)),
                inner.y + 3,
                4,
                1,
            ),
        );
    }
    // Who cannot be in the queue: blocked and finished threads.
    if inner.height > 7 {
        let mut spans = vec![Span::styled(" blocked: ", style::label())];
        let blocked: Vec<_> = sched
            .threads()
            .iter()
            .filter_map(|t| match t.state {
                ThreadState::Blocked(on) => Some((t.tid, on)),
                _ => None,
            })
            .collect();
        if blocked.is_empty() {
            spans.push(Span::styled("none", style::label()));
        }
        for (tid, on) in blocked {
            spans.push(thread_span(tid));
            spans.push(Span::styled(format!(" waits T{on}  "), style::label()));
        }
        spans.push(Span::styled("   finished: ", style::label()));
        let done: Vec<_> = sched
            .threads()
            .iter()
            .filter(|t| t.state == ThreadState::Exited)
            .map(|t| t.tid)
            .collect();
        if done.is_empty() {
            spans.push(Span::styled("none", style::label()));
        }
        for tid in done {
            spans.push(thread_span(tid));
            spans.push(Span::raw(" "));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, inner.y + 7, inner.width, 1),
        );
    }
}

fn render_timeline(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let now = sched.now();
    let label_w = 6u16;
    let inner = render_panel(f, area, panel::panel("timeline — who ran on each core", PanelKind::Plain));
    let cols = inner.width.saturating_sub(label_w + 1).max(1) as u64;
    // Every tick so far, squeezed into the width: each column is a span of
    // ticks, shown as whatever held the core at the span's start.
    let per_col = now.div_ceil(cols).max(1);
    let selected = app.threads_selected_tid();
    let mut lines = Vec::new();
    for core in 0..sched.cores().len().min(inner.height.saturating_sub(1) as usize) {
        let segments: Vec<_> = sched.timeline(core).copied().collect();
        let mut spans = vec![Span::styled(
            format!(" C{core:<3} "),
            if core == app.selected_core {
                Style::default().fg(theme::ACCENT).bold()
            } else {
                style::label()
            },
        )];
        let mut seg_at = 0usize;
        let shown = now.div_ceil(per_col);
        for col in 0..shown {
            let tick = col * per_col;
            while seg_at < segments.len() && segments[seg_at].end <= tick {
                seg_at += 1;
            }
            let lane = segments
                .get(seg_at)
                .filter(|s| s.start <= tick)
                .map(|s| s.lane);
            let (glyph, mut style) = match lane {
                Some(Lane::Run(tid)) => (thread_glyph(tid), Style::default().fg(thread_color(tid))),
                Some(Lane::Switch(_)) => ('·', Style::default().fg(theme::SPECULATIVE)),
                Some(Lane::Idle) => ('─', Style::default().fg(theme::BORDER)),
                // Older than what the timeline keeps.
                None => (' ', Style::default()),
            };
            // The selected thread stands out.
            if matches!(lane, Some(Lane::Run(tid)) if Some(tid) == selected) {
                style = style.bg(theme::BG_RAISED).bold();
            }
            spans.push(Span::styled(glyph.to_string(), style));
        }
        spans.push(Span::styled("▏now", style::label()));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(vec![
        Span::styled(format!(" {:<w$}", "", w = label_w as usize), style::label()),
        Span::styled(
            format!(
                "0 … {now}   {per_col} tick{} per column · digit/letter = thread · `·` switching · `─` idle",
                if per_col == 1 { "" } else { "s" }
            ),
            style::label(),
        ),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

/// The selected thread's context: what the scheduler saves when it leaves a
/// core and loads back when it returns.
fn render_context(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let Some(tid) = app.threads_selected_tid() else {
        return;
    };
    let Some(t) = sched.thread(tid) else {
        return;
    };
    let switching = matches!(
        sched.core_of(tid).map(|c| sched.cores()[c]),
        Some(CoreState::Switching { .. })
    );
    let (state, state_color) = state_style(t.state, switching);
    let title = format!("CONTEXT of T{tid} — the thread control block (↑/↓ picks another)");
    let inner = render_panel(f, area, panel::panel(title, PanelKind::Plain));
    let Some((cpu, live)) = app.thread_cpu(tid) else {
        return;
    };

    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        thread_span(tid),
        Span::raw("  "),
        Span::styled(state, Style::default().fg(state_color).bold()),
        Span::styled(
            format!(
                "   priority {}   parent {}   started at {:#010x}",
                t.priority,
                t.parent.map_or("—".to_string(), |p| format!("T{p}")),
                t.entry_pc
            ),
            style::label(),
        ),
    ])];

    // What this context is right now, in words.
    let explain = match t.state {
        ThreadState::Running(core) if live => format!(
            "On core {core}: these registers are in the core itself, changing every tick. When T{tid} leaves the core they are copied here."
        ),
        ThreadState::Running(core) => format!("Being loaded into core {core}."),
        ThreadState::Ready => match t.last_leave {
            Some((tick, core, reason)) => format!(
                "Saved at tick {tick}, when it left core {core} ({}). Frozen while it waits; loaded back as is when it gets a core, and it resumes at pc.",
                reason_text(reason)
            ),
            None => "Never ran yet: this is the context hart_start built — entry pc, its stack in sp, its argument in a0.".to_string(),
        },
        ThreadState::Blocked(on) => format!(
            "Saved when it called thread_join(T{on}). It cannot be dispatched until T{on} exits; then it goes to the ready queue."
        ),
        ThreadState::Exited => "Finished: the registers as they were at hart_exit.".to_string(),
    };
    for chunk in wrap(&explain, inner.width.saturating_sub(2) as usize) {
        lines.push(Line::from(Span::styled(format!(" {chunk}"), style::idle())));
    }

    let disasm = app
        .disassemble_at(u64::from(cpu.pc))
        .unwrap_or_else(|| "?".to_string());
    lines.push(Line::from(vec![
        Span::styled(" pc ", style::label()),
        Span::styled(
            format!("0x{:08X}", cpu.pc),
            Style::default().fg(theme::LABEL_Y).bold(),
        ),
        Span::styled(
            if live { "   executing  " } else { "   resumes at  " },
            style::label(),
        ),
        Span::styled(disasm, style::value()),
    ]));

    // The 32 registers, four columns of eight.
    let col_w = (inner.width.saturating_sub(2) / 4).max(18) as usize;
    for row in 0..8 {
        let mut spans = vec![Span::raw(" ")];
        for col in 0..4 {
            let r = col * 8 + row;
            let v = cpu.x[r];
            let name = format!("{:<4}", ABI[r]);
            let value_style = if v == 0 {
                Style::default().fg(theme::BORDER)
            } else if matches!(r, 1 | 2 | 10) {
                Style::default().fg(thread_color(tid)).bold()
            } else {
                style::value()
            };
            let cell = format!("{name} {v:08X}");
            let pad = col_w.saturating_sub(cell.chars().count());
            spans.push(Span::styled(name, style::label()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("{v:08X}"), value_style));
            spans.push(Span::raw(" ".repeat(pad)));
        }
        lines.push(Line::from(spans));
    }

    let st = &t.stats;
    let opt = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" times  ", style::label()),
        Span::styled(
            format!(
                "arrived {} · first ran {} · finished {} · response {} · turnaround {}",
                st.arrival,
                opt(st.first_run),
                opt(st.finish),
                opt(st.response()),
                opt(st.turnaround())
            ),
            style::value(),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" ticks  ", style::label()),
        Span::styled(format!("running {}", st.run_ticks), Style::default().fg(theme::RUNNING)),
        Span::styled(" · ", style::label()),
        Span::styled(format!("waiting {}", st.ready_ticks), Style::default().fg(theme::PAUSED)),
        Span::styled(" · ", style::label()),
        Span::styled(format!("blocked {}", st.blocked_ticks), Style::default().fg(theme::DANGER)),
        Span::styled(
            format!(
                "   dispatched {}× · preempted {}× · yielded {}×",
                st.dispatches, st.preemptions, st.yields
            ),
            style::label(),
        ),
    ]));

    // What a context switch does, when there is room to say it.
    let steps = [
        "What a context switch does:",
        "1  save the leaving thread's pc and registers here, in its TCB",
        "2  put it at the back of the ready queue (or mark it blocked / exited)",
        "3  load the next thread's TCB into the core's registers",
        "4  empty the pipeline and fetch again from the new pc",
        "   (switch cost: ticks the core spends on 1-4, running no thread)",
    ];
    if inner.height as usize >= lines.len() + steps.len() + 1 {
        lines.push(Line::raw(""));
        for (i, step) in steps.iter().enumerate() {
            lines.push(Line::from(Span::styled(
                format!(" {step}"),
                if i == 0 { style::label() } else { style::idle() },
            )));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_thread_list(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let inner = render_panel(f, area, panel::panel("threads", PanelKind::Plain));
    let selected = app.threads_selected_tid();
    let mut lines = vec![Line::from(Span::styled(
        format!(
            " {:<4} {:<15} {:>4} {:>4} {:>6} {:>6} {:>6}",
            "TID", "STATE", "CORE", "PRIO", "WAIT", "RESP", "TURN"
        ),
        style::label(),
    ))];
    let room = inner.height.saturating_sub(1) as usize;
    let threads = sched.threads();
    // Keep the selected thread in view.
    let sel_at = selected.map_or(0, |t| t as usize);
    let skip = sel_at.saturating_sub(room.saturating_sub(1)).min(threads.len().saturating_sub(room));
    for (i, t) in threads.iter().enumerate().skip(skip).take(room) {
        let switching = matches!(
            sched.core_of(t.tid).map(|c| sched.cores()[c]),
            Some(CoreState::Switching { .. })
        );
        let (state, color) = state_style(t.state, switching);
        let core = match t.state {
            ThreadState::Running(c) => c.to_string(),
            _ => "—".to_string(),
        };
        let opt = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
        let y = inner.y + 1 + (i - skip) as u16;
        hit(app, Rect::new(inner.x, y, inner.width, 1), ThreadsHit::Thread(t.tid));
        let row_style = if selected == Some(t.tid) {
            Style::default().bg(theme::SEL_ROW_BG)
        } else if hovered(app, ThreadsHit::Thread(t.tid)) {
            Style::default().bg(theme::BG_HOVER)
        } else {
            Style::default()
        };
        lines.push(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("{:<4}", format!("T{}", t.tid)),
                    Style::default().fg(thread_color(t.tid)).bold(),
                ),
                Span::raw(" "),
                Span::styled(format!("{state:<15}"), Style::default().fg(color)),
                Span::styled(
                    format!(
                        " {core:>4} {:>4} {:>6} {:>6} {:>6}",
                        t.priority,
                        t.stats.ready_ticks,
                        opt(t.stats.response()),
                        opt(t.stats.turnaround()),
                    ),
                    style::value(),
                ),
            ])
            .style(row_style),
        );
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_events(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let inner = render_panel(f, area, panel::panel("events", PanelKind::Plain));
    let room = inner.height as usize;
    let mut lines: Vec<Line> = sched
        .events()
        .rev()
        .take(room)
        .map(|e| {
            let what = match e.kind {
                SchedEventKind::Spawn { parent: Some(p) } => format!("created by T{p} → ready queue"),
                SchedEventKind::Spawn { parent: None } => "created (the program)".to_string(),
                SchedEventKind::Dispatch { core } => format!("context loaded → core {core}"),
                SchedEventKind::Leave { core, reason } => {
                    format!("context saved ← core {core} ({})", reason_text(reason))
                }
                SchedEventKind::Wake => "woken: the thread it joined exited".to_string(),
                SchedEventKind::Priority { priority } => format!("priority = {priority}"),
            };
            Line::from(vec![
                Span::styled(format!(" {:>7}  ", e.tick), style::label()),
                Span::styled(
                    format!("T{:<3} ", e.tid),
                    Style::default().fg(thread_color(e.tid)).bold(),
                ),
                Span::styled(what, style::value()),
            ])
        })
        .collect();
    lines.reverse();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Break `text` into lines of at most `width` columns, at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::falcon::sched::SchedPolicy;
    use crate::ui::app::App;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 44)).unwrap();
        terminal
            .draw(|f| super::render_threads(f, Rect::new(0, 0, 160, 44), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn loaded(policy: SchedPolicy) -> App {
        let mut app = App::new(None);
        app.max_cores = 1;
        app.sched_config.policy = policy;
        app.sched_config.quantum = 4;
        app.editor.buf.lines = [
            ".text",
            "_start:",
            "li t5, -8192",
            "add a1, sp, t5",
            "la a0, worker",
            "li a2, 77",
            "li a7, 1100",
            "ecall",
            "spin:",
            "j spin",
            "worker:",
            "j worker",
        ]
        .iter()
        .map(|l| l.to_string())
        .collect();
        app.assemble_and_load();
        app
    }

    #[test]
    fn the_tab_draws_the_cpu_the_queue_the_timeline_and_a_context() {
        let mut app = loaded(SchedPolicy::RoundRobin);
        for _ in 0..60 {
            app.single_step();
        }
        let text = screen(&app);
        assert!(text.contains("Round robin"), "{text}");
        assert!(text.contains("core 0"), "{text}");
        assert!(text.contains("READY QUEUE"), "{text}");
        assert!(text.contains("timeline"), "{text}");
        assert!(text.contains("CONTEXT of T"), "{text}");
        assert!(text.contains("force context switch on core 0"), "{text}");
        assert!(text.contains("context saved ← core 0 (quantum over)"), "{text}");
        assert!(!app.threads_view.hits.borrow().is_empty());
    }

    #[test]
    fn a_waiting_thread_shows_the_context_hart_start_built() {
        // FIFO: the worker never runs while main spins, so it sits in the
        // queue with the registers hart_start gave it.
        let mut app = loaded(SchedPolicy::Fifo);
        for _ in 0..20 {
            app.single_step();
        }
        app.select_thread(1);
        let text = screen(&app);
        assert!(text.contains("Never ran yet"), "{text}");
        // a0 = 77 = 0x4D, the argument.
        assert!(text.contains("0000004D"), "{text}");
    }

    #[test]
    fn forcing_a_switch_puts_the_waiting_thread_on_the_core() {
        let mut app = loaded(SchedPolicy::Fifo);
        for _ in 0..20 {
            app.single_step();
        }
        let main_pc = app.thread_pc(0).unwrap();
        app.force_context_switch();
        let sched = app.scheduler().unwrap();
        assert_eq!(sched.running_on(0), Some(1), "{:?}", app.threads_view.status);
        assert_eq!(sched.ready_queue().collect::<Vec<_>>(), vec![0]);
        // Main's context waits in the queue exactly as it left the core.
        assert_eq!(app.thread_pc(0), Some(main_pc));
        let (cpu, live) = app.thread_cpu(0).unwrap();
        assert!(!live);
        assert_eq!(cpu.pc, main_pc);
        // The worker runs from its entry with its argument.
        assert_eq!(app.thread_cpu(1).unwrap().0.x[10], 77);
        let text = screen(&app);
        assert!(text.contains("forced by you"), "{text}");

        // Enter on main puts it back.
        app.select_thread(0);
        app.dispatch_selected_thread();
        assert_eq!(app.scheduler().unwrap().running_on(0), Some(0));
        assert_eq!(app.thread_pc(0), Some(main_pc));
        assert!(app.thread_cpu(0).unwrap().1, "live on the core again");
    }

    #[test]
    fn without_a_scheduler_the_tab_says_how_to_turn_one_on() {
        let app = App::new(None);
        let text = screen(&app);
        assert!(text.contains("No scheduler"), "{text}");
        assert!(text.contains("THREAD SCHEDULING"), "{text}");
    }
}
