// ui/view/tlb/stats.rs — what translation cost.
//
//   ╭─ Summary ────────────────────────────────────────────────────────────╮
//   │ hit rate 96.2%   hits 1812   misses 71   faults 0   evictions 12      │
//   │ cost     1420 cycles walking page tables · 11.8% of the run's 12034   │
//   │ reach    32 entries · the program has touched 19 pages ✓              │
//   ╰───────────────────────────────────────────────────────────────────────╯
//   ╭─ Hit rate ────────────────────────────╮╭─ Why it missed ──────────────╮
//   ╭─ Snapshots ───────────────────────────────────────────────────────────╮
//
// The snapshots are the Cache tab's list, read for its translation numbers.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

use raven_engine::capability::AddressTranslation;

use super::size_long;
use crate::ui::app::{App, VmHit};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let history_h = if app.cache.session_history.is_empty() {
        0
    } else {
        (app.cache.session_history.len() as u16 + 2).min(7)
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(history_h),
        ])
        .split(area);

    render_summary(f, layout[0], app);
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(30), Constraint::Length(44)])
        .split(layout[1]);
    if layout[1].width >= 84 {
        render_hit_chart(f, middle[0], app);
        render_miss_causes(f, middle[1], app);
    } else {
        render_hit_chart(f, layout[1], app);
    }
    if history_h > 0 {
        render_history_table(f, layout[2], app);
    }
}

fn metric(label: &str, value: String, color: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled(label.to_string(), style::idle()),
        Span::styled(
            format!(" {value}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
    ]
}

fn render_summary(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel(super::core_title(app, "Summary"), PanelKind::Plain));
    let Some(translation) = app.translation() else {
        return;
    };
    if inner.height == 0 {
        return;
    }
    let stats = translation.tlb_stats();
    let hit_rate = stats.hit_rate();
    let hit_color = if hit_rate >= 90.0 {
        theme::RUNNING
    } else if hit_rate >= 60.0 {
        theme::PAUSED
    } else {
        theme::DANGER
    };

    let mut first = vec![Span::raw(" ")];
    first.extend(metric("hit rate", format!("{hit_rate:.1}%"), hit_color));
    first.extend(metric("hits", stats.hits.to_string(), theme::TEXT));
    first.extend(metric("misses", stats.misses.to_string(), theme::TEXT));
    first.extend(metric(
        "faults",
        stats.faults.to_string(),
        if stats.faults > 0 {
            theme::DANGER
        } else {
            theme::TEXT
        },
    ));
    first.extend(metric("evictions", stats.evictions.to_string(), theme::TEXT));

    // Walks are the cost: a hit's cycle hides behind the pipeline, a walk's
    // twenty do not.
    let program_cycles = app.execution_totals().cycles;
    let share = if program_cycles > 0 {
        stats.walk_cycles as f64 / program_cycles as f64 * 100.0
    } else {
        0.0
    };
    let narrow = inner.width < 100;
    let cost = Line::from(vec![
        Span::styled(" cost     ", style::idle()),
        Span::styled(
            format!("{} cycles", stats.walk_cycles),
            Style::default().fg(theme::METRIC_CYC).bold(),
        ),
        Span::styled(
            if narrow {
                format!(" walking · {share:.1}% of the run")
            } else {
                format!(
                    " walking page tables · {share:.1}% of the run's {program_cycles} cycles"
                )
            },
            style::label(),
        ),
    ]);

    // Reach in pages, not bytes: an entry holds one page of either size, so
    // what matters is whether the pages in use outnumber the entries.
    let entries = translation.tlb_len() as u64;
    let pages = stats.first_touches;
    let (verdict, verdict_color) = if pages == 0 {
        ("", theme::IDLE)
    } else if pages <= entries {
        (" ✓ they all fit", theme::RUNNING)
    } else {
        (" ✗ more pages than entries", theme::PAUSED)
    };
    let reach_line = Line::from(vec![
        Span::styled(" reach    ", style::idle()),
        Span::styled(format!("{entries} entries"), style::value()),
        Span::styled(
            if narrow {
                format!(" · {pages} pages touched")
            } else {
                format!(
                    " · the program has touched {pages} pages ({})",
                    size_long(stats.first_touch_bytes)
                )
            },
            style::label(),
        ),
        Span::styled(verdict, Style::default().fg(verdict_color)),
    ]);

    let ways = translation.tlb_len() / translation.tlb_sets().max(1);
    let shape = Line::from(Span::styled(
        format!(
            " shape    {} sets × {ways} ways · {}{}",
            translation.tlb_sets(),
            translation.tlb_policy(),
            if translation.tlb_enabled() {
                ""
            } else {
                " · off: every access walks"
            }
        ),
        style::label(),
    ));

    f.render_widget(
        Paragraph::new(vec![Line::from(first), cost, reach_line, shape]),
        inner,
    );
}

/// Every miss has one cause: a page touched for the first time, a page the TLB
/// held and lost, a store recording a page's first write, or no mapping at all.
fn render_miss_causes(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::panel("Why it missed", PanelKind::Plain));
    let Some(translation) = app.translation() else {
        return;
    };
    let stats = translation.tlb_stats();
    let total = stats.misses.max(1) as f64;
    let rows = [
        (
            "first touch of a page",
            stats.first_touches,
            "unavoidable",
            theme::IDLE,
        ),
        (
            "page held before, lost",
            stats.returning_misses(),
            "bigger TLB helps",
            theme::PAUSED,
        ),
        (
            "store to a clean page",
            stats.dirty_rewalks,
            "sets D",
            theme::IDLE,
        ),
        ("no mapping", stats.walk_faults, "page fault", theme::DANGER),
    ];
    let mut lines = Vec::new();
    for (label, count, note, color) in rows {
        let pct = count as f64 / total * 100.0;
        lines.push(Line::from(vec![
            Span::styled(format!(" {label:<23}"), style::label()),
            Span::styled(
                format!("{count:>6}"),
                Style::default().fg(theme::TEXT).bold(),
            ),
            Span::styled(format!(" {pct:>3.0}%"), style::idle()),
        ]));
        lines.push(Line::from(Span::styled(
            format!("   {note}"),
            Style::default().fg(color),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_hit_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = panel::panel("Hit rate", PanelKind::Plain);
    let pts: Vec<(f64, f64)> = app
        .translation()
        .map(|translation| translation.hit_rate_history())
        .unwrap_or_default();
    if pts.is_empty() || area.height < 5 {
        let inner = render_panel(f, area, block);
        let msg = if pts.is_empty() {
            "  no data yet — run a program with virtual memory on"
        } else {
            "  (chart hidden — the terminal is too short)"
        };
        if super::idle_notice(app).is_some() && pts.is_empty() {
            super::render_empty(f, inner, app, msg);
        } else {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(msg, style::label()))),
                inner,
            );
        }
        return;
    }
    let x_min = pts.first().map(|p| p.0).unwrap_or(0.0);
    let x_max = pts.last().map(|p| p.0).unwrap_or(1.0).max(x_min + 1.0);
    let datasets = vec![
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::RUNNING))
            .data(&pts),
    ];
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{x_min:.0}")),
                    Span::raw(format!("{x_max:.0}")),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("100%")]),
        );
    f.render_widget(chart, area);
}

/// Session snapshots, translation lens: the same list as the Cache tab.
fn render_history_table(f: &mut Frame, area: Rect, app: &App) {
    let is_running = app.session.is_running;
    let state = if is_running {
        "pause to inspect"
    } else {
        "enter opens · d deletes"
    };
    let inner = render_panel(
        f,
        area,
        panel::panel_state("Snapshots", state, PanelKind::Plain),
    );
    if inner.height == 0 {
        return;
    }

    let history = &app.cache.session_history;
    let selected = app.cache.history_scroll;
    let visible = inner.height as usize;
    let start = (selected + 1).saturating_sub(visible);

    for (i, snap) in history.iter().enumerate().skip(start).take(visible) {
        let y = inner.y + (i - start) as u16;
        let row = Rect::new(inner.x, y, inner.width, 1);
        let hit = VmHit::Snapshot(i);
        app.tlb.hit(hit, row);
        let text = match &snap.tlb {
            Some(t) => format!(
                "  {:<14}  hit rate {:.1}%   hits {}   misses {}   faults {}   evictions {}",
                snap.label,
                t.hit_rate(),
                t.hits,
                t.misses,
                t.page_faults,
                t.evictions
            ),
            None => format!("  {:<14}  (virtual memory was off)", snap.label),
        };
        let style = if is_running {
            Style::default().fg(theme::IDLE)
        } else if i == selected {
            Style::default().bg(theme::SEL_ROW_BG).fg(theme::TEXT)
        } else if app.tlb.is_hovered(hit) {
            Style::default().bg(theme::HOVER_ROW_BG).fg(theme::TEXT)
        } else {
            Style::default().fg(theme::TEXT)
        };
        f.render_widget(Paragraph::new(Span::styled(text, style)), row);
    }
}
