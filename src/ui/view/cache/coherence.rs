// ui/view/cache/coherence.rs — the Coherence subtab
//
//   ┌ Coherence · MESI ──────────────────────────────────────────────────────┐
//   │ what the protocol does, in a sentence          snoop 4 cyc · c2c 10 cyc │
//   ├ Bus traffic per core ───────────────────────────────────────────────────┤
//   │ core  BusRd BusRdX BusUpgr BusWr  inval  flush  c2c  coh.miss  bus cyc  │
//   ├ Recent lines ───────────────────────────────────────────────────────────┤
//   │ line        core0 core1 …                                               │
//   ├ Bus requests (newest first) ────────────────────────────────────────────┤
//   │ #instr  core  request  line   what every cache did                      │
//   └─────────────────────────────────────────────────────────────────────────┘
//
// Everything here reads `CacheHierarchy::coherence` and the per-core sets; it
// never touches the caches.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use raven_engine::capability::{
    CacheHierarchy, CacheRole, CoherenceEvent, CoherenceRequest, CoherenceState,
};

use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

/// How many recently touched lines the state matrix lists.
const RECENT_LINES: usize = 8;

pub(super) fn render_coherence(f: &mut Frame, area: Rect, app: &App) {
    let Some(caches) = app.cache_hierarchy() else {
        return;
    };
    let Some(coherence) = caches.coherence() else {
        render_shared_notice(f, area, app);
        return;
    };

    let cores = coherence.cores.len().max(caches.core_count());
    let recent = recent_lines(&coherence.events);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(cores as u16 + 3),
            Constraint::Length(recent.len().max(1) as u16 + 3),
            Constraint::Min(4),
        ])
        .split(area);

    // ── Protocol ─────────────────────────────────────────────────────────
    let title = format!("Coherence · {}", coherence.protocol);
    let inner = render_panel(f, layout[0], panel::panel(title, PanelKind::Accent));
    let mut header = vec![Line::from(Span::styled(
        format!(" {}", coherence.description),
        style::value(),
    ))];
    let mut costs = vec![Span::raw(" ")];
    if coherence.coherent {
        costs.push(Span::styled("snoop ", style::label()));
        costs.push(Span::styled(
            format!("{} cyc", coherence.snoop_latency),
            Style::default().fg(theme::LABEL_Y),
        ));
        costs.push(Span::styled("  ·  cache-to-cache ", style::label()));
        costs.push(Span::styled(
            format!("{} cyc", coherence.cache_to_cache_latency),
            Style::default().fg(theme::LABEL_Y),
        ));
        costs.push(Span::styled("  ·  states ", style::label()));
        for state in coherence.states {
            costs.push(state_span(*state));
            costs.push(Span::raw(" "));
        }
    } else {
        costs.push(Span::styled(
            "No bus between the L1s: a core keeps its own copy until it evicts it.",
            style::warning(),
        ));
    }
    header.push(Line::from(costs));
    f.render_widget(
        Paragraph::new(header).wrap(ratatui::widgets::Wrap { trim: false }),
        inner,
    );

    // ── Counters ─────────────────────────────────────────────────────────
    let inner = render_panel(
        f,
        layout[1],
        panel::panel("Bus traffic per core", PanelKind::Plain),
    );
    const COLS: [(&str, usize); 10] = [
        ("core", 6),
        ("BusRd", 7),
        ("BusRdX", 8),
        ("BusUpgr", 9),
        ("BusWr", 7),
        ("inval.", 8),
        ("flush", 7),
        ("c2c", 6),
        ("coh.miss", 10),
        ("bus cyc", 9),
    ];
    let mut lines = vec![Line::from(
        COLS.iter()
            .map(|(name, w)| Span::styled(format!("{name:>w$}"), style::label()))
            .collect::<Vec<_>>(),
    )];
    for core in 0..cores {
        let c = coherence.cores.get(core).copied().unwrap_or_default();
        let selected = core == app.selected_core;
        let values = [
            c.bus_reads,
            c.bus_read_exclusive,
            c.bus_upgrades,
            c.bus_writes,
            c.invalidations_received,
            c.flushes,
            c.cache_to_cache,
            c.coherence_misses,
            c.bus_cycles,
        ];
        let core_style = if selected {
            Style::default().fg(theme::ACCENT).bold()
        } else {
            style::value()
        };
        let mut spans = vec![Span::styled(
            format!(
                "{:>6}",
                format!("{}{core}", if selected { "▸" } else { "" })
            ),
            core_style,
        )];
        for (value, (_, w)) in values.iter().zip(&COLS[1..]) {
            let text = format!("{value:>w$}", w = *w);
            let s = if *value == 0 {
                style::label()
            } else {
                style::value()
            };
            spans.push(Span::styled(text, s));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);

    // ── Recent lines × cores ─────────────────────────────────────────────
    let inner = render_panel(
        f,
        layout[2],
        panel::panel(
            "Recent lines — state in each core's D-cache",
            PanelKind::Plain,
        ),
    );
    let mut lines = Vec::new();
    let mut head = vec![Span::styled(format!("{:<12}", " line"), style::label())];
    for core in 0..cores {
        head.push(Span::styled(
            format!("{:>7}", format!("core{core}")),
            style::label(),
        ));
    }
    lines.push(Line::from(head));
    if recent.is_empty() {
        lines.push(Line::from(Span::styled(
            " No bus request yet — run a program that shares data between harts.",
            style::label(),
        )));
    }
    for line_base in &recent {
        let mut spans = vec![Span::styled(format!(" 0x{line_base:08X} "), style::value())];
        for core in 0..cores {
            let state = line_state(caches.inner, core, *line_base);
            let cell = match state {
                Some(state) if coherence.coherent => state_span(state),
                Some(state) => Span::styled(
                    if state == CoherenceState::Invalid {
                        "-"
                    } else {
                        "V"
                    }
                    .to_string(),
                    Style::default().fg(theme::coherence_state(state)),
                ),
                None => Span::styled("-", style::label()),
            };
            spans.push(Span::raw(" ".repeat(6)));
            spans.push(cell);
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);

    // ── Event log ────────────────────────────────────────────────────────
    let inner = render_panel(
        f,
        layout[3],
        panel::panel("Bus requests — newest first", PanelKind::Plain),
    );
    let mut lines = vec![Line::from(Span::styled(
        format!(
            " {:>8}  {:<5} {:<9} {:<11} {}",
            "#instr", "core", "request", "line", "transitions"
        ),
        style::label(),
    ))];
    let rows = inner.height.saturating_sub(1) as usize;
    for event in coherence.events.iter().rev().take(rows) {
        lines.push(event_line(event));
    }
    if coherence.events.is_empty() {
        lines.push(Line::from(Span::styled(" (empty)", style::label())));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// One letter in its state colour.
fn state_span(state: CoherenceState) -> Span<'static> {
    Span::styled(
        state.letter().to_string(),
        Style::default().fg(theme::coherence_state(state)).bold(),
    )
}

/// `core`'s D-cache state for the line at `line_base`: `None` when that core
/// has no private caches yet.
fn line_state(caches: &dyn CacheHierarchy, core: usize, line_base: u64) -> Option<CoherenceState> {
    let view = caches.core_cache(core, 0, CacheRole::Data)?;
    let cfg = view.config;
    if !cfg.is_enabled() {
        return None;
    }
    let offset_bits = cfg.offset_bits();
    let index_bits = cfg.index_bits();
    let set = ((line_base >> offset_bits) & ((1u64 << index_bits) - 1)) as usize;
    let tag = line_base >> (offset_bits + index_bits);
    let set = caches.core_set(core, 0, CacheRole::Data, set)?;
    let line = set.lines.iter().find(|line| line.valid && line.tag == tag);
    Some(match line {
        Some(line) => line.state.unwrap_or(CoherenceState::Shared),
        None => CoherenceState::Invalid,
    })
}

/// The distinct lines the most recent requests touched, newest first.
fn recent_lines(events: &std::collections::VecDeque<CoherenceEvent>) -> Vec<u64> {
    let mut lines = Vec::with_capacity(RECENT_LINES);
    for event in events.iter().rev() {
        if !lines.contains(&event.line) {
            lines.push(event.line);
            if lines.len() == RECENT_LINES {
                break;
            }
        }
    }
    lines
}

fn event_line(event: &CoherenceEvent) -> Line<'static> {
    let request_color = match event.request {
        CoherenceRequest::BusRd | CoherenceRequest::IFetch => theme::CACHE_I,
        CoherenceRequest::BusRdX | CoherenceRequest::BusUpgr | CoherenceRequest::BusWr => {
            theme::DIRTY
        }
        CoherenceRequest::WriteBack => theme::CACHE_L2,
    };
    let mut spans = vec![
        Span::styled(format!(" {:>8}  ", event.instruction), style::label()),
        Span::styled(
            format!("{:<5} ", format!("c{}", event.core)),
            style::value(),
        ),
        Span::styled(
            format!("{:<9} ", event.request.name()),
            Style::default().fg(request_color).bold(),
        ),
        Span::styled(format!("0x{:08X}  ", event.line), style::value()),
    ];
    for (i, t) in event.transitions.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(", ", style::label()));
        }
        spans.push(Span::styled(format!("c{} ", t.core), style::label()));
        spans.push(state_span(t.from));
        spans.push(Span::styled("→", style::label()));
        spans.push(state_span(t.to));
    }
    if let Some(core) = event.supplied_by {
        spans.push(Span::styled(
            format!("  data from c{core}'s cache"),
            Style::default().fg(theme::SPECULATIVE),
        ));
    } else if event.flushed {
        spans.push(Span::styled(
            "  written back first",
            Style::default().fg(theme::CACHE_L2),
        ));
    }
    Line::from(spans)
}

/// What the subtab says while every core shares one L1.
fn render_shared_notice(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel("Coherence · Shared L1", PanelKind::Accent),
    );
    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  Every core uses the same L1, so each line has exactly one copy and",
            style::value(),
        )),
        Line::from(Span::styled(
            "  there is nothing to keep coherent.",
            style::value(),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "  To give each core its own L1, pick a protocol in Settings → coherence:",
            style::label(),
        )),
        Line::from(Span::styled(
            "    none   private L1s with no protocol — watch a core read stale data",
            style::label(),
        )),
        Line::from(Span::styled(
            "    MSI    Modified / Shared / Invalid",
            style::label(),
        )),
        Line::from(Span::styled(
            "    MESI   + Exclusive: the first write to an unshared line is silent",
            style::label(),
        )),
        Line::from(Span::styled(
            "    MOESI  + Owned: a dirty line is shared without a write-back",
            style::label(),
        )),
    ];
    if app.max_cores < 2 {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Raise Settings → max cores to 2 or more first.",
            style::warning(),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}
