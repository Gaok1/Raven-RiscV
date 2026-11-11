// ui/view/threads.rs — the Threads tab: what the scheduler did, tick by tick
//
// Four blocks, top to bottom: the policy and its averages; one row per thread
// with the times a scheduling course asks for (arrival, first run, finish,
// response, turnaround, waiting); one timeline per core, each column a span of
// ticks coloured by the thread that ran; and the ready queue beside the most
// recent scheduling events.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use crate::falcon::sched::{CoreState, Lane, SchedEventKind, ThreadState, Tid};
use crate::ui::app::App;
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

pub(super) fn render_threads(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("Threads", PanelKind::Accent));
    if inner.height == 0 {
        return;
    }
    let Some(sched) = app.scheduler() else {
        render_scheduler_off(f, inner, app);
        return;
    };

    let threads = sched.threads();
    let cores = sched.cores().len();
    // Two borders and the header row, then one row per thread.
    let table_h = (threads.len() as u16 + 3).min(inner.height / 2).max(4);
    let gantt_h = (cores as u16 + 3).min(inner.height.saturating_sub(table_h + 4));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(table_h),
            Constraint::Length(gantt_h),
            Constraint::Min(0),
        ])
        .split(inner);

    render_summary(f, rows[0], app);
    render_thread_table(f, rows[1], app);
    render_timeline(f, rows[2], app);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[3]);
    render_ready_queue(f, bottom[0], app);
    render_events(f, bottom[1], app);
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
                "—".to_string()
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
            "utilization",
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

fn render_thread_table(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let inner = render_panel(f, area, panel::panel("threads", PanelKind::Plain));
    let header = format!(
        " {:>3}  {:<9} {:>4} {:>4}  {:<10} {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}  {:>4} {:>4} {:>4}",
        "TID", "STATE", "CORE", "PRIO", "PC", "ARRIV", "1ST", "END", "RUN", "WAIT", "BLOCK",
        "RESP", "TURN", "DISP", "PRE", "YLD"
    );
    let mut lines = vec![Line::from(Span::styled(header, style::label()))];
    let opt = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
    let room = inner.height.saturating_sub(1) as usize;
    let threads = sched.threads();
    // Keep the most recent threads in view when they do not all fit.
    let skip = threads.len().saturating_sub(room);
    for t in threads.iter().skip(skip) {
        let st = &t.stats;
        let (state, state_color) = match t.state {
            ThreadState::Ready => ("READY".to_string(), theme::PAUSED),
            ThreadState::Running(core) => match sched.cores()[core] {
                CoreState::Switching { .. } => ("SWITCH".to_string(), theme::SPECULATIVE),
                _ => ("RUN".to_string(), theme::RUNNING),
            },
            ThreadState::Blocked(on) => (format!("JOIN T{on}"), theme::DANGER),
            ThreadState::Exited => ("EXIT".to_string(), theme::IDLE),
        };
        let core = match t.state {
            ThreadState::Running(core) => core.to_string(),
            _ => "—".to_string(),
        };
        let pc = app
            .thread_pc(t.tid)
            .map_or_else(|| "—".to_string(), |pc| format!("0x{pc:08X}"));
        let row_style = if sched.core_of(t.tid) == Some(app.selected_core) {
            Style::default().bg(theme::SEL_ROW_BG)
        } else {
            Style::default()
        };
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!(" {:>3}", format!("T{}", t.tid)),
                    Style::default().fg(thread_color(t.tid)).bold(),
                ),
                Span::raw("  "),
                Span::styled(format!("{state:<9}"), Style::default().fg(state_color)),
                Span::styled(
                    format!(
                        " {core:>4} {:>4}  {pc:<10} {:>6} {:>6} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}  {:>4} {:>4} {:>4}",
                        t.priority,
                        st.arrival,
                        opt(st.first_run),
                        opt(st.finish),
                        st.run_ticks,
                        st.ready_ticks,
                        st.blocked_ticks,
                        opt(st.response()),
                        opt(st.turnaround()),
                        st.dispatches,
                        st.preemptions,
                        st.yields,
                    ),
                    style::value(),
                ),
            ])
            .style(row_style),
        );
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_timeline(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let now = sched.now();
    let label_w = 6u16;
    let inner = render_panel(f, area, panel::panel("timeline", PanelKind::Plain));
    let cols = inner.width.saturating_sub(label_w + 1).max(1) as u64;
    // Every tick so far, squeezed into the width: each column is a span of
    // ticks, shown as whatever held the core at the span's start.
    let per_col = now.div_ceil(cols).max(1);
    let mut lines = Vec::new();
    for core in 0..sched.cores().len() {
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
            let (glyph, color) = match lane {
                Some(Lane::Run(tid)) => (thread_glyph(tid), thread_color(tid)),
                Some(Lane::Switch(_)) => ('·', theme::SPECULATIVE),
                Some(Lane::Idle) => ('─', theme::BORDER),
                // Older than what the timeline keeps.
                None => (' ', theme::BORDER),
            };
            spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(vec![
        Span::styled(format!(" {:<w$}", "", w = label_w as usize), style::label()),
        Span::styled(
            format!(
                "0 … {now}  ({per_col} tick{} per column · 0-9 a-z thread · `·` switching · `─` idle)",
                if per_col == 1 { "" } else { "s" }
            ),
            style::label(),
        ),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_ready_queue(f: &mut Frame, area: Rect, app: &App) {
    let Some(sched) = app.scheduler() else {
        return;
    };
    let inner = render_panel(f, area, panel::panel("ready queue", PanelKind::Plain));
    let mut spans = vec![Span::styled(" head → ", style::label())];
    let mut any = false;
    for tid in sched.ready_queue() {
        any = true;
        let prio = sched.thread(tid).map_or(0, |t| t.priority);
        spans.push(Span::styled(
            format!("T{tid}"),
            Style::default().fg(thread_color(tid)).bold(),
        ));
        if sched.config().policy == crate::falcon::sched::SchedPolicy::Priority {
            spans.push(Span::styled(format!("(p{prio})"), style::label()));
        }
        spans.push(Span::raw(" "));
    }
    if !any {
        spans.push(Span::styled("(empty)", style::label()));
    }
    let mut lines = vec![Line::from(spans), Line::raw("")];
    for (core, state) in sched.cores().iter().enumerate() {
        let (text, color) = match *state {
            CoreState::Idle => ("idle".to_string(), theme::IDLE),
            CoreState::Switching { tid, left } => {
                (format!("switching to T{tid} ({left} left)"), theme::SPECULATIVE)
            }
            CoreState::Running { tid, slice } => {
                let quantum = sched.config().quantum;
                let q = if sched.config().policy.is_preemptive() {
                    format!("  {slice}/{quantum} of its quantum")
                } else {
                    String::new()
                };
                (format!("T{tid}{q}"), thread_color(tid))
            }
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" core {core}: "), style::label()),
            Span::styled(text, Style::default().fg(color)),
        ]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
                SchedEventKind::Spawn { parent: Some(p) } => format!("started by T{p}"),
                SchedEventKind::Spawn { parent: None } => "started (program)".to_string(),
                SchedEventKind::Dispatch { core } => format!("→ core {core}"),
                SchedEventKind::Leave { core, reason } => {
                    format!("leaves core {core} ({})", reason.label())
                }
                SchedEventKind::Wake => "woken: its join target exited".to_string(),
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

    #[test]
    fn the_tab_draws_the_threads_the_queue_and_the_timeline() {
        let mut app = App::new(None);
        app.max_cores = 1;
        app.sched_config.policy = SchedPolicy::RoundRobin;
        app.sched_config.quantum = 4;
        app.editor.buf.lines = [
            ".text",
            "_start:",
            "li t5, -8192",
            "add a1, sp, t5",
            "la a0, worker",
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
        for _ in 0..60 {
            app.single_step();
        }
        let text = screen(&app);
        assert!(text.contains("Round robin"), "{text}");
        assert!(text.contains(" T0 "), "{text}");
        assert!(text.contains(" T1 "), "{text}");
        assert!(text.contains("ready queue"), "{text}");
        assert!(text.contains("timeline"), "{text}");
        assert!(text.contains("C0"), "{text}");
        assert!(text.contains("leaves core 0 (quantum)"), "{text}");
    }

    #[test]
    fn without_a_scheduler_the_tab_says_how_to_turn_one_on() {
        let app = App::new(None);
        let text = screen(&app);
        assert!(text.contains("No scheduler"), "{text}");
        assert!(text.contains("THREAD SCHEDULING"), "{text}");
    }
}
